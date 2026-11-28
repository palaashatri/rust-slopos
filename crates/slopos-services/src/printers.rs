//! Printers through CUPS: the queues the local CUPS server knows, which one
//! is the default, and whether each is idle, printing or stopped.
//!
//! The answer comes from CUPS itself: IPP requests over HTTP on CUPS' local
//! Unix socket, and the binary IPP reply is decoded here. No command is run
//! and no text output is parsed. When CUPS is not running the provider says
//! so; when the socket refuses this user it says that instead. Adding and
//! removing printers is not here: it needs an authorized administrator.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use slopos_core::capability::Availability;
use thiserror::Error;

/// CUPS' local socket (`/run/cups/cups.sock` is the same file).
pub const SOCKET: &str = "/var/run/cups/cups.sock";
const TIMEOUT: Duration = Duration::from_secs(5);
/// The largest answer read. A queue list is far smaller.
const MAX_ANSWER: u64 = 1 << 20;

/// CUPS-Get-Default and CUPS-Get-Printers (CUPS extensions to IPP).
const OP_GET_DEFAULT: u16 = 0x4001;
const OP_GET_PRINTERS: u16 = 0x4002;
const PRINTER_ATTRIBUTES: &[&str] = &["printer-name", "printer-state", "printer-is-accepting-jobs"];

const TAG_BOOLEAN: u8 = 0x22;
const TAG_ENUM: u8 = 0x23;
const TAG_KEYWORD: u8 = 0x44;
const TAG_CHARSET: u8 = 0x47;
const TAG_LANGUAGE: u8 = 0x48;
const TAG_OPERATION_GROUP: u8 = 0x01;
const TAG_PRINTER_GROUP: u8 = 0x04;
const TAG_END: u8 = 0x03;

const STATUS_NOT_FOUND: u16 = 0x0406;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Idle,
    Printing,
    Stopped,
    Unknown,
}

impl PrinterState {
    /// IPP `printer-state`: 3 idle, 4 processing, 5 stopped.
    pub fn from_ipp(value: u32) -> Self {
        match value {
            3 => PrinterState::Idle,
            4 => PrinterState::Printing,
            5 => PrinterState::Stopped,
            _ => PrinterState::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printer {
    pub name: String,
    pub state: PrinterState,
    pub accepting_jobs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintersSnapshot {
    pub availability: Availability,
    /// Queues in CUPS' order.
    pub printers: Vec<Printer>,
    /// The default queue's name, when CUPS has one.
    pub default: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PrintersError {
    #[error("no CUPS socket answers")]
    NotRunning,
    #[error("the CUPS socket refuses this user")]
    PermissionDenied,
    #[error("{0}")]
    Failed(String),
}

impl PrintersError {
    pub fn availability(&self) -> Availability {
        match self {
            PrintersError::NotRunning => Availability::MissingService,
            PrintersError::PermissionDenied => Availability::PermissionDenied,
            PrintersError::Failed(_) => Availability::Failed,
        }
    }
}

/// What a failed socket operation means for the printers panel.
pub fn error_from_io(error: &std::io::Error) -> PrintersError {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => PrintersError::NotRunning,
        ErrorKind::PermissionDenied => PrintersError::PermissionDenied,
        _ => PrintersError::Failed(error.to_string()),
    }
}

/// The printers and the default, from CUPS on its default socket.
pub fn snapshot() -> Result<PrintersSnapshot, PrintersError> {
    snapshot_at(Path::new(SOCKET))
}

/// The printers and the default, from CUPS listening on `socket`.
pub fn snapshot_at(socket: &Path) -> Result<PrintersSnapshot, PrintersError> {
    let listing = request(socket, OP_GET_PRINTERS, PRINTER_ATTRIBUTES)?;
    let default = request(socket, OP_GET_DEFAULT, &["printer-name"])?;
    Ok(PrintersSnapshot {
        availability: Availability::Available,
        printers: printers_from(&listing),
        default: default_from(&default),
    })
}

/// One attribute of an IPP reply. Additional values of a multi-valued
/// attribute are left out; nothing here needs them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Attribute {
    /// Counts the attribute groups before it, so each printer is its own.
    group_index: usize,
    group: u8,
    name: String,
    tag: u8,
    value: Vec<u8>,
}

#[derive(Debug)]
struct Reply {
    status: u16,
    attributes: Vec<Attribute>,
}

fn request(
    socket: &Path,
    operation: u16,
    requested: &[&str],
) -> Result<Vec<Attribute>, PrintersError> {
    let body = encode_request(operation, requested);
    let mut stream = UnixStream::connect(socket).map_err(|e| error_from_io(&e))?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)))
        .map_err(|e| error_from_io(&e))?;
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&body))
        .map_err(|e| error_from_io(&e))?;
    let mut answer = Vec::new();
    (&mut stream)
        .take(MAX_ANSWER)
        .read_to_end(&mut answer)
        .map_err(|e| error_from_io(&e))?;
    let reply = decode_reply(&http_body(&answer)?)?;
    check_status(reply.status)?;
    Ok(reply.attributes)
}

/// Refuse what the user cannot do; "not found" is an empty answer (no
/// default queue, for instance).
fn check_status(status: u16) -> Result<(), PrintersError> {
    match status {
        0x0401..=0x0403 => Err(PrintersError::PermissionDenied),
        STATUS_NOT_FOUND => Ok(()),
        status if status >= 0x0400 => Err(PrintersError::Failed(format!(
            "CUPS answered with IPP status 0x{status:04x}"
        ))),
        _ => Ok(()),
    }
}

fn encode_request(operation: u16, requested: &[&str]) -> Vec<u8> {
    let mut body = vec![0x02, 0x00];
    body.extend_from_slice(&operation.to_be_bytes());
    body.extend_from_slice(&1u32.to_be_bytes());
    body.push(TAG_OPERATION_GROUP);
    push_attribute(&mut body, TAG_CHARSET, "attributes-charset", b"utf-8");
    push_attribute(
        &mut body,
        TAG_LANGUAGE,
        "attributes-natural-language",
        b"en",
    );
    for (index, keyword) in requested.iter().enumerate() {
        let name = if index == 0 {
            "requested-attributes"
        } else {
            ""
        };
        push_attribute(&mut body, TAG_KEYWORD, name, keyword.as_bytes());
    }
    body.push(TAG_END);
    body
}

fn push_attribute(body: &mut Vec<u8>, tag: u8, name: &str, value: &[u8]) {
    body.push(tag);
    body.extend_from_slice(&(name.len() as u16).to_be_bytes());
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(&(value.len() as u16).to_be_bytes());
    body.extend_from_slice(value);
}

/// The IPP body of an HTTP answer. A non-200 answer is refused.
fn http_body(answer: &[u8]) -> Result<Vec<u8>, PrintersError> {
    let malformed = || PrintersError::Failed("CUPS sent an answer that is not HTTP".into());
    let split = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(malformed)?;
    let head = String::from_utf8_lossy(&answer[..split]).into_owned();
    let code: u16 = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(malformed)?;
    match code {
        200 => {}
        401 | 403 => return Err(PrintersError::PermissionDenied),
        code => return Err(PrintersError::Failed(format!("CUPS answered HTTP {code}"))),
    }
    let rest = &answer[split + 4..];
    let chunked = head.lines().any(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with("transfer-encoding:") && line.contains("chunked")
    });
    if chunked {
        dechunk(rest)
    } else {
        Ok(rest.to_vec())
    }
}

fn dechunk(mut rest: &[u8]) -> Result<Vec<u8>, PrintersError> {
    let malformed = || PrintersError::Failed("CUPS sent a malformed HTTP body".into());
    let mut body = Vec::new();
    loop {
        let line_end = rest
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(malformed)?;
        let size_text = String::from_utf8_lossy(&rest[..line_end]).into_owned();
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| malformed())?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Ok(body);
        }
        body.extend_from_slice(rest.get(..size).ok_or_else(malformed)?);
        rest = rest.get(size + 2..).ok_or_else(malformed)?;
    }
}

fn be_u16(body: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*body.get(at)?, *body.get(at + 1)?]))
}

/// A length-prefixed IPP string at `position`, which moves past it.
fn take_bytes(body: &[u8], position: &mut usize) -> Option<Vec<u8>> {
    let len = usize::from(be_u16(body, *position)?);
    let start = *position + 2;
    let value = body.get(start..start + len)?.to_vec();
    *position = start + len;
    Some(value)
}

fn decode_reply(body: &[u8]) -> Result<Reply, PrintersError> {
    let malformed = || PrintersError::Failed("CUPS sent a malformed IPP answer".into());
    let status = be_u16(body, 2).ok_or_else(malformed)?;
    let mut position = 8;
    let mut group = 0u8;
    let mut group_index = 0usize;
    let mut attributes = Vec::new();
    loop {
        let tag = *body.get(position).ok_or_else(malformed)?;
        position += 1;
        match tag {
            TAG_END => break,
            0x00..=0x0f => {
                group = tag;
                group_index += 1;
            }
            _ => {
                let name = take_bytes(body, &mut position).ok_or_else(malformed)?;
                let value = take_bytes(body, &mut position).ok_or_else(malformed)?;
                if !name.is_empty() {
                    attributes.push(Attribute {
                        group_index,
                        group,
                        name: String::from_utf8_lossy(&name).into_owned(),
                        tag,
                        value,
                    });
                }
            }
        }
    }
    Ok(Reply { status, attributes })
}

/// Each printer-attributes group in the listing is one queue.
fn printers_from(attributes: &[Attribute]) -> Vec<Printer> {
    let mut printers: Vec<(usize, Printer)> = Vec::new();
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.group == TAG_PRINTER_GROUP)
    {
        if printers.last().map(|(index, _)| *index) != Some(attribute.group_index) {
            printers.push((
                attribute.group_index,
                Printer {
                    name: String::new(),
                    state: PrinterState::Unknown,
                    accepting_jobs: false,
                },
            ));
        }
        let Some((_, printer)) = printers.last_mut() else {
            continue;
        };
        match attribute.name.as_str() {
            "printer-name" => {
                printer.name = String::from_utf8_lossy(&attribute.value).into_owned();
            }
            "printer-state" if attribute.tag == TAG_ENUM => {
                if let Ok(bytes) = <[u8; 4]>::try_from(attribute.value.as_slice()) {
                    printer.state = PrinterState::from_ipp(u32::from_be_bytes(bytes));
                }
            }
            "printer-is-accepting-jobs" if attribute.tag == TAG_BOOLEAN => {
                printer.accepting_jobs = attribute.value.first() == Some(&1);
            }
            _ => {}
        }
    }
    printers
        .into_iter()
        .map(|(_, printer)| printer)
        .filter(|printer| !printer.name.is_empty())
        .collect()
}

fn default_from(attributes: &[Attribute]) -> Option<String> {
    attributes
        .iter()
        .find(|attribute| attribute.group == TAG_PRINTER_GROUP && attribute.name == "printer-name")
        .map(|attribute| String::from_utf8_lossy(&attribute.value).into_owned())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{ErrorKind, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    /// One attribute in a test reply: (tag, name, value).
    type TestAttr<'a> = (u8, &'a str, Vec<u8>);
    /// One attribute group in a test reply: (delimiter, attributes).
    type TestGroup<'a> = (u8, Vec<TestAttr<'a>>);

    /// An IPP reply: `status`, then each group.
    fn reply(status: u16, groups: &[TestGroup]) -> Vec<u8> {
        let mut body = vec![0x02, 0x00];
        body.extend_from_slice(&status.to_be_bytes());
        body.extend_from_slice(&7u32.to_be_bytes());
        for (delimiter, attributes) in groups {
            body.push(*delimiter);
            for (tag, name, value) in attributes {
                push_attribute(&mut body, *tag, name, value);
            }
        }
        body.push(TAG_END);
        body
    }

    fn printer_group(name: &str, state: u32, accepting: bool) -> TestGroup<'static> {
        (
            TAG_PRINTER_GROUP,
            vec![
                (0x42, "printer-name", name.as_bytes().to_vec()),
                (TAG_ENUM, "printer-state", state.to_be_bytes().to_vec()),
                (
                    TAG_BOOLEAN,
                    "printer-is-accepting-jobs",
                    vec![u8::from(accepting)],
                ),
            ],
        )
    }

    #[test]
    fn printer_states_follow_the_ipp_values() {
        assert_eq!(PrinterState::from_ipp(3), PrinterState::Idle);
        assert_eq!(PrinterState::from_ipp(4), PrinterState::Printing);
        assert_eq!(PrinterState::from_ipp(5), PrinterState::Stopped);
        assert_eq!(PrinterState::from_ipp(9), PrinterState::Unknown);
    }

    #[test]
    fn the_queue_list_is_read_one_printer_per_group() {
        let body = reply(
            0x0000,
            &[
                (TAG_OPERATION_GROUP, vec![]),
                printer_group("Office", 3, true),
                printer_group("Lab", 5, false),
                printer_group("Plotter", 4, true),
            ],
        );
        let attributes = decode_reply(&body).unwrap().attributes;
        let printers = printers_from(&attributes);
        assert_eq!(
            printers,
            vec![
                Printer {
                    name: "Office".into(),
                    state: PrinterState::Idle,
                    accepting_jobs: true
                },
                Printer {
                    name: "Lab".into(),
                    state: PrinterState::Stopped,
                    accepting_jobs: false
                },
                Printer {
                    name: "Plotter".into(),
                    state: PrinterState::Printing,
                    accepting_jobs: true
                },
            ]
        );
        // No queues: an empty list, not an error.
        let none = decode_reply(&reply(0x0000, &[(TAG_OPERATION_GROUP, vec![])])).unwrap();
        assert!(printers_from(&none.attributes).is_empty());
    }

    #[test]
    fn the_default_queue_is_the_one_cups_names() {
        let body = reply(
            0x0000,
            &[(
                TAG_PRINTER_GROUP,
                vec![(0x42, "printer-name", b"Lab".to_vec())],
            )],
        );
        assert_eq!(
            default_from(&decode_reply(&body).unwrap().attributes),
            Some("Lab".to_owned())
        );
        let none = reply(0x0406, &[(TAG_OPERATION_GROUP, vec![])]);
        let decoded = decode_reply(&none).unwrap();
        assert_eq!(decoded.status, STATUS_NOT_FOUND);
        assert_eq!(default_from(&decoded.attributes), None);
    }

    #[test]
    fn refused_and_failed_ipp_answers_are_reported() {
        assert_eq!(check_status(0x0000), Ok(()));
        assert_eq!(check_status(STATUS_NOT_FOUND), Ok(()));
        for status in [0x0401, 0x0402, 0x0403] {
            assert_eq!(check_status(status), Err(PrintersError::PermissionDenied));
        }
        assert!(matches!(
            check_status(0x0500),
            Err(PrintersError::Failed(text)) if text.contains("0x0500")
        ));
    }

    #[test]
    fn http_answers_are_checked_and_chunked_bodies_joined() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc";
        assert_eq!(http_body(plain).unwrap(), b"abc".to_vec());
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nabcd\r\n2;ext=1\r\nef\r\n0\r\n\r\n";
        assert_eq!(http_body(chunked).unwrap(), b"abcdef".to_vec());
        assert_eq!(
            http_body(b"HTTP/1.1 401 Unauthorized\r\n\r\n"),
            Err(PrintersError::PermissionDenied)
        );
        assert!(matches!(
            http_body(b"HTTP/1.1 500 Oops\r\n\r\n"),
            Err(PrintersError::Failed(_))
        ));
        assert!(matches!(
            http_body(b"not http at all"),
            Err(PrintersError::Failed(_))
        ));
        assert!(matches!(
            http_body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n"),
            Err(PrintersError::Failed(_))
        ));
    }

    #[test]
    fn truncated_or_garbled_answers_are_errors_never_panics() {
        let body = reply(
            0x0000,
            &[
                (TAG_OPERATION_GROUP, vec![]),
                printer_group("Office", 3, true),
            ],
        );
        // Only the whole answer decodes; every cut is an error.
        for length in 0..body.len() {
            assert!(decode_reply(&body[..length]).is_err(), "cut at {length}");
        }
        assert!(decode_reply(&body).is_ok());
        let mut garbled = body.clone();
        for byte in garbled.iter_mut().skip(8) {
            *byte = 0xff;
        }
        let _ = decode_reply(&garbled);
        assert!(dechunk(b"ffffffff\r\nx").is_err());
    }

    #[test]
    fn socket_errors_mean_cups_is_missing_or_refused() {
        assert_eq!(
            error_from_io(&std::io::Error::from(ErrorKind::NotFound)),
            PrintersError::NotRunning
        );
        assert_eq!(
            error_from_io(&std::io::Error::from(ErrorKind::ConnectionRefused)),
            PrintersError::NotRunning
        );
        let refused = error_from_io(&std::io::Error::from(ErrorKind::PermissionDenied));
        assert_eq!(refused, PrintersError::PermissionDenied);
        assert_eq!(refused.availability(), Availability::PermissionDenied);
        assert_eq!(
            PrintersError::NotRunning.availability(),
            Availability::MissingService
        );
        assert_eq!(
            error_from_io(&std::io::Error::from(ErrorKind::TimedOut)).availability(),
            Availability::Failed
        );
    }

    #[test]
    fn a_missing_cups_socket_is_reported_as_not_running() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            snapshot_at(&dir.path().join("cups.sock")),
            Err(PrintersError::NotRunning)
        );
    }

    #[test]
    fn a_socket_that_refuses_this_user_is_reported_as_permission_denied() {
        // SAFETY: geteuid(2) has no preconditions.
        if unsafe { libc::geteuid() } == 0 {
            // Root may open any socket, so there is nothing to refuse.
            eprintln!("NOT_RUN: root ignores socket permissions");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cups.sock");
        let _listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(snapshot_at(&path), Err(PrintersError::PermissionDenied));
    }

    /// Answer the two requests `snapshot_at` makes, as CUPS would.
    #[test]
    fn a_cups_server_on_its_socket_gives_the_queues_and_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cups.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let listing = reply(
            0x0000,
            &[
                (TAG_OPERATION_GROUP, vec![]),
                printer_group("Office", 3, true),
                printer_group("Lab", 5, false),
            ],
        );
        let default = reply(
            0x0000,
            &[
                (TAG_OPERATION_GROUP, vec![]),
                (
                    TAG_PRINTER_GROUP,
                    vec![(0x42, "printer-name", b"Office".to_vec())],
                ),
            ],
        );
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for answer in [listing, default] {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                seen.push(u16::from_be_bytes([request[2], request[3]]));
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    answer.len()
                );
                stream.write_all(head.as_bytes()).unwrap();
                stream.write_all(&answer).unwrap();
            }
            seen
        });
        let snapshot = snapshot_at(&path).expect("CUPS answers");
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.printers.len(), 2);
        assert_eq!(snapshot.printers[1].state, PrinterState::Stopped);
        assert_eq!(snapshot.default.as_deref(), Some("Office"));
        assert_eq!(
            server.join().unwrap(),
            vec![OP_GET_PRINTERS, OP_GET_DEFAULT]
        );
    }

    /// Read one HTTP request and return its IPP body.
    fn read_request(stream: &mut UnixStream) -> Vec<u8> {
        let mut data = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            if let Some(split) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..split]).into_owned();
                let length: usize = head
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0);
                if data.len() >= split + 4 + length {
                    return data[split + 4..split + 4 + length].to_vec();
                }
            }
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0, "the client closed before sending its request");
            data.extend_from_slice(&buffer[..read]);
        }
    }

    #[test]
    fn requests_carry_the_operation_and_the_attributes_asked_for() {
        let body = encode_request(OP_GET_PRINTERS, PRINTER_ATTRIBUTES);
        assert_eq!(&body[..2], &[0x02, 0x00]);
        assert_eq!(u16::from_be_bytes([body[2], body[3]]), OP_GET_PRINTERS);
        assert_eq!(body[8], TAG_OPERATION_GROUP);
        assert_eq!(*body.last().unwrap(), TAG_END);
        let text = String::from_utf8_lossy(&body).into_owned();
        assert!(text.contains("requested-attributes"));
        assert!(text.contains("printer-is-accepting-jobs"));
    }
}
