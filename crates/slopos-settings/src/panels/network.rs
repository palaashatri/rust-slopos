//! Network panel: live NetworkManager state with networking and Wi-Fi radio
//! switches. Connection editing is delegated to `nm-connection-editor`.

use crate::providers::availability::command_exists;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, CheckButton, Dialog, DialogFlags, Frame, Grid, Label, Orientation,
    ResponseType, Window,
};
use slopos_services::network::{
    NetworkDeviceState, NetworkDeviceType, NetworkManagerClient, NetworkSnapshot,
};
use std::cell::Cell;
use std::rc::Rc;
use std::thread;

/// What the panel shows, derived from one snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkView {
    pub networking_enabled: bool,
    /// `None` when there is no Wi-Fi hardware.
    pub wifi_enabled: Option<bool>,
    /// (interface, kind, state)
    pub devices: Vec<(String, String, String)>,
    /// One block of lines per active connection.
    pub connections: Vec<Vec<String>>,
    /// True when at least one connection reached NetworkManager state 2
    /// (activated). Merely appearing in the active list is not enough: a
    /// connection waiting for authentication or DHCP is still activating.
    pub connected: bool,
}

impl NetworkView {
    fn empty() -> Self {
        Self {
            networking_enabled: false,
            wifi_enabled: None,
            devices: Vec::new(),
            connections: Vec::new(),
            connected: false,
        }
    }
}

/// The status line, derived only from established state.
fn status_text(view: &NetworkView) -> &'static str {
    if view.connected {
        "Connected."
    } else if view.connections.is_empty() {
        "Not connected."
    } else {
        "Connecting…"
    }
}

fn device_kind(kind: NetworkDeviceType) -> &'static str {
    match kind {
        NetworkDeviceType::Ethernet => "Wired",
        NetworkDeviceType::Wifi => "Wi-Fi",
        NetworkDeviceType::Bluetooth => "Bluetooth",
        NetworkDeviceType::Other(_) => "Other",
    }
}

fn device_state(state: NetworkDeviceState) -> &'static str {
    use NetworkDeviceState::*;
    match state {
        Activated => "Connected",
        Disconnected => "Disconnected",
        Unavailable => "Unavailable",
        Unmanaged => "Not managed by NetworkManager",
        Prepare | Config | IpConfig | IpCheck | Secondaries => "Connecting…",
        NeedAuth => "Waiting for a password",
        Deactivating => "Disconnecting…",
        Failed => "Failed",
        Unknown | Other(_) => "Unknown",
    }
}

pub fn describe(snapshot: &NetworkSnapshot) -> NetworkView {
    let has_wifi = snapshot
        .devices
        .iter()
        .any(|device| device.kind == NetworkDeviceType::Wifi);
    let mut devices: Vec<(String, String, String)> = snapshot
        .devices
        .iter()
        .filter(|device| device.interface != "lo")
        .map(|device| {
            (
                device.interface.clone(),
                device_kind(device.kind).to_owned(),
                device_state(device.state).to_owned(),
            )
        })
        .collect();
    devices.sort();

    let connections = snapshot
        .active_connections
        .iter()
        .map(|connection| {
            let mut lines = vec![format!(
                "{}{}",
                connection.id,
                if connection.default_ipv4 || connection.default_ipv6 {
                    " (default route)"
                } else {
                    ""
                }
            )];
            for config in [&connection.ipv4, &connection.ipv6].into_iter().flatten() {
                for address in &config.addresses {
                    lines.push(format!("Address: {}/{}", address.address, address.prefix));
                }
                if let Some(gateway) = &config.gateway {
                    lines.push(format!("Gateway: {gateway}"));
                }
                if !config.nameservers.is_empty() {
                    lines.push(format!("DNS: {}", config.nameservers.join(", ")));
                }
            }
            lines
        })
        .collect();

    NetworkView {
        networking_enabled: snapshot.networking_enabled,
        wifi_enabled: has_wifi.then_some(snapshot.wireless_enabled),
        devices,
        connected: snapshot
            .active_connections
            .iter()
            .any(|connection| connection.state == 2),
        connections,
    }
}

enum Command {
    SetNetworking(bool),
    SetWifi(bool),
}

enum Update {
    Snapshot(NetworkSnapshot),
    Unavailable(String),
    CommandFailed(String),
}

pub fn show_network_dialog(parent: &Window) {
    let dialog = Dialog::with_buttons(
        Some("Network"),
        Some(parent),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[("Close", ResponseType::Close)],
    );
    dialog.set_default_response(ResponseType::Close);
    dialog.set_default_size(520, 380);
    set_accessible_name(&dialog, "SLOPOS network settings");

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let status = Label::new(Some("Reading network state…"));
    status.set_xalign(0.0);
    status.set_line_wrap(true);
    set_accessible_name(&status, "Network status");
    content.pack_start(&status, false, false, 0);

    let switches = GtkBox::new(Orientation::Horizontal, 16);
    let networking = CheckButton::with_label("Networking");
    let wifi = CheckButton::with_label("Wi-Fi");
    networking.set_sensitive(false);
    wifi.set_sensitive(false);
    wifi.set_no_show_all(true);
    switches.pack_start(&networking, false, false, 0);
    switches.pack_start(&wifi, false, false, 0);
    content.pack_start(&switches, false, false, 0);

    let devices_frame = Frame::new(Some("Devices"));
    let devices_grid = Grid::new();
    devices_grid.set_row_spacing(4);
    devices_grid.set_column_spacing(16);
    devices_grid.set_margin_start(10);
    devices_grid.set_margin_end(10);
    devices_grid.set_margin_top(8);
    devices_grid.set_margin_bottom(8);
    devices_frame.add(&devices_grid);
    content.pack_start(&devices_frame, false, false, 0);

    let connections_frame = Frame::new(Some("Active connections"));
    let connections_box = GtkBox::new(Orientation::Vertical, 6);
    connections_box.set_margin_start(10);
    connections_box.set_margin_end(10);
    connections_box.set_margin_top(8);
    connections_box.set_margin_bottom(8);
    connections_frame.add(&connections_box);
    content.pack_start(&connections_frame, true, true, 0);

    if command_exists("nm-connection-editor") {
        let editor = Button::with_label("Edit Connections…");
        set_accessible_name(&editor, "Open the NetworkManager connection editor");
        let status = status.clone();
        editor.connect_clicked(move |_| {
            if let Err(error) = slopos_core::process::launch("nm-connection-editor", &[]) {
                status.set_text(&error);
            }
        });
        content.pack_start(&editor, false, false, 0);
    }

    #[allow(deprecated)]
    let (update_sender, update_receiver) =
        glib::MainContext::channel::<Update>(glib::Priority::default());
    let (command_sender, command_receiver) = tokio::sync::mpsc::unbounded_channel::<Command>();
    spawn_worker(update_sender, command_receiver);

    // Ignore toggles caused by applying a snapshot to the switches.
    let syncing = Rc::new(Cell::new(false));
    {
        let syncing = syncing.clone();
        let commands = command_sender.clone();
        networking.connect_toggled(move |button| {
            if !syncing.get() {
                let _ = commands.send(Command::SetNetworking(button.is_active()));
            }
        });
    }
    {
        let syncing = syncing.clone();
        let commands = command_sender.clone();
        wifi.connect_toggled(move |button| {
            if !syncing.get() {
                let _ = commands.send(Command::SetWifi(button.is_active()));
            }
        });
    }

    let status_for_updates = status.clone();
    let update_source = update_receiver.attach(None, move |update| {
        match update {
            Update::Snapshot(snapshot) => {
                let view = describe(&snapshot);
                syncing.set(true);
                networking.set_active(view.networking_enabled);
                networking.set_sensitive(true);
                match view.wifi_enabled {
                    Some(enabled) => {
                        wifi.set_active(enabled);
                        wifi.set_sensitive(true);
                        wifi.show();
                    }
                    None => wifi.hide(),
                }
                syncing.set(false);
                status_for_updates.set_text(status_text(&view));
                fill_devices(&devices_grid, &view);
                fill_connections(&connections_box, &view);
            }
            Update::Unavailable(reason) => {
                networking.set_sensitive(false);
                wifi.set_sensitive(false);
                wifi.hide();
                status_for_updates.set_text(&format!("Network status is unavailable: {reason}"));
                let empty = NetworkView::empty();
                fill_devices(&devices_grid, &empty);
                fill_connections(&connections_box, &empty);
            }
            Update::CommandFailed(error) => status_for_updates.set_text(&error),
        }
        glib::ControlFlow::Continue
    });

    dialog.show_all();
    let _ = dialog.run();
    // The dialog is hidden but not destroyed yet. Remove the update source
    // before returning so a queued snapshot cannot touch its widgets.
    update_source.remove();
    // Dropping the last command sender stops the worker.
    drop(command_sender);
    dialog.close();
}

fn fill_devices(grid: &Grid, view: &NetworkView) {
    for child in grid.children() {
        grid.remove(&child);
    }
    if view.devices.is_empty() {
        let none = Label::new(Some("No network devices"));
        none.set_xalign(0.0);
        grid.attach(&none, 0, 0, 3, 1);
    }
    for (row, (interface, kind, state)) in view.devices.iter().enumerate() {
        for (column, text) in [interface, kind, state].into_iter().enumerate() {
            let label = Label::new(Some(text));
            label.set_xalign(0.0);
            grid.attach(&label, column as i32, row as i32, 1, 1);
        }
    }
    grid.show_all();
}

fn fill_connections(container: &GtkBox, view: &NetworkView) {
    for child in container.children() {
        container.remove(&child);
    }
    if view.connections.is_empty() {
        let none = Label::new(Some("None"));
        none.set_xalign(0.0);
        container.pack_start(&none, false, false, 0);
    }
    for lines in &view.connections {
        let label = Label::new(Some(&lines.join("\n")));
        label.set_xalign(0.0);
        label.set_selectable(true);
        container.pack_start(&label, false, false, 0);
    }
    container.show_all();
}

fn spawn_worker(
    updates: glib::Sender<Update>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
) {
    let spawned = thread::Builder::new()
        .name("slopos-settings-network".to_owned())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                let _ = updates.send(Update::Unavailable("internal error".to_owned()));
                return;
            };
            runtime.block_on(async move {
                let client = match NetworkManagerClient::connect().await {
                    Ok(client) => client,
                    Err(error) => {
                        let _ = updates.send(Update::Unavailable(explain(&error)));
                        return;
                    }
                };
                match client.snapshot().await {
                    Ok(snapshot) => {
                        let _ = updates.send(Update::Snapshot(snapshot));
                    }
                    Err(error) => {
                        let _ = updates.send(Update::Unavailable(explain(&error)));
                        return;
                    }
                }
                let mut events = client.watch();
                loop {
                    tokio::select! {
                        command = commands.recv() => {
                            let Some(command) = command else { break };
                            let result = match command {
                                Command::SetNetworking(on) => client.set_networking_enabled(on).await,
                                Command::SetWifi(on) => client.set_wireless_enabled(on).await,
                            };
                            if let Err(error) = result {
                                // Show the real state before the failure, so the
                                // snapshot cannot immediately hide the message.
                                if let Ok(snapshot) = client.snapshot().await {
                                    let _ = updates.send(Update::Snapshot(snapshot));
                                }
                                let _ = updates.send(Update::CommandFailed(format!(
                                    "The change was not applied: {error}"
                                )));
                            }
                        }
                        event = events.next() => match event {
                            Some(Ok(snapshot)) => {
                                let _ = updates.send(Update::Snapshot(snapshot));
                            }
                            Some(Err(error)) => {
                                let _ = updates.send(Update::Unavailable(explain(&error)));
                                break;
                            }
                            None => {
                                let _ = updates.send(Update::Unavailable(
                                    "NetworkManager stopped sending updates.".to_owned(),
                                ));
                                break;
                            }
                        }
                    }
                }
            });
        });
    if let Err(error) = spawned {
        log::warn!("Could not start the network worker: {error}");
    }
}

fn explain(error: &slopos_services::network::NetworkError) -> String {
    use slopos_core::capability::Availability;
    match error.availability() {
        Availability::MissingService => "NetworkManager is not running.".to_owned(),
        Availability::PermissionDenied => {
            "you are not allowed to read network settings.".to_owned()
        }
        _ => error.to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_services::network::{
        ActiveNetworkConnection, NetworkDevice, NetworkIpAddress, NetworkIpConfig,
    };

    fn device(
        interface: &str,
        kind: NetworkDeviceType,
        state: NetworkDeviceState,
    ) -> NetworkDevice {
        NetworkDevice {
            object_path: format!("/d/{interface}"),
            interface: interface.to_owned(),
            ip_interface: interface.to_owned(),
            kind,
            state,
            active_access_point: None,
            access_points: vec![],
            active_connection: None,
        }
    }

    #[test]
    fn describes_devices_addresses_and_wifi_radio() {
        let snapshot = NetworkSnapshot {
            revision: 3,
            networking_enabled: true,
            wireless_enabled: false,
            wireless_hardware_enabled: true,
            devices: vec![
                device(
                    "wlan0",
                    NetworkDeviceType::Wifi,
                    NetworkDeviceState::Unavailable,
                ),
                device(
                    "eth0",
                    NetworkDeviceType::Ethernet,
                    NetworkDeviceState::Activated,
                ),
                device(
                    "lo",
                    NetworkDeviceType::Other(14),
                    NetworkDeviceState::Unmanaged,
                ),
            ],
            active_connections: vec![ActiveNetworkConnection {
                object_path: "/ac/1".to_owned(),
                id: "Wired connection 1".to_owned(),
                uuid: "u".to_owned(),
                connection_type: "802-3-ethernet".to_owned(),
                state: 2,
                default_ipv4: true,
                default_ipv6: false,
                is_vpn: false,
                ipv4: Some(NetworkIpConfig {
                    addresses: vec![NetworkIpAddress {
                        address: "192.0.2.5".to_owned(),
                        prefix: 24,
                    }],
                    gateway: Some("192.0.2.1".to_owned()),
                    nameservers: vec!["192.0.2.53".to_owned()],
                }),
                ipv6: None,
            }],
        };
        let view = describe(&snapshot);
        assert_eq!(view.wifi_enabled, Some(false));
        assert!(view.connected);
        assert_eq!(status_text(&view), "Connected.");
        assert_eq!(
            view.devices,
            vec![
                ("eth0".into(), "Wired".into(), "Connected".into()),
                ("wlan0".into(), "Wi-Fi".into(), "Unavailable".into()),
            ]
        );
        assert_eq!(
            view.connections,
            vec![vec![
                "Wired connection 1 (default route)".to_owned(),
                "Address: 192.0.2.5/24".to_owned(),
                "Gateway: 192.0.2.1".to_owned(),
                "DNS: 192.0.2.53".to_owned(),
            ]]
        );
    }

    #[test]
    fn systems_without_wifi_hide_the_radio_switch() {
        let snapshot = NetworkSnapshot {
            revision: 1,
            networking_enabled: false,
            wireless_enabled: true,
            wireless_hardware_enabled: true,
            devices: vec![device(
                "eth0",
                NetworkDeviceType::Ethernet,
                NetworkDeviceState::Disconnected,
            )],
            active_connections: vec![],
        };
        let view = describe(&snapshot);
        assert_eq!(view.wifi_enabled, None);
        assert!(!view.networking_enabled);
        assert!(view.connections.is_empty());
        assert!(!view.connected);
        assert_eq!(status_text(&view), "Not connected.");
    }

    #[test]
    fn an_activating_connection_is_not_reported_as_connected() {
        let snapshot = NetworkSnapshot {
            revision: 2,
            networking_enabled: true,
            wireless_enabled: true,
            wireless_hardware_enabled: true,
            devices: vec![device(
                "wlan0",
                NetworkDeviceType::Wifi,
                NetworkDeviceState::Prepare,
            )],
            active_connections: vec![ActiveNetworkConnection {
                object_path: "/ac/1".to_owned(),
                id: "Home Wi-Fi".to_owned(),
                uuid: "u".to_owned(),
                connection_type: "802-11-wireless".to_owned(),
                state: 1,
                default_ipv4: false,
                default_ipv6: false,
                is_vpn: false,
                ipv4: None,
                ipv6: None,
            }],
        };
        let view = describe(&snapshot);
        assert!(!view.connected);
        assert_eq!(view.connections.len(), 1);
        assert_eq!(status_text(&view), "Connecting…");
    }
}
