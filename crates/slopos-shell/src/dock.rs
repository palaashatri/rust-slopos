//! Compact SLOPOS Platinum Application Strip.
//!
//! Event-driven visibility, window dodge, and launch management without subprocess polling.

use crate::launcher::Launcher;
use crate::services::session;
use crate::x11::{Monitor, MonitorModel, X11Event};
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, IconSize, Image, Label, Orientation, Separator, Window, WindowPosition,
    WindowType,
};
use std::cell::{Cell, RefCell};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

pub struct Dock {
    window: Window,
    is_active_fullscreen: Cell<bool>,
    is_active_maximized: Cell<bool>,
    primary_monitor: RefCell<Option<Monitor>>,
}

#[derive(Clone, Copy)]
struct LaunchSpec {
    program: &'static str,
    args: &'static [&'static str],
}

impl Dock {
    pub fn new(launcher: Rc<Launcher>) -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Application Strip");
        window.set_app_paintable(true);
        if let Some(screen) = gtk::prelude::GtkWindowExt::screen(&window) {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
        }
        window.style_context().add_class("slopos-dock-window");
        window.connect_draw(|_, cr| {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
            cr.set_operator(gtk::cairo::Operator::Source);
            let _ = cr.paint();
            glib::Propagation::Proceed
        });
        let (screen_width, screen_height) = screen_geometry();
        let width = 540;
        let height = 54;
        window.set_default_size(width, height);
        window.set_position(WindowPosition::None);
        window.move_(
            (screen_width - width).max(0) / 2,
            (screen_height - height - 6).max(28),
        );
        window.set_decorated(false);
        window.set_type_hint(gdk::WindowTypeHint::Dock);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        set_accessible_name(&window, "SLOPOS application strip");

        let dock_box = GtkBox::new(Orientation::Horizontal, 3);
        dock_box.style_context().add_class("slopos-dock-container");
        dock_box.set_hexpand(true);
        dock_box.set_vexpand(true);

        let strip_label = Label::new(Some("Apps"));
        strip_label.style_context().add_class("slopos-dock-label");
        strip_label.set_xalign(0.5);
        strip_label.set_yalign(0.5);
        set_accessible_name(&strip_label, "Application launchers");
        dock_box.pack_start(&strip_label, false, false, 2);

        let label_separator = Separator::new(Orientation::Vertical);
        label_separator
            .style_context()
            .add_class("slopos-dock-separator");
        dock_box.pack_start(&label_separator, false, false, 1);

        add_action_item(
            &dock_box,
            "search.svg",
            "system-search-symbolic",
            "Search applications (Super+Space)",
            {
                let launcher = launcher.clone();
                move || launcher.toggle()
            },
        );
        add_launch_item(
            &dock_box,
            "folder.svg",
            "folder-symbolic",
            "Files",
            &[
                LaunchSpec {
                    program: "pcmanfm",
                    args: &[],
                },
                LaunchSpec {
                    program: "thunar",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "terminal.svg",
            "utilities-terminal-symbolic",
            "Terminal",
            &[
                LaunchSpec {
                    program: "xfce4-terminal",
                    args: &[],
                },
                LaunchSpec {
                    program: "xterm",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "textedit.svg",
            "accessories-text-editor-symbolic",
            "Text Editor",
            &[
                LaunchSpec {
                    program: "mousepad",
                    args: &[],
                },
                LaunchSpec {
                    program: "xed",
                    args: &[],
                },
                LaunchSpec {
                    program: "gedit",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "browser.svg",
            "web-browser-symbolic",
            "Web Browser",
            &[
                LaunchSpec {
                    program: "start-slopos-browser",
                    args: &[],
                },
                LaunchSpec {
                    program: "firefox",
                    args: &[],
                },
                LaunchSpec {
                    program: "chromium",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "game.svg",
            "applications-games-symbolic",
            "Games",
            &[
                LaunchSpec {
                    program: "chocolate-doom",
                    args: &[],
                },
                LaunchSpec {
                    program: "doom",
                    args: &[],
                },
                LaunchSpec {
                    program: "supertux2",
                    args: &[],
                },
                LaunchSpec {
                    program: "supertux",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "software.svg",
            "system-software-install-symbolic",
            "Software Catalogue",
            &[LaunchSpec {
                program: "slopos-catalogue",
                args: &[],
            }],
        );
        add_launch_item(
            &dock_box,
            "settings.svg",
            "preferences-system-symbolic",
            "System Settings",
            &[LaunchSpec {
                program: "slopos-settings",
                args: &[],
            }],
        );

        let separator = Separator::new(Orientation::Vertical);
        separator.style_context().add_class("slopos-dock-separator");
        dock_box.pack_start(&separator, false, false, 1);

        add_launch_item(
            &dock_box,
            "trash.svg",
            "user-trash-symbolic",
            "Trash",
            &[LaunchSpec {
                program: "pcmanfm",
                args: &["trash:///"],
            }],
        );

        window.add(&dock_box);
        window.show_all();

        Rc::new(Self {
            window,
            is_active_fullscreen: Cell::new(false),
            is_active_maximized: Cell::new(false),
            primary_monitor: RefCell::new(None),
        })
    }

    pub fn handle_x11_event(&self, event: &X11Event) {
        match event {
            X11Event::ActiveWindowChanged {
                is_fullscreen,
                is_maximized,
                ..
            } => {
                self.is_active_fullscreen.set(*is_fullscreen);
                self.is_active_maximized.set(*is_maximized);
                self.update_visibility();
            }
            X11Event::WindowStateChanged {
                is_fullscreen,
                is_maximized,
                ..
            } => {
                self.is_active_fullscreen.set(*is_fullscreen);
                self.is_active_maximized.set(*is_maximized);
                self.update_visibility();
            }
            X11Event::MonitorsChanged { model } => {
                self.reposition_for_monitors(model);
            }
            X11Event::PointerEdgeChanged { near_bottom } => {
                self.handle_pointer_edge(*near_bottom);
            }
            _ => {}
        }
    }

    fn update_visibility(&self) {
        if self.is_active_fullscreen.get() {
            if self.window.is_visible() {
                self.window.set_visible(false);
            }
        } else if is_dock_dodge_enabled() && self.is_active_maximized.get() {
            // In dodge mode with maximized window, start hidden until pointer nears bottom
            if self.window.is_visible() {
                self.window.set_visible(false);
            }
        } else if !self.window.is_visible() {
            self.window.set_visible(true);
            self.window.set_keep_above(true);
        }
    }

    fn handle_pointer_edge(&self, near_bottom: bool) {
        if self.is_active_fullscreen.get() {
            return;
        }
        if !is_dock_dodge_enabled() || !self.is_active_maximized.get() {
            return;
        }

        let is_visible = self.window.is_visible();
        if near_bottom && !is_visible {
            if let Some(primary) = self.primary_monitor.borrow().as_ref() {
                let width = 540;
                let height = 54;
                let x = primary.gdk_x() + (primary.gdk_width() - width).max(0) / 2;
                let y = primary.gdk_y() + (primary.gdk_height() - height - 6).max(28);
                self.window.move_(x, y);
            } else {
                let (sw, sh) = screen_geometry();
                self.window
                    .move_((sw - 540).max(0) / 2, (sh - 54 - 6).max(28));
            }
            self.window.set_keep_above(true);
            self.window.set_visible(true);
            self.window.present();
        } else if !near_bottom && is_visible {
            self.window.set_visible(false);
        }
    }

    fn reposition_for_monitors(&self, model: &MonitorModel) {
        if let Some(primary) = model.primary() {
            *self.primary_monitor.borrow_mut() = Some(primary.clone());
            let width = 540;
            let height = 54;
            let x = primary.gdk_x() + (primary.gdk_width() - width).max(0) / 2;
            let y = primary.gdk_y() + (primary.gdk_height() - height - 6).max(28);
            self.window.move_(x, y);
        }
    }
}

fn add_action_item<F>(
    dock: &GtkBox,
    custom_icon: &str,
    fallback_icon: &str,
    tooltip: &str,
    action: F,
) where
    F: Fn() + 'static,
{
    let button = dock_button(custom_icon, fallback_icon, tooltip);
    button.connect_clicked(move |_| action());
    dock.pack_start(&button, false, false, 0);
}

fn add_launch_item(
    dock: &GtkBox,
    custom_icon: &str,
    fallback_icon: &str,
    tooltip: &str,
    candidates: &[LaunchSpec],
) {
    let button = dock_button(custom_icon, fallback_icon, tooltip);
    let resolved = candidates.iter().find_map(|candidate| {
        session::resolve_program(candidate.program).map(|program| {
            (
                program,
                candidate
                    .args
                    .iter()
                    .map(|argument| (*argument).to_string())
                    .collect::<Vec<_>>(),
            )
        })
    });

    if let Some((program, args)) = resolved {
        button.connect_clicked(move |_| {
            if let Err(error) = Command::new(&program).args(&args).spawn() {
                log::warn!("Failed to launch {}: {error}", program.display());
            }
        });
    } else {
        button.set_sensitive(false);
        button.set_tooltip_text(Some(&format!("{tooltip} (not installed)")));
    }
    dock.pack_start(&button, false, false, 0);
}

fn dock_button(custom_icon: &str, fallback_icon: &str, tooltip: &str) -> Button {
    let button = Button::new();
    button.style_context().add_class("slopos-dock-btn");
    button.set_tooltip_text(Some(tooltip));
    set_accessible_name(&button, tooltip);
    let icon = load_dock_icon(custom_icon, fallback_icon);
    button.set_image(Some(&icon));
    button.set_always_show_image(true);
    button
}

fn load_dock_icon(file_name: &str, fallback: &str) -> Image {
    let mut candidates = Vec::new();
    if let Ok(share) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(format!("{share}/themes/platinum/icons/{file_name}"));
        candidates.push(format!(
            "{share}/slopos-i/themes/platinum/icons/{file_name}"
        ));
    }
    candidates.extend([
        format!("themes/platinum/icons/{file_name}"),
        format!("/usr/local/share/slopos-i/themes/platinum/icons/{file_name}"),
        format!("/usr/share/slopos-i/themes/platinum/icons/{file_name}"),
    ]);
    for path in candidates {
        if Path::new(&path).exists() {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 32, 32, true) {
                return Image::from_pixbuf(Some(&pixbuf));
            }
        }
    }
    Image::from_icon_name(Some(fallback), IconSize::LargeToolbar)
}

fn is_dock_dodge_enabled() -> bool {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let flag_file = config_home.join("slopos-i/dock_dodge");
        if let Ok(content) = std::fs::read_to_string(flag_file) {
            let t = content.trim();
            return t == "1" || t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("yes");
        }
    }
    false
}

fn screen_geometry() -> (i32, i32) {
    gdk::Display::default()
        .and_then(|disp| {
            disp.primary_monitor()
                .or_else(|| disp.monitor(0))
                .map(|mon| {
                    let rect = mon.geometry();
                    (rect.width().max(1), rect.height().max(1))
                })
        })
        .unwrap_or((1280, 800))
}

fn set_accessible_name<W>(widget: &W, name: &str)
where
    W: IsA<gtk::Widget>,
{
    let Some(accessible) = widget.accessible() else {
        return;
    };
    let Ok(accessible) = accessible.downcast::<gtk::atk::Object>() else {
        return;
    };
    accessible.set_name(name);
}
