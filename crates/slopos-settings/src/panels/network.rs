//! Native Network & Wi-Fi Settings Panel.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{Button, Dialog, DialogFlags, Label, ResponseType, Window};
use std::process::Command;

pub fn show_network_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Network & Wi-Fi"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[("Close", ResponseType::Close)],
    );
    dialog.set_default_response(ResponseType::Close);
    dialog.set_default_size(480, 220);
    set_accessible_name(&dialog, "SLOPOS network settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let status = Label::new(Some(
        "Live network details and controls are not available in this panel.",
    ));
    status.set_xalign(0.0);
    set_accessible_name(&status, "Network integration status");
    content.pack_start(&status, false, false, 0);

    if command_exists("nm-connection-editor") {
        let button = Button::with_label("Open Network Connections");
        set_accessible_name(&button, "Open external NetworkManager connection editor");
        let status = status.clone();
        button.connect_clicked(
            move |_| match Command::new("nm-connection-editor").spawn() {
                Ok(_) => status.set_text("Opened NetworkManager connection editor."),
                Err(error) => status.set_text(&format!(
                    "Could not open NetworkManager connection editor: {error}"
                )),
            },
        );
        content.pack_start(&button, false, false, 0);
    } else {
        status.set_text(
            "Live network details are unavailable; nm-connection-editor is not installed.",
        );
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
