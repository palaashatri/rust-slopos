//! Native Network & Wi-Fi Settings Panel.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, Dialog, DialogFlags, Frame, IconSize, Image, Label, ListBox,
    Orientation, ResponseType, Switch, Window,
};
use std::process::Command;

pub fn show_network_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Network & Wi-Fi"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[("Close", ResponseType::Close)],
    );
    dialog.set_default_response(ResponseType::Close);
    dialog.set_default_size(480, 440);
    set_accessible_name(&dialog, "SLOPOS network and wifi settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let title = Label::new(Some("Network Connections"));
    title.set_xalign(0.0);
    title.style_context().add_class("slopos-control-title");
    content.pack_start(&title, false, false, 0);

    // Wired Ethernet Section
    let eth_frame = Frame::new(Some("Wired Ethernet"));
    eth_frame.style_context().add_class("slopos-section");
    let eth_box = GtkBox::new(Orientation::Vertical, 6);
    eth_box.set_margin_start(10);
    eth_box.set_margin_end(10);
    eth_box.set_margin_top(8);
    eth_box.set_margin_bottom(8);

    let eth_status_row = GtkBox::new(Orientation::Horizontal, 8);
    eth_status_row.pack_start(
        &Image::from_icon_name(Some("network-wired-symbolic"), IconSize::Button),
        false,
        false,
        0,
    );
    let eth_label = Label::new(Some("Interface: eth0 (Connected — 1000 Mbps Full Duplex)"));
    eth_label.set_xalign(0.0);
    eth_status_row.pack_start(&eth_label, true, true, 0);
    eth_box.pack_start(&eth_status_row, false, false, 0);

    let ip_label = Label::new(Some("IP Address: 192.168.1.100  |  Gateway: 192.168.1.1"));
    ip_label.set_xalign(0.0);
    ip_label.style_context().add_class("slopos-secondary-text");
    eth_box.pack_start(&ip_label, false, false, 0);

    eth_frame.add(&eth_box);
    content.pack_start(&eth_frame, false, false, 0);

    // Wireless Wi-Fi Section
    let wifi_frame = Frame::new(Some("Wireless Wi-Fi"));
    wifi_frame.style_context().add_class("slopos-section");
    let wifi_box = GtkBox::new(Orientation::Vertical, 8);
    wifi_box.set_margin_start(10);
    wifi_box.set_margin_end(10);
    wifi_box.set_margin_top(8);
    wifi_box.set_margin_bottom(8);

    let wifi_switch_row = GtkBox::new(Orientation::Horizontal, 8);
    wifi_switch_row.pack_start(
        &Image::from_icon_name(Some("network-wireless-symbolic"), IconSize::Button),
        false,
        false,
        0,
    );
    wifi_switch_row.pack_start(&Label::new(Some("Wi-Fi Adapter")), false, false, 0);
    let wifi_switch = Switch::new();
    wifi_switch.set_active(true);
    wifi_switch_row.pack_end(&wifi_switch, false, false, 0);
    wifi_box.pack_start(&wifi_switch_row, false, false, 0);

    let wifi_list = ListBox::new();
    wifi_list.style_context().add_class("slopos-list-frame");

    for (ssid, signal, secure, connected) in [
        ("SLOPOS-Fast-5G", "100%", true, true),
        ("Home-Network-Guest", "82%", true, false),
        ("CoffeeShop_Free_WiFi", "55%", false, false),
    ] {
        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.set_margin_start(6);
        row.set_margin_end(6);
        row.set_margin_top(4);
        row.set_margin_bottom(4);

        let icon_name = if secure {
            "network-wireless-encrypted-symbolic"
        } else {
            "network-wireless-symbolic"
        };
        row.pack_start(
            &Image::from_icon_name(Some(icon_name), IconSize::Button),
            false,
            false,
            0,
        );

        let name_label = Label::new(Some(ssid));
        name_label.set_xalign(0.0);
        row.pack_start(&name_label, true, true, 0);

        let signal_label = Label::new(Some(signal));
        signal_label
            .style_context()
            .add_class("slopos-secondary-text");
        row.pack_start(&signal_label, false, false, 0);

        if connected {
            let badge = Label::new(Some("Connected"));
            badge.style_context().add_class("slopos-active-app");
            row.pack_start(&badge, false, false, 0);
        } else {
            let connect_btn = Button::with_label("Connect");
            connect_btn.style_context().add_class("slopos-push-btn");
            row.pack_start(&connect_btn, false, false, 0);
        }

        wifi_list.add(&row);
    }
    wifi_box.pack_start(&wifi_list, true, true, 0);

    wifi_frame.add(&wifi_box);
    content.pack_start(&wifi_frame, false, false, 0);

    if command_exists("nm-connection-editor") {
        let nm_btn = Button::with_label("Advanced Network Connections (nm-connection-editor)");
        nm_btn.style_context().add_class("slopos-push-btn");
        nm_btn.connect_clicked(|_| {
            let _ = Command::new("nm-connection-editor").spawn();
        });
        content.pack_start(&nm_btn, false, false, 0);
    }

    dialog.show_all();
    let _ = dialog.run();
    dialog.close();
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
