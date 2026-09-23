//! Typed monitor topology model and RandR integration.

use std::env;
use x11rb::protocol::randr::ConnectionExt as RandrConnectionExt;
use x11rb::protocol::xproto::{ConnectionExt as XprotoConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub output_id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
    pub scale: i32,
}

impl Monitor {
    /// Return the width in GDK scaled coordinate space.
    pub fn scaled_width(&self) -> i32 {
        (self.width / self.scale.max(1)).max(1)
    }

    /// Return the height in GDK scaled coordinate space.
    pub fn scaled_height(&self) -> i32 {
        (self.height / self.scale.max(1)).max(1)
    }

    /// Return the X origin in GDK scaled coordinate space.
    pub fn gdk_x(&self) -> i32 {
        self.x / self.scale.max(1)
    }

    /// Return the Y origin in GDK scaled coordinate space.
    pub fn gdk_y(&self) -> i32 {
        self.y / self.scale.max(1)
    }

    /// Return the width in GDK scaled coordinate space.
    pub fn gdk_width(&self) -> i32 {
        self.scaled_width()
    }

    /// Return the height in GDK scaled coordinate space.
    pub fn gdk_height(&self) -> i32 {
        self.scaled_height()
    }

    /// Return the bottom Y coordinate in root X11 pixel space.
    pub fn root_bottom(&self) -> i32 {
        self.y + self.height
    }

    /// Return the top Y coordinate in root X11 pixel space.
    pub fn root_top(&self) -> i32 {
        self.y
    }

    /// Return the left X coordinate in root X11 pixel space.
    pub fn root_left(&self) -> i32 {
        self.x
    }

    /// Return the right X coordinate in root X11 pixel space.
    pub fn root_right(&self) -> i32 {
        self.x + self.width
    }

    /// Returns true if the root X11 pixel point (px, py) is within this monitor.
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MonitorModel {
    pub monitors: Vec<Monitor>,
    pub primary_index: usize,
    pub total_width: i32,
    pub total_height: i32,
}

impl MonitorModel {
    pub fn primary(&self) -> Option<&Monitor> {
        self.monitors
            .get(self.primary_index)
            .or_else(|| self.monitors.first())
    }

    pub fn monitor_for_point(&self, x: i32, y: i32) -> Option<&Monitor> {
        self.monitors
            .iter()
            .find(|m| m.contains_point(x, y))
            .or_else(|| self.primary())
    }
}

pub fn query_monitors(conn: &RustConnection, root: Window) -> MonitorModel {
    let scale = env::var("GDK_SCALE")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);

    // Try RandR get_screen_resources_current first
    if let Ok(resources_cookie) = conn.randr_get_screen_resources_current(root) {
        if let Ok(resources) = resources_cookie.reply() {
            let primary_output = conn
                .randr_get_output_primary(root)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.output)
                .unwrap_or(0);

            let mut monitors = Vec::new();

            for &output in &resources.outputs {
                let Ok(output_info_cookie) =
                    conn.randr_get_output_info(output, resources.config_timestamp)
                else {
                    continue;
                };
                let Ok(output_info) = output_info_cookie.reply() else {
                    continue;
                };

                if output_info.crtc == 0
                    || output_info.connection != x11rb::protocol::randr::Connection::CONNECTED
                {
                    continue;
                }

                let Ok(crtc_info_cookie) =
                    conn.randr_get_crtc_info(output_info.crtc, resources.config_timestamp)
                else {
                    continue;
                };
                let Ok(crtc_info) = crtc_info_cookie.reply() else {
                    continue;
                };

                let name = String::from_utf8_lossy(&output_info.name).to_string();
                let is_primary =
                    output == primary_output || (primary_output == 0 && monitors.is_empty());

                monitors.push(Monitor {
                    output_id: output,
                    name,
                    x: crtc_info.x as i32,
                    y: crtc_info.y as i32,
                    width: crtc_info.width as i32,
                    height: crtc_info.height as i32,
                    primary: is_primary,
                    scale,
                });
            }

            if !monitors.is_empty() {
                let primary_index = monitors.iter().position(|m| m.primary).unwrap_or(0);
                let total_width = monitors.iter().map(|m| m.x + m.width).max().unwrap_or(1280);
                let total_height = monitors.iter().map(|m| m.y + m.height).max().unwrap_or(800);

                return MonitorModel {
                    monitors,
                    primary_index,
                    total_width,
                    total_height,
                };
            }
        }
    }

    // Fallback to root window geometry
    let (root_width, root_height) = conn
        .get_geometry(root)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|geom| (geom.width as i32, geom.height as i32))
        .unwrap_or((1280, 800));

    let fallback_monitor = Monitor {
        output_id: 1,
        name: "Default".to_string(),
        x: 0,
        y: 0,
        width: root_width,
        height: root_height,
        primary: true,
        scale,
    };

    MonitorModel {
        monitors: vec![fallback_monitor],
        primary_index: 0,
        total_width: root_width,
        total_height: root_height,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn single_monitor_model_geometry() {
        let monitor = Monitor {
            output_id: 1,
            name: "eDP-1".to_string(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            primary: true,
            scale: 1,
        };
        let model = MonitorModel {
            monitors: vec![monitor.clone()],
            primary_index: 0,
            total_width: 1920,
            total_height: 1080,
        };

        assert_eq!(model.primary(), Some(&monitor));
        assert_eq!(monitor.scaled_width(), 1920);
        assert_eq!(monitor.scaled_height(), 1080);
        assert_eq!(monitor.gdk_x(), 0);
        assert_eq!(monitor.gdk_y(), 0);
        assert_eq!(monitor.root_bottom(), 1080);
        assert!(monitor.contains_point(100, 100));
        assert!(!monitor.contains_point(2000, 500));
    }

    #[test]
    fn dual_monitor_horizontal_layout() {
        let left = Monitor {
            output_id: 1,
            name: "HDMI-1".to_string(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            primary: false,
            scale: 1,
        };
        let right = Monitor {
            output_id: 2,
            name: "DP-1".to_string(),
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
            primary: true,
            scale: 2,
        };
        let model = MonitorModel {
            monitors: vec![left.clone(), right.clone()],
            primary_index: 1,
            total_width: 4480,
            total_height: 1440,
        };

        assert_eq!(model.primary(), Some(&right));
        assert_eq!(right.scaled_width(), 1280);
        assert_eq!(right.scaled_height(), 720);
        assert_eq!(right.gdk_x(), 960);
        assert_eq!(right.gdk_y(), 0);
        assert_eq!(right.root_bottom(), 1440);
        assert_eq!(model.monitor_for_point(500, 500), Some(&left));
        assert_eq!(model.monitor_for_point(2500, 500), Some(&right));
    }

    #[test]
    fn vertical_stacked_monitor_layout() {
        let top = Monitor {
            output_id: 1,
            name: "DP-1".to_string(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            primary: false,
            scale: 1,
        };
        let bottom_primary = Monitor {
            output_id: 2,
            name: "eDP-1".to_string(),
            x: 0,
            y: 1080,
            width: 1920,
            height: 1080,
            primary: true,
            scale: 1,
        };
        let model = MonitorModel {
            monitors: vec![top.clone(), bottom_primary.clone()],
            primary_index: 1,
            total_width: 1920,
            total_height: 2160,
        };

        assert_eq!(model.primary(), Some(&bottom_primary));
        assert_eq!(bottom_primary.gdk_x(), 0);
        assert_eq!(bottom_primary.gdk_y(), 1080);
        assert_eq!(bottom_primary.root_top(), 1080);
        assert_eq!(bottom_primary.root_bottom(), 2160);
        assert_eq!(model.monitor_for_point(100, 500), Some(&top));
        assert_eq!(model.monitor_for_point(100, 1500), Some(&bottom_primary));
    }

    #[test]
    fn hidpi_and_non_zero_origin_coordinate_mapping() {
        let hidpi_monitor = Monitor {
            output_id: 1,
            name: "4K-Primary".to_string(),
            x: 1920,
            y: 1080,
            width: 3840,
            height: 2160,
            primary: true,
            scale: 2,
        };

        assert_eq!(hidpi_monitor.gdk_x(), 960);
        assert_eq!(hidpi_monitor.gdk_y(), 540);
        assert_eq!(hidpi_monitor.gdk_width(), 1920);
        assert_eq!(hidpi_monitor.gdk_height(), 1080);
        assert_eq!(hidpi_monitor.root_left(), 1920);
        assert_eq!(hidpi_monitor.root_right(), 5760);
        assert_eq!(hidpi_monitor.root_top(), 1080);
        assert_eq!(hidpi_monitor.root_bottom(), 3240);
    }
}
