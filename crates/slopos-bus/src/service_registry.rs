use crate::{BusError, BusMessage, Result, ServiceId};
use std::collections::HashMap;

pub type ServiceHandler = Box<dyn Fn(BusMessage) -> Result<Option<BusMessage>> + Send + Sync>;

pub struct ServiceRegistration {
    pub id: ServiceId,
    pub name: String,
    pub handler: ServiceHandler,
}

pub struct ServiceRegistry {
    services: HashMap<ServiceId, ServiceRegistration>,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: ServiceId, name: &str, handler: ServiceHandler) {
        self.services.insert(
            id.clone(),
            ServiceRegistration {
                id,
                name: name.to_string(),
                handler,
            },
        );
    }

    pub fn unregister(&mut self, id: &str) {
        self.services.remove(id);
    }

    pub fn send(&self, message: BusMessage) -> Result<Option<BusMessage>> {
        if let Some(target) = &message.target {
            if let Some(service) = self.services.get(target) {
                (service.handler)(message)
            } else {
                Err(BusError::ServiceNotFound(target.clone()))
            }
        } else {
            // Broadcast to all services
            for service in self.services.values() {
                (service.handler)(message.clone())?;
            }
            Ok(None)
        }
    }

    pub fn lookup(&self, id: &str) -> Option<&ServiceRegistration> {
        self.services.get(id)
    }

    pub fn services(&self) -> Vec<&ServiceRegistration> {
        self.services.values().collect()
    }
}
