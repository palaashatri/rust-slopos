//! Network locations (SMB and SFTP) through one bounded provider.
//!
//! Files never touches the network on the UI thread. A single worker thread
//! runs the desktop's `gio` tool (GVfs) under a hard timeout, classifies any
//! failure and wakes the event loop with the answer. Passwords are never part
//! of an address and are never stored here: a sign-in prompt has to come from
//! the desktop secret agent, which is not running yet, so an auth failure is
//! shown as text.

use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread;
use std::time::Duration;

use crate::model::MAX_ENTRIES;
use crate::worker::{wake, WakeSlot};

/// Hard limit for one listing, from start to finish.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(15);
/// Largest provider output accepted for one listing.
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SEGMENTS: usize = 64;
const MAX_SEGMENT_BYTES: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Smb,
    Sftp,
}

impl Scheme {
    /// Split `text` into its scheme and the rest, if it names a network scheme.
    fn split(text: &str) -> Option<(Self, &str)> {
        let (scheme, rest) = text.split_once("://")?;
        let scheme = match scheme.to_ascii_lowercase().as_str() {
            "smb" => Self::Smb,
            "sftp" => Self::Sftp,
            _ => return None,
        };
        Some((scheme, rest))
    }

    fn name(self) -> &'static str {
        match self {
            Self::Smb => "smb",
            Self::Sftp => "sftp",
        }
    }
}

/// True when the location bar's text names a network address.
pub fn is_network_address(text: &str) -> bool {
    Scheme::split(text.trim()).is_some()
}

/// A rejected address, with text the user can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationError(&'static str);

impl std::fmt::Display for LocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// A validated network address. It never carries a password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub scheme: Scheme,
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    /// Path segments below the server. For SMB the first one is the share.
    pub segments: Vec<String>,
}

impl Location {
    /// Parse `smb://[user@]server[/share[/path]]` or
    /// `sftp://[user@]server[:port][/path]`.
    pub fn parse(text: &str) -> Result<Self, LocationError> {
        let (scheme, rest) = Scheme::split(text.trim()).ok_or(LocationError(
            "Type an address that starts with smb:// or sftp://.",
        ))?;
        if rest
            .chars()
            .any(|c| c.is_control() || matches!(c, '?' | '#' | '%' | '\\'))
        {
            return Err(LocationError(
                "The address contains characters that aren't supported.",
            ));
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (user, hostport) = match authority.rsplit_once('@') {
            Some((user, hostport)) => {
                if user.contains(':') {
                    return Err(LocationError(
                        "Don't put a password in the address. Files asks for sign-in separately.",
                    ));
                }
                if !valid_user(user) {
                    return Err(LocationError(
                        "The user name contains characters that aren't allowed.",
                    ));
                }
                (Some(user.to_owned()), hostport)
            }
            None => (None, authority),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((host, port)) => {
                if scheme == Scheme::Smb {
                    return Err(LocationError("SMB addresses can't take a port number."));
                }
                let port = port
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or(LocationError("The port must be a number from 1 to 65535."))?;
                (host, Some(port))
            }
            None => (hostport, None),
        };
        if host.is_empty() {
            return Err(LocationError("Enter a server name after the scheme."));
        }
        if !valid_host(host) {
            return Err(LocationError(
                "The server name contains characters that aren't allowed.",
            ));
        }
        let mut segments = Vec::new();
        for segment in path.split('/') {
            match segment {
                "" => {}
                "." | ".." => {
                    return Err(LocationError("Paths can't contain “.” or “..”."));
                }
                name if name.len() > MAX_SEGMENT_BYTES => {
                    return Err(LocationError("A name in the address is too long."));
                }
                name => segments.push(name.to_owned()),
            }
        }
        if segments.len() > MAX_SEGMENTS {
            return Err(LocationError("The address has too many folders."));
        }
        Ok(Self {
            scheme,
            user,
            host: host.to_owned(),
            port,
            segments,
        })
    }

    /// The address as the user types it (the location bar shows this form).
    pub fn display(&self) -> String {
        self.render(false)
    }

    /// The address as a URI for the provider, with names percent-encoded.
    pub fn uri(&self) -> String {
        self.render(true)
    }

    fn render(&self, encode: bool) -> String {
        let mut out = format!("{}://", self.scheme.name());
        if let Some(user) = &self.user {
            out.push_str(user);
            out.push('@');
        }
        out.push_str(&self.host);
        if let Some(port) = self.port {
            out.push_str(&format!(":{port}"));
        }
        if self.segments.is_empty() {
            out.push('/');
        }
        for segment in &self.segments {
            out.push('/');
            if !encode {
                out.push_str(segment);
                continue;
            }
            for byte in segment.bytes() {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                    out.push(char::from(byte));
                } else {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
        out
    }
}

fn valid_host(host: &str) -> bool {
    host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn valid_user(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 64
        && user
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Why a listing failed, in the words the status bar shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The server did not answer, or refused the name or connection.
    Unreachable(String),
    /// The provider did not finish within [`LIST_TIMEOUT`] and was stopped.
    TimedOut,
    /// The server wants credentials that Files doesn't have.
    SignInRequired,
    /// The `gio` tool is not installed.
    ProviderMissing,
    /// Anything else the provider reported.
    Failed(String),
    /// More rows than one listing may hold.
    TooManyRows,
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(detail) => write!(f, "Can't reach the server. {detail}"),
            Self::TimedOut => write!(
                f,
                "The server didn't answer within {} seconds, so the request was stopped.",
                LIST_TIMEOUT.as_secs()
            ),
            Self::SignInRequired => f.write_str(
                "Sign-in is required. Saved sign-in prompts need the desktop secret agent, \
                 which isn't running, so Files can't ask for a password yet.",
            ),
            Self::ProviderMissing => {
                f.write_str("Network locations need the gio tool from GVfs, which isn't installed.")
            }
            Self::Failed(detail) => write!(f, "The server reported an error: {detail}"),
            Self::TooManyRows => write!(
                f,
                "This location has more than {MAX_ENTRIES} entries and can't be listed."
            ),
        }
    }
}

/// Map a failed provider run's diagnostics to a state. Matching uses the
/// English phrases GVfs and OpenSSH print. Anything unrecognised is shown
/// verbatim as a failure, never as a success.
pub fn classify(diagnostics: &str) -> NetError {
    const SIGN_IN: [&str; 6] = [
        "authentication",
        "password",
        "logon failure",
        "access denied",
        "permission denied",
        "login failed",
    ];
    const UNREACHABLE: [&str; 9] = [
        "could not resolve",
        "name or service not known",
        "unknown host",
        "could not connect",
        "connection refused",
        "no route to host",
        "host is down",
        "network is unreachable",
        "connection timed out",
    ];
    let lower = diagnostics.to_lowercase();
    let first: String = diagnostics
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no details")
        .chars()
        .take(160)
        .collect();
    if SIGN_IN.iter().any(|phrase| lower.contains(phrase)) {
        NetError::SignInRequired
    } else if UNREACHABLE.iter().any(|phrase| lower.contains(phrase)) {
        NetError::Unreachable(first)
    } else {
        NetError::Failed(first)
    }
}

/// Run `program` with `args` and stop it after `timeout`. The child leads its
/// own process group, so a timeout also stops whatever it started (GVfs
/// helpers, ssh). Only the timeout is waited on; nothing polls.
fn run_bounded(program: &Path, args: &[String], timeout: Duration) -> Result<Output, NetError> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => NetError::ProviderMissing,
            _ => NetError::Failed(error.to_string()),
        })?;
    let group = child.id();
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("files-network-wait".into())
        .spawn(move || {
            let _ = tx.send(child.wait_with_output());
        })
        .map_err(|error| NetError::Failed(error.to_string()))?;
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(NetError::Failed(error.to_string())),
        Err(RecvTimeoutError::Timeout) => {
            kill_group(group);
            Err(NetError::TimedOut)
        }
        Err(RecvTimeoutError::Disconnected) => {
            Err(NetError::Failed("the provider stopped unexpectedly".into()))
        }
    }
}

fn kill_group(group: u32) {
    // SAFETY: a negative pid addresses the process group led by our child.
    // The child has not been reaped yet (its waiter has not reported), so
    // the group id cannot have been reused.
    unsafe {
        libc::kill(-(group as libc::pid_t), libc::SIGKILL);
    }
}

/// Parse provider output: one name per line, blank lines ignored, sorted
/// case-insensitively. Output or row counts beyond the caps are refused.
pub fn parse_listing(stdout: &[u8], max_rows: usize) -> Result<Vec<String>, NetError> {
    if stdout.len() > MAX_OUTPUT_BYTES {
        return Err(NetError::TooManyRows);
    }
    let mut names: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(stdout).lines() {
        if line.trim().is_empty() {
            continue;
        }
        if names.len() >= max_rows {
            return Err(NetError::TooManyRows);
        }
        names.push(line.to_owned());
    }
    names.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    Ok(names)
}

/// Lists a location with the GVfs `gio` tool.
#[derive(Debug, Clone)]
pub struct Provider {
    program: std::path::PathBuf,
    /// Arguments placed before the subcommand (tests put a shell here).
    prefix: Vec<String>,
    timeout: Duration,
}

impl Provider {
    pub fn gio() -> Self {
        Self {
            program: "gio".into(),
            prefix: Vec::new(),
            timeout: LIST_TIMEOUT,
        }
    }

    /// Names inside `location`, sorted.
    pub fn list(&self, location: &Location) -> Result<Vec<String>, NetError> {
        let mut args = self.prefix.clone();
        args.push("list".into());
        args.push(location.uri());
        let output = run_bounded(&self.program, &args, self.timeout)?;
        if !output.status.success() {
            return Err(classify(&String::from_utf8_lossy(&output.stderr)));
        }
        parse_listing(&output.stdout, MAX_ENTRIES)
    }
}

/// One finished listing: the request sequence, the address and the answer.
pub type Answer = (u64, Location, Result<Vec<String>, NetError>);

/// One thread serves network listings, one at a time, queueing one more.
pub struct Lister {
    requests: SyncSender<(u64, Location)>,
    pub results: Receiver<Answer>,
}

impl Lister {
    pub fn new(slot: WakeSlot, provider: Provider) -> io::Result<Self> {
        let (requests, rx) = mpsc::sync_channel::<(u64, Location)>(1);
        let (tx, results) = mpsc::channel::<Answer>();
        thread::Builder::new()
            .name("files-network".into())
            .spawn(move || {
                while let Ok((seq, location)) = rx.recv() {
                    let result = provider.list(&location);
                    if tx.send((seq, location, result)).is_err() {
                        break;
                    }
                    wake(&slot);
                }
            })?;
        Ok(Self { requests, results })
    }

    pub fn request(&self, seq: u64, location: Location) -> Result<(), &'static str> {
        match self.requests.try_send((seq, location)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                Err("A network request is still running. Try again when it finishes.")
            }
            Err(TrySendError::Disconnected(_)) => Err("The network worker is unavailable."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    fn parse(text: &str) -> Location {
        Location::parse(text).unwrap()
    }

    fn shell(script: &str, timeout: Duration) -> Provider {
        Provider {
            program: "sh".into(),
            prefix: vec!["-c".into(), script.into(), "gio".into()],
            timeout,
        }
    }

    #[test]
    fn smb_addresses_name_a_share_and_path() {
        let location = parse("smb://nas.local/Media/Films/2026");
        assert_eq!(location.scheme, Scheme::Smb);
        assert_eq!(location.host, "nas.local");
        assert_eq!(location.port, None);
        assert_eq!(location.segments, ["Media", "Films", "2026"]);
        assert_eq!(location.display(), "smb://nas.local/Media/Films/2026");
        assert_eq!(location.uri(), "smb://nas.local/Media/Films/2026");
    }

    #[test]
    fn sftp_addresses_take_user_port_and_path() {
        let location = parse("SFTP://alice@files.example.org:2222/srv/data/");
        assert_eq!(location.scheme, Scheme::Sftp);
        assert_eq!(location.user.as_deref(), Some("alice"));
        assert_eq!(location.port, Some(2222));
        assert_eq!(location.segments, ["srv", "data"]);
        assert_eq!(
            location.display(),
            "sftp://alice@files.example.org:2222/srv/data"
        );
    }

    #[test]
    fn bare_servers_address_their_root_and_names_are_encoded() {
        let bare = parse("smb://nas");
        assert!(bare.segments.is_empty());
        assert_eq!(bare.uri(), "smb://nas/");
        assert_eq!(
            parse("smb://nas/My Share/a b").uri(),
            "smb://nas/My%20Share/a%20b"
        );
    }

    #[test]
    fn only_network_schemes_are_network_addresses() {
        assert!(is_network_address("  smb://nas/share"));
        assert!(is_network_address("sftp://host"));
        assert!(!is_network_address("https://example.org"));
        assert!(!is_network_address("/home/user/smb://x"));
    }

    #[test]
    fn invalid_addresses_are_refused_with_readable_text() {
        let cases = [
            ("smb://", "server name"),
            ("sftp://alice:secret@host/", "password"),
            ("smb://host:445/share", "port number"),
            ("sftp://host:0/", "port must"),
            ("sftp://host:70000/", "port must"),
            ("smb://bad_host/x", "server name contains"),
            ("smb://host/share/../etc", "“.”"),
            ("smb://host/a?b", "characters"),
            ("sftp://bad user@host/", "user name"),
            ("https://example.org", "smb:// or sftp://"),
        ];
        for (input, fragment) in cases {
            let error = Location::parse(input).unwrap_err();
            assert!(
                error.to_string().contains(fragment),
                "{input}: {error} should mention {fragment}"
            );
        }
    }

    #[test]
    fn provider_diagnostics_become_states() {
        assert_eq!(
            classify("Failed to mount: Logon failure: bad password."),
            NetError::SignInRequired
        );
        assert_eq!(
            classify("sftp: Permission denied (publickey,password)."),
            NetError::SignInRequired
        );
        assert!(matches!(
            classify("Could not resolve host: nas.example"),
            NetError::Unreachable(_)
        ));
        assert!(matches!(
            classify("ssh: connect to host x port 22: Connection refused"),
            NetError::Unreachable(_)
        ));
        assert_eq!(
            classify("Something odd happened"),
            NetError::Failed("Something odd happened".into())
        );
        assert_eq!(classify(""), NetError::Failed("no details".into()));
    }

    #[test]
    fn listings_are_sorted_and_blank_lines_ignored() {
        let provider = shell(
            "printf 'Zeta\\nalpha\\n\\nBeta\\r\\n'",
            Duration::from_secs(5),
        );
        let names = provider.list(&parse("smb://nas/share")).unwrap();
        assert_eq!(names, ["alpha", "Beta", "Zeta"]);
    }

    #[test]
    fn the_provider_receives_list_and_the_encoded_address() {
        let provider = shell(
            "[ \"$1\" = list ] && [ \"$2\" = \"smb://nas/My%20Share\" ] && echo Ok",
            Duration::from_secs(5),
        );
        let names = provider.list(&parse("smb://nas/My Share")).unwrap();
        assert_eq!(names, ["Ok"]);
    }

    #[test]
    fn auth_and_unreachable_failures_are_distinct_states() {
        let location = parse("sftp://files.example.org/");
        let auth = shell(
            "echo 'Logon failure: bad password' >&2; exit 1",
            Duration::from_secs(5),
        );
        assert_eq!(auth.list(&location).unwrap_err(), NetError::SignInRequired);
        let down = shell(
            "echo 'Could not resolve host' >&2; exit 2",
            Duration::from_secs(5),
        );
        assert!(matches!(
            down.list(&location).unwrap_err(),
            NetError::Unreachable(_)
        ));
    }

    #[test]
    fn slow_providers_are_stopped_at_the_timeout() {
        let started = Instant::now();
        let slow = shell("sleep 30", Duration::from_millis(300));
        assert_eq!(
            slow.list(&parse("smb://nas/share")).unwrap_err(),
            NetError::TimedOut
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_missing_provider_is_reported_not_faked() {
        let provider = Provider {
            program: "/nonexistent/slopos-gio-for-tests".into(),
            prefix: Vec::new(),
            timeout: Duration::from_secs(1),
        };
        assert_eq!(
            provider.list(&parse("smb://nas/share")).unwrap_err(),
            NetError::ProviderMissing
        );
    }

    #[test]
    fn output_beyond_the_row_cap_is_refused() {
        assert_eq!(parse_listing(b"a\nb\n", 1), Err(NetError::TooManyRows));
        assert_eq!(parse_listing(b"a\nb\n", 2).unwrap(), ["a", "b"]);
        let huge = vec![b'x'; MAX_OUTPUT_BYTES + 1];
        assert_eq!(
            parse_listing(&huge, MAX_ENTRIES),
            Err(NetError::TooManyRows)
        );
    }

    #[test]
    fn the_worker_answers_with_the_sequence_it_was_given() {
        let slot: WakeSlot = Arc::new(Mutex::new(None));
        let lister = Lister::new(slot, shell("sleep 30", Duration::from_millis(400))).unwrap();
        lister.request(41, parse("smb://nas/share")).unwrap();
        let (seq, location, result) = lister
            .results
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert_eq!(seq, 41);
        assert_eq!(location.host, "nas");
        assert_eq!(result, Err(NetError::TimedOut));
    }
}
