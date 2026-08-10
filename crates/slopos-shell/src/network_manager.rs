//! NetworkManager D-Bus client with an Unavailable fallback.
//!
//! On Linux, queries org.freedesktop.NetworkManager over the system bus.
//! On other platforms (or when NM/D-Bus is missing), returns
//! [`ConnectivityState::Unavailable`].

/// High-level NetworkManager connectivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityState {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
    /// D-Bus / NetworkManager not available (or non-Linux host).
    Unavailable,
}

impl ConnectivityState {
    /// Map NetworkManager `NMConnectivityState` integer.
    pub fn from_nm_u32(value: u32) -> Self {
        match value {
            1 => Self::None,
            2 => Self::Portal,
            3 => Self::Limited,
            4 => Self::Full,
            0 => Self::Unknown,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::None => "None",
            Self::Portal => "Portal",
            Self::Limited => "Limited",
            Self::Full => "Full",
            Self::Unavailable => "Unavailable",
        }
    }
}

/// Snapshot of network connectivity suitable for status UI / Settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkStatus {
    pub state: ConnectivityState,
    pub primary_connection_id: Option<String>,
    pub primary_connection_name: Option<String>,
    /// `true` when NetworkManager answered over D-Bus.
    pub available: bool,
}

impl NetworkStatus {
    pub fn unavailable() -> Self {
        Self {
            state: ConnectivityState::Unavailable,
            primary_connection_id: None,
            primary_connection_name: None,
            available: false,
        }
    }

    pub fn summary_line(&self) -> String {
        if !self.available {
            return "Network: Unavailable".to_string();
        }
        match &self.primary_connection_name {
            Some(name) if !name.is_empty() => {
                format!("Network: {} ({})", name, self.state.as_str())
            }
            _ => format!("Network: {}", self.state.as_str()),
        }
    }
}

/// Query NetworkManager connectivity (or return Unavailable).
pub fn get_network_status() -> NetworkStatus {
    #[cfg(target_os = "linux")]
    {
        match query_network_manager() {
            Ok(status) => status,
            Err(err) => {
                tracing::debug!(error = %err, "NetworkManager query failed");
                NetworkStatus::unavailable()
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        NetworkStatus::unavailable()
    }
}

#[cfg(target_os = "linux")]
fn query_network_manager() -> Result<NetworkStatus, Box<dyn std::error::Error>> {
    use zbus::blocking::Connection;
    use zbus::zvariant::OwnedObjectPath;

    let conn = Connection::system()?;
    let nm = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )?;

    let connectivity: u32 = nm.get_property("Connectivity")?;
    let state = ConnectivityState::from_nm_u32(connectivity);

    let primary: OwnedObjectPath = nm.get_property("PrimaryConnection")?;
    let path = primary.as_str();

    let (primary_connection_id, primary_connection_name) = if path.is_empty() || path == "/" {
        (None, None)
    } else {
        let active = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.NetworkManager",
            path,
            "org.freedesktop.NetworkManager.Connection.Active",
        )?;
        let id: String = active.get_property("Id").unwrap_or_default();
        let uuid: String = active.get_property("Uuid").unwrap_or_default();
        let name = if id.is_empty() { None } else { Some(id) };
        let conn_id = if uuid.is_empty() { None } else { Some(uuid) };
        // Prefer human Id as name; fall back to Uuid for id when name missing.
        let (id_out, name_out) = match (conn_id, name) {
            (Some(u), Some(n)) => (Some(u), Some(n)),
            (Some(u), None) => (Some(u.clone()), Some(u)),
            (None, Some(n)) => (Some(n.clone()), Some(n)),
            (None, None) => (None, None),
        };
        (id_out, name_out)
    };

    Ok(NetworkStatus {
        state,
        primary_connection_id,
        primary_connection_name,
        available: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nm_connectivity_mapping() {
        assert_eq!(
            ConnectivityState::from_nm_u32(0),
            ConnectivityState::Unknown
        );
        assert_eq!(ConnectivityState::from_nm_u32(1), ConnectivityState::None);
        assert_eq!(ConnectivityState::from_nm_u32(2), ConnectivityState::Portal);
        assert_eq!(
            ConnectivityState::from_nm_u32(3),
            ConnectivityState::Limited
        );
        assert_eq!(ConnectivityState::from_nm_u32(4), ConnectivityState::Full);
        assert_eq!(
            ConnectivityState::from_nm_u32(99),
            ConnectivityState::Unknown
        );
    }

    #[test]
    fn unavailable_status_summary() {
        let status = NetworkStatus::unavailable();
        assert!(!status.available);
        assert_eq!(status.state, ConnectivityState::Unavailable);
        assert_eq!(status.summary_line(), "Network: Unavailable");
    }

    #[test]
    fn full_status_summary_with_name() {
        let status = NetworkStatus {
            state: ConnectivityState::Full,
            primary_connection_id: Some("uuid".into()),
            primary_connection_name: Some("Wi-Fi".into()),
            available: true,
        };
        assert_eq!(status.summary_line(), "Network: Wi-Fi (Full)");
    }

    #[test]
    fn get_network_status_is_safe_on_host() {
        // Must not panic when NM/D-Bus is absent (macOS CI host).
        let status = get_network_status();
        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(status, NetworkStatus::unavailable());
        }
        let _ = status.summary_line();
    }
}
