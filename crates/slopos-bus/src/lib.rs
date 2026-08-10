pub mod message;
pub mod service_registry;
pub mod services;
pub mod session_control;
pub mod spaces;
pub mod transport;

pub use message::*;
pub use service_registry::ServiceRegistry;
pub use services::*;
pub use session_control::*;
pub use spaces::*;
pub use transport::Transport;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, BusError>;

#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("service not found: {0}")]
    ServiceNotFound(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("timeout: {0}")]
    Timeout(String),
}

pub type ServiceId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMessage {
    pub id: String,
    pub source: ServiceId,
    pub target: Option<ServiceId>,
    pub kind: MessageKind,
    pub payload: serde_json::Value,
    pub timestamp: u64,
}

pub struct SloposBus {
    pub registry: Arc<RwLock<ServiceRegistry>>,
    pub transport: Box<dyn Transport>,
}

impl Default for SloposBus {
    fn default() -> Self {
        Self::new(Box::new(transport::LocalTransport::new()))
    }
}

impl SloposBus {
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            registry: Arc::new(RwLock::new(ServiceRegistry::new())),
            transport,
        }
    }

    pub fn send_message(&self, message: BusMessage) -> Result<Option<BusMessage>> {
        if self.transport.is_connected() {
            self.transport.send(message.clone())?;
        }
        self.registry.read().send(message)
    }

    pub fn receive(&self) -> Result<Option<BusMessage>> {
        self.transport.receive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slopos_bus_queue_and_dispatch() {
        let bus = SloposBus::default();
        let msg = BusMessage {
            id: "msg-1".into(),
            source: "shell".into(),
            target: None,
            kind: MessageKind::Event(Event::ThemeChanged {
                name: "graphite".into(),
                is_dark: true,
            }),
            payload: serde_json::Value::Null,
            timestamp: 100,
        };

        bus.send_message(msg.clone()).expect("send success");
        let received = bus.receive().expect("receive success");
        assert!(received.is_some());
        let recv_msg = received.unwrap();
        assert_eq!(recv_msg.id, "msg-1");
    }
}
