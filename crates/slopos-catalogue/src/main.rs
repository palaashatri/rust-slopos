//! SLOPOS-I AppImage Catalogue GTK3 GUI

mod installer;
mod model;

use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Entry, HeaderBar, IconSize, Image, Label, ListBox, ListBoxRow,
    Orientation, PolicyType, ScrolledWindow, Window, WindowPosition, WindowType,
};
use installer::{install_appimage, uninstall_appimage};
use model::{get_curated_catalogue, CatalogueApp};
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting SLOPOS-I AppImage Catalogue");

    gtk::init().expect("Failed to initialize GTK3");

    let window = Window::new(WindowType::Toplevel);
    window.set_title("SLOPOS Applications");
    window.set_default_size(720, 520);
    window.set_position(WindowPosition::Center);

    let header = HeaderBar::new();
    header.set_show_close_button(true);
    header.set_title(Some("SLOPOS Catalogue"));
    header.set_subtitle(Some("Curated AppImage Software Store"));
    window.set_titlebar(Some(&header));

    let main_box = GtkBox::new(Orientation::Vertical, 12);
    main_box.set_margin_start(16);
    main_box.set_margin_end(16);
    main_box.set_margin_top(16);
    main_box.set_margin_bottom(16);

    // Search bar
    let search_entry = Entry::new();
    search_entry.set_placeholder_text(Some("Search AppImages (Kdenlive, Inkscape, GIMP, OBS)..."));
    search_entry.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, Some("system-search"));
    main_box.pack_start(&search_entry, false, false, 0);

    // Scrollable List of Apps
    let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(PolicyType::Never, PolicyType::Automatic);

    let list_box = ListBox::new();
    scroll.add(&list_box);
    main_box.pack_start(&scroll, true, true, 0);

    window.add(&main_box);

    let apps = Rc::new(RefCell::new(get_curated_catalogue()));
    render_apps(&list_box, &apps.borrow(), "");

    let apps_ref = apps.clone();
    let lb_ref = list_box.clone();
    search_entry.connect_changed(move |entry| {
        let q = entry.text().to_lowercase();
        render_apps(&lb_ref, &apps_ref.borrow(), &q);
    });

    window.show_all();
    gtk::main();
}

fn render_apps(list_box: &ListBox, apps: &[CatalogueApp], query: &str) {
    for child in list_box.children() {
        list_box.remove(&child);
    }

    for app in apps {
        if !query.is_empty()
            && !app.name.to_lowercase().contains(query)
            && !app.summary.to_lowercase().contains(query)
            && !app.category.to_lowercase().contains(query)
        {
            continue;
        }

        let row = ListBoxRow::new();
        let hbox = GtkBox::new(Orientation::Horizontal, 16);
        hbox.set_margin_start(12);
        hbox.set_margin_end(12);
        hbox.set_margin_top(10);
        hbox.set_margin_bottom(10);

        let img = Image::from_icon_name(Some(&app.icon_name), IconSize::Dnd);
        hbox.pack_start(&img, false, false, 0);

        let vbox = GtkBox::new(Orientation::Vertical, 4);
        let title_box = GtkBox::new(Orientation::Horizontal, 8);
        let title = Label::new(Some(&app.name));
        title.set_xalign(0.0);
        title_box.pack_start(&title, false, false, 0);

        let ver = Label::new(Some(&format!("v{}", app.version)));
        title_box.pack_start(&ver, false, false, 0);
        vbox.pack_start(&title_box, false, false, 0);

        let summary = Label::new(Some(&app.summary));
        summary.set_xalign(0.0);
        vbox.pack_start(&summary, false, false, 0);

        hbox.pack_start(&vbox, true, true, 0);

        // Action Button (Install / Uninstall)
        let is_installed = app.is_installed();
        let btn_label = if is_installed { "Uninstall" } else { "Install AppImage" };
        let action_btn = Button::with_label(btn_label);
        action_btn.set_valign(Align::Center);

        let app_clone = app.clone();
        let lb_clone = list_box.clone();
        let apps_vec = apps.to_vec();
        let q_clone = query.to_string();

        action_btn.connect_clicked(move |_| {
            if app_clone.is_installed() {
                let _ = uninstall_appimage(&app_clone);
            } else {
                let _ = install_appimage(&app_clone);
            }
            render_apps(&lb_clone, &apps_vec, &q_clone);
        });

        hbox.pack_end(&action_btn, false, false, 0);
        row.add(&hbox);
        list_box.add(&row);
    }
    list_box.show_all();
}
