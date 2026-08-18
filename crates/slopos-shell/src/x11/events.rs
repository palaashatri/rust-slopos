//! Background event loop thread and typed X11 event dispatch.

use super::connection::X11Connection;
use super::ewmh;
use super::monitors::{self, MonitorModel};
use super::pointer;
use super::windows;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as RandrConnectionExt, NotifyMask};
use x11rb::protocol::xproto::Window;
use x11rb::protocol::Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X11Event {
    ActiveWindowChanged {
        window_id: Option<u32>,
        title: String,
        class_name: String,
        instance_name: String,
        is_fullscreen: bool,
        is_maximized: bool,
    },
    WindowStateChanged {
        window_id: u32,
        is_fullscreen: bool,
        is_maximized: bool,
    },
    WindowTitleChanged {
        window_id: u32,
        title: String,
    },
    DesktopChanged {
        desktop: u32,
    },
    MonitorsChanged {
        model: MonitorModel,
    },
    PointerEdgeChanged {
        near_bottom: bool,
    },
}

pub struct X11EventBus {
    running: Arc<AtomicBool>,
}

impl X11EventBus {
    pub fn start<F>(mut callback: F) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnMut(X11Event) + Send + 'static,
    {
        let connection = X11Connection::connect()?;
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        thread::Builder::new()
            .name("slopos-x11-events".to_string())
            .spawn(move || {
                let conn = connection.raw_conn();
                let root = connection.root();
                let atoms = *connection.atoms();

                // Select RandR screen change notify
                let _ = conn.randr_select_input(
                    root,
                    NotifyMask::SCREEN_CHANGE | NotifyMask::CRTC_CHANGE | NotifyMask::OUTPUT_CHANGE,
                );
                let _ = conn.flush();

                // Emit initial monitor model
                let mut current_monitors = monitors::query_monitors(conn, root);
                callback(X11Event::MonitorsChanged {
                    model: current_monitors.clone(),
                });

                // Emit initial active window state
                let mut current_active: Option<Window> =
                    ewmh::get_active_window(conn, root, &atoms);
                if let Some(win) = current_active {
                    let _ = connection.subscribe_window_events(win);
                    let info = windows::get_window_info(conn, win, &atoms);
                    callback(X11Event::ActiveWindowChanged {
                        window_id: Some(win),
                        title: info.title,
                        class_name: info.class_name,
                        instance_name: info.instance_name,
                        is_fullscreen: info.is_fullscreen,
                        is_maximized: info.is_maximized,
                    });
                } else {
                    callback(X11Event::ActiveWindowChanged {
                        window_id: None,
                        title: "SLOPOS Desktop".to_string(),
                        class_name: String::new(),
                        instance_name: String::new(),
                        is_fullscreen: false,
                        is_maximized: false,
                    });
                }

                let mut last_pointer_check = Instant::now();
                let mut last_near_bottom = false;

                while running_clone.load(Ordering::Relaxed) {
                    let event = match conn.poll_for_event() {
                        Ok(Some(event)) => Some(event),
                        Ok(None) => None,
                        Err(error) => {
                            log::error!("X11 connection error in event loop: {error}");
                            break;
                        }
                    };

                    if let Some(event) = event {
                        match event {
                            Event::PropertyNotify(pn) => {
                                if pn.window == root {
                                    if pn.atom == atoms.net_active_window {
                                        let new_active =
                                            ewmh::get_active_window(conn, root, &atoms);
                                        if new_active != current_active {
                                            current_active = new_active;
                                            if let Some(win) = new_active {
                                                let _ = connection.subscribe_window_events(win);
                                                let info =
                                                    windows::get_window_info(conn, win, &atoms);
                                                callback(X11Event::ActiveWindowChanged {
                                                    window_id: Some(win),
                                                    title: info.title,
                                                    class_name: info.class_name,
                                                    instance_name: info.instance_name,
                                                    is_fullscreen: info.is_fullscreen,
                                                    is_maximized: info.is_maximized,
                                                });
                                            } else {
                                                callback(X11Event::ActiveWindowChanged {
                                                    window_id: None,
                                                    title: "SLOPOS Desktop".to_string(),
                                                    class_name: String::new(),
                                                    instance_name: String::new(),
                                                    is_fullscreen: false,
                                                    is_maximized: false,
                                                });
                                            }
                                        }
                                    } else if pn.atom == atoms.net_current_desktop {
                                        if let Some(desktop) =
                                            ewmh::get_current_desktop(conn, root, &atoms)
                                        {
                                            callback(X11Event::DesktopChanged { desktop });
                                        }
                                    }
                                } else if Some(pn.window) == current_active {
                                    if pn.atom == atoms.net_wm_state {
                                        let (is_fullscreen, is_maximized) =
                                            windows::get_window_state(conn, pn.window, &atoms);
                                        callback(X11Event::WindowStateChanged {
                                            window_id: pn.window,
                                            is_fullscreen,
                                            is_maximized,
                                        });
                                    } else if pn.atom == atoms.net_wm_name
                                        || pn.atom == atoms.wm_name
                                    {
                                        if let Some(title) =
                                            windows::get_window_title(conn, pn.window, &atoms)
                                        {
                                            callback(X11Event::WindowTitleChanged {
                                                window_id: pn.window,
                                                title,
                                            });
                                        }
                                    }
                                }
                            }
                            Event::DestroyNotify(dn) => {
                                if Some(dn.window) == current_active {
                                    current_active = None;
                                    callback(X11Event::ActiveWindowChanged {
                                        window_id: None,
                                        title: "SLOPOS Desktop".to_string(),
                                        class_name: String::new(),
                                        instance_name: String::new(),
                                        is_fullscreen: false,
                                        is_maximized: false,
                                    });
                                }
                            }
                            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => {
                                current_monitors = monitors::query_monitors(conn, root);
                                callback(X11Event::MonitorsChanged {
                                    model: current_monitors.clone(),
                                });
                            }
                            _ => {}
                        }
                    }

                    // Background pointer edge transition check every 50ms without blocking GTK
                    if last_pointer_check.elapsed() >= Duration::from_millis(50) {
                        last_pointer_check = Instant::now();
                        let screen_height = current_monitors
                            .primary()
                            .map(|m| m.scaled_height())
                            .unwrap_or(800);
                        let near_bottom = pointer::is_pointer_near_bottom(
                            conn,
                            root,
                            screen_height,
                            last_near_bottom,
                        );
                        if near_bottom != last_near_bottom {
                            last_near_bottom = near_bottom;
                            callback(X11Event::PointerEdgeChanged { near_bottom });
                        }
                    }

                    thread::sleep(Duration::from_millis(16));
                }
            })?;

        Ok(Self { running })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
