//! Background event loop thread and typed X11 event dispatch.

use super::connection::X11Connection;
use super::ewmh;
use super::monitors::{self, MonitorModel};
use super::windows;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as RandrConnectionExt, NotifyMask};
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt as XprotoConnectionExt, CreateWindowAux, EventMask, Window,
    WindowClass,
};
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
    ClientListChanged {
        windows: Vec<super::windows::WindowInfo>,
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

                // Query initial monitors
                let mut current_monitors = monitors::query_monitors(conn, root);
                callback(X11Event::MonitorsChanged {
                    model: current_monitors.clone(),
                });

                // Create an X11 InputOnly edge trigger window along the primary monitor's bottom edge.
                // This eliminates pointer polling entirely; the X11 server natively sends EnterNotify
                // and LeaveNotify events when the mouse crosses the edge.
                let trigger_win = conn.generate_id().unwrap_or(0);
                if trigger_win != 0 {
                    let (tx, ty, tw, th) = edge_trigger_geometry(&current_monitors);
                    let aux = CreateWindowAux::new()
                        .event_mask(EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW)
                        .override_redirect(1);
                    if conn
                        .create_window(
                            0,
                            trigger_win,
                            root,
                            tx as i16,
                            ty as i16,
                            tw as u16,
                            th as u16,
                            0,
                            WindowClass::INPUT_ONLY,
                            0,
                            &aux,
                        )
                        .is_ok()
                    {
                        let _ = conn.map_window(trigger_win);
                    }
                }
                let _ = conn.flush();

                // Emit initial active window state and client list
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
                callback(X11Event::ClientListChanged {
                    windows: fetch_client_list(conn, root, &atoms),
                });

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
                            // Native X11 event-driven pointer edge detection
                            Event::EnterNotify(en) => {
                                if trigger_win != 0
                                    && (en.event == trigger_win || en.child == trigger_win)
                                {
                                    callback(X11Event::PointerEdgeChanged { near_bottom: true });
                                }
                            }
                            Event::LeaveNotify(ln) => {
                                if trigger_win != 0
                                    && (ln.event == trigger_win || ln.child == trigger_win)
                                {
                                    callback(X11Event::PointerEdgeChanged { near_bottom: false });
                                }
                            }
                            Event::PropertyNotify(pn) => {
                                if pn.window == root {
                                    if pn.atom == atoms.net_client_list {
                                        callback(X11Event::ClientListChanged {
                                            windows: fetch_client_list(conn, root, &atoms),
                                        });
                                    } else if pn.atom == atoms.net_active_window {
                                        callback(X11Event::ClientListChanged {
                                            windows: fetch_client_list(conn, root, &atoms),
                                        });
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
                                if trigger_win != 0 {
                                    let (tx, ty, tw, th) = edge_trigger_geometry(&current_monitors);
                                    let aux = ConfigureWindowAux::new()
                                        .x(tx)
                                        .y(ty)
                                        .width(tw as u32)
                                        .height(th as u32);
                                    let _ = conn.configure_window(trigger_win, &aux);
                                    let _ = conn.flush();
                                }
                                callback(X11Event::MonitorsChanged {
                                    model: current_monitors.clone(),
                                });
                            }
                            _ => {}
                        }
                    }

                    thread::sleep(Duration::from_millis(16));
                }

                if trigger_win != 0 {
                    let _ = conn.destroy_window(trigger_win);
                    let _ = conn.flush();
                }
            })?;

        Ok(Self { running })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn edge_trigger_geometry(monitors: &MonitorModel) -> (i32, i32, i32, i32) {
    if let Some(primary) = monitors.primary() {
        (
            primary.root_left(),
            (primary.root_bottom() - 3).max(0),
            primary.width.max(100),
            3,
        )
    } else {
        (0, 797, 1280, 3)
    }
}

fn fetch_client_list(
    conn: &x11rb::rust_connection::RustConnection,
    root: Window,
    atoms: &super::atoms::Atoms,
) -> Vec<windows::WindowInfo> {
    let clients = ewmh::get_client_list(conn, root, atoms);
    let mut infos = Vec::new();
    for win in clients {
        let info = windows::get_window_info(conn, win, atoms);
        if !windows::is_shell_surface(&info.title, &info.class_name) {
            infos.push(info);
        }
    }
    infos
}
