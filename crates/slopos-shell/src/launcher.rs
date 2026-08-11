//! Spotlight Application Search Launcher
//! Triggered by Super+Space or SLOPOS Search button.

use crate::app_finder::{scan_desktop_apps, DesktopApp};
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Entry, IconSize, Image, Label, ListBox, ListBoxRow, Orientation, PolicyType,
    ScrolledWindow, Window, WindowPosition, WindowType,
};
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;

pub struct Launcher {
    window: Window,
    search_entry: Entry,
    list_box: ListBox,
    all_apps: Rc<RefCell<Vec<DesktopApp>>>,
}

impl Launcher {
    pub fn new() -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Spotlight Launcher");
        window.set_default_size(560, 380);
        window.set_position(WindowPosition::Center);
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);

        let main_box = GtkBox::new(Orientation::Vertical, 8);
        main_box.set_margin_start(16);
        main_box.set_margin_end(16);
        main_box.set_margin_top(16);
        main_box.set_margin_bottom(16);

        // Search bar
        let search_entry = Entry::new();
        search_entry.set_placeholder_text(Some("Search applications and AppImages..."));
        search_entry.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, Some("system-search"));
        main_box.pack_start(&search_entry, false, false, 0);

        // App List
        let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(PolicyType::Never, PolicyType::Automatic);

        let list_box = ListBox::new();
        scroll.add(&list_box);
        main_box.pack_start(&scroll, true, true, 0);

        window.add(&main_box);

        let all_apps = Rc::new(RefCell::new(scan_desktop_apps()));

        let launcher = Rc::new(Self {
            window,
            search_entry,
            list_box,
            all_apps,
        });

        launcher.setup_events();
        launcher
    }

    fn setup_events(self: &Rc<Self>) {
        let l = self.clone();

        // Refresh search results on text change
        self.search_entry.connect_changed(move |entry| {
            let query = entry.text().to_lowercase();
            l.filter_apps(&query);
        });

        // Launch top item on Enter
        let l2 = self.clone();
        self.search_entry.connect_activate(move |_| {
            l2.launch_selected_or_first();
        });

        // Close on focus loss
        self.window.connect_focus_out_event(move |win, _| {
            win.hide();
            glib::Propagation::Proceed
        });

        // Close on Escape
        self.window.connect_key_press_event(move |win, event| {
            if event.keyval() == gdk::keys::constants::Escape {
                win.hide();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }

    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.window.hide();
        } else {
            self.show();
        }
    }

    pub fn show(&self) {
        // Rescan applications
        *self.all_apps.borrow_mut() = scan_desktop_apps();
        self.search_entry.set_text("");
        self.filter_apps("");
        self.window.show_all();
        self.search_entry.grab_focus();
    }

    fn filter_apps(&self, query: &str) {
        // Clear list
        for child in self.list_box.children() {
            self.list_box.remove(&child);
        }

        let apps = self.all_apps.borrow();
        for app in apps.iter() {
            if query.is_empty()
                || app.name.to_lowercase().contains(query)
                || app.comment.to_lowercase().contains(query)
                || app.exec.to_lowercase().contains(query)
            {
                let row = ListBoxRow::new();
                let hbox = GtkBox::new(Orientation::Horizontal, 12);
                hbox.set_margin_start(8);
                hbox.set_margin_end(8);
                hbox.set_margin_top(6);
                hbox.set_margin_bottom(6);

                let img = Image::from_icon_name(Some(&app.icon), IconSize::Dnd);
                hbox.pack_start(&img, false, false, 0);

                let vbox = GtkBox::new(Orientation::Vertical, 2);
                let title = Label::new(Some(&app.name));
                title.set_xalign(0.0);
                vbox.pack_start(&title, false, false, 0);

                if !app.comment.is_empty() {
                    let desc = Label::new(Some(&app.comment));
                    desc.set_xalign(0.0);
                    vbox.pack_start(&desc, false, false, 0);
                }

                hbox.pack_start(&vbox, true, true, 0);
                row.add(&hbox);

                let exec_cmd = app.exec.clone();
                let win = self.window.clone();
                row.connect_activate(move |_| {
                    spawn_app(&exec_cmd);
                    win.hide();
                });

                self.list_box.add(&row);
            }
        }
        self.list_box.show_all();
    }

    fn launch_selected_or_first(&self) {
        if let Some(row) = self.list_box.selected_row() {
            row.activate();
        } else if let Some(first) = self.list_box.children().first() {
            if let Ok(row) = first.clone().downcast::<ListBoxRow>() {
                row.activate();
            }
        }
    }
}

fn spawn_app(cmd: &str) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if let Some((first, rest)) = parts.split_first() {
        let _ = Command::new(first).args(rest).spawn();
    }
}
