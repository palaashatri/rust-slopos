//! Settings Subsystem Service Traits & Adapters (`crates/slopos-bus/src/services.rs`)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal_strength: u8,
    pub is_secure: bool,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkStatus {
    Disconnected,
    Connecting,
    Connected {
        ip_address: String,
        ssid: Option<String>,
    },
}

pub trait NetworkServiceTrait {
    fn scan_wifi_networks(&self) -> Result<Vec<WifiNetwork>, String>;
    fn connect_wifi(&self, ssid: &str, password: &str) -> Result<(), String>;
    fn disconnect(&self, interface: &str) -> Result<(), String>;
    fn get_status(&self) -> NetworkStatus;
}

pub trait AudioServiceTrait {
    fn get_master_volume(&self) -> f32;
    fn set_master_volume(&self, volume: f32) -> Result<(), String>;
    fn is_muted(&self) -> bool;
    fn set_muted(&self, muted: bool) -> Result<(), String>;
}

pub trait PowerServiceTrait {
    fn get_battery_level(&self) -> Option<f32>;
    fn is_charging(&self) -> bool;
    fn suspend(&self) -> Result<(), String>;
    fn shutdown(&self) -> Result<(), String>;
    fn reboot(&self) -> Result<(), String>;
}

// Concrete Linux adapters own the real NetworkManager, PipeWire/PulseAudio
// and logind/UPower implementations. No fabricated fallback service belongs
// in this shared crate: unavailable system services must be reported as
// errors or explicit unavailable state by their platform adapter.
