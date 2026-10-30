//! Keyboard and pointer settings on X11: keyboard layout (XKB), key repeat
//! (XKB controls), pointer speed and natural scrolling (libinput device
//! properties through XInput 2) and left-handed buttons (core pointer
//! mapping). Reading uses X requests and the XKB rules file; the only
//! program run is `setxkbmap`, to compile a new keymap, and its output is
//! never parsed.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use slopos_core::capability::Availability;
use thiserror::Error;
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _, DeviceType, XIChangePropertyAux};
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, MappingStatus, PropMode, Window};
use x11rb::rust_connection::RustConnection;

/// libinput's properties, as the X driver names them.
const ACCEL_SPEED: &str = "libinput Accel Speed";
const NATURAL_SCROLLING: &str = "libinput Natural Scrolling Enabled";

#[derive(Debug, Clone, PartialEq)]
pub struct InputSettings {
    /// XKB layout and variant, e.g. "us" and "" or "de" and "nodeadkeys".
    pub layout: String,
    pub variant: String,
    /// Milliseconds before a held key repeats, and repeats per second.
    pub repeat_delay_ms: u16,
    pub repeat_rate_hz: u16,
    /// libinput pointer speed, -1 (slowest) to 1 (fastest).
    pub pointer_speed: f32,
    pub natural_scrolling: bool,
    /// Primary and secondary buttons swapped.
    pub left_handed: bool,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            layout: "us".into(),
            variant: String::new(),
            repeat_delay_ms: 500,
            repeat_rate_hz: 30,
            pointer_speed: 0.0,
            natural_scrolling: false,
            left_handed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputSnapshot {
    pub availability: Availability,
    pub settings: InputSettings,
    /// Pointer devices that take libinput speed / scrolling settings.
    pub tunable_pointers: usize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InputError {
    #[error("no X display: {0}")]
    NoDisplay(String),
    #[error("{0}")]
    Invalid(String),
    #[error("the change failed: {0}")]
    Failed(String),
}

impl InputError {
    pub fn availability(&self) -> Availability {
        match self {
            InputError::NoDisplay(_) => Availability::MissingService,
            InputError::Invalid(_) | InputError::Failed(_) => Availability::Failed,
        }
    }
}

/// Check settings before they reach the server.
pub fn validate(settings: &InputSettings) -> Result<(), InputError> {
    let word = |text: &str| {
        text.len() <= 32
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    if settings.layout.is_empty() || !word(&settings.layout) || !word(&settings.variant) {
        return Err(InputError::Invalid(format!(
            "“{}” is not a keyboard layout",
            settings.layout
        )));
    }
    if !(100..=2000).contains(&settings.repeat_delay_ms)
        || !(2..=100).contains(&settings.repeat_rate_hz)
    {
        return Err(InputError::Invalid("key repeat is out of range".into()));
    }
    if !(-1.0..=1.0).contains(&settings.pointer_speed) {
        return Err(InputError::Invalid("pointer speed is out of range".into()));
    }
    Ok(())
}

/// Layouts from an XKB rules list (`evdev.lst`): (code, description), by
/// description.
pub fn parse_layouts(list: &str) -> Vec<(String, String)> {
    let mut layouts: Vec<(String, String)> = list
        .lines()
        .skip_while(|line| line.trim() != "! layout")
        .skip(1)
        .take_while(|line| !line.starts_with('!'))
        .filter_map(|line| {
            let line = line.trim();
            let (code, description) = line.split_once(char::is_whitespace)?;
            Some((code.to_owned(), description.trim().to_owned()))
        })
        .filter(|(code, description)| !code.is_empty() && !description.is_empty())
        .collect();
    layouts.sort_by_key(|layout| layout.1.to_lowercase());
    layouts
}

/// The layouts this system's XKB data offers.
pub fn layouts() -> Vec<(String, String)> {
    [
        "/usr/share/X11/xkb/rules/evdev.lst",
        "/usr/share/X11/xkb/rules/base.lst",
    ]
    .iter()
    .find_map(|path| std::fs::read_to_string(path).ok())
    .map(|text| parse_layouts(&text))
    .unwrap_or_default()
}

/// `_XKB_RULES_NAMES`: rules, model, layout, variant, options. Only the
/// first layout and variant are used.
pub fn parse_rules_names(value: &[u8]) -> (String, String) {
    let parts: Vec<String> = value
        .split(|&b| b == 0)
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    let first = |index: usize| {
        parts
            .get(index)
            .and_then(|text| text.split(',').next())
            .unwrap_or("")
            .to_owned()
    };
    (first(2), first(3))
}

/// Accel speed is a 32-bit float stored in a 32-bit property item.
pub fn speed_from_bits(bits: u32) -> f32 {
    f32::from_bits(bits).clamp(-1.0, 1.0)
}

/// Save settings for the next session.
pub fn save(path: &Path, settings: &InputSettings) -> std::io::Result<()> {
    let text = format!(
        "# SLOPOS keyboard and mouse\nlayout={}\nvariant={}\nrepeat_delay_ms={}\nrepeat_rate_hz={}\npointer_speed={}\nnatural_scrolling={}\nleft_handed={}\n",
        settings.layout,
        settings.variant,
        settings.repeat_delay_ms,
        settings.repeat_rate_hz,
        settings.pointer_speed,
        u8::from(settings.natural_scrolling),
        u8::from(settings.left_handed),
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(partial, path)
}

/// Saved settings, or `None` when there are none or they don't validate.
pub fn load(path: &Path) -> Option<InputSettings> {
    let text = std::fs::read_to_string(path).ok()?;
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .map(str::trim)
    };
    let settings = InputSettings {
        layout: field("layout")?.to_owned(),
        variant: field("variant").unwrap_or("").to_owned(),
        repeat_delay_ms: field("repeat_delay_ms")?.parse().ok()?,
        repeat_rate_hz: field("repeat_rate_hz")?.parse().ok()?,
        pointer_speed: field("pointer_speed")?.parse().ok()?,
        natural_scrolling: field("natural_scrolling")? == "1",
        left_handed: field("left_handed")? == "1",
    };
    validate(&settings).ok().map(|()| settings)
}

pub struct InputClient {
    conn: RustConnection,
    root: Window,
    xkb: bool,
    xinput: bool,
}

impl InputClient {
    pub fn connect() -> Result<Self, InputError> {
        let (conn, screen) =
            x11rb::connect(None).map_err(|error| InputError::NoDisplay(error.to_string()))?;
        let root = conn.setup().roots[screen].root;
        let xkb = conn
            .xkb_use_extension(1, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| reply.supported);
        let xinput = conn
            .xinput_xi_query_version(2, 2)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| reply.major_version >= 2);
        Ok(Self {
            conn,
            root,
            xkb,
            xinput,
        })
    }

    fn atom(&self, name: &str) -> Result<u32, InputError> {
        self.conn
            .intern_atom(false, name.as_bytes())
            .map_err(|error| InputError::Failed(error.to_string()))?
            .reply()
            .map(|reply| reply.atom)
            .map_err(|error| InputError::Failed(error.to_string()))
    }

    /// Slave pointers that have a given libinput property.
    fn pointers_with(&self, property: u32) -> Vec<u16> {
        if !self.xinput {
            return Vec::new();
        }
        let Ok(reply) = self
            .conn
            .xinput_xi_query_device(u16::from(xinput::Device::ALL))
            .map_err(|_| ())
            .and_then(|cookie| cookie.reply().map_err(|_| ()))
        else {
            return Vec::new();
        };
        reply
            .infos
            .iter()
            .filter(|info| info.type_ == DeviceType::SLAVE_POINTER && info.enabled)
            .map(|info| info.deviceid)
            .filter(|&device| {
                self.conn
                    .xinput_xi_list_properties(device)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .is_some_and(|reply| reply.properties.contains(&property))
            })
            .collect()
    }

    fn property(&self, device: u16, property: u32) -> Option<u32> {
        let reply = self
            .conn
            .xinput_xi_get_property(device, false, property, AtomEnum::ANY.into(), 0, 1)
            .ok()?
            .reply()
            .ok()?;
        match reply.items {
            xinput::XIGetPropertyItems::Data8(values) => values.first().map(|&v| u32::from(v)),
            xinput::XIGetPropertyItems::Data32(values) => values.first().copied(),
            _ => None,
        }
    }

    pub fn snapshot(&self) -> Result<InputSnapshot, InputError> {
        let failed = |error: &dyn std::fmt::Display| InputError::Failed(error.to_string());
        let mut settings = InputSettings::default();
        let rules = self.atom("_XKB_RULES_NAMES")?;
        let names = self
            .conn
            .get_property(false, self.root, rules, AtomEnum::STRING, 0, 1024)
            .map_err(|e| failed(&e))?
            .reply()
            .map_err(|e| failed(&e))?;
        let (layout, variant) = parse_rules_names(&names.value);
        if !layout.is_empty() {
            settings.layout = layout;
            settings.variant = variant;
        }
        if self.xkb {
            let controls = self
                .conn
                .xkb_get_controls(xkb::ID::USE_CORE_KBD.into())
                .map_err(|e| failed(&e))?
                .reply()
                .map_err(|e| failed(&e))?;
            settings.repeat_delay_ms = controls.repeat_delay;
            settings.repeat_rate_hz = (1000 / controls.repeat_interval.max(1)).max(1);
        }
        let mapping = self
            .conn
            .get_pointer_mapping()
            .map_err(|e| failed(&e))?
            .reply()
            .map_err(|e| failed(&e))?;
        settings.left_handed = mapping.map.len() >= 3 && mapping.map[0] == 3 && mapping.map[2] == 1;
        let speed = self.atom(ACCEL_SPEED)?;
        let natural = self.atom(NATURAL_SCROLLING)?;
        let tunable = self.pointers_with(speed);
        if let Some(&device) = tunable.first() {
            if let Some(bits) = self.property(device, speed) {
                settings.pointer_speed = speed_from_bits(bits);
            }
        }
        if let Some(&device) = self.pointers_with(natural).first() {
            settings.natural_scrolling = self.property(device, natural) == Some(1);
        }
        Ok(InputSnapshot {
            availability: Availability::Available,
            settings,
            tunable_pointers: tunable.len(),
        })
    }

    pub fn apply(&self, settings: &InputSettings) -> Result<InputSnapshot, InputError> {
        validate(settings)?;
        let before = self.snapshot()?.settings;
        let failed = |error: &dyn std::fmt::Display| InputError::Failed(error.to_string());
        if (&before.layout, &before.variant) != (&settings.layout, &settings.variant) {
            set_layout(&settings.layout, &settings.variant)?;
        }
        if self.xkb
            && (before.repeat_delay_ms, before.repeat_rate_hz)
                != (settings.repeat_delay_ms, settings.repeat_rate_hz)
        {
            let current = self
                .conn
                .xkb_get_controls(xkb::ID::USE_CORE_KBD.into())
                .map_err(|e| failed(&e))?
                .reply()
                .map_err(|e| failed(&e))?;
            self.conn
                .xkb_set_controls(
                    xkb::ID::USE_CORE_KBD.into(),
                    0u8.into(),
                    0u8.into(),
                    0u8.into(),
                    0u8.into(),
                    0u16.into(),
                    0u16.into(),
                    0u16.into(),
                    0u16.into(),
                    current.mouse_keys_dflt_btn,
                    current.groups_wrap,
                    current.access_x_option,
                    0u32.into(),
                    0u32.into(),
                    // XkbRepeatKeysMask: the repeat delay and interval.
                    xkb::Control::from(u32::from(xkb::BoolCtrl::REPEAT_KEYS)),
                    settings.repeat_delay_ms,
                    1000 / settings.repeat_rate_hz,
                    current.slow_keys_delay,
                    current.debounce_delay,
                    current.mouse_keys_delay,
                    current.mouse_keys_interval,
                    current.mouse_keys_time_to_max,
                    current.mouse_keys_max_speed,
                    current.mouse_keys_curve,
                    current.access_x_timeout,
                    current.access_x_timeout_mask,
                    current.access_x_timeout_values,
                    current.access_x_timeout_options_mask,
                    current.access_x_timeout_options_values,
                    &current.per_key_repeat,
                )
                .map_err(|e| failed(&e))?
                .check()
                .map_err(|e| failed(&e))?;
        }
        if before.left_handed != settings.left_handed {
            let mut map = self
                .conn
                .get_pointer_mapping()
                .map_err(|e| failed(&e))?
                .reply()
                .map_err(|e| failed(&e))?
                .map;
            if map.len() >= 3 {
                (map[0], map[2]) = if settings.left_handed { (3, 1) } else { (1, 3) };
                let status = self
                    .conn
                    .set_pointer_mapping(&map)
                    .map_err(|e| failed(&e))?
                    .reply()
                    .map_err(|e| failed(&e))?
                    .status;
                if status != MappingStatus::SUCCESS {
                    return Err(InputError::Failed(
                        "release the mouse buttons and try again".into(),
                    ));
                }
            }
        }
        let float = self.atom("FLOAT")?;
        let speed = self.atom(ACCEL_SPEED)?;
        for device in self.pointers_with(speed) {
            self.conn
                .xinput_xi_change_property(
                    device,
                    PropMode::REPLACE,
                    speed,
                    float,
                    1,
                    &XIChangePropertyAux::Data32(vec![settings.pointer_speed.to_bits()]),
                )
                .map_err(|e| failed(&e))?;
        }
        let natural = self.atom(NATURAL_SCROLLING)?;
        for device in self.pointers_with(natural) {
            self.conn
                .xinput_xi_change_property(
                    device,
                    PropMode::REPLACE,
                    natural,
                    AtomEnum::INTEGER.into(),
                    1,
                    &XIChangePropertyAux::Data8(vec![u8::from(settings.natural_scrolling)]),
                )
                .map_err(|e| failed(&e))?;
        }
        self.conn.flush().map_err(|e| failed(&e))?;
        self.snapshot()
    }
}

/// Compile and load a keymap for the layout. Arguments were validated.
fn set_layout(layout: &str, variant: &str) -> Result<(), InputError> {
    let mut command = Command::new("setxkbmap");
    command
        .arg("-layout")
        .arg(layout)
        .arg("-variant")
        .arg(variant)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| InputError::Failed(format!("setxkbmap: {error}")))?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let id = child.id();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait());
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(_)) => Err(InputError::Invalid(format!(
            "this system has no “{layout}” keyboard layout"
        ))),
        Ok(Err(error)) => Err(InputError::Failed(error.to_string())),
        Err(_) => {
            // SAFETY: plain kill(2) on the child we started.
            unsafe { libc::kill(id as libc::pid_t, libc::SIGKILL) };
            Err(InputError::Failed("setxkbmap did not finish".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_and_rules_names_are_read() {
        let list = "! model\n  pc105  Generic 105-key PC\n\n! layout\n  us              English (US)\n  de              German\n  af              Dari\n\n! variant\n  nodeadkeys      de: German (no dead keys)\n";
        assert_eq!(
            parse_layouts(list),
            vec![
                ("af".to_owned(), "Dari".to_owned()),
                ("us".to_owned(), "English (US)".to_owned()),
                ("de".to_owned(), "German".to_owned()),
            ]
        );
        assert!(parse_layouts("nothing").is_empty());
        assert_eq!(
            parse_rules_names(b"evdev\0pc105\0de,us\0nodeadkeys,\0grp:alt_shift_toggle\0"),
            ("de".to_owned(), "nodeadkeys".to_owned())
        );
        assert_eq!(parse_rules_names(b""), (String::new(), String::new()));
        assert_eq!(speed_from_bits(0.5f32.to_bits()), 0.5);
        assert!(speed_from_bits(f32::NAN.to_bits()).is_nan());
        assert_eq!(speed_from_bits(4.0f32.to_bits()), 1.0);
    }

    #[test]
    fn settings_are_validated_saved_and_loaded() {
        let mut settings = InputSettings {
            layout: "de".into(),
            variant: "nodeadkeys".into(),
            repeat_delay_ms: 300,
            repeat_rate_hz: 40,
            pointer_speed: -0.25,
            natural_scrolling: true,
            left_handed: true,
        };
        assert!(validate(&settings).is_ok());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slopos-i/input.conf");
        save(&path, &settings).unwrap();
        assert_eq!(load(&path), Some(settings.clone()));
        for bad in [
            InputSettings {
                layout: "us; rm -rf".into(),
                ..settings.clone()
            },
            InputSettings {
                layout: String::new(),
                ..settings.clone()
            },
            InputSettings {
                repeat_rate_hz: 0,
                ..settings.clone()
            },
            InputSettings {
                pointer_speed: 2.0,
                ..settings.clone()
            },
        ] {
            assert!(validate(&bad).is_err(), "{bad:?}");
        }
        std::fs::write(&path, "layout=us\nrepeat_delay_ms=5\n").unwrap();
        assert_eq!(load(&path), None);
        settings.variant.clear();
        assert!(load(&dir.path().join("missing")).is_none());
    }

    #[test]
    #[ignore = "needs an X server (run under Xvfb)"]
    fn reads_and_applies_on_the_live_x_server() {
        let client = InputClient::connect().unwrap();
        let mut settings = client.snapshot().unwrap().settings;
        settings.repeat_delay_ms = 250;
        settings.repeat_rate_hz = 25;
        settings.left_handed = !settings.left_handed;
        let after = client.apply(&settings).unwrap().settings;
        assert_eq!(after.repeat_delay_ms, 250);
        assert_eq!(after.repeat_rate_hz, 25);
        assert_eq!(after.left_handed, settings.left_handed);
        settings.left_handed = !settings.left_handed;
        client.apply(&settings).unwrap();
    }
}
