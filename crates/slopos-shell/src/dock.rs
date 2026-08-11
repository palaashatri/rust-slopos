//! Bottom Application Dock
//! Macintosh-inspired bottom task dock displaying pinned launchers and active open window tasks.

use crate::launcher::Launcher;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, IconSize, Image, Orientation, Window, WindowPosition, WindowType,
};
use std::process::Command;
use std::rc::Rc;

pub struct Dock {
    _window: Window,
    _dock_box: GtkBox,
}

impl Dock {
    pub fn new(launcher: Rc<Launcher>) -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Dock");
        window.set_default_size(500, 54);
        window.set_position(WindowPosition::Center);
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);

        let dock_box = GtkBox::new(Orientation::Horizontal, 8);
        dock_box.set_margin_start(12);
        dock_box.set_margin_end(12);
        dock_box.set_margin_top(4);
        dock_box.set_margin_bottom(4);

        // Add Pinned Quick Launch Items
        add_dock_item(&dock_box, "system-search", "Spotlight Launcher", {
            let l = launcher.clone();
            move || l.toggle()
        });
        add_dock_item(&dock_box, "system-file-manager", "Files (PCManFM)", || {
            let _ = Command::new("pcmanfm").spawn();
        });
        add_dock_item(&dock_box, "utilities-terminal", "Terminal", || {
            let _ = Command::new("xfce4-terminal").spawn();
        });
        add_dock_item(&dock_box, "text-editor", "Text Editor (Mousepad)", || {
            let _ = Command::new("mousepad").spawn();
        });
        add_dock_item(&dock_box, "web-browser", "Web Browser (Firefox)", || {
            let _ = Command::new("firefox").spawn();
        });
        add_dock_item(&dock_box, "system-software-install", "AppImage Catalogue", || {
            let _ = Command::new("slopos-catalogue").spawn();
        });
        add_dock_item(&dock_box, "preferences-system", "System Settings", || {
            let _ = Command::new("slopos-settings").spawn();
        });
        add_dock_item(&dock_box, "user-trash", "Trash", || {
            let _ = Command::new("pcmanfm").arg("trash:///").spawn();
        });

        window.add(&dock_box);
        window.show_all();

        Rc::new(Self {
            _window: window,
            _dock_box: dock_box,
        })
    }
}

fn add_dock_item<F>(dock_box: &GtkBox, icon: &str, tooltip: &str, on_click: F)
where
    F: Fn() + 'static,
{
    let btn = Button::new();
    let img = Image::from_icon_name(Some(icon), IconSize::Dnd);
    btn.set_image(Some(&img));
    btn.set_tooltip_text(Some(tooltip));
    btn.set_relief(gtk::ReliefStyle::None);

    btn.connect_clicked(move |_| {
        on_click();
    });

    dock_box.pack_start(&btn, false, false, 0);
}
