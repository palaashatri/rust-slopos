use slopos_bus::transport::LocalTransport;
use slopos_bus::{BusMessage, Command, Event, MessageKind, Query, ServiceRegistry, Transport};

#[test]
fn test_message_creation() {
    let msg = BusMessage {
        id: "msg-1".to_string(),
        source: "service-1".to_string(),
        target: None,
        kind: MessageKind::Command(Command::SetTheme {
            name: "Platinum".to_string(),
        }),
        payload: serde_json::Value::Null,
        timestamp: 0,
    };
    assert_eq!(msg.id, "msg-1");
    assert_eq!(msg.source, "service-1");
}

#[test]
fn test_service_registry() {
    let mut registry = ServiceRegistry::new();
    registry.register(
        "test-service".to_string(),
        "Test",
        Box::new(|msg| Ok(Some(msg))),
    );

    assert!(registry.lookup("test-service").is_some());
    assert_eq!(registry.services().len(), 1);

    let msg = BusMessage {
        id: "msg-2".to_string(),
        source: "caller".to_string(),
        target: Some("test-service".to_string()),
        kind: MessageKind::Query(Query::GetTheme),
        payload: serde_json::Value::Null,
        timestamp: 0,
    };
    let response = registry.send(msg).unwrap();
    assert!(response.is_some());
}

#[test]
fn test_local_transport() {
    let mut transport = LocalTransport::new();
    assert!(transport.is_connected());
    transport.disconnect().unwrap();
    assert!(!transport.is_connected());
    transport.connect("localhost").unwrap();
    assert!(transport.is_connected());

    let msg = BusMessage {
        id: "msg-3".to_string(),
        source: "caller".to_string(),
        target: None,
        kind: MessageKind::Event(Event::ScreenLocked),
        payload: serde_json::Value::Null,
        timestamp: 0,
    };
    transport.send(msg).unwrap();
    transport.disconnect().unwrap();
    assert!(!transport.is_connected());
}
