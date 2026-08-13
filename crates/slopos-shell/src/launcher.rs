//! SLOPOS application Search palette.

use crate::app_finder::{scan_desktop_apps, DesktopApp};
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Entry, IconSize, Image, Label, ListBox, ListBoxRow, Orientation, PolicyType,
    ScrolledWindow, SelectionMode, Window, WindowPosition, WindowType,
};
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

pub struct Launcher {
    window: Window,
    search_entry: Entry,
    list_box: ListBox,
    status_label: Label,
    all_apps: Rc<RefCell<Vec<DesktopApp>>>,
}

impl Launcher {
    pub fn new() -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Search");
        // Reserve enough vertical space for complete result rows.  The list
        // remains scrollable for larger catalogues, but the canonical palette
        // must not present a visibly clipped row at its default size.
        window.set_default_size(560, 450);
        window.set_position(WindowPosition::Center);
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.style_context().add_class("slopos-launcher-window");
        set_accessible_name(&window, "SLOPOS application search");

        let main_box = GtkBox::new(Orientation::Vertical, 6);
        main_box.style_context().add_class("slopos-launcher");

        let title = Label::new(Some("Find Applications"));
        title.style_context().add_class("slopos-dialog-title");
        title.set_xalign(0.0);
        main_box.pack_start(&title, false, false, 0);

        let search_entry = Entry::new();
        search_entry.set_placeholder_text(Some("Type an application name…"));
        search_entry.set_icon_from_icon_name(
            gtk::EntryIconPosition::Primary,
            Some("system-search-symbolic"),
        );
        search_entry
            .style_context()
            .add_class("slopos-search-entry");
        search_entry.set_tooltip_text(Some("Search installed desktop applications"));
        set_accessible_name(&search_entry, "Application search field");
        main_box.pack_start(&search_entry, false, false, 0);

        let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroll.set_overlay_scrolling(false);
        scroll.set_min_content_height(280);
        scroll.style_context().add_class("slopos-list-frame");

        let list_box = ListBox::new();
        list_box.set_selection_mode(SelectionMode::Single);
        list_box.style_context().add_class("slopos-search-results");
        set_accessible_name(&list_box, "Application search results");
        scroll.add(&list_box);
        main_box.pack_start(&scroll, true, true, 0);

        let status_label = Label::new(Some(""));
        status_label.style_context().add_class("slopos-statusbar");
        status_label.set_xalign(0.0);
        set_accessible_name(&status_label, "Search result status");
        main_box.pack_start(&status_label, false, false, 0);

        window.add(&main_box);
        let all_apps = Rc::new(RefCell::new(scan_desktop_apps()));

        let launcher = Rc::new(Self {
            window,
            search_entry,
            list_box,
            status_label,
            all_apps,
        });
        launcher.setup_events();
        launcher
    }

    fn setup_events(self: &Rc<Self>) {
        let launcher = self.clone();
        self.search_entry.connect_changed(move |entry| {
            launcher.filter_apps(&entry.text().to_lowercase());
        });

        let launcher = self.clone();
        self.search_entry
            .connect_activate(move |_| launcher.launch_selected_or_first());

        self.window.connect_focus_out_event(|window, _| {
            window.hide();
            glib::Propagation::Proceed
        });

        let launcher = self.clone();
        self.window.connect_key_press_event(move |window, event| {
            match event.keyval() {
                gdk::keys::constants::Escape => {
                    window.hide();
                    return glib::Propagation::Stop;
                }
                gdk::keys::constants::Down => {
                    launcher.move_selection(1);
                    return glib::Propagation::Stop;
                }
                gdk::keys::constants::Up => {
                    launcher.move_selection(-1);
                    return glib::Propagation::Stop;
                }
                _ => {}
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
        *self.all_apps.borrow_mut() = scan_desktop_apps();
        self.search_entry.set_text("");
        self.filter_apps("");
        self.window.show_all();
        self.window.present();
        self.search_entry.grab_focus();
    }

    fn filter_apps(&self, query: &str) {
        for child in self.list_box.children() {
            self.list_box.remove(&child);
        }

        let mut count = 0usize;
        for app in self.all_apps.borrow().iter() {
            let command_text = app.argv.join(" ").to_lowercase();
            if !query.is_empty()
                && !app.name.to_lowercase().contains(query)
                && !app.comment.to_lowercase().contains(query)
                && !command_text.contains(query)
            {
                continue;
            }

            count += 1;
            let row = ListBoxRow::new();
            row.style_context().add_class("slopos-list-row");
            let accessible_name = if app.comment.is_empty() {
                app.name.clone()
            } else {
                format!("{} — {}", app.name, app.comment)
            };
            set_accessible_name(&row, &accessible_name);
            row.set_tooltip_text(Some(&accessible_name));
            let hbox = GtkBox::new(Orientation::Horizontal, 9);
            hbox.set_margin_start(7);
            hbox.set_margin_end(7);
            hbox.set_margin_top(4);
            hbox.set_margin_bottom(4);

            let icon = load_launcher_icon(app);
            icon.style_context().add_class("slopos-result-icon");
            hbox.pack_start(&icon, false, false, 0);

            let labels = GtkBox::new(Orientation::Vertical, 1);
            let title = Label::new(Some(&app.name));
            title.set_xalign(0.0);
            title.style_context().add_class("slopos-result-title");
            labels.pack_start(&title, false, false, 0);
            if !app.comment.is_empty() {
                let description = Label::new(Some(&app.comment));
                description.set_xalign(0.0);
                description
                    .style_context()
                    .add_class("slopos-secondary-text");
                labels.pack_start(&description, false, false, 0);
            }
            hbox.pack_start(&labels, true, true, 0);
            row.add(&hbox);

            let app = app.clone();
            let window = self.window.clone();
            row.connect_activate(move |_| {
                if let Err(error) = spawn_app(&app) {
                    log::warn!("Failed to launch {}: {error}", app.name);
                }
                window.hide();
            });
            self.list_box.add(&row);
        }

        self.status_label
            .set_text(&format!("{count} matching applications"));
        self.list_box.show_all();

        if let Some(first) = self.first_row() {
            self.list_box.select_row(Some(&first));
        }
    }

    fn launch_selected_or_first(&self) {
        if let Some(row) = self.list_box.selected_row().or_else(|| self.first_row()) {
            row.activate();
        }
    }

    fn first_row(&self) -> Option<ListBoxRow> {
        self.list_box
            .children()
            .first()
            .and_then(|widget| widget.clone().downcast::<ListBoxRow>().ok())
    }

    fn move_selection(&self, direction: isize) {
        let rows: Vec<ListBoxRow> = self
            .list_box
            .children()
            .into_iter()
            .filter_map(|widget| widget.downcast::<ListBoxRow>().ok())
            .collect();
        if rows.is_empty() {
            return;
        }

        let current = self
            .list_box
            .selected_row()
            .and_then(|selected| rows.iter().position(|row| row == &selected))
            .unwrap_or(0) as isize;
        let next = (current + direction).clamp(0, rows.len() as isize - 1) as usize;
        self.list_box.select_row(Some(&rows[next]));
    }
}

fn load_launcher_icon(app: &DesktopApp) -> Image {
    if let Some(file_name) = role_icon_file(app) {
        let mut candidates = Vec::new();
        if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
            candidates.push(
                PathBuf::from(share_dir)
                    .join("slopos-i/themes/platinum/icons")
                    .join(file_name),
            );
        }
        candidates.extend([
            PathBuf::from("themes/platinum/icons").join(file_name),
            PathBuf::from("/usr/local/share/slopos-i/themes/platinum/icons").join(file_name),
            PathBuf::from("/usr/share/slopos-i/themes/platinum/icons").join(file_name),
        ]);
        for path in candidates {
            if path.is_file() {
                if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 32, 32, true) {
                    return Image::from_pixbuf(Some(&pixbuf));
                }
            }
        }
    }

    let icon_name = if app.icon.is_empty() {
        "application-x-executable"
    } else {
        &app.icon
    };
    Image::from_icon_name(Some(icon_name), IconSize::Dnd)
}

fn role_icon_file(app: &DesktopApp) -> Option<&'static str> {
    let command = app.argv.first().map(String::as_str).unwrap_or_default();
    let haystack = format!("{} {} {}", app.id, app.name, command).to_ascii_lowercase();
    if haystack.contains("pcmanfm") || haystack.contains("file manager") {
        Some("folder.svg")
    } else if haystack.contains("xfce4-terminal")
        || haystack.contains("xterm")
        || haystack.contains("terminal")
    {
        Some("terminal.svg")
    } else if haystack.contains("mousepad")
        || haystack.contains("text editor")
        || haystack.contains("xed")
        || haystack.contains("gedit")
    {
        Some("textedit.svg")
    } else if haystack.contains("firefox")
        || haystack.contains("chromium")
        || haystack.contains("google-chrome")
        || haystack.contains("web browser")
    {
        Some("browser.svg")
    } else if haystack.contains("ristretto")
        || haystack.contains("image viewer")
        || haystack.contains("viewnior")
    {
        Some("desktop.svg")
    } else if haystack.contains("slopos-catalogue") || haystack.contains("software catalogue") {
        Some("software.svg")
    } else if haystack.contains("slopos-settings") || haystack.contains("system settings") {
        Some("settings.svg")
    } else if haystack.contains("desktop preferences") {
        Some("desktop.svg")
    } else {
        None
    }
}

fn spawn_app(app: &DesktopApp) -> Result<(), String> {
    let Some((program, args)) = app.argv.split_first() else {
        return Err("desktop entry has an empty command".to_string());
    };

    if app.terminal {
        for terminal in ["xfce4-terminal", "xterm"] {
            if command_exists(terminal) {
                let mut command = Command::new(terminal);
                if terminal == "xfce4-terminal" {
                    command.arg("--execute");
                } else {
                    command.arg("-e");
                }
                command.arg(program).args(args);
                return command
                    .spawn()
                    .map(|_| ())
                    .map_err(|error| error.to_string());
            }
        }
        return Err(
            "application requires a terminal, but no supported terminal is installed".into(),
        );
    }

    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(command).is_file()))
        .unwrap_or(false)
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
