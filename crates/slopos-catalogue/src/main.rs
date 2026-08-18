//! SLOPOS-I curated AppImage catalogue.

mod installer;
mod model;

use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Entry, IconSize, Image, Label, ListBox, ListBoxRow, Orientation,
    PolicyType, ScrolledWindow, Window, WindowPosition, WindowType,
};
use installer::{install_appimage, uninstall_appimage};
use model::{get_appimage_path, get_curated_catalogue, CatalogueApp};
use std::cell::RefCell;
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    gtk::init().expect("Failed to initialize GTK3");
    load_css_theme();

    let window = Window::new(WindowType::Toplevel);
    window.set_title("Software Catalogue");
    set_accessible_name(&window, "SLOPOS software catalogue");
    window.set_default_size(640, 400);
    window.set_position(WindowPosition::Center);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });

    let body = GtkBox::new(Orientation::Vertical, 7);
    body.style_context().add_class("slopos-window-body");

    let title = Label::new(Some("Software"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-panel-title");
    set_accessible_name(&title, "SLOPOS Software Catalogue");
    body.pack_start(&title, false, false, 0);

    let subtitle = Label::new(Some(
        "Curated AppImages with pinned integrity metadata. System packages remain managed by the base OS.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_line_wrap(true);
    subtitle.style_context().add_class("slopos-panel-subtitle");
    body.pack_start(&subtitle, false, false, 0);

    let search = Entry::new();
    search.set_placeholder_text(Some("Search software…"));
    search.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, Some("edit-find"));
    search.set_tooltip_text(Some("Filter the curated software catalogue"));
    set_accessible_name(&search, "Catalogue search field");
    body.pack_start(&search, false, false, 2);

    let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroll.style_context().add_class("slopos-list-frame");
    let list = ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    set_accessible_name(&list, "Catalogue application results");
    scroll.add(&list);
    body.pack_start(&scroll, true, true, 0);

    let status = Label::new(Some("Curated AppImage catalogue"));
    status.set_xalign(0.0);
    status.style_context().add_class("slopos-statusbar");
    set_accessible_name(&status, "Catalogue status");
    body.pack_end(&status, false, false, 0);

    window.add(&body);

    let apps = Rc::new(RefCell::new(get_curated_catalogue()));
    render_apps(&list, &apps.borrow(), "", &status);

    let apps_ref = apps.clone();
    let list_ref = list.clone();
    let status_ref = status.clone();
    search.connect_changed(move |entry| {
        render_apps(
            &list_ref,
            &apps_ref.borrow(),
            &entry.text().to_lowercase(),
            &status_ref,
        );
    });

    window.show_all();
    gtk::main();
}

fn render_apps(list: &ListBox, apps: &[CatalogueApp], query: &str, status: &Label) {
    for child in list.children() {
        list.remove(&child);
    }

    let mut shown = 0usize;
    for app in apps {
        if !query.is_empty()
            && !app.name.to_lowercase().contains(query)
            && !app.summary.to_lowercase().contains(query)
            && !app.category.to_lowercase().contains(query)
        {
            continue;
        }
        shown += 1;

        let row = ListBoxRow::new();
        row.style_context().add_class("slopos-catalogue-row");
        set_accessible_name(&row, &format!("{} {}", app.name, app.version));
        let content = GtkBox::new(Orientation::Horizontal, 10);
        content.set_margin_start(8);
        content.set_margin_end(8);
        content.set_margin_top(6);
        content.set_margin_bottom(6);

        content.pack_start(&load_catalogue_icon(&app.icon_name), false, false, 0);

        let text = GtkBox::new(Orientation::Vertical, 1);
        let name = Label::new(Some(&format!("{}  {}", app.name, app.version)));
        name.set_xalign(0.0);
        name.style_context().add_class("slopos-result-title");
        text.pack_start(&name, false, false, 0);

        let summary = Label::new(Some(&format!("{}  ·  {}", app.summary, app.category)));
        summary.set_xalign(0.0);
        summary.style_context().add_class("slopos-secondary-text");
        text.pack_start(&summary, false, false, 0);

        if !app.metadata_is_installable() && !app.is_installed() {
            let warning = Label::new(Some("Trusted checksum metadata pending"));
            warning.set_xalign(0.0);
            warning.style_context().add_class("slopos-warning-text");
            text.pack_start(&warning, false, false, 0);
        }
        content.pack_start(&text, true, true, 0);

        let button_box = GtkBox::new(Orientation::Horizontal, 4);
        button_box.set_valign(Align::Center);

        if app.is_appimage_installed() {
            let launch_btn = Button::with_label("Launch");
            launch_btn.style_context().add_class("suggested-action");
            set_accessible_name(&launch_btn, &format!("Launch {}", app.name));
            let app_id = app.id.clone();
            let app_name = app.name.clone();
            let status_c = status.clone();
            launch_btn.connect_clicked(move |_| match spawn_app(&app_id) {
                Ok(_) => status_c.set_text(&format!("Launched {}", app_name)),
                Err(err) => status_c.set_text(&format!("Launch failed: {err}")),
            });
            button_box.pack_start(&launch_btn, false, false, 0);

            let remove_btn = Button::with_label("Remove");
            set_accessible_name(&remove_btn, &format!("Remove {}", app.name));
            let app_c = app.clone();
            let status_c = status.clone();
            let list_c = list.clone();
            let apps_c = apps.to_vec();
            let query_s = query.to_string();
            let remove_btn_c = remove_btn.clone();
            remove_btn.connect_clicked(move |_| {
                remove_btn_c.set_sensitive(false);
                match uninstall_appimage(&app_c) {
                    Ok(_) => {
                        status_c.set_text(&format!("Removed {}", app_c.name));
                        render_apps(&list_c, &apps_c, &query_s, &status_c);
                    }
                    Err(err) => {
                        status_c.set_text(&format!("Error: {err}"));
                        remove_btn_c.set_sensitive(true);
                    }
                }
            });
            button_box.pack_start(&remove_btn, false, false, 0);
        } else if app.is_system_installed() {
            let launch_btn = Button::with_label("Launch");
            launch_btn.style_context().add_class("suggested-action");
            set_accessible_name(&launch_btn, &format!("Launch {}", app.name));
            let app_id = app.id.clone();
            let app_name = app.name.clone();
            let status_c = status.clone();
            launch_btn.connect_clicked(move |_| match spawn_app(&app_id) {
                Ok(_) => status_c.set_text(&format!("Launched {}", app_name)),
                Err(err) => status_c.set_text(&format!("Launch failed: {err}")),
            });
            button_box.pack_start(&launch_btn, false, false, 0);
        } else if app.metadata_is_installable() {
            let install_btn = Button::with_label("Install");
            set_accessible_name(&install_btn, &format!("Install {}", app.name));
            let app_c = app.clone();
            let status_c = status.clone();
            let list_c = list.clone();
            let apps_c = apps.to_vec();
            let query_s = query.to_string();
            let install_btn_c = install_btn.clone();
            install_btn.connect_clicked(move |_| {
                install_btn_c.set_sensitive(false);
                status_c.set_text(&format!("Installing {}…", app_c.name));
                let app_inner = app_c.clone();
                let app_name = app_c.name.clone();
                let status_inner = status_c.clone();
                let list_inner = list_c.clone();
                let apps_inner = apps_c.clone();
                let query_inner = query_s.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let res = install_appimage(&app_inner);
                    let _ = tx.send(res);
                });
                glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                    if let Ok(res) = rx.try_recv() {
                        match res {
                            Ok(_) => {
                                status_inner.set_text(&format!("Installed {app_name}"));
                                render_apps(&list_inner, &apps_inner, &query_inner, &status_inner);
                            }
                            Err(err) => {
                                status_inner.set_text(&format!("Install error: {err}"));
                                render_apps(&list_inner, &apps_inner, &query_inner, &status_inner);
                            }
                        }
                        glib::ControlFlow::Break
                    } else {
                        glib::ControlFlow::Continue
                    }
                });
            });
            button_box.pack_start(&install_btn, false, false, 0);
        } else {
            let btn = Button::with_label("Unavailable");
            btn.set_sensitive(false);
            set_accessible_name(&btn, &format!("{} unavailable", app.name));
            button_box.pack_start(&btn, false, false, 0);
        }

        content.pack_end(&button_box, false, false, 0);
        row.add(&content);
        list.add(&row);
    }

    status.set_text(&format!("{shown} catalogue entries shown"));
    list.show_all();
}

fn spawn_app(id: &str) -> std::io::Result<std::process::Child> {
    let appimage_path = get_appimage_path(id);
    if appimage_path.is_file() {
        return std::process::Command::new(appimage_path).spawn();
    }
    let launch_cmd = match id {
        "firefox" | "firefox-esr" => "start-slopos-browser",
        "thunderbird" => "thunderbird",
        "chocolate-doom" | "doom" => {
            if std::path::Path::new("/usr/games/chocolate-doom").exists() {
                "/usr/games/chocolate-doom"
            } else {
                "chocolate-doom"
            }
        }
        "supertux" | "supertux2" => {
            if std::path::Path::new("/usr/games/supertux2").exists() {
                "/usr/games/supertux2"
            } else {
                "supertux2"
            }
        }
        other => other,
    };
    std::process::Command::new(launch_cmd).spawn()
}

fn load_catalogue_icon(icon_name: &str) -> Image {
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates
            .push(PathBuf::from(&share_dir).join(format!("themes/platinum/icons/{icon_name}.svg")));
        candidates
            .push(PathBuf::from(&share_dir).join(format!("themes/platinum/icons/{icon_name}.png")));
    }
    candidates.extend([
        PathBuf::from(format!("themes/platinum/icons/{icon_name}.svg")),
        PathBuf::from(format!("themes/platinum/icons/{icon_name}.png")),
        PathBuf::from(format!(
            "/usr/local/share/slopos-i/themes/platinum/icons/{icon_name}.svg"
        )),
        PathBuf::from(format!(
            "/usr/local/share/slopos-i/themes/platinum/icons/{icon_name}.png"
        )),
        PathBuf::from(format!(
            "/usr/share/slopos-i/themes/platinum/icons/{icon_name}.svg"
        )),
        PathBuf::from(format!(
            "/usr/share/slopos-i/themes/platinum/icons/{icon_name}.png"
        )),
    ]);

    for fallback_name in ["software.svg", "software.png"] {
        if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
            candidates.push(
                PathBuf::from(share_dir).join(format!("themes/platinum/icons/{fallback_name}")),
            );
        }
        candidates.extend([
            PathBuf::from(format!("themes/platinum/icons/{fallback_name}")),
            PathBuf::from(format!(
                "/usr/local/share/slopos-i/themes/platinum/icons/{fallback_name}"
            )),
            PathBuf::from(format!(
                "/usr/share/slopos-i/themes/platinum/icons/{fallback_name}"
            )),
        ]);
    }

    for path in candidates {
        if Path::new(&path).exists() {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 32, 32, true) {
                return Image::from_pixbuf(Some(&pixbuf));
            }
        }
    }

    Image::from_icon_name(Some("application-x-executable"), IconSize::Dnd)
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

fn current_appearance() -> &'static str {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    if let Some(config_home) = config_home {
        if let Ok(value) = std::fs::read_to_string(config_home.join("slopos-i/appearance")) {
            let v = value.trim();
            if v.eq_ignore_ascii_case("graphite") {
                return "graphite";
            }
            if v.eq_ignore_ascii_case("classic") {
                return "classic";
            }
        }
    }
    "platinum"
}

fn load_css_theme() {
    let appearance = current_appearance();
    let installed_theme = match appearance {
        "graphite" => "slopos-gtk-graphite",
        "classic" => "slopos-gtk-classic",
        _ => "slopos-gtk",
    };
    let source_css = match appearance {
        "graphite" => "assets/config/gtk-3.0/gtk-graphite.css",
        "classic" => "assets/config/gtk-3.0/gtk-classic.css",
        _ => "assets/config/gtk-3.0/gtk.css",
    };
    let mut css_paths = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        css_paths.push(
            PathBuf::from(share_dir)
                .join("themes")
                .join(installed_theme)
                .join("gtk-3.0/gtk.css"),
        );
    }
    css_paths.extend([
        PathBuf::from(source_css),
        PathBuf::from(format!(
            "/usr/local/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
        PathBuf::from(format!(
            "/usr/share/themes/{installed_theme}/gtk-3.0/gtk.css"
        )),
    ]);
    for path in css_paths {
        if !path.exists() {
            continue;
        }
        let provider = gtk::CssProvider::new();
        let Some(path_text) = path.to_str() else {
            continue;
        };
        if provider.load_from_path(path_text).is_ok() {
            if let Some(screen) = gdk::Screen::default() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            break;
        }
    }
}
