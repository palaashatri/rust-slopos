//! Display outputs through the X RandR extension (1.3 or newer): which
//! monitors are connected, their modes, rotation, position and the primary
//! one, and changing them. The planning, arrangement and settings file are
//! plain functions; [`DisplayClient`] is the thin X11 adapter.
//!
//! A change is applied the way RandR requires: every CRTC whose setup
//! changes is switched off, the screen is resized to fit the new layout,
//! then the CRTCs are set up again, all under one server grab.

use std::path::Path;
use std::thread;

use slopos_core::capability::Availability;
use thiserror::Error;
use x11rb::connection::Connection;
use x11rb::protocol::randr::{self, ConnectionExt as _, ModeFlag, NotifyMask, SetConfig};
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rotation {
    Normal,
    Left,
    Inverted,
    Right,
}

impl Rotation {
    pub const ALL: [Rotation; 4] = [
        Rotation::Normal,
        Rotation::Left,
        Rotation::Inverted,
        Rotation::Right,
    ];

    fn bits(self) -> u16 {
        match self {
            Rotation::Normal => 1,
            Rotation::Left => 2,
            Rotation::Inverted => 4,
            Rotation::Right => 8,
        }
    }

    fn from_bits(bits: u16) -> Self {
        match bits & 0x0f {
            2 => Rotation::Left,
            4 => Rotation::Inverted,
            8 => Rotation::Right,
            _ => Rotation::Normal,
        }
    }

    /// Whether width and height swap on screen.
    pub fn is_sideways(self) -> bool {
        matches!(self, Rotation::Left | Rotation::Right)
    }

    pub fn name(self) -> &'static str {
        match self {
            Rotation::Normal => "normal",
            Rotation::Left => "left",
            Rotation::Inverted => "inverted",
            Rotation::Right => "right",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|rotation| rotation.name() == text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMode {
    pub id: u32,
    pub width: u16,
    pub height: u16,
    /// Refresh rate in millihertz (60 Hz is 60000).
    pub refresh_millihz: u32,
}

/// A connected output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayOutput {
    pub id: u32,
    /// The RandR name, e.g. "HDMI-1".
    pub name: String,
    pub enabled: bool,
    pub primary: bool,
    /// Modes the monitor supports, largest first.
    pub modes: Vec<DisplayMode>,
    /// The monitor's preferred mode.
    pub preferred: Option<u32>,
    /// The mode in use while enabled.
    pub mode: Option<u32>,
    pub rotation: Rotation,
    /// Rotations the CRTC can do.
    pub rotations: Vec<Rotation>,
    pub x: i16,
    pub y: i16,
    /// The CRTC driving it (0 when off) and the CRTCs that could.
    pub crtc: u32,
    pub crtcs: Vec<u32>,
    pub width_mm: u32,
    pub height_mm: u32,
}

impl DisplayOutput {
    pub fn current_mode(&self) -> Option<&DisplayMode> {
        let id = self.mode?;
        self.modes.iter().find(|mode| mode.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplaySnapshot {
    pub availability: Availability,
    /// Connected outputs, in the server's order.
    pub outputs: Vec<DisplayOutput>,
    /// The screen size limits, in pixels.
    pub min_size: (u16, u16),
    pub max_size: (u16, u16),
}

/// How one output should be set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputConfig {
    pub name: String,
    pub enabled: bool,
    pub width: u16,
    pub height: u16,
    pub refresh_millihz: u32,
    pub rotation: Rotation,
    pub x: i16,
    pub y: i16,
    pub primary: bool,
}

impl OutputConfig {
    /// Width and height on screen, after rotation.
    pub fn extent(&self) -> (u16, u16) {
        if self.rotation.is_sideways() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DisplayError {
    #[error("no X display: {0}")]
    NoDisplay(String),
    #[error("the X server has no RandR 1.3: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Invalid(String),
    #[error("the X server refused the change: {0}")]
    Failed(String),
}

impl DisplayError {
    pub fn availability(&self) -> Availability {
        match self {
            DisplayError::NoDisplay(_) => Availability::MissingService,
            DisplayError::Unsupported(_) => Availability::UnsupportedHardware,
            DisplayError::Invalid(_) | DisplayError::Failed(_) => Availability::Failed,
        }
    }
}

/// The current setup of every connected output.
pub fn current_configs(snapshot: &DisplaySnapshot) -> Vec<OutputConfig> {
    snapshot
        .outputs
        .iter()
        .map(|output| {
            let mode = output
                .current_mode()
                .or_else(|| {
                    output
                        .preferred
                        .and_then(|id| output.modes.iter().find(|mode| mode.id == id))
                })
                .or(output.modes.first());
            OutputConfig {
                name: output.name.clone(),
                enabled: output.enabled,
                width: mode.map_or(0, |mode| mode.width),
                height: mode.map_or(0, |mode| mode.height),
                refresh_millihz: mode.map_or(0, |mode| mode.refresh_millihz),
                rotation: output.rotation,
                x: output.x,
                y: output.y,
                primary: output.primary,
            }
        })
        .collect()
}

/// The mode of `output` with this size closest to `refresh_millihz`.
pub fn find_mode(
    output: &DisplayOutput,
    width: u16,
    height: u16,
    refresh_millihz: u32,
) -> Option<&DisplayMode> {
    output
        .modes
        .iter()
        .filter(|mode| mode.width == width && mode.height == height)
        .min_by_key(|mode| mode.refresh_millihz.abs_diff(refresh_millihz))
}

/// Distinct sizes `output` supports, largest first.
pub fn sizes(output: &DisplayOutput) -> Vec<(u16, u16)> {
    let mut sizes: Vec<(u16, u16)> = output
        .modes
        .iter()
        .map(|mode| (mode.width, mode.height))
        .collect();
    sizes.sort_by(|a, b| {
        (u32::from(b.0) * u32::from(b.1))
            .cmp(&(u32::from(a.0) * u32::from(a.1)))
            .then(b.0.cmp(&a.0))
    });
    sizes.dedup();
    sizes
}

/// Refresh rates `output` offers at a size, highest first.
pub fn refresh_rates(output: &DisplayOutput, width: u16, height: u16) -> Vec<u32> {
    let mut rates: Vec<u32> = output
        .modes
        .iter()
        .filter(|mode| mode.width == width && mode.height == height)
        .map(|mode| mode.refresh_millihz)
        .collect();
    rates.sort_unstable_by(|a, b| b.cmp(a));
    // Modes a few millihertz apart read the same to people.
    rates.dedup_by(|a, b| a.abs_diff(*b) < 50);
    rates
}

/// The refresh rate of a mode line, in millihertz.
pub fn refresh_millihz(dot_clock: u32, htotal: u16, vtotal: u16, flags: u32) -> u32 {
    let mut pixels = u64::from(htotal) * u64::from(vtotal);
    let mut clock = u64::from(dot_clock) * 1000;
    if flags & u32::from(ModeFlag::DOUBLE_SCAN) != 0 {
        pixels *= 2;
    }
    // Interlaced: each refresh draws half the lines.
    if flags & u32::from(ModeFlag::INTERLACE) != 0 {
        clock *= 2;
    }
    if pixels == 0 {
        return 0;
    }
    ((clock + pixels / 2) / pixels) as u32
}

/// Where one output goes relative to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    RightOf,
    LeftOf,
    Above,
    Below,
    /// The same picture on both (mirroring).
    SameAs,
}

impl Placement {
    pub const ALL: [Placement; 5] = [
        Placement::RightOf,
        Placement::LeftOf,
        Placement::Above,
        Placement::Below,
        Placement::SameAs,
    ];
}

/// Move `configs[moved]` next to `configs[anchor]`, then shift the layout so
/// it starts at 0,0.
pub fn place(configs: &mut [OutputConfig], moved: usize, placement: Placement, anchor: usize) {
    if moved == anchor || moved >= configs.len() || anchor >= configs.len() {
        return;
    }
    let (ax, ay) = (i32::from(configs[anchor].x), i32::from(configs[anchor].y));
    let (aw, ah) = configs[anchor].extent();
    let (mw, mh) = configs[moved].extent();
    let (x, y) = match placement {
        Placement::RightOf => (ax + i32::from(aw), ay),
        Placement::LeftOf => (ax - i32::from(mw), ay),
        Placement::Above => (ax, ay - i32::from(mh)),
        Placement::Below => (ax, ay + i32::from(ah)),
        Placement::SameAs => (ax, ay),
    };
    configs[moved].x = x.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
    configs[moved].y = y.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
    normalize(configs);
}

/// Where `configs[moved]` sits relative to `configs[anchor]`, if it touches.
pub fn placement_of(configs: &[OutputConfig], moved: usize, anchor: usize) -> Option<Placement> {
    let (m, a) = (configs.get(moved)?, configs.get(anchor)?);
    let (aw, ah) = a.extent();
    let (mw, mh) = m.extent();
    let (mx, my, ax, ay) = (
        i32::from(m.x),
        i32::from(m.y),
        i32::from(a.x),
        i32::from(a.y),
    );
    if (mx, my) == (ax, ay) {
        Some(Placement::SameAs)
    } else if mx == ax + i32::from(aw) {
        Some(Placement::RightOf)
    } else if mx + i32::from(mw) == ax {
        Some(Placement::LeftOf)
    } else if my + i32::from(mh) == ay {
        Some(Placement::Above)
    } else if my == ay + i32::from(ah) {
        Some(Placement::Below)
    } else {
        None
    }
}

/// Shift enabled outputs so the layout's top-left corner is 0,0.
pub fn normalize(configs: &mut [OutputConfig]) {
    let enabled = configs.iter().filter(|config| config.enabled);
    let min_x = enabled.clone().map(|config| config.x).min().unwrap_or(0);
    let min_y = enabled.map(|config| config.y).min().unwrap_or(0);
    for config in configs.iter_mut().filter(|config| config.enabled) {
        config.x -= min_x;
        config.y -= min_y;
    }
}

/// After one output changes size, keep outputs that were beside it beside
/// it rather than overlapping or leaving a gap.
pub fn reflow(before: &[OutputConfig], after: &mut [OutputConfig], changed: usize) {
    for other in 0..after.len() {
        if other == changed {
            continue;
        }
        if let Some(placement) = placement_of(before, other, changed) {
            place(after, other, placement, changed);
        }
    }
    normalize(after);
}

/// What applying a set of configs takes, in RandR terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Screen size in pixels and millimetres.
    pub screen: (u16, u16, u32, u32),
    /// (crtc, mode, rotation bits, x, y, outputs) for every CRTC to light.
    pub crtcs: Vec<(u32, u32, u16, i16, i16, Vec<u32>)>,
    /// CRTCs to switch off for good.
    pub off: Vec<u32>,
    pub primary: Option<u32>,
}

/// Check `configs` against what the hardware offers and plan the change.
pub fn plan(snapshot: &DisplaySnapshot, configs: &[OutputConfig]) -> Result<Plan, DisplayError> {
    if !configs.iter().any(|config| config.enabled) {
        return Err(DisplayError::Invalid(
            "at least one display has to stay on".into(),
        ));
    }
    let mut crtcs: Vec<(u32, u32, u16, i16, i16, Vec<u32>)> = Vec::new();
    let mut used = Vec::new();
    let mut primary = None;
    let mut right = 0i32;
    let mut bottom = 0i32;
    for config in configs.iter().filter(|config| config.enabled) {
        let output = snapshot
            .outputs
            .iter()
            .find(|output| output.name == config.name)
            .ok_or_else(|| DisplayError::Invalid(format!("{} is not connected", config.name)))?;
        let mode = find_mode(output, config.width, config.height, config.refresh_millihz)
            .ok_or_else(|| {
                DisplayError::Invalid(format!(
                    "{} can't show {} × {}",
                    config.name, config.width, config.height
                ))
            })?;
        if !output.rotations.contains(&config.rotation) {
            return Err(DisplayError::Invalid(format!(
                "{} can't be turned {}",
                config.name,
                config.rotation.name()
            )));
        }
        if config.x < 0 || config.y < 0 {
            return Err(DisplayError::Invalid("the layout must start at 0,0".into()));
        }
        // Outputs showing the same picture share one CRTC when they can.
        let shared = crtcs.iter_mut().find(|(crtc, mode_id, rotation, x, y, _)| {
            *mode_id == mode.id
                && *rotation == config.rotation.bits()
                && (*x, *y) == (config.x, config.y)
                && output.crtcs.contains(crtc)
        });
        if let Some(entry) = shared {
            entry.5.push(output.id);
        } else {
            let crtc = std::iter::once(output.crtc)
                .filter(|&crtc| crtc != 0)
                .chain(output.crtcs.iter().copied())
                .find(|crtc| !used.contains(crtc))
                .ok_or_else(|| {
                    DisplayError::Invalid(format!("no free display controller for {}", config.name))
                })?;
            used.push(crtc);
            crtcs.push((
                crtc,
                mode.id,
                config.rotation.bits(),
                config.x,
                config.y,
                vec![output.id],
            ));
        }
        let (width, height) = config.extent();
        right = right.max(i32::from(config.x) + i32::from(width));
        bottom = bottom.max(i32::from(config.y) + i32::from(height));
        if config.primary {
            primary = Some(output.id);
        }
    }
    let (min, max) = (snapshot.min_size, snapshot.max_size);
    if right > i32::from(max.0) || bottom > i32::from(max.1) {
        return Err(DisplayError::Invalid(format!(
            "the layout is {right} × {bottom}; this screen goes up to {} × {}",
            max.0, max.1
        )));
    }
    let width = right.max(i32::from(min.0)) as u16;
    let height = bottom.max(i32::from(min.1)) as u16;
    // Millimetres at 96 dpi, what X reports when nothing better is known.
    let mm = |pixels: u16| (u32::from(pixels) * 254 + 480) / 960;
    let off = snapshot
        .outputs
        .iter()
        .map(|output| output.crtc)
        .filter(|&crtc| crtc != 0 && !used.contains(&crtc))
        .collect();
    Ok(Plan {
        screen: (width, height, mm(width), mm(height)),
        crtcs,
        off,
        primary,
    })
}

/// Save the layout for the next session (one line per output).
pub fn save(path: &Path, configs: &[OutputConfig]) -> std::io::Result<()> {
    let mut text = String::from("# SLOPOS display layout\n");
    for config in configs {
        text.push_str(&format!(
            "output={} enabled={} mode={}x{}@{} rotation={} position={},{} primary={}\n",
            config.name,
            u8::from(config.enabled),
            config.width,
            config.height,
            config.refresh_millihz,
            config.rotation.name(),
            config.x,
            config.y,
            u8::from(config.primary),
        ));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(partial, path)
}

/// Read a saved layout; malformed lines are skipped.
pub fn load(path: &Path) -> Vec<OutputConfig> {
    std::fs::read_to_string(path)
        .map(|text| text.lines().filter_map(parse_line).collect())
        .unwrap_or_default()
}

fn parse_line(line: &str) -> Option<OutputConfig> {
    let line = line.trim();
    if line.starts_with('#') || line.is_empty() {
        return None;
    }
    let field = |key: &str| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(key)?.strip_prefix('='))
    };
    let name = field("output").filter(|name| !name.is_empty() && name.len() <= 64)?;
    let (size, refresh) = field("mode")?.split_once('@')?;
    let (width, height) = size.split_once('x')?;
    let (x, y) = field("position")?.split_once(',')?;
    Some(OutputConfig {
        name: name.to_owned(),
        enabled: field("enabled")? == "1",
        width: width.parse().ok().filter(|&w| w > 0)?,
        height: height.parse().ok().filter(|&h| h > 0)?,
        refresh_millihz: refresh.parse().ok()?,
        rotation: Rotation::parse(field("rotation")?)?,
        x: x.parse().ok().filter(|&x: &i16| x >= 0)?,
        y: y.parse().ok().filter(|&y: &i16| y >= 0)?,
        primary: field("primary")? == "1",
    })
}

/// Saved configs for the outputs connected now; `None` when the saved
/// layout is for a different set of monitors.
pub fn saved_for(snapshot: &DisplaySnapshot, saved: &[OutputConfig]) -> Option<Vec<OutputConfig>> {
    let mut names: Vec<&str> = snapshot.outputs.iter().map(|o| o.name.as_str()).collect();
    let mut saved_names: Vec<&str> = saved.iter().map(|c| c.name.as_str()).collect();
    names.sort_unstable();
    saved_names.sort_unstable();
    (!names.is_empty() && names == saved_names).then(|| saved.to_vec())
}

/// The X11 side.
pub struct DisplayClient {
    conn: RustConnection,
    root: Window,
}

impl DisplayClient {
    pub fn connect() -> Result<Self, DisplayError> {
        let (conn, screen) =
            x11rb::connect(None).map_err(|error| DisplayError::NoDisplay(error.to_string()))?;
        let root = conn.setup().roots[screen].root;
        let version = conn
            .randr_query_version(1, 3)
            .map_err(|error| DisplayError::Unsupported(error.to_string()))?
            .reply()
            .map_err(|error| DisplayError::Unsupported(error.to_string()))?;
        if (version.major_version, version.minor_version) < (1, 3) {
            return Err(DisplayError::Unsupported(format!(
                "version {}.{}",
                version.major_version, version.minor_version
            )));
        }
        Ok(Self { conn, root })
    }

    pub fn snapshot(&self) -> Result<DisplaySnapshot, DisplayError> {
        snapshot(&self.conn, self.root).map_err(|error| DisplayError::Failed(error.to_string()))
    }

    /// Apply `configs` and return the setup the server ended up with.
    pub fn apply(&self, configs: &[OutputConfig]) -> Result<DisplaySnapshot, DisplayError> {
        let before = self.snapshot()?;
        let plan = plan(&before, configs)?;
        let failed = |error: &dyn std::fmt::Display| DisplayError::Failed(error.to_string());
        let conn = &self.conn;
        let resources = conn
            .randr_get_screen_resources_current(self.root)
            .map_err(|e| failed(&e))?
            .reply()
            .map_err(|e| failed(&e))?;
        let stamp = resources.config_timestamp;
        let check = |status: SetConfig| {
            if status == SetConfig::SUCCESS {
                Ok(())
            } else {
                Err(DisplayError::Failed(format!("{status:?}")))
            }
        };
        conn.grab_server().map_err(|e| failed(&e))?;
        let result = (|| {
            // Switch off what goes away or changes, so the screen can shrink.
            for output in &before.outputs {
                if output.crtc == 0 {
                    continue;
                }
                let unchanged = plan.crtcs.iter().any(|(crtc, mode, rotation, x, y, _)| {
                    *crtc == output.crtc
                        && Some(*mode) == output.mode
                        && *rotation == output.rotation.bits()
                        && (*x, *y) == (output.x, output.y)
                });
                if !unchanged {
                    let reply = conn
                        .randr_set_crtc_config(
                            output.crtc,
                            CURRENT_TIME,
                            stamp,
                            0,
                            0,
                            0,
                            randr::Rotation::ROTATE0,
                            &[],
                        )
                        .map_err(|e| failed(&e))?
                        .reply()
                        .map_err(|e| failed(&e))?;
                    check(reply.status)?;
                }
            }
            let (width, height, width_mm, height_mm) = plan.screen;
            conn.randr_set_screen_size(self.root, width, height, width_mm, height_mm)
                .map_err(|e| failed(&e))?
                .check()
                .map_err(|e| failed(&e))?;
            for (crtc, mode, rotation, x, y, outputs) in &plan.crtcs {
                let reply = conn
                    .randr_set_crtc_config(
                        *crtc,
                        CURRENT_TIME,
                        stamp,
                        *x,
                        *y,
                        *mode,
                        randr::Rotation::from(*rotation),
                        outputs,
                    )
                    .map_err(|e| failed(&e))?
                    .reply()
                    .map_err(|e| failed(&e))?;
                check(reply.status)?;
            }
            if let Some(primary) = plan.primary {
                conn.randr_set_output_primary(self.root, primary)
                    .map_err(|e| failed(&e))?
                    .check()
                    .map_err(|e| failed(&e))?;
            }
            Ok(())
        })();
        let _ = conn.ungrab_server();
        let _ = conn.flush();
        result?;
        self.snapshot()
    }

    /// Report every change of outputs or modes (a monitor plugged in, or
    /// another program's change) from a thread blocked on RandR events.
    pub fn watch(
        report: impl Fn(Result<DisplaySnapshot, DisplayError>) + Send + 'static,
    ) -> Result<(), DisplayError> {
        let client = Self::connect()?;
        client
            .conn
            .randr_select_input(
                client.root,
                NotifyMask::SCREEN_CHANGE | NotifyMask::CRTC_CHANGE | NotifyMask::OUTPUT_CHANGE,
            )
            .map_err(|error| DisplayError::Failed(error.to_string()))?;
        client
            .conn
            .flush()
            .map_err(|error| DisplayError::Failed(error.to_string()))?;
        thread::Builder::new()
            .name("display-watch".into())
            .spawn(move || loop {
                let Ok(event) = client.conn.wait_for_event() else {
                    return report(Err(DisplayError::NoDisplay("connection closed".into())));
                };
                if !matches!(
                    event,
                    Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_)
                ) {
                    continue;
                }
                // One change sends several events; report once for them.
                while let Ok(Some(_)) = client.conn.poll_for_event() {}
                report(client.snapshot());
            })
            .map_err(|error| DisplayError::Failed(error.to_string()))?;
        Ok(())
    }
}

fn snapshot(
    conn: &RustConnection,
    root: Window,
) -> Result<DisplaySnapshot, Box<dyn std::error::Error>> {
    let resources = conn.randr_get_screen_resources_current(root)?.reply()?;
    let range = conn.randr_get_screen_size_range(root)?.reply()?;
    let primary = conn.randr_get_output_primary(root)?.reply()?.output;
    let all_modes: Vec<DisplayMode> = resources
        .modes
        .iter()
        .map(|mode| DisplayMode {
            id: mode.id,
            width: mode.width,
            height: mode.height,
            refresh_millihz: refresh_millihz(
                mode.dot_clock,
                mode.htotal,
                mode.vtotal,
                u32::from(mode.mode_flags),
            ),
        })
        .collect();
    let mut outputs = Vec::new();
    for &id in &resources.outputs {
        let info = conn
            .randr_get_output_info(id, resources.config_timestamp)?
            .reply()?;
        if info.connection != randr::Connection::CONNECTED {
            continue;
        }
        let mut modes: Vec<DisplayMode> = info
            .modes
            .iter()
            .filter_map(|mode| all_modes.iter().find(|m| m.id == *mode).cloned())
            .collect();
        let preferred = info
            .modes
            .get(usize::from(info.num_preferred).saturating_sub(1))
            .copied()
            .filter(|_| info.num_preferred > 0);
        modes.sort_by(|a, b| {
            (u32::from(b.width) * u32::from(b.height))
                .cmp(&(u32::from(a.width) * u32::from(a.height)))
                .then(b.refresh_millihz.cmp(&a.refresh_millihz))
        });
        let (mut mode, mut rotation, mut rotations, mut x, mut y) =
            (None, Rotation::Normal, vec![Rotation::Normal], 0, 0);
        let crtc_for_rotations = if info.crtc != 0 {
            info.crtc
        } else {
            info.crtcs.first().copied().unwrap_or(0)
        };
        if crtc_for_rotations != 0 {
            let crtc = conn
                .randr_get_crtc_info(crtc_for_rotations, resources.config_timestamp)?
                .reply()?;
            let supported = u16::from(crtc.rotations);
            rotations = Rotation::ALL
                .into_iter()
                .filter(|rotation| supported & rotation.bits() != 0)
                .collect();
            if rotations.is_empty() {
                rotations.push(Rotation::Normal);
            }
            if info.crtc != 0 && crtc.mode != 0 {
                mode = Some(crtc.mode);
                rotation = Rotation::from_bits(u16::from(crtc.rotation));
                x = crtc.x;
                y = crtc.y;
            }
        }
        outputs.push(DisplayOutput {
            id,
            name: String::from_utf8_lossy(&info.name).into_owned(),
            enabled: mode.is_some(),
            primary: id == primary,
            modes,
            preferred,
            mode,
            rotation,
            rotations,
            x,
            y,
            crtc: if mode.is_some() { info.crtc } else { 0 },
            crtcs: info.crtcs.clone(),
            width_mm: info.mm_width,
            height_mm: info.mm_height,
        });
    }
    // With no primary set, the first enabled output acts as it.
    if !outputs.iter().any(|output| output.primary) {
        if let Some(first) = outputs.iter_mut().find(|output| output.enabled) {
            first.primary = true;
        }
    }
    Ok(DisplaySnapshot {
        availability: if outputs.is_empty() {
            Availability::UnsupportedHardware
        } else {
            Availability::Available
        },
        outputs,
        min_size: (range.min_width, range.min_height),
        max_size: (range.max_width, range.max_height),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(id: u32, width: u16, height: u16, hz: u32) -> DisplayMode {
        DisplayMode {
            id,
            width,
            height,
            refresh_millihz: hz,
        }
    }

    fn output(id: u32, name: &str, crtc: u32, x: i16) -> DisplayOutput {
        DisplayOutput {
            id,
            name: name.into(),
            enabled: crtc != 0,
            primary: id == 1,
            // RandR mode ids are server-wide: equal timings share an id.
            modes: vec![
                mode(10, 1920, 1080, 60000),
                mode(20, 1920, 1080, 50000),
                mode(30, 1280, 720, 60000),
            ],
            preferred: Some(10),
            mode: (crtc != 0).then_some(10),
            rotation: Rotation::Normal,
            rotations: Rotation::ALL.to_vec(),
            x,
            y: 0,
            crtc,
            crtcs: vec![100, 200],
            width_mm: 520,
            height_mm: 290,
        }
    }

    fn two_screens() -> DisplaySnapshot {
        DisplaySnapshot {
            availability: Availability::Available,
            outputs: vec![output(1, "eDP-1", 100, 0), output(2, "HDMI-1", 200, 1920)],
            min_size: (320, 200),
            max_size: (8192, 8192),
        }
    }

    #[test]
    fn mode_lines_give_refresh_rates() {
        // 1920x1080@60 CEA timing: 148.5 MHz, 2200 x 1125.
        assert_eq!(refresh_millihz(148_500_000, 2200, 1125, 0), 60000);
        // 59.94 Hz NTSC-style timing.
        assert_eq!(refresh_millihz(148_351_648, 2200, 1125, 0), 59940);
        let interlaced = u32::from(ModeFlag::INTERLACE);
        assert_eq!(refresh_millihz(74_250_000, 2200, 1125, interlaced), 60000);
        assert_eq!(refresh_millihz(1, 0, 0, 0), 0);
        let snapshot = two_screens();
        assert_eq!(sizes(&snapshot.outputs[0]), vec![(1920, 1080), (1280, 720)]);
        assert_eq!(
            refresh_rates(&snapshot.outputs[0], 1920, 1080),
            vec![60000, 50000]
        );
        assert_eq!(
            find_mode(&snapshot.outputs[0], 1920, 1080, 49_900).map(|m| m.id),
            Some(20)
        );
        assert!(find_mode(&snapshot.outputs[0], 640, 480, 60000).is_none());
    }

    #[test]
    fn plans_check_the_hardware_and_size_the_screen() {
        let snapshot = two_screens();
        let mut configs = current_configs(&snapshot);
        let plan = plan(&snapshot, &configs).unwrap();
        assert_eq!(plan.screen.0, 3840);
        assert_eq!(plan.screen.1, 1080);
        assert_eq!(plan.screen.2, 1016);
        assert_eq!(plan.primary, Some(1));
        assert!(plan.off.is_empty());
        assert_eq!(plan.crtcs[1], (200, 10, 1, 1920, 0, vec![2]));

        // Turn the laptop panel sideways: HDMI stays beside it.
        let before = configs.clone();
        configs[0].rotation = Rotation::Left;
        reflow(&before, &mut configs, 0);
        assert_eq!((configs[1].x, configs[1].y), (1080, 0));
        let plan = super::plan(&snapshot, &configs).unwrap();
        assert_eq!((plan.screen.0, plan.screen.1), (3000, 1920));

        // Mirroring shares one CRTC.
        place(&mut configs, 1, Placement::SameAs, 0);
        configs[0].rotation = Rotation::Normal;
        configs[1].rotation = Rotation::Normal;
        let plan = super::plan(&snapshot, &configs).unwrap();
        assert_eq!(plan.crtcs.len(), 1);
        assert_eq!(plan.crtcs[0].5, vec![1, 2]);
        assert_eq!(plan.off, vec![200]);
        assert_eq!(placement_of(&configs, 1, 0), Some(Placement::SameAs));

        // Above, then normalised to start at 0,0.
        place(&mut configs, 1, Placement::Above, 0);
        assert_eq!((configs[1].y, configs[0].y), (0, 1080));
        assert_eq!(placement_of(&configs, 1, 0), Some(Placement::Above));

        // Refused: all off, unknown size, unsupported rotation, too big.
        let mut off = configs.clone();
        off.iter_mut().for_each(|c| c.enabled = false);
        assert!(super::plan(&snapshot, &off).is_err());
        let mut odd = configs.clone();
        odd[0].width = 640;
        assert!(super::plan(&snapshot, &odd).is_err());
        let mut small = snapshot.clone();
        small.outputs[0].rotations = vec![Rotation::Normal];
        let mut turned = configs.clone();
        turned[0].rotation = Rotation::Right;
        assert!(super::plan(&small, &turned).is_err());
        small.max_size = (2048, 2048);
        assert!(super::plan(&small, &configs).is_err());
        let mut unknown = configs;
        unknown[0].name = "DP-9".into();
        assert!(super::plan(&snapshot, &unknown).is_err());
    }

    #[test]
    fn layouts_are_saved_and_loaded_and_bad_lines_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slopos/displays.conf");
        let snapshot = two_screens();
        let mut configs = current_configs(&snapshot);
        configs[1].rotation = Rotation::Inverted;
        save(&path, &configs).unwrap();
        assert_eq!(load(&path), configs);
        assert_eq!(saved_for(&snapshot, &load(&path)), Some(configs.clone()));
        let mut one = snapshot.clone();
        one.outputs.pop();
        assert_eq!(saved_for(&one, &configs), None);
        std::fs::write(
            &path,
            "garbage\noutput=X enabled=1 mode=0x10@60000 rotation=normal position=0,0 primary=1\n\
             output=Y enabled=1 mode=10x10@60000 rotation=sideways position=0,0 primary=1\n\
             output=Z enabled=1 mode=10x10@60000 rotation=left position=-1,0 primary=0\n\
             output=OK enabled=0 mode=10x10@1 rotation=right position=5,6 primary=0\n",
        )
        .unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "OK");
        assert_eq!(loaded[0].extent(), (10, 10));
        assert!(load(&dir.path().join("missing")).is_empty());
    }

    #[test]
    #[ignore = "needs an X server with RandR (run under Xvfb)"]
    fn reads_the_live_x_server() {
        let client = DisplayClient::connect().unwrap();
        let snapshot = client.snapshot().unwrap();
        assert!(!snapshot.outputs.is_empty());
        let configs = current_configs(&snapshot);
        // Re-applying the current layout is accepted and changes nothing.
        let after = client.apply(&configs).unwrap();
        assert_eq!(current_configs(&after), configs);
    }
}
