//! Bluetooth state adapter using BlueZ D-Bus.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothStatus {
    PoweredOn,
    PoweredOff,
    Unavailable,
}

pub fn query_bluetooth_status() -> BluetoothStatus {
    let Ok(connection) = zbus::blocking::Connection::system() else {
        return BluetoothStatus::Unavailable;
    };

    let Ok(proxy) = zbus::blocking::Proxy::new(
        &connection,
        "org.bluez",
        "/org/bluez/hci0",
        "org.bluez.Adapter1",
    ) else {
        return BluetoothStatus::Unavailable;
    };

    match proxy.get_property::<bool>("Powered") {
        Ok(true) => BluetoothStatus::PoweredOn,
        Ok(false) => BluetoothStatus::PoweredOff,
        Err(_) => BluetoothStatus::Unavailable,
    }
}
