//! Which software sources the Software window offers on this machine.
//!
//! The tabs are the SLOPOS catalogue (always), the distribution's native
//! package manager (when the distribution is known), the Sources tab (always)
//! and the optional Flatpak, Snap and AUR sources the user has enabled. A tab
//! whose tool is missing stays visible and says so; nothing is invented to
//! fill it.
//!
//! Everything here that decides something is a pure function over data the
//! caller passes in: the os-release text, a PATH lookup and the text of the
//! Flatpak repository configs. Only [`Environment::probe`] reads the real
//! files and runs the real lookups.

use std::path::PathBuf;
use std::sync::Arc;

use slopos_core::software_config::{self, SoftwareConfig};

/// A tool lookup: does this program exist on the system?
pub type ToolCheck = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// The remote Flatpak adds for the Flathub app store.
pub const FLATHUB: &str = "flathub";

/// The places a tab's content can come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The distribution's own package manager (apt, dnf or pacman).
    Native,
    /// The signed SLOPOS catalogue.
    Slopos,
    Flatpak,
    Snap,
    /// Arch User Repository, through an AUR helper.
    Aur,
    /// The tab that lists the package sources and switches them on and off.
    Sources,
}

impl Source {
    /// Whether the tab searches an external source (and so has a search box).
    pub fn is_backend(self) -> bool {
        !matches!(self, Source::Slopos | Source::Sources)
    }
}

/// The native package manager family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Apt,
    Dnf,
    Pacman,
}

impl Family {
    /// The program that searches this family's repositories. The manager is
    /// present when this program is.
    pub fn tool(self) -> &'static str {
        match self {
            Family::Apt => "apt-cache",
            Family::Dnf => "dnf",
            Family::Pacman => "pacman",
        }
    }

    /// The program that installs packages with this family's manager.
    pub fn install_tool(self) -> &'static str {
        match self {
            Family::Apt => "apt-get",
            Family::Dnf => "dnf",
            Family::Pacman => "pacman",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Family::Apt => "apt",
            Family::Dnf => "dnf",
            Family::Pacman => "pacman",
        }
    }
}

/// What the machine is, from its os-release text: its native family and
/// whether the distribution is Arch Linux or based on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    /// The family the OS release names. Whether its tool is installed is a
    /// separate question, answered by [`Status`].
    pub family: Option<Family>,
    pub arch_linux: bool,
}

impl Machine {
    /// Build from the text of `/etc/os-release`.
    pub fn detect(os_release: &str) -> Self {
        let ids = os_release_ids(os_release);
        Self {
            family: family_from_ids(&ids),
            arch_linux: ids.iter().any(|id| id == "arch"),
        }
    }
}

/// `ID` and `ID_LIKE` from os-release, lower-cased, in file order.
fn os_release_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        if let Some(value) = line
            .strip_prefix("ID=")
            .or_else(|| line.strip_prefix("ID_LIKE="))
        {
            let value = unquote(value.trim());
            ids.extend(value.split_whitespace().map(str::to_ascii_lowercase));
        }
    }
    ids
}

/// The value without one matching pair of surrounding quotes, single or
/// double. An unmatched quote is part of the value.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

fn family_from_ids(ids: &[String]) -> Option<Family> {
    // Membership decides, not order: ID and ID_LIKE are checked alike.
    let has = |name: &str| ids.iter().any(|id| id == name);
    if has("debian") || has("ubuntu") {
        Some(Family::Apt)
    } else if has("fedora") || has("rhel") {
        Some(Family::Dnf)
    } else if has("arch") {
        Some(Family::Pacman)
    } else {
        None
    }
}

/// Whether a tab can be used, with the reason when it cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// The tool to run is present; `program` is its name. The catalogue and
    /// Sources tabs have no tool and carry an empty name.
    Ready {
        program: String,
    },
    Unavailable(String),
}

/// One tab in the Software window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub source: Source,
    pub label: String,
    pub availability: Availability,
}

impl Tab {
    /// The tab's caption: the label, marked when the source is unavailable.
    pub fn caption(&self) -> String {
        match self.availability {
            Availability::Ready { .. } => self.label.clone(),
            Availability::Unavailable(_) => format!("{} (unavailable)", self.label),
        }
    }
}

/// The tabs for this machine and configuration, in display order.
pub fn tabs(
    machine: &Machine,
    config: &SoftwareConfig,
    tool_present: &dyn Fn(&str) -> bool,
) -> Vec<Tab> {
    let mut tabs = Vec::new();
    if let Some(family) = machine.family {
        tabs.push(Tab {
            source: Source::Native,
            label: format!("Native ({})", family.label()),
            availability: require(&[family.tool()], tool_present),
        });
    }
    tabs.push(Tab {
        source: Source::Slopos,
        label: "SLOPOS".into(),
        // The catalogue's own state is shown on the window, not here.
        availability: Availability::Ready {
            program: String::new(),
        },
    });
    if config.flatpak {
        tabs.push(Tab {
            source: Source::Flatpak,
            label: "Flatpak".into(),
            availability: require(&["flatpak"], tool_present),
        });
    }
    if config.snap {
        tabs.push(Tab {
            source: Source::Snap,
            label: "Snap".into(),
            availability: require(&["snap"], tool_present),
        });
    }
    if machine.arch_linux && config.aur {
        tabs.push(Tab {
            source: Source::Aur,
            label: "AUR".into(),
            availability: require(&["yay", "paru"], tool_present),
        });
    }
    tabs.push(Tab {
        source: Source::Sources,
        label: "Sources".into(),
        availability: Availability::Ready {
            program: String::new(),
        },
    });
    tabs
}

/// The first candidate tool that is present, or the reason none is.
fn require(candidates: &[&str], tool_present: &dyn Fn(&str) -> bool) -> Availability {
    match candidates.iter().find(|name| tool_present(name)) {
        Some(name) => Availability::Ready {
            program: (*name).to_owned(),
        },
        None => Availability::Unavailable(match candidates {
            [one] => format!("unavailable: {one} not found"),
            [first, second] => format!("unavailable: {first} or {second} not found"),
            _ => "unavailable: no tool found".into(),
        }),
    }
}

/// What is installed and configured on this machine right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Status {
    /// The native manager, when the OS names one and its tool is installed.
    pub manager: Option<Family>,
    pub flatpak_installed: bool,
    /// Whether a Flatpak repository config defines the Flathub remote.
    pub flathub: bool,
    pub snap_installed: bool,
    /// The first AUR helper found, `yay` before `paru`.
    pub aur_helper: Option<String>,
}

/// What the Flatpak files on disk say about the Flathub remote, as text.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlatpakFiles<'a> {
    /// The text of each repository `config` file that could be read.
    pub repo_configs: &'a [&'a str],
    /// The text of each `xa.applied-remotes` file that could be read: one
    /// remote name per line, for the remotes Flatpak has already applied.
    pub applied_remotes: &'a [&'a str],
    /// Whether a remotes.d directory ships `flathub.flatpakrepo`.
    pub remotes_d_has_flathub: bool,
}

/// Work out the [`Status`] from its inputs.
pub fn status(
    machine: &Machine,
    tool_present: &dyn Fn(&str) -> bool,
    flatpak: &FlatpakFiles,
) -> Status {
    Status {
        manager: machine.family.filter(|family| tool_present(family.tool())),
        flatpak_installed: tool_present("flatpak"),
        flathub: flathub_configured(flatpak),
        snap_installed: tool_present("snap"),
        aur_helper: ["yay", "paru"]
            .into_iter()
            .find(|name| tool_present(name))
            .map(str::to_owned),
    }
}

/// Whether Flathub is configured. A repository config that defines it says
/// so. Otherwise a remotes.d definition counts, unless Flatpak has already
/// applied it: an applied remote that is no longer defined was removed, and
/// stays removed.
fn flathub_configured(files: &FlatpakFiles) -> bool {
    if files
        .repo_configs
        .iter()
        .any(|text| remote_defined(text, FLATHUB))
    {
        return true;
    }
    files.remotes_d_has_flathub
        && !files
            .applied_remotes
            .iter()
            .any(|text| lists_remote(text, FLATHUB))
}

/// Whether a Flatpak repository config (an OSTree `config` file) has a
/// `[remote "name"]` section.
pub fn remote_defined(config_text: &str, name: &str) -> bool {
    let header = format!("[remote \"{name}\"]");
    config_text.lines().any(|line| line.trim() == header)
}

/// Whether an `xa.applied-remotes` text names `name` on one of its lines.
fn lists_remote(applied_text: &str, name: &str) -> bool {
    applied_text.lines().any(|line| line.trim() == name)
}

/// One block of the Sources tab: a source, what it reports, and its switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub source: Source,
    pub title: String,
    pub lines: Vec<String>,
    /// `Some(on)` when the source can be switched on and off from this tab,
    /// with whether it is on now.
    pub switch: Option<bool>,
}

/// The Sources tab's contents for this machine, configuration and status.
/// Every line states what was found; a missing provider reads as
/// "unavailable".
pub fn sections(machine: &Machine, config: &SoftwareConfig, status: &Status) -> Vec<Section> {
    let mut sections = vec![Section {
        source: Source::Native,
        title: "System packages".into(),
        lines: vec![match status.manager {
            Some(family) => format!("Package manager: {}", family.label()),
            None => "Package manager: unavailable (no apt, dnf or pacman found)".into(),
        }],
        switch: None,
    }];
    sections.push(Section {
        source: Source::Flatpak,
        title: "Flatpak".into(),
        lines: vec![
            format!("Flatpak installed: {}", yes_no(status.flatpak_installed)),
            format!(
                "Flathub remote: {}",
                if status.flathub {
                    "configured"
                } else {
                    "not configured"
                }
            ),
        ],
        switch: Some(config.flatpak),
    });
    if machine.arch_linux {
        sections.push(Section {
            source: Source::Aur,
            title: "AUR (Arch User Repository)".into(),
            lines: vec![match &status.aur_helper {
                Some(helper) => format!("AUR helper: {helper}"),
                None => "AUR helper: unavailable (yay or paru not found)".into(),
            }],
            switch: Some(config.aur),
        });
    }
    sections.push(Section {
        source: Source::Snap,
        title: "Snap".into(),
        lines: vec![format!("Snap installed: {}", yes_no(status.snap_installed))],
        switch: Some(config.snap),
    });
    sections
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

/// Everything the window needs to know about the machine it runs on.
#[derive(Clone)]
pub struct Environment {
    pub machine: Machine,
    pub config_path: PathBuf,
    pub tool_present: ToolCheck,
    /// Flatpak repository directories. Each may hold a `config` and an
    /// `xa.applied-remotes` file.
    pub flatpak_repos: Vec<PathBuf>,
    /// Directories that ship `*.flatpakrepo` remote definitions.
    pub flatpak_remotes_d: Vec<PathBuf>,
}

impl Environment {
    /// Read the real system: os-release, the config file location, the
    /// Flatpak directories and `PATH` for tool lookups.
    pub fn from_system() -> Self {
        let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let machine = Machine::detect(&os_release);
        let config_path = software_config::default_path()
            .unwrap_or_else(|_| PathBuf::from("slopos-i/software.conf"));
        let mut flatpak_repos = vec![PathBuf::from("/var/lib/flatpak/repo")];
        // Flatpak reads FLATPAK_USER_DIR before the XDG data directory, so the
        // probe checks the same installation Flatpak uses.
        match std::env::var_os("FLATPAK_USER_DIR").filter(|dir| !dir.is_empty()) {
            Some(dir) => flatpak_repos.push(PathBuf::from(dir).join("repo")),
            None => {
                if let Ok(paths) = slopos_core::xdg::XdgPaths::from_environment() {
                    flatpak_repos.push(paths.data.join("flatpak/repo"));
                }
            }
        }
        Self {
            machine,
            config_path,
            tool_present: Arc::new(|name: &str| {
                slopos_core::process::resolve_program(name).is_some()
            }),
            flatpak_repos,
            flatpak_remotes_d: vec![
                PathBuf::from("/etc/flatpak/remotes.d"),
                PathBuf::from("/usr/share/flatpak/remotes.d"),
            ],
        }
    }

    /// The live status: the files are read now, and each tool is looked up
    /// now. The result is a snapshot; the window takes a new one whenever it
    /// rebuilds its tabs, and before it acts on a Flatpak confirmation.
    pub fn probe(&self) -> Status {
        let read_all = |dirs: &[PathBuf], file: &str| -> Vec<String> {
            dirs.iter()
                .filter_map(|dir| std::fs::read_to_string(dir.join(file)).ok())
                .collect()
        };
        let configs = read_all(&self.flatpak_repos, "config");
        let applied = read_all(&self.flatpak_repos, "xa.applied-remotes");
        let config_refs: Vec<&str> = configs.iter().map(String::as_str).collect();
        let applied_refs: Vec<&str> = applied.iter().map(String::as_str).collect();
        let files = FlatpakFiles {
            repo_configs: &config_refs,
            applied_remotes: &applied_refs,
            remotes_d_has_flathub: self
                .flatpak_remotes_d
                .iter()
                .any(|dir| dir.join("flathub.flatpakrepo").is_file()),
        };
        status(&self.machine, &*self.tool_present, &files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UBUNTU: &str = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"24.04\"\n";
    const FEDORA: &str = "NAME=Fedora Linux\nID=fedora\nVERSION_ID=41\n";
    const ARCH: &str = "NAME=\"Arch Linux\"\nID=arch\nBUILD_ID=rolling\n";
    const MANJARO: &str = "ID=manjaro\nID_LIKE=arch\n";
    const ALPINE: &str = "ID=alpine\n";
    const FLATHUB_CONFIG: &str =
        "[core]\nrepo_version=1\n\n[remote \"flathub\"]\nurl=https://dl.flathub.org/repo/\n";
    const OTHER_REMOTE: &str = "[remote \"fedora\"]\nurl=oci+https://registry.fedoraproject.org\n";
    const NONE: FlatpakFiles<'static> = FlatpakFiles {
        repo_configs: &[],
        applied_remotes: &[],
        remotes_d_has_flathub: false,
    };

    /// Files with only repository configs.
    fn repo(configs: &'static [&'static str]) -> FlatpakFiles<'static> {
        FlatpakFiles {
            repo_configs: configs,
            ..NONE
        }
    }

    fn present(names: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |name: &str| names.contains(&name)
    }

    #[test]
    fn families_come_from_id_and_id_like() {
        assert_eq!(Machine::detect(UBUNTU).family, Some(Family::Apt));
        assert_eq!(Machine::detect(FEDORA).family, Some(Family::Dnf));
        assert_eq!(Machine::detect(ARCH).family, Some(Family::Pacman));
        assert_eq!(Machine::detect(MANJARO).family, Some(Family::Pacman));
        assert_eq!(Machine::detect(ALPINE).family, None);
        assert_eq!(Machine::detect("").family, None);
    }

    #[test]
    fn arch_is_read_from_os_release_alone() {
        assert!(Machine::detect(ARCH).arch_linux);
        assert!(Machine::detect(MANJARO).arch_linux, "ID_LIKE=arch counts");
        assert!(!Machine::detect(UBUNTU).arch_linux);
        assert!(!Machine::detect(FEDORA).arch_linux);
        // A name that merely starts with "arch" is not a match.
        assert!(!Machine::detect("ID=archaic\n").arch_linux);
    }

    #[test]
    fn the_manager_needs_its_tool_on_the_path() {
        let apt = Machine::detect(UBUNTU);
        assert_eq!(
            status(&apt, &present(&["apt-cache"]), &NONE).manager,
            Some(Family::Apt)
        );
        assert_eq!(status(&apt, &present(&[]), &NONE).manager, None);
        // A binary for another family does not stand in for the OS's own.
        assert_eq!(status(&apt, &present(&["dnf"]), &NONE).manager, None);
        let none = Machine::detect(ALPINE);
        assert_eq!(status(&none, &present(&["apt-cache"]), &NONE).manager, None);
    }

    #[test]
    fn flathub_is_found_in_a_repo_config_section() {
        assert!(remote_defined(FLATHUB_CONFIG, "flathub"));
        assert!(!remote_defined(OTHER_REMOTE, "flathub"));
        assert!(!remote_defined("[core]\nurl=flathub\n", "flathub"));
        let machine = Machine::detect(UBUNTU);
        let configured = status(
            &machine,
            &present(&["flatpak"]),
            &repo(&[OTHER_REMOTE, FLATHUB_CONFIG]),
        );
        assert!(configured.flathub && configured.flatpak_installed);
        let missing = status(&machine, &present(&["flatpak"]), &repo(&[OTHER_REMOTE]));
        assert!(!missing.flathub && missing.flatpak_installed);
        let no_flatpak = status(&machine, &present(&[]), &NONE);
        assert!(!no_flatpak.flatpak_installed && !no_flatpak.flathub);
    }

    #[test]
    fn aur_helper_prefers_yay_then_paru() {
        let arch = Machine::detect(ARCH);
        assert_eq!(
            status(&arch, &present(&["yay", "paru"]), &NONE)
                .aur_helper
                .as_deref(),
            Some("yay")
        );
        assert_eq!(
            status(&arch, &present(&["paru"]), &NONE)
                .aur_helper
                .as_deref(),
            Some("paru")
        );
        assert_eq!(status(&arch, &present(&[]), &NONE).aur_helper, None);
    }

    #[test]
    fn catalogue_and_sources_are_always_there_and_native_comes_first() {
        let machine = Machine::detect(UBUNTU);
        let tabs = tabs(
            &machine,
            &SoftwareConfig::default(),
            &present(&["apt-cache"]),
        );
        let sources: Vec<Source> = tabs.iter().map(|tab| tab.source).collect();
        assert_eq!(sources, [Source::Native, Source::Slopos, Source::Sources]);
        assert_eq!(tabs[0].label, "Native (apt)");
        assert_eq!(
            tabs[0].availability,
            Availability::Ready {
                program: "apt-cache".into()
            }
        );
        let none = self::tabs(
            &Machine::detect(ALPINE),
            &SoftwareConfig::default(),
            &present(&[]),
        );
        let sources: Vec<Source> = none.iter().map(|tab| tab.source).collect();
        assert_eq!(sources, [Source::Slopos, Source::Sources]);
    }

    #[test]
    fn missing_tools_are_named_not_hidden() {
        let machine = Machine::detect(FEDORA);
        let config = SoftwareConfig {
            flatpak: true,
            snap: true,
            aur: true,
        };
        let tabs = tabs(&machine, &config, &present(&[]));
        assert_eq!(
            tabs[0].availability,
            Availability::Unavailable("unavailable: dnf not found".into())
        );
        assert_eq!(tabs[0].caption(), "Native (dnf) (unavailable)");
        // AUR is only offered on Arch, so it is absent here.
        let sources: Vec<Source> = tabs.iter().map(|tab| tab.source).collect();
        assert_eq!(
            sources,
            [
                Source::Native,
                Source::Slopos,
                Source::Flatpak,
                Source::Snap,
                Source::Sources
            ]
        );
        assert_eq!(
            tabs[2].availability,
            Availability::Unavailable("unavailable: flatpak not found".into())
        );
        assert_eq!(
            tabs[3].availability,
            Availability::Unavailable("unavailable: snap not found".into())
        );
    }

    #[test]
    fn optional_sources_appear_only_when_enabled() {
        let arch = Machine::detect(ARCH);
        let off = tabs(&arch, &SoftwareConfig::default(), &present(&["pacman"]));
        assert_eq!(
            off.iter().map(|tab| tab.source).collect::<Vec<_>>(),
            [Source::Native, Source::Slopos, Source::Sources]
        );
        let on = SoftwareConfig {
            flatpak: true,
            snap: true,
            aur: true,
        };
        let all = tabs(&arch, &on, &present(&["pacman", "flatpak", "snap", "paru"]));
        assert_eq!(
            all.iter().map(|tab| tab.source).collect::<Vec<_>>(),
            [
                Source::Native,
                Source::Slopos,
                Source::Flatpak,
                Source::Snap,
                Source::Aur,
                Source::Sources
            ]
        );
        assert_eq!(
            all[4].availability,
            Availability::Ready {
                program: "paru".into()
            },
            "an AUR helper is used when yay is absent"
        );
        let no_helper = tabs(&arch, &on, &present(&["pacman"]));
        assert_eq!(
            no_helper[4].availability,
            Availability::Unavailable("unavailable: yay or paru not found".into())
        );
        // AUR needs Arch: the flag alone does not add the tab.
        let debian = tabs(&Machine::detect(UBUNTU), &on, &present(&[]));
        assert!(debian.iter().all(|tab| tab.source != Source::Aur));
        // A derivative based on Arch counts as Arch for AUR.
        let manjaro = tabs(&Machine::detect(MANJARO), &on, &present(&["yay"]));
        assert!(manjaro.iter().any(|tab| tab.source == Source::Aur));
    }

    #[test]
    fn the_sources_tab_reports_what_the_machine_has() {
        let debian = Machine::detect(UBUNTU);
        let found = status(
            &debian,
            &present(&["apt-cache", "flatpak"]),
            &repo(&[FLATHUB_CONFIG]),
        );
        let config = SoftwareConfig {
            flatpak: true,
            ..SoftwareConfig::default()
        };
        let rows = sections(&debian, &config, &found);
        let titles: Vec<&str> = rows.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["System packages", "Flatpak", "Snap"],
            "no AUR section off Arch"
        );
        assert_eq!(rows[0].lines, ["Package manager: apt"]);
        assert_eq!(rows[0].switch, None);
        assert_eq!(
            rows[1].lines,
            ["Flatpak installed: yes", "Flathub remote: configured"]
        );
        assert_eq!(rows[1].switch, Some(true));
        assert_eq!(rows[2].lines, ["Snap installed: no"]);
        assert_eq!(rows[2].switch, Some(false));
    }

    #[test]
    fn missing_providers_read_as_unavailable_in_the_sources_tab() {
        let fedora = Machine::detect(FEDORA);
        let none = status(&fedora, &present(&[]), &NONE);
        let rows = sections(&fedora, &SoftwareConfig::default(), &none);
        assert_eq!(
            rows[0].lines,
            ["Package manager: unavailable (no apt, dnf or pacman found)"]
        );
        assert_eq!(
            rows[1].lines,
            ["Flatpak installed: no", "Flathub remote: not configured"]
        );
        // Arch shows the AUR section and says when no helper is there.
        let arch = Machine::detect(ARCH);
        let arch_status = status(&arch, &present(&[]), &NONE);
        let rows = sections(&arch, &SoftwareConfig::default(), &arch_status);
        let aur = rows.iter().find(|s| s.source == Source::Aur).unwrap();
        assert_eq!(
            aur.lines,
            ["AUR helper: unavailable (yay or paru not found)"]
        );
        assert_eq!(aur.switch, Some(false));
    }

    #[test]
    fn quoted_os_release_values_are_read() {
        let single = Machine::detect("ID='arch'\n");
        assert_eq!(single.family, Some(Family::Pacman));
        assert!(single.arch_linux, "a single-quoted ID is read");
        assert!(Machine::detect("ID=manjaro\nID_LIKE='arch'\n").arch_linux);
        assert_eq!(
            Machine::detect("ID_LIKE='debian'\n").family,
            Some(Family::Apt)
        );
        assert_eq!(
            Machine::detect("ID=\"ubuntu\"\nID_LIKE=\"debian\"\n").family,
            Some(Family::Apt)
        );
        assert_eq!(Machine::detect("ID=''\n").family, None);
        // Only one matching pair is removed, and an unmatched quote stays.
        assert!(!Machine::detect("ID=\"'arch'\"\n").arch_linux);
        assert!(!Machine::detect("ID='arch\"\n").arch_linux);
        assert!(!Machine::detect("ID=arch'\n").arch_linux);
    }

    #[test]
    fn flathub_counts_a_shipped_remote_unless_it_was_applied() {
        let machine = Machine::detect(UBUNTU);
        let tools = present(&["flatpak"]);
        let shipped = FlatpakFiles {
            remotes_d_has_flathub: true,
            ..NONE
        };
        assert!(
            status(&machine, &tools, &shipped).flathub,
            "a remotes.d file that is not applied yet counts"
        );
        let removed = FlatpakFiles {
            remotes_d_has_flathub: true,
            applied_remotes: &["fedora\nflathub\n"],
            ..NONE
        };
        assert!(
            !status(&machine, &tools, &removed).flathub,
            "an applied remote with no definition was removed"
        );
        let applied_only = FlatpakFiles {
            applied_remotes: &["flathub\n"],
            ..NONE
        };
        assert!(!status(&machine, &tools, &applied_only).flathub);
        let other_applied = FlatpakFiles {
            remotes_d_has_flathub: true,
            applied_remotes: &["flathub-beta\nfedora\n"],
            ..NONE
        };
        assert!(
            status(&machine, &tools, &other_applied).flathub,
            "only an exact name counts as applied"
        );
        let defined = FlatpakFiles {
            repo_configs: &[FLATHUB_CONFIG],
            applied_remotes: &["flathub\n"],
            remotes_d_has_flathub: true,
        };
        assert!(
            status(&machine, &tools, &defined).flathub,
            "a repo definition counts even when it was applied"
        );
        assert!(!status(&machine, &tools, &NONE).flathub);
    }

    #[test]
    fn probe_reads_the_flatpak_files_on_disk() {
        let dir = std::env::temp_dir().join(format!("slopos-sources-test-{}", std::process::id()));
        let repo = dir.join("repo");
        let remotes = dir.join("remotes.d");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&remotes).unwrap();
        std::fs::write(remotes.join("flathub.flatpakrepo"), "[Flatpak Repo]\n").unwrap();
        let environment = Environment {
            machine: Machine::detect(UBUNTU),
            config_path: dir.join("software.conf"),
            tool_present: Arc::new(|name: &str| name == "flatpak"),
            flatpak_repos: vec![repo.clone()],
            flatpak_remotes_d: vec![remotes.clone()],
        };
        assert!(
            environment.probe().flathub,
            "a shipped remote that is not applied yet counts"
        );
        std::fs::write(repo.join("xa.applied-remotes"), "flathub\n").unwrap();
        assert!(
            !environment.probe().flathub,
            "an applied remote with no definition was removed"
        );
        std::fs::write(repo.join("config"), FLATHUB_CONFIG).unwrap();
        assert!(environment.probe().flathub, "a repo definition counts");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
