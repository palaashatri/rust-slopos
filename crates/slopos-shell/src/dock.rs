//! Compact SLOPOS Platinum Application Strip.
//!
//! Event-driven visibility, configurable placement (bottom/left/right),
//! orientation (horizontal/vertical), alignment (center/start/end),
//! application pinning, and window dodge without subprocess polling.

use crate::launcher::Launcher;
use crate::services::session;
use crate::x11::{Monitor, MonitorModel, X11Event};
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, IconSize, Image, Label, Menu, MenuItem, Orientation, Separator,
    SeparatorMenuItem, Window, WindowType,
};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockPosition {
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockAlignment {
    Center,
    Start,
    End,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PinnedItem {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub fallback_icon: String,
    pub exec: String,
    pub args: Vec<String>,
}

pub struct Dock {
    window: Window,
    launcher: Rc<Launcher>,
    is_active_fullscreen: Cell<bool>,
    is_active_maximized: Cell<bool>,
    primary_monitor: RefCell<Option<Monitor>>,
    container_box: RefCell<GtkBox>,
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

        window.set_decorated(false);
        window.set_type_hint(gdk::WindowTypeHint::Dock);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        set_accessible_name(&window, "SLOPOS application strip");

        let pos = current_dock_position();
        let box_orientation = match pos {
            DockPosition::Bottom => Orientation::Horizontal,
            DockPosition::Left | DockPosition::Right => Orientation::Vertical,
        };
        let dock_box = GtkBox::new(box_orientation, 3);
        dock_box.style_context().add_class("slopos-dock-container");
        dock_box.set_hexpand(true);
        dock_box.set_vexpand(true);

        window.add(&dock_box);

        let dock = Rc::new(Self {
            window,
            launcher,
            is_active_fullscreen: Cell::new(false),
            is_active_maximized: Cell::new(false),
            primary_monitor: RefCell::new(None),
            container_box: RefCell::new(dock_box),
        });

        dock.rebuild_items();
        dock.reposition();
        dock.window.show_all();

        dock
    }

    pub fn rebuild_items(&self) {
        let pos = current_dock_position();
        let container = self.container_box.borrow();
        for child in container.children() {
            container.remove(&child);
        }

        let sep_orientation = match pos {
            DockPosition::Bottom => Orientation::Vertical,
            DockPosition::Left | DockPosition::Right => Orientation::Horizontal,
        };

        // Apps label
        let strip_label = Label::new(Some("Apps"));
        strip_label.style_context().add_class("slopos-dock-label");
        strip_label.set_xalign(0.5);
        strip_label.set_yalign(0.5);
        set_accessible_name(&strip_label, "Application launchers");
        container.pack_start(&strip_label, false, false, 2);

        let label_separator = Separator::new(sep_orientation);
        label_separator
            .style_context()
            .add_class("slopos-dock-separator");
        container.pack_start(&label_separator, false, false, 1);

        // Search Action
        add_action_item(
            &container,
            "search.svg",
            "system-search-symbolic",
            "Search applications (Super+Space)",
            {
                let launcher = self.launcher.clone();
                move || launcher.toggle()
            },
        );

        // Pinned Items
        let items = load_pinned_items();
        for item in &items {
            add_pinned_dock_item(&container, item);
        }

        // Separator before Trash
        let end_separator = Separator::new(sep_orientation);
        end_separator
            .style_context()
            .add_class("slopos-dock-separator");
        container.pack_start(&end_separator, false, false, 1);

        // Trash
        add_trash_item(&container);

        container.show_all();
    }

    pub fn reposition(&self) {
        let pos = current_dock_position();
        let align = current_dock_alignment();
        let (screen_width, screen_height, mon_x, mon_y) =
            if let Some(ref primary) = *self.primary_monitor.borrow() {
                (
                    primary.gdk_width(),
                    primary.gdk_height(),
                    primary.gdk_x(),
                    primary.gdk_y(),
                )
            } else {
                let (w, h) = screen_geometry();
                (w, h, 0, 0)
            };

        let items_count = load_pinned_items().len() + 3; // search + items + trash + label
        match pos {
            DockPosition::Bottom => {
                let width = (items_count as i32 * 46 + 60).min(screen_width - 24);
                let height = 54;
                self.window.set_default_size(width, height);
                let x = match align {
                    DockAlignment::Center => mon_x + (screen_width - width).max(0) / 2,
                    DockAlignment::Start => mon_x + 12,
                    DockAlignment::End => mon_x + screen_width - width - 12,
                };
                let y = mon_y + (screen_height - height - 6).max(28);
                self.window.move_(x, y);
            }
            DockPosition::Left => {
                let width = 54;
                let height = (items_count as i32 * 46 + 60).min(screen_height - 60);
                self.window.set_default_size(width, height);
                let x = mon_x + 6;
                let y = match align {
                    DockAlignment::Center => mon_y + (screen_height - height).max(0) / 2,
                    DockAlignment::Start => mon_y + 32,
                    DockAlignment::End => mon_y + screen_height - height - 12,
                };
                self.window.move_(x, y);
            }
            DockPosition::Right => {
                let width = 54;
                let height = (items_count as i32 * 46 + 60).min(screen_height - 60);
                self.window.set_default_size(width, height);
                let x = mon_x + screen_width - width - 6;
                let y = match align {
                    DockAlignment::Center => mon_y + (screen_height - height).max(0) / 2,
                    DockAlignment::Start => mon_y + 32,
                    DockAlignment::End => mon_y + screen_height - height - 12,
                };
                self.window.move_(x, y);
            }
        }
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
        let should_hide = self.is_active_fullscreen.get()
            || (is_dock_dodge_enabled() && self.is_active_maximized.get());
        if should_hide {
            if self.window.is_visible() {
                self.window.set_visible(false);
            }
        } else if !self.window.is_visible() {
            self.window.set_visible(true);
            self.window.set_keep_above(true);
        }
    }

    fn handle_pointer_edge(&self, near_edge: bool) {
        if self.is_active_fullscreen.get() {
            return;
        }
        if !is_dock_dodge_enabled() || !self.is_active_maximized.get() {
            return;
        }

        let is_visible = self.window.is_visible();
        if near_edge && !is_visible {
            self.reposition();
            self.window.set_keep_above(true);
            self.window.set_visible(true);
            self.window.present();
        } else if !near_edge && is_visible {
            self.window.set_visible(false);
        }
    }

    fn reposition_for_monitors(&self, model: &MonitorModel) {
        if let Some(primary) = model.primary() {
            *self.primary_monitor.borrow_mut() = Some(primary.clone());
            self.reposition();
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

fn add_pinned_dock_item(dock: &GtkBox, item: &PinnedItem) {
    let button = dock_button(&item.icon, &item.fallback_icon, &item.name);
    let exec = item.exec.clone();
    let args = item.args.clone();
    let item_id = item.id.clone();
    let item_name = item.name.clone();

    button.connect_button_press_event(move |btn, event| {
        if event.button() == 3 {
            // Right-click context menu
            let menu = Menu::new();
            let open_item = MenuItem::with_label(&format!("Open {}", item_name));
            let exec_c = exec.clone();
            let args_c = args.clone();
            open_item.connect_activate(move |_| {
                if let Some(resolved) = session::resolve_program(&exec_c) {
                    let _ = Command::new(resolved).args(&args_c).spawn();
                }
            });
            menu.append(&open_item);

            let unpin_item = MenuItem::with_label("Remove from Dock");
            let unpin_id = item_id.clone();
            unpin_item.connect_activate(move |_| {
                unpin_application(&unpin_id);
            });
            menu.append(&unpin_item);

            menu.append(&SeparatorMenuItem::new());
            let pref_item = MenuItem::with_label("Desktop & Dock Settings…");
            pref_item.connect_activate(|_| {
                let _ = Command::new("slopos-settings").arg("--desktop").spawn();
            });
            menu.append(&pref_item);

            menu.show_all();
            menu.popup_at_widget(
                btn,
                gdk::Gravity::SouthWest,
                gdk::Gravity::NorthWest,
                Some(event),
            );
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

    let exec_launch = item.exec.clone();
    let args_launch = item.args.clone();
    button.connect_clicked(move |_| {
        if let Some(resolved) = session::resolve_program(&exec_launch) {
            if let Err(error) = Command::new(&resolved).args(&args_launch).spawn() {
                log::warn!("Failed to launch {}: {error}", resolved.display());
            }
        } else if let Ok(child) = Command::new(&exec_launch).args(&args_launch).spawn() {
            log::info!("Spawned {}", exec_launch);
            let _ = child.id();
        }
    });

    dock.pack_start(&button, false, false, 0);
}

fn add_trash_item(dock: &GtkBox) {
    let button = dock_button("trash.svg", "user-trash-symbolic", "Trash");
    button.connect_clicked(|_| {
        if let Some(pcmanfm) = session::resolve_program("pcmanfm") {
            let _ = Command::new(pcmanfm).arg("trash:///").spawn();
        }
    });
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

pub fn default_pinned_items() -> Vec<PinnedItem> {
    vec![
        PinnedItem {
            id: "pcmanfm".into(),
            name: "Files".into(),
            icon: "folder.svg".into(),
            fallback_icon: "folder-symbolic".into(),
            exec: "pcmanfm".into(),
            args: vec![],
        },
        PinnedItem {
            id: "terminal".into(),
            name: "Terminal".into(),
            icon: "terminal.svg".into(),
            fallback_icon: "utilities-terminal-symbolic".into(),
            exec: "xfce4-terminal".into(),
            args: vec![],
        },
        PinnedItem {
            id: "textedit".into(),
            name: "Text Editor".into(),
            icon: "textedit.svg".into(),
            fallback_icon: "accessories-text-editor-symbolic".into(),
            exec: "mousepad".into(),
            args: vec![],
        },
        PinnedItem {
            id: "browser".into(),
            name: "Web Browser".into(),
            icon: "browser.svg".into(),
            fallback_icon: "web-browser-symbolic".into(),
            exec: "firefox".into(),
            args: vec![],
        },
        PinnedItem {
            id: "games".into(),
            name: "Games".into(),
            icon: "game.svg".into(),
            fallback_icon: "applications-games-symbolic".into(),
            exec: "chocolate-doom".into(),
            args: vec![],
        },
        PinnedItem {
            id: "catalogue".into(),
            name: "Software Catalogue".into(),
            icon: "software.svg".into(),
            fallback_icon: "system-software-install-symbolic".into(),
            exec: "slopos-catalogue".into(),
            args: vec![],
        },
        PinnedItem {
            id: "settings".into(),
            name: "System Settings".into(),
            icon: "settings.svg".into(),
            fallback_icon: "preferences-system-symbolic".into(),
            exec: "slopos-settings".into(),
            args: vec![],
        },
    ]
}

pub fn load_pinned_items() -> Vec<PinnedItem> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let path = config_home.join("slopos-i/dock_pinned.json");
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(items) = serde_json::from_str::<Vec<PinnedItem>>(&content) {
                return items;
            }
        }
    }
    default_pinned_items()
}

pub fn save_pinned_items(items: &[PinnedItem]) {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let dir = config_home.join("slopos-i");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("dock_pinned.json");
        if let Ok(json) = serde_json::to_string_pretty(items) {
            let _ = fs::write(path, json);
        }
    }
}

pub fn pin_application(item: PinnedItem) {
    let mut items = load_pinned_items();
    if !items.iter().any(|i| i.id == item.id) {
        items.push(item);
        save_pinned_items(&items);
    }
}

pub fn unpin_application(id: &str) {
    let mut items = load_pinned_items();
    items.retain(|i| i.id != id);
    save_pinned_items(&items);
}

pub fn current_dock_position() -> DockPosition {
    if let Ok(pos) = env::var("SLOPOS_DOCK_POSITION") {
        let p = pos.trim().to_ascii_lowercase();
        if p == "left" {
            return DockPosition::Left;
        }
        if p == "right" {
            return DockPosition::Right;
        }
        if p == "bottom" {
            return DockPosition::Bottom;
        }
    }
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(val) = fs::read_to_string(config_home.join("slopos-i/dock_position")) {
            let v = val.trim().to_ascii_lowercase();
            if v == "left" {
                return DockPosition::Left;
            }
            if v == "right" {
                return DockPosition::Right;
            }
        }
    }
    DockPosition::Bottom
}

pub fn current_dock_alignment() -> DockAlignment {
    if let Ok(align) = env::var("SLOPOS_DOCK_ALIGNMENT") {
        let a = align.trim().to_ascii_lowercase();
        if a == "start" || a == "left" || a == "top" {
            return DockAlignment::Start;
        }
        if a == "end" || a == "right" || a == "bottom" {
            return DockAlignment::End;
        }
    }
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(val) = fs::read_to_string(config_home.join("slopos-i/dock_alignment")) {
            let a = val.trim().to_ascii_lowercase();
            if a == "start" || a == "left" || a == "top" {
                return DockAlignment::Start;
            }
            if a == "end" || a == "right" || a == "bottom" {
                return DockAlignment::End;
            }
        }
    }
    DockAlignment::Center
}

fn is_dock_dodge_enabled() -> bool {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        let flag_file = config_home.join("slopos-i/dock_dodge");
        if let Ok(content) = fs::read_to_string(flag_file) {
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
