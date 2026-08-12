//! SLOPOS-I curated AppImage catalogue.

mod installer;
mod model;

use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Entry, IconSize, Image, Label, ListBox, ListBoxRow, Orientation,
    PolicyType, ScrolledWindow, Window, WindowPosition, WindowType,
};
use installer::{install_appimage, uninstall_appimage};
use model::{get_curated_catalogue, CatalogueApp};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    gtk::init().expect("Failed to initialize GTK3");
    load_css_theme();

    let window = Window::new(WindowType::Toplevel);
    window.set_title("Software Catalogue");
    window.set_default_size(700, 510);
    window.set_position(WindowPosition::Center);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });

    let body = GtkBox::new(Orientation::Vertical, 7);
    body.style_context().add_class("slopos-window-body");

    let title = Label::new(Some("SLOPOS Software Catalogue"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-panel-title");
    body.pack_start(&title, false, false, 0);

    let subtitle = Label::new(Some(
        "Curated AppImages — installation is enabled only for integrity-verified metadata.",
    ));
    subtitle.set_xalign(0.0);
    subtitle
        .style_context()
        .add_class("slopos-panel-subtitle");
    body.pack_start(&subtitle, false, false, 0);

    let search = Entry::new();
    search.set_placeholder_text(Some("Search applications…"));
    search.set_icon_from_icon_name(
        gtk::EntryIconPosition::Primary,
        Some("system-search-symbolic"),
    );
    search.set_tooltip_text(Some("Filter the curated software catalogue"));
    body.pack_start(&search, false, false, 2);

    let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroll.style_context().add_class("slopos-list-frame");
    let list = ListBox::new();
    scroll.add(&list);
    body.pack_start(&scroll, true, true, 0);

    let status = Label::new(Some(
        "Browse the curated catalogue. Unverified entries remain unavailable.",
    ));
    status.set_xalign(0.0);
    status.style_context().add_class("slopos-statusbar");
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
        let content = GtkBox::new(Orientation::Horizontal, 10);
        content.set_margin_start(8);
        content.set_margin_end(8);
        content.set_margin_top(6);
        content.set_margin_bottom(6);

        content.pack_start(
            &Image::from_icon_name(Some(&app.icon_name), IconSize::Dnd),
            false,
            false,
            0,
        );

        let text = GtkBox::new(Orientation::Vertical, 1);
        let name = Label::new(Some(&format!("{}  {}", app.name, app.version)));
        name.set_xalign(0.0);
        name.style_context().add_class("slopos-result-title");
        text.pack_start(&name, false, false, 0);

        let summary = Label::new(Some(&app.summary));
        summary.set_xalign(0.0);
        summary
            .style_context()
            .add_class("slopos-secondary-text");
        text.pack_start(&summary, false, false, 0);

        if !app.metadata_is_installable() && !app.is_installed() {
            let warning = Label::new(Some("Trusted checksum metadata pending"));
            warning.set_xalign(0.0);
            warning.style_context().add_class("slopos-warning-text");
            text.pack_start(&warning, false, false, 0);
        }
        content.pack_start(&text, true, true, 0);

        let button = if app.is_installed() {
            Button::with_label("Remove")
        } else if app.metadata_is_installable() {
            Button::with_label("Install")
        } else {
            let button = Button::with_label("Unavailable");
            button.set_sensitive(false);
            button
        };
        button.set_valign(Align::Center);

        if app.is_installed() || app.metadata_is_installable() {
            let app = app.clone();
            let status = status.clone();
            let state_button = button.clone();
            button.connect_clicked(move |_| {
                state_button.set_sensitive(false);
                let operation = if app.is_installed() {
                    uninstall_appimage(&app).map(|_| format!("Removed {}", app.name))
                } else {
                    install_appimage(&app).map(|_| format!("Installed {}", app.name))
                };

                match operation {
                    Ok(message) => {
                        status.set_text(&message);
                        if app.is_installed() {
                            state_button.set_label("Remove");
                        } else {
                            state_button.set_label("Install");
                        }
                    }
                    Err(error) => status.set_text(&format!("Error: {error}")),
                }
                state_button.set_sensitive(true);
            });
        }

        content.pack_end(&button, false, false, 0);
        row.add(&content);
        list.add(&row);
    }

    status.set_text(&format!("{shown} catalogue entries shown"));
    list.show_all();
}

fn load_css_theme() {
    for path in [
        "assets/config/gtk-3.0/gtk.css",
        "/usr/local/share/themes/slopos-gtk/gtk-3.0/gtk.css",
        "/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css",
    ] {
        if !Path::new(path).exists() {
            continue;
        }
        let provider = gtk::CssProvider::new();
        if provider.load_from_path(path).is_ok() {
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
