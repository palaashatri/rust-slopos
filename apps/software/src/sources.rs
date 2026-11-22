//! Which software sources the Software window offers on this machine.
//!
//! The tabs are the SLOPOS catalogue (always), the distribution's native
//! package manager (when the distribution is known), and the optional
//! Flatpak, Snap and AUR sources the user has enabled. A tab whose tool is
//! missing stays visible and says so; nothing is invented to fill it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use slopos_core::software_config::{self, SoftwareConfig};

/// A tool lookup: does this program exist on the system?
pub type ToolCheck = Arc<dyn Fn(&str) -> bool + Send + Sync>;

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
}

impl Source {
    pub fn is_backend(self) -> bool {
        !matches!(self, Source::Slopos)
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
    /// The program that searches this family's repositories.
    pub fn tool(self) -> &'static str {
        match self {
            Family::Apt => "apt-cache",
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

/// What the machine is: its native family and whether it is Arch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    pub family: Option<Family>,
    pub arch_linux: bool,
}

impl Machine {
    /// Build from the text of `/etc/os-release` and whether
    /// `/etc/arch-release` exists.
    pub fn detect(os_release: &str, arch_release_present: bool) -> Self {
        Self {
            family: family_from_os_release(os_release),
            arch_linux: arch_release_present,
        }
    }
}

/// Map `ID` and `ID_LIKE` from os-release to a package family.
pub fn family_from_os_release(text: &str) -> Option<Family> {
    let mut ids = Vec::new();
    for line in text.lines() {
        if let Some(value) = line
            .strip_prefix("ID=")
            .or_else(|| line.strip_prefix("ID_LIKE="))
        {
            let value = value.trim().trim_matches('"');
            ids.extend(value.split_whitespace().map(str::to_ascii_lowercase));
        }
    }
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
    /// The tool to run is present; `program` is its name. The catalogue tab
    /// has no tool and carries an empty name.
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

/// Everything the window needs to know about the machine it runs on.
#[derive(Clone)]
pub struct Environment {
    pub machine: Machine,
    pub config_path: PathBuf,
    pub tool_present: ToolCheck,
}

impl Environment {
    /// Read the real system: os-release, the Arch marker, the config file
    /// location and `PATH` for tool lookups.
    pub fn from_system() -> Self {
        let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let machine = Machine::detect(&os_release, Path::new("/etc/arch-release").exists());
        let config_path = software_config::default_path()
            .unwrap_or_else(|_| PathBuf::from("slopos-i/software.conf"));
        Self {
            machine,
            config_path,
            tool_present: Arc::new(|name: &str| {
                slopos_core::process::resolve_program(name).is_some()
            }),
        }
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

    fn present(names: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |name: &str| names.contains(&name)
    }

    #[test]
    fn families_come_from_id_and_id_like() {
        assert_eq!(family_from_os_release(UBUNTU), Some(Family::Apt));
        assert_eq!(family_from_os_release(FEDORA), Some(Family::Dnf));
        assert_eq!(family_from_os_release(ARCH), Some(Family::Pacman));
        assert_eq!(family_from_os_release(MANJARO), Some(Family::Pacman));
        assert_eq!(family_from_os_release(ALPINE), None);
        assert_eq!(family_from_os_release(""), None);
    }

    #[test]
    fn catalogue_is_always_there_and_native_comes_first() {
        let machine = Machine::detect(UBUNTU, false);
        let tabs = tabs(
            &machine,
            &SoftwareConfig::default(),
            &present(&["apt-cache"]),
        );
        let sources: Vec<Source> = tabs.iter().map(|tab| tab.source).collect();
        assert_eq!(sources, [Source::Native, Source::Slopos]);
        assert_eq!(tabs[0].label, "Native (apt)");
        assert_eq!(
            tabs[0].availability,
            Availability::Ready {
                program: "apt-cache".into()
            }
        );
        let none = self::tabs(
            &Machine::detect(ALPINE, false),
            &SoftwareConfig::default(),
            &present(&[]),
        );
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].source, Source::Slopos);
    }

    #[test]
    fn missing_tools_are_named_not_hidden() {
        let machine = Machine::detect(FEDORA, false);
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
                Source::Snap
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
        let arch = Machine::detect(ARCH, true);
        let off = tabs(&arch, &SoftwareConfig::default(), &present(&["pacman"]));
        assert_eq!(
            off.iter().map(|tab| tab.source).collect::<Vec<_>>(),
            [Source::Native, Source::Slopos]
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
                Source::Aur
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
        let debian = tabs(&Machine::detect(UBUNTU, false), &on, &present(&[]));
        assert!(debian.iter().all(|tab| tab.source != Source::Aur));
        // A derivative with arch-release counts as Arch for AUR.
        let manjaro = tabs(&Machine::detect(MANJARO, true), &on, &present(&["yay"]));
        assert!(manjaro.iter().any(|tab| tab.source == Source::Aur));
    }
}
