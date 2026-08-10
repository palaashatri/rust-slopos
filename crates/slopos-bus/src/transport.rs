use crate::{BusMessage, Result};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub trait Transport: Send + Sync {
    fn send(&self, message: BusMessage) -> Result<()>;
    fn receive(&self) -> Result<Option<BusMessage>>;
    fn connect(&mut self, endpoint: &str) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
}

#[derive(Clone)]
pub struct LocalTransport {
    connected: bool,
    queue: Arc<Mutex<VecDeque<BusMessage>>>,
}

impl Default for LocalTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalTransport {
    pub fn new() -> Self {
        Self {
            connected: true,
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl Transport for LocalTransport {
    fn send(&self, message: BusMessage) -> Result<()> {
        tracing::debug!("[LocalTransport] sent: {:?}", message);
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(message);
        }
        Ok(())
    }

    fn receive(&self) -> Result<Option<BusMessage>> {
        if let Ok(mut q) = self.queue.lock() {
            Ok(q.pop_front())
        } else {
            Ok(None)
        }
    }

    fn connect(&mut self, _endpoint: &str) -> Result<()> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(feature = "dbus")]
pub mod dbus_transport {
    use crate::{BusMessage, Result, Transport};

    pub struct DBusTransport {
        connected: bool,
    }

    impl Default for DBusTransport {
        fn default() -> Self {
            Self::new()
        }
    }

    impl DBusTransport {
        pub fn new() -> Self {
            Self { connected: false }
        }
    }

    impl Transport for DBusTransport {
        fn send(&self, message: BusMessage) -> Result<()> {
            tracing::debug!("[DBus] sending: {:?}", message);
            Ok(())
        }

        fn receive(&self) -> Result<Option<BusMessage>> {
            Ok(None)
        }

        fn connect(&mut self, _endpoint: &str) -> Result<()> {
            self.connected = true;
            Ok(())
        }

        fn disconnect(&mut self) -> Result<()> {
            self.connected = false;
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected
        }
    }
}
