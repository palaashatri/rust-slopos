//! Searching the native, Flatpak, Snap and AUR sources.
//!
//! Each source has a query builder (pure, validates the search text), a
//! parser (pure, takes the tool's stdout) and the one runner that actually
//! starts the tool. Nothing else reads command output. Results are listing
//! only: installing from these sources is not built yet.

use std::process::{Command, Stdio};
use std::sync::Arc;

/// Most results kept from one search; the rest are counted, not shown.
pub const MAX_RESULTS: usize = 500;
/// Longest search text accepted.
const MAX_QUERY: usize = 64;
/// Output past this many bytes is cut, bounding memory on a huge search.
const MAX_OUTPUT: usize = 8 * 1024 * 1024;

/// One package as a source lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// What the source calls it (package name, Flatpak application ID, Snap name).
    pub id: String,
    pub name: String,
    pub version: String,
    pub summary: String,
    /// Repository, remote or publisher, when the tool reports one.
    pub origin: String,
}

/// Which parser reads a tool's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `apt-cache search`
    Apt,
    /// `dnf search`
    Dnf,
    /// `pacman -Ss`, also used for AUR helpers' `-Ss`
    Pacman,
    /// `flatpak search`
    Flatpak,
    /// `snap find`
    Snap,
}

/// A search ready to run: the program name (looked up on `PATH`) and its
/// arguments. The query is one argument, never passed through a shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub format: Format,
    pub program: String,
    pub args: Vec<String>,
}

/// Runs a search and returns the tool's standard output.
pub type Runner = Arc<dyn Fn(&Search) -> Result<String, String> + Send + Sync>;

/// Accept only plain search text: letters, digits and `+ - . _` and spaces,
/// not starting with a dash (so it cannot be read as an option).
pub fn validate_query(query: &str) -> Result<&str, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Type a search term first.".into());
    }
    if query.chars().count() > MAX_QUERY {
        return Err(format!("Search for at most {MAX_QUERY} characters."));
    }
    if query.starts_with('-')
        || !query
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '+' | '-' | '.' | '_' | ' '))
    {
        return Err("Search terms may use letters, digits, spaces and + - . _ only.".into());
    }
    Ok(query)
}

/// The search for `format`, run through `program` (the tool, or an AUR helper).
pub fn search(format: Format, program: &str, query: &str) -> Result<Search, String> {
    let query = validate_query(query)?.to_owned();
    let args: Vec<String> = match format {
        Format::Apt => vec!["search".into(), query],
        Format::Dnf => vec!["-q".into(), "search".into(), query],
        Format::Pacman => vec!["-Ss".into(), query],
        Format::Flatpak => vec![
            "search".into(),
            "--columns=application,name,description,version".into(),
            query,
        ],
        Format::Snap => vec!["find".into(), query],
    };
    Ok(Search {
        format,
        program: program.to_owned(),
        args,
    })
}

/// Parse a search's output with the parser for its format.
pub fn parse(format: Format, text: &str) -> Vec<Package> {
    match format {
        Format::Apt => parse_apt(text),
        Format::Dnf => parse_dnf(text),
        Format::Pacman => parse_pacman(text),
        Format::Flatpak => parse_flatpak(text),
        Format::Snap => parse_snap(text),
    }
}

/// Start the tool with `LC_ALL=C` and return its standard output.
///
/// A non-zero exit with no output means "no matches" (pacman does this), so
/// it returns empty text. A non-zero exit with output on stderr is an error.
pub fn run_system(search: &Search) -> Result<String, String> {
    let program = slopos_core::process::resolve_program(&search.program)
        .ok_or_else(|| format!("{} not found", search.program))?;
    let output = Command::new(program)
        .args(&search.args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("{}: {error}", search.program))?;
    let mut stdout = output.stdout;
    stdout.truncate(MAX_OUTPUT);
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    if !output.status.success() && stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return match stderr.lines().map(str::trim).find(|line| !line.is_empty()) {
            Some(line) => Err(line.to_owned()),
            None => Ok(String::new()),
        };
    }
    Ok(stdout)
}

/// `name - summary`, one package per line (`apt-cache search`).
pub fn parse_apt(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(|line| {
            let (name, summary) = line.split_once(" - ")?;
            let name = name.trim();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return None;
            }
            Some(package(name, "", summary.trim(), "apt"))
        })
        .collect()
}

/// `name.arch : summary` under optional `===` headings (`dnf search`).
/// This format is written from the tool's documented output, not captured
/// from a Fedora system; the parser skips any line it does not recognise.
pub fn parse_dnf(text: &str) -> Vec<Package> {
    const ARCHES: [&str; 6] = [".x86_64", ".aarch64", ".noarch", ".i686", ".i386", ".src"];
    text.lines()
        .filter_map(|line| {
            if line.starts_with("===") || line.starts_with("Last metadata") {
                return None;
            }
            let (left, summary) = line.split_once(" : ")?;
            let left = left.trim();
            let name = ARCHES
                .iter()
                .find_map(|arch| left.strip_suffix(arch))
                .unwrap_or(left);
            if name.is_empty() || name.contains(char::is_whitespace) {
                return None;
            }
            Some(package(name, "", summary.trim(), "dnf"))
        })
        .collect()
}

/// `repo/name version [installed]` with the description on the next,
/// indented line (`pacman -Ss`). AUR helpers print the same layout with
/// `aur/` as the repository.
pub fn parse_pacman(text: &str) -> Vec<Package> {
    let mut packages: Vec<Package> = Vec::new();
    for line in text.lines() {
        if line.starts_with(char::is_whitespace) {
            if let Some(last) = packages.last_mut() {
                if last.summary.is_empty() {
                    last.summary = line.trim().to_owned();
                }
            }
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some((repo, name)) = fields.next().and_then(|full| full.split_once('/')) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let version = fields.next().unwrap_or("");
        packages.push(package(name, version, "", repo));
    }
    packages
}

/// Tab-separated `application`, `name`, `description`, `version` columns, as
/// requested by `--columns`. An `Application ID` heading line is skipped.
/// This layout is a guess at what `flatpak search --columns` prints; it has
/// not been checked against a live Flatpak install.
pub fn parse_flatpak(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(|line| {
            if !line.contains('\t') {
                return None;
            }
            let mut fields = line.split('\t').map(str::trim);
            let id = fields.next()?;
            if id.is_empty() || id.eq_ignore_ascii_case("application id") {
                return None;
            }
            let name = fields.next().unwrap_or("");
            let summary = fields.next().unwrap_or("");
            let version = fields.next().unwrap_or("");
            let name = if name.is_empty() { id } else { name };
            Some(Package {
                id: id.to_owned(),
                name: name.to_owned(),
                version: version.to_owned(),
                summary: summary.to_owned(),
                origin: "flatpak".into(),
            })
        })
        .collect()
}

/// `snap find` prints aligned columns under a heading line. Each column is
/// read at the position of its heading, counted in characters, so multi-byte
/// publisher marks such as a check mark do not shift the others. Without that
/// heading (for example a "no matches" message) nothing is listed.
pub fn parse_snap(text: &str) -> Vec<Package> {
    let Some(header) = text
        .lines()
        .find(|line| line.trim_start().starts_with("Name") && line.contains("Summary"))
    else {
        return Vec::new();
    };
    let (Some(version_at), Some(summary_at)) = (header.find("Version"), header.find("Summary"))
    else {
        return Vec::new();
    };
    // The heading is ASCII, so its byte offsets are character offsets.
    let publisher_at = header.find("Publisher");
    let notes_at = header.find("Notes");
    let publisher_end = notes_at.unwrap_or(summary_at);
    let body = text.lines().skip_while(|line| *line != header).skip(1);
    let mut packages = Vec::new();
    for line in body {
        let chars: Vec<char> = line.chars().collect();
        let slice = |from: usize, to: usize| -> String {
            let from = from.min(chars.len());
            let to = to.min(chars.len()).max(from);
            chars[from..to].iter().collect::<String>().trim().to_owned()
        };
        let name = slice(0, version_at);
        if name.is_empty() {
            continue;
        }
        let version = slice(version_at, publisher_at.unwrap_or(summary_at));
        let origin = publisher_at.map_or_else(String::new, |from| slice(from, publisher_end));
        let summary = slice(summary_at, usize::MAX);
        packages.push(Package {
            id: name.clone(),
            name,
            version,
            summary,
            origin,
        });
    }
    packages
}

fn package(name: &str, version: &str, summary: &str, origin: &str) -> Package {
    Package {
        id: name.to_owned(),
        name: name.to_owned(),
        version: version.to_owned(),
        summary: summary.to_owned(),
        origin: origin.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from `LC_ALL=C apt-cache search zstd` on Ubuntu 24.04 (first lines).
    const APT: &str = "\
libzstd-dev - fast lossless compression algorithm -- development files
libzstd1 - fast lossless compression algorithm
zstd - fast lossless compression algorithm -- CLI tool
librust-async-compression-dev - Adaptor between compression crates and Rust's async types - Rust source code
just a line with no separator
 - no name
";

    /// Representative `pacman -Ss` output (repo/name version, indented summary).
    const PACMAN: &str = "\
core/firefox 131.0-1 [installed]
    Standalone web browser from mozilla.org
extra/firefox-developer-edition 132.0b1-1
    Developer Edition of Firefox
community/firefox-esr 115.14.0-1
extra/firefox-i18n-de 131.0-1
    German language pack for Firefox
";

    /// Representative AUR helper output: same layout, `aur/` repository.
    const AUR: &str = "\
aur/zen-browser-bin 1.0.1b-1 (+12 0.84)
    Experience calm and beauty while browsing the web
aur/librewolf-bin 130.0.1-1 (+40 1.20) [installed]
    Community-maintained fork of Firefox
";

    /// Representative `dnf search` output, with headings and a wrapped header line.
    const DNF: &str = "\
Last metadata expiration check: 0:12:33 ago on Mon 02 Nov 2026 10:00:00 AM UTC.
=== Name Exactly Matched: firefox ===
firefox.x86_64 : Mozilla Firefox Web browser
=== Name & Summary Matched: web ===
gnome-web.x86_64 : Web browser for GNOME : a second colon
python3.12.noarch : Interpreter with a dot in the name
";

    /// Representative `flatpak search --columns` output (tab-separated).
    const FLATPAK: &str = "\
Application ID\tName\tDescription\tVersion
org.mozilla.firefox\tFirefox\tWeb browser by Mozilla\t131.0
org.gnome.Calculator\tCalculator\tPerform arithmetic\t
\tNo name\tdropped because the id is empty\t
";

    /// Representative `snap find` output: aligned columns with a check mark.
    const SNAP: &str = "\
Name       Version     Publisher      Notes  Summary
firefox    131.0-1     mozilla✓       -      Mozilla's Firefox web browser
chromium   131.0.6778  canonical✓     -      Chromium web browser
vlc        3.0.21      videolan✓      -      Media player
";

    #[test]
    fn apt_parser_reads_name_and_summary() {
        let packages = parse_apt(APT);
        assert_eq!(packages.len(), 4);
        assert_eq!(packages[0].name, "libzstd-dev");
        assert_eq!(
            packages[0].summary,
            "fast lossless compression algorithm -- development files"
        );
        assert_eq!(packages[2].id, "zstd");
        // Only the first " - " separates name from summary.
        assert_eq!(
            packages[3].summary,
            "Adaptor between compression crates and Rust's async types - Rust source code"
        );
        assert!(packages
            .iter()
            .all(|p| p.version.is_empty() && p.origin == "apt"));
    }

    #[test]
    fn pacman_parser_reads_repo_name_version_and_description() {
        let packages = parse_pacman(PACMAN);
        assert_eq!(packages.len(), 4);
        assert_eq!(packages[0].origin, "core");
        assert_eq!(packages[0].name, "firefox");
        assert_eq!(packages[0].version, "131.0-1");
        assert_eq!(
            packages[0].summary,
            "Standalone web browser from mozilla.org"
        );
        // No description line: the summary stays empty rather than taking the next package's.
        assert_eq!(packages[2].name, "firefox-esr");
        assert!(packages[2].summary.is_empty());
        assert_eq!(packages[3].summary, "German language pack for Firefox");
    }

    #[test]
    fn aur_uses_the_pacman_parser() {
        let packages = parse(Format::Pacman, AUR);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].origin, "aur");
        assert_eq!(packages[0].name, "zen-browser-bin");
        assert_eq!(packages[0].version, "1.0.1b-1");
        assert_eq!(packages[1].summary, "Community-maintained fork of Firefox");
    }

    #[test]
    fn dnf_parser_strips_arch_and_skips_headings() {
        let packages = parse_dnf(DNF);
        let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["firefox", "gnome-web", "python3.12"]);
        assert_eq!(
            packages[1].summary,
            "Web browser for GNOME : a second colon"
        );
        assert_eq!(packages[0].origin, "dnf");
    }

    #[test]
    fn flatpak_parser_reads_ids_and_skips_the_heading() {
        let packages = parse_flatpak(FLATPAK);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "org.mozilla.firefox");
        assert_eq!(packages[0].name, "Firefox");
        assert_eq!(packages[0].version, "131.0");
        assert_eq!(packages[1].version, "");
        assert_eq!(packages[1].summary, "Perform arithmetic");
    }

    #[test]
    fn flatpak_name_falls_back_to_the_id() {
        let packages = parse_flatpak("org.example.App\t\tA summary\t1.2\n");
        assert_eq!(packages[0].name, "org.example.App");
    }

    #[test]
    fn snap_parser_reads_columns_by_heading_position() {
        let packages = parse_snap(SNAP);
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "firefox");
        assert_eq!(packages[0].version, "131.0-1");
        assert_eq!(packages[0].origin, "mozilla✓");
        assert_eq!(packages[0].summary, "Mozilla's Firefox web browser");
        assert_eq!(packages[1].version, "131.0.6778");
        assert_eq!(packages[2].summary, "Media player");
    }

    #[test]
    fn snap_without_a_heading_lists_nothing() {
        assert!(parse_snap("No matching snaps for \"zzz\".\n").is_empty());
        assert!(parse_snap("").is_empty());
    }

    #[test]
    fn empty_and_unrecognised_output_lists_nothing() {
        for format in [
            Format::Apt,
            Format::Dnf,
            Format::Pacman,
            Format::Flatpak,
            Format::Snap,
        ] {
            assert!(parse(format, "").is_empty(), "{format:?}");
            assert!(
                parse(format, "Some unrelated message\n").is_empty(),
                "{format:?}"
            );
        }
    }

    #[test]
    fn queries_are_validated_before_any_command_is_built() {
        assert_eq!(validate_query("  firefox  ").unwrap(), "firefox");
        assert_eq!(validate_query("g++ c.h_x").unwrap(), "g++ c.h_x");
        assert!(validate_query("   ").is_err());
        assert!(validate_query("--help").is_err());
        assert!(validate_query("a;rm -rf /").is_err());
        assert!(validate_query("$(id)").is_err());
        assert!(validate_query(&"x".repeat(65)).is_err());
    }

    #[test]
    fn each_source_gets_its_own_arguments() {
        let apt = search(Format::Apt, "apt-cache", "zstd").unwrap();
        assert_eq!(apt.program, "apt-cache");
        assert_eq!(apt.args, ["search", "zstd"]);
        assert_eq!(
            search(Format::Pacman, "pacman", "fire").unwrap().args,
            ["-Ss", "fire"]
        );
        assert_eq!(
            search(Format::Snap, "snap", "vlc").unwrap().args,
            ["find", "vlc"]
        );
        let flatpak = search(Format::Flatpak, "flatpak", "gimp").unwrap();
        assert_eq!(flatpak.args[0], "search");
        assert_eq!(flatpak.args[2], "gimp");
        assert!(search(Format::Apt, "apt-cache", "-x").is_err());
    }
}
