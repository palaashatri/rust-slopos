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

/// Fallback / Mock Network Service for test environments where NetworkManager D-Bus is unavailable.
#[derive(Debug, Default)]
pub struct MockNetworkService;

impl NetworkServiceTrait for MockNetworkService {
    fn scan_wifi_networks(&self) -> Result<Vec<WifiNetwork>, String> {
        Ok(vec![
            WifiNetwork {
                ssid: "SLOPOS-I-WiFi".to_string(),
                signal_strength: 95,
                is_secure: true,
                is_connected: true,
            },
            WifiNetwork {
                ssid: "Guest-Network".to_string(),
                signal_strength: 60,
                is_secure: false,
                is_connected: false,
            },
        ])
    }

    fn connect_wifi(&self, _ssid: &str, _password: &str) -> Result<(), String> {
        Ok(())
    }

    fn disconnect(&self, _interface: &str) -> Result<(), String> {
        Ok(())
    }

    fn get_status(&self) -> NetworkStatus {
        NetworkStatus::Connected {
            ip_address: "192.168.64.15".to_string(),
            ssid: Some("SLOPOS-I-WiFi".to_string()),
        }
    }
}

/// Fallback / Mock Audio Service for PipeWire/PulseAudio D-Bus.
#[derive(Debug, Default)]
pub struct MockAudioService;

impl AudioServiceTrait for MockAudioService {
    fn get_master_volume(&self) -> f32 {
        0.80
    }

    fn set_master_volume(&self, _volume: f32) -> Result<(), String> {
        Ok(())
    }

    fn is_muted(&self) -> bool {
        false
    }

    fn set_muted(&self, _muted: bool) -> Result<(), String> {
        Ok(())
    }
}

/// Fallback / Mock Power Service for systemd/logind & UPower.
#[derive(Debug, Default)]
pub struct MockPowerService;

impl PowerServiceTrait for MockPowerService {
    fn get_battery_level(&self) -> Option<f32> {
        Some(100.0)
    }

    fn is_charging(&self) -> bool {
        true
    }

    fn suspend(&self) -> Result<(), String> {
        Ok(())
    }

    fn shutdown(&self) -> Result<(), String> {
        Ok(())
    }

    fn reboot(&self) -> Result<(), String> {
        Ok(())
    }
}
