//! Pure state-machine and mapping tests, and fixtures on a private bus. The
//! fixtures run fake NetworkManager and BlueZ agent managers that call the
//! agents' methods, so each method's reply is checked over D-Bus.

use std::{
    collections::HashMap,
    io::BufRead,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::PromptOutcome;
use super::{
    bluez::{
        self, BluezAgent, Interaction, PairingDialogs, PairingFailure, PairingReply, PairingRequest,
    },
    network_secret::{
        self, NetworkSecretAgent, SecretFailure, SecretFlags, SecretKind, SecretRefusal,
        SecretRequest, SecretRequests, SettingsMap,
    },
    DialogAnswer, DialogSpec, Presenter, PromptCommand, Secret, AGENT_IDENTIFIER,
};

const WIFI: &str = "802-11-wireless-security";
const NM_SECRET_IFACE: &str = "org.freedesktop.NetworkManager.SecretAgent";
const NM_ERROR_PREFIX: &str = "org.freedesktop.NetworkManager.SecretAgent.Error.";
const BLUEZ_AGENT_IFACE: &str = "org.bluez.Agent1";
const DEVICE_PATH: &str = "/org/bluez/hci0/dev_00_11_22_33_44_55";
const CONNECTION_PATH: &str = "/org/freedesktop/NetworkManager/Settings/7";

fn chosen(index: usize, text: Option<&str>) -> PromptOutcome {
    PromptOutcome::Answered(DialogAnswer::Chosen {
        index,
        text: text.map(Secret::new),
    })
}

fn wifi_request(flags: u32) -> SecretRequest {
    SecretRequest {
        connection_path: CONNECTION_PATH.into(),
        connection_id: "Home".into(),
        setting: WIFI.into(),
        hints: vec!["psk".into()],
        flags: SecretFlags::from_raw(flags),
    }
}

// ----- NetworkManager: pure state and mapping ----------------------------

#[test]
fn secret_flags_read_allow_interaction_bit() {
    assert!(SecretFlags::from_raw(0x1).allow_interaction());
    assert!(SecretFlags::from_raw(0x1 | 0x4).allow_interaction());
    assert!(!SecretFlags::from_raw(0x2 | 0x4).allow_interaction());
    assert!(!SecretFlags::from_raw(0).allow_interaction());
}

#[test]
fn wifi_psk_request_maps_to_a_wifi_password_dialog() {
    let (kind, spec) = network_secret::prompt_for(&wifi_request(0x1)).unwrap();
    assert_eq!(kind, SecretKind::WifiPsk);
    assert_eq!(spec.title, "Wi-Fi password");
    assert!(spec.message.contains("Home"));
    assert_eq!(spec.buttons, ["Cancel", "Connect"]);
    assert_eq!(spec.default_button(), 1);
    let field = spec.field.unwrap();
    assert_eq!((field.min_len, field.max_len), (8, 64));
}

#[test]
fn vpn_password_request_maps_to_a_vpn_dialog() {
    let mut request = wifi_request(0x1);
    request.setting = network_secret::VPN.into();
    request.hints = vec!["password".into()];
    let (kind, spec) = network_secret::prompt_for(&request).unwrap();
    assert_eq!(kind, SecretKind::VpnPassword);
    assert_eq!(spec.title, "VPN password");
}

#[test]
fn unsupported_settings_and_hints_are_refused_and_no_prompt_is_shown_without_interaction() {
    let mut eap = wifi_request(0x1);
    eap.setting = "802-1x".into();
    assert_eq!(
        network_secret::prompt_for(&eap),
        Err(SecretRefusal::Unsupported)
    );

    let mut wep = wifi_request(0x1);
    wep.hints = vec!["wep-key0".into()];
    assert_eq!(
        network_secret::prompt_for(&wep),
        Err(SecretRefusal::Unsupported)
    );

    assert_eq!(
        network_secret::prompt_for(&wifi_request(0)),
        Err(SecretRefusal::NoInteraction)
    );
}

#[test]
fn answers_are_validated_per_secret_kind() {
    assert!(network_secret::valid_answer(
        SecretKind::WifiPsk,
        "12345678"
    ));
    assert!(!network_secret::valid_answer(
        SecretKind::WifiPsk,
        "1234567"
    ));
    assert!(network_secret::valid_answer(
        SecretKind::WifiPsk,
        &"ab".repeat(32)
    ));
    assert!(!network_secret::valid_answer(
        SecretKind::WifiPsk,
        &"x".repeat(64)
    ));
    assert!(network_secret::valid_answer(SecretKind::VpnPassword, "p"));
    assert!(!network_secret::valid_answer(SecretKind::VpnPassword, ""));
}

#[test]
fn dialog_outcomes_map_to_nm_secret_results() {
    let ok = network_secret::secret_outcome(SecretKind::WifiPsk, chosen(1, Some("password1")));
    assert_eq!(ok.unwrap().expose(), "password1");
    assert_eq!(
        network_secret::secret_outcome(SecretKind::WifiPsk, chosen(0, Some("password1"))),
        Err(SecretFailure::UserCanceled)
    );
    assert_eq!(
        network_secret::secret_outcome(
            SecretKind::WifiPsk,
            PromptOutcome::Answered(DialogAnswer::Dismissed)
        ),
        Err(SecretFailure::UserCanceled)
    );
    assert_eq!(
        network_secret::secret_outcome(SecretKind::WifiPsk, PromptOutcome::Cancelled),
        Err(SecretFailure::AgentCanceled)
    );
    assert!(matches!(
        network_secret::secret_outcome(SecretKind::WifiPsk, PromptOutcome::Unavailable),
        Err(SecretFailure::NoSecrets(_))
    ));
    assert!(matches!(
        network_secret::secret_outcome(SecretKind::WifiPsk, chosen(1, Some("short"))),
        Err(SecretFailure::NoSecrets(_))
    ));
}

#[test]
fn reply_nests_the_secret_under_the_setting_nm_asked_about() {
    let wifi =
        network_secret::secrets_reply(SecretKind::WifiPsk, &Secret::new("password1")).unwrap();
    assert_eq!(string_at(&wifi, WIFI, "psk"), "password1");

    let vpn =
        network_secret::secrets_reply(SecretKind::VpnPassword, &Secret::new("vpn-pass")).unwrap();
    let secrets = wifi_value(&vpn, "vpn", "secrets");
    let map: HashMap<String, String> = HashMap::try_from(secrets).unwrap();
    assert_eq!(map.get("password").map(String::as_str), Some("vpn-pass"));
}

#[test]
fn open_requests_are_unique_per_connection_and_setting() {
    let mut requests = SecretRequests::default();
    assert!(requests.begin(CONNECTION_PATH, WIFI, 1));
    assert!(
        !requests.begin(CONNECTION_PATH, WIFI, 2),
        "duplicate refused"
    );
    assert!(
        requests.begin(CONNECTION_PATH, "vpn", 3),
        "other setting allowed"
    );

    // A stale id from a superseded request changes nothing.
    requests.finish(CONNECTION_PATH, WIFI, 99);
    assert_eq!(requests.len(), 2);

    assert_eq!(requests.cancel(CONNECTION_PATH, WIFI), Some(1));
    assert_eq!(requests.cancel(CONNECTION_PATH, WIFI), None);
    requests.finish(CONNECTION_PATH, "vpn", 3);
    assert!(requests.is_empty());
}

#[test]
fn connection_id_comes_from_the_settings_and_falls_back_to_the_path() {
    let mut settings = SettingsMap::new();
    settings.insert(
        "connection".into(),
        HashMap::from([("id".into(), owned("Cafe"))]),
    );
    let named = SecretRequest::new(
        &settings,
        CONNECTION_PATH,
        WIFI,
        vec![],
        SecretFlags::from_raw(1),
    );
    assert_eq!(named.connection_id, "Cafe");
    let unnamed = SecretRequest::new(
        &SettingsMap::new(),
        CONNECTION_PATH,
        WIFI,
        vec![],
        SecretFlags::from_raw(1),
    );
    assert_eq!(unnamed.connection_id, CONNECTION_PATH);
}

// ----- BlueZ: pure state and mapping ---------------------------------------

#[test]
fn device_label_prefers_alias_then_address_then_path() {
    assert_eq!(
        bluez::label_from(Some("Keyboard"), Some("00:11"), DEVICE_PATH),
        "Keyboard"
    );
    assert_eq!(
        bluez::label_from(Some(""), Some("00:11"), DEVICE_PATH),
        "00:11"
    );
    assert_eq!(
        bluez::label_from(None, None, DEVICE_PATH),
        "dev_00_11_22_33_44_55"
    );
}

#[test]
fn pin_and_passkey_validation() {
    assert!(bluez::valid_pin("0000"));
    assert!(!bluez::valid_pin(""));
    assert!(!bluez::valid_pin(&"1".repeat(17)));
    assert_eq!(bluez::parse_passkey("123456"), Some(123_456));
    assert_eq!(bluez::parse_passkey("000000"), Some(0));
    assert_eq!(bluez::parse_passkey("12345"), None);
    assert_eq!(bluez::parse_passkey("1234567"), None);
    assert_eq!(bluez::parse_passkey("12a456"), None);
}

#[test]
fn display_requests_are_detached_and_interactive_ones_wait() {
    let device = DEVICE_PATH.to_owned();
    assert_eq!(
        PairingRequest::DisplayPasskey {
            device: device.clone(),
            passkey: 1,
            entered: 0
        }
        .interaction(),
        Interaction::Display
    );
    assert_eq!(
        PairingRequest::PinCode { device }.interaction(),
        Interaction::Answer
    );
}

#[test]
fn pairing_dialogs_are_specified_per_request_kind() {
    let pin = bluez::pairing_dialog(
        &PairingRequest::PinCode {
            device: DEVICE_PATH.into(),
        },
        "Keyboard",
    );
    assert_eq!(pin.buttons, ["Cancel", "Pair"]);
    assert_eq!(pin.field.as_ref().unwrap().max_len, 16);
    assert!(pin.message.contains("Keyboard"));

    let passkey = bluez::pairing_dialog(
        &PairingRequest::Passkey {
            device: DEVICE_PATH.into(),
        },
        "Keyboard",
    );
    assert_eq!(passkey.field.as_ref().unwrap().min_len, 6);
    assert_eq!(
        passkey.field.as_ref().unwrap().kind,
        super::FieldKind::Digits
    );

    let display = bluez::pairing_dialog(
        &PairingRequest::DisplayPasskey {
            device: DEVICE_PATH.into(),
            passkey: 123,
            entered: 2,
        },
        "Keyboard",
    );
    assert_eq!(display.buttons, ["Close"]);
    assert!(display.field.is_none());
    assert!(display.message.contains("000123"));

    let confirm = bluez::pairing_dialog(
        &PairingRequest::Confirmation {
            device: DEVICE_PATH.into(),
            passkey: 654_321,
        },
        "Keyboard",
    );
    assert_eq!(confirm.buttons, ["No", "Yes"]);
    assert!(confirm.message.contains("654321"));
}

#[test]
fn interactive_answers_map_to_bluez_replies_and_errors() {
    let pin = PairingRequest::PinCode {
        device: DEVICE_PATH.into(),
    };
    assert_eq!(
        bluez::pairing_reply(&pin, chosen(1, Some("0000"))),
        Ok(PairingReply::Pin(Secret::new("0000")))
    );
    assert!(matches!(
        bluez::pairing_reply(&pin, chosen(0, Some("0000"))),
        Err(PairingFailure::Canceled(_))
    ));
    assert!(matches!(
        bluez::pairing_reply(&pin, chosen(1, Some(""))),
        Err(PairingFailure::Rejected(_))
    ));

    let passkey = PairingRequest::Passkey {
        device: DEVICE_PATH.into(),
    };
    assert_eq!(
        bluez::pairing_reply(&passkey, chosen(1, Some("123456"))),
        Ok(PairingReply::Passkey(123_456))
    );
    assert!(matches!(
        bluez::pairing_reply(&passkey, chosen(1, Some("1234567"))),
        Err(PairingFailure::Rejected(_))
    ));

    let confirm = PairingRequest::Confirmation {
        device: DEVICE_PATH.into(),
        passkey: 1,
    };
    assert_eq!(
        bluez::pairing_reply(&confirm, chosen(1, None)),
        Ok(PairingReply::Accepted)
    );
    assert!(matches!(
        bluez::pairing_reply(&confirm, chosen(0, None)),
        Err(PairingFailure::Rejected(_))
    ));

    let service = PairingRequest::AuthorizeService {
        device: DEVICE_PATH.into(),
        uuid: "0000110b".into(),
    };
    assert!(matches!(
        bluez::pairing_reply(&service, chosen(0, None)),
        Err(PairingFailure::Rejected(_))
    ));
    assert!(matches!(
        bluez::pairing_reply(&confirm, PromptOutcome::Cancelled),
        Err(PairingFailure::Canceled(_))
    ));
    assert!(matches!(
        bluez::pairing_reply(&confirm, PromptOutcome::Unavailable),
        Err(PairingFailure::Rejected(_))
    ));
    assert!(matches!(
        bluez::pairing_reply(&confirm, PromptOutcome::Answered(DialogAnswer::Dismissed)),
        Err(PairingFailure::Canceled(_))
    ));
}

#[test]
fn only_one_interactive_dialog_is_open_and_displays_are_replaced() {
    let mut dialogs = PairingDialogs::default();
    assert_eq!(dialogs.begin(1, Interaction::Answer), Ok(None));
    assert_eq!(dialogs.begin(2, Interaction::Answer), Err(bluez::Busy));
    dialogs.finish(2);
    dialogs.finish(1);

    assert_eq!(dialogs.begin(3, Interaction::Display), Ok(None));
    assert_eq!(dialogs.begin(4, Interaction::Display), Ok(Some(3)));
    assert_eq!(dialogs.begin(5, Interaction::Answer), Ok(Some(4)));
    dialogs.finish(4); // stale: changes nothing
    assert_eq!(dialogs.cancel(), Some(5));
    assert_eq!(dialogs.cancel(), None);
}

// ----- Fixtures on a private bus -------------------------------------------

struct BusDaemon(Child);

impl Drop for BusDaemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn private_bus() -> (BusDaemon, String) {
    let mut daemon = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("dbus-daemon must be installed for agent fixture tests");
    let mut address = String::new();
    std::io::BufReader::new(daemon.stdout.as_mut().unwrap())
        .read_line(&mut address)
        .expect("private bus must print its address");
    (BusDaemon(daemon), address.trim().to_owned())
}

type Held = Arc<Mutex<HashMap<u64, oneshot::Sender<DialogAnswer>>>>;

/// What the fixture host did, for assertions.
#[derive(Clone, Default)]
struct HostLog {
    shown: Arc<Mutex<Vec<(u64, DialogSpec)>>>,
    closed: Arc<Mutex<Vec<u64>>>,
}

impl HostLog {
    fn shown_count(&self) -> usize {
        self.shown.lock().unwrap().len()
    }

    fn last_shown(&self) -> (u64, DialogSpec) {
        self.shown.lock().unwrap().last().cloned().unwrap()
    }
}

/// Answers each dialog with `answer`, or holds it open when `answer` returns
/// None, as a host does until the user acts or an agent closes the dialog.
fn spawn_host(
    answer: impl Fn(&DialogSpec) -> Option<DialogAnswer> + Send + 'static,
) -> (Presenter, HostLog) {
    let (presenter, mut commands) = mpsc::channel::<PromptCommand>(16);
    let log = HostLog::default();
    let host_log = log.clone();
    tokio::spawn(async move {
        let held: Held = Arc::new(Mutex::new(HashMap::new()));
        while let Some(command) = commands.recv().await {
            match command {
                PromptCommand::Show { id, spec, reply } => {
                    host_log.shown.lock().unwrap().push((id, spec.clone()));
                    match answer(&spec) {
                        Some(answer) => {
                            let _ = reply.send(answer);
                        }
                        None => {
                            held.lock().unwrap().insert(id, reply);
                        }
                    }
                }
                PromptCommand::Close { id } => {
                    host_log.closed.lock().unwrap().push(id);
                    // Dropping the reply tells the agent it was cancelled.
                    held.lock().unwrap().remove(&id);
                }
            }
        }
    });
    (presenter, log)
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    for _ in 0..400 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition not reached in time");
}

fn owned(text: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(text)).unwrap()
}

fn owned_path(path: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path).unwrap()
}

fn string_of(value: &OwnedValue) -> String {
    String::try_from(value.try_clone().unwrap()).unwrap()
}

fn wifi_value(reply: &SettingsMap, setting: &str, key: &str) -> OwnedValue {
    reply[setting][key].try_clone().unwrap()
}

fn string_at(reply: &SettingsMap, setting: &str, key: &str) -> String {
    string_of(&reply[setting][key])
}

fn error_name(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(name, _, _) => name.as_str().to_owned(),
        other => panic!("expected a D-Bus method error, got {other}"),
    }
}

#[derive(Clone, Default)]
struct FakeNetworkManager {
    registrations: Arc<Mutex<Vec<(String, String)>>>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.AgentManager")]
impl FakeNetworkManager {
    async fn register(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        identifier: String,
    ) {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        self.registrations
            .lock()
            .unwrap()
            .push((sender, identifier));
    }
}

#[derive(Clone, Default)]
struct FakeBluez {
    registrations: Arc<Mutex<Vec<(String, String, String)>>>,
    defaults: Arc<Mutex<Vec<String>>>,
}

#[zbus::interface(name = "org.bluez.AgentManager1")]
impl FakeBluez {
    async fn register_agent(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        agent: OwnedObjectPath,
        capability: String,
    ) {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        self.registrations
            .lock()
            .unwrap()
            .push((sender, agent.to_string(), capability));
    }

    async fn request_default_agent(&self, agent: OwnedObjectPath) {
        self.defaults.lock().unwrap().push(agent.to_string());
    }
}

fn wifi_connection() -> SettingsMap {
    HashMap::from([(
        "connection".to_owned(),
        HashMap::from([("id".to_owned(), owned("Home"))]),
    )])
}

/// A fake NetworkManager on the private bus plus an agent connection that
/// serves the secret agent and registers it, as `serve` does on the system
/// bus.
struct Fixture<M> {
    _bus: BusDaemon,
    _manager_conn: zbus::Connection,
    client: zbus::Connection,
    agent: zbus::Connection,
    fake: M,
}

async fn network_fixture(presenter: Presenter) -> Fixture<FakeNetworkManager> {
    let (bus, address) = private_bus();
    let manager = FakeNetworkManager::default();
    let manager_conn = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/AgentManager",
            manager.clone(),
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let agent_conn = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    agent_conn
        .object_server()
        .at(
            network_secret::AGENT_PATH,
            NetworkSecretAgent::new(presenter),
        )
        .await
        .unwrap();
    network_secret::register(&agent_conn).await.unwrap();
    Fixture {
        _bus: bus,
        _manager_conn: manager_conn,
        client,
        agent: agent_conn,
        fake: manager,
    }
}

async fn secret_proxy(client: &zbus::Connection, agent: &zbus::Connection) -> zbus::Proxy<'static> {
    zbus::Proxy::new(
        client,
        agent.unique_name().unwrap().to_owned(),
        network_secret::AGENT_PATH,
        NM_SECRET_IFACE,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn network_agent_registers_and_returns_the_typed_wifi_password() {
    let (presenter, host) = spawn_host(|spec| {
        (spec.title == "Wi-Fi password").then(|| chosen_answer(1, "correct horse"))
    });
    let fixture = network_fixture(presenter).await;
    let (client, agent, manager) = (
        fixture.client.clone(),
        fixture.agent.clone(),
        fixture.fake.clone(),
    );

    let registrations = manager.registrations.lock().unwrap().clone();
    assert_eq!(
        registrations,
        vec![(
            agent.unique_name().unwrap().to_string(),
            AGENT_IDENTIFIER.to_owned()
        )]
    );

    let proxy = secret_proxy(&client, &agent).await;
    let reply: SettingsMap = proxy
        .call(
            "GetSecrets",
            &(
                wifi_connection(),
                owned_path(CONNECTION_PATH),
                WIFI,
                vec!["psk".to_owned()],
                0x1u32,
            ),
        )
        .await
        .unwrap();
    assert_eq!(string_at(&reply, WIFI, "psk"), "correct horse");
    assert!(host.last_shown().1.message.contains("Home"));
}

fn chosen_answer(index: usize, text: &str) -> DialogAnswer {
    DialogAnswer::Chosen {
        index,
        text: Some(Secret::new(text)),
    }
}

#[tokio::test]
async fn network_agent_refuses_without_interaction_and_reports_user_cancel() {
    let (presenter, host) = spawn_host(|_| Some(DialogAnswer::Dismissed));
    let fixture = network_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = secret_proxy(&client, &agent).await;

    let refused: Result<SettingsMap, zbus::Error> = proxy
        .call(
            "GetSecrets",
            &(
                wifi_connection(),
                owned_path(CONNECTION_PATH),
                WIFI,
                vec!["psk".to_owned()],
                0u32,
            ),
        )
        .await;
    assert_eq!(
        error_name(&refused.unwrap_err()),
        format!("{NM_ERROR_PREFIX}NoSecrets")
    );
    assert_eq!(host.shown_count(), 0, "no dialog without interaction");

    let cancelled: Result<SettingsMap, zbus::Error> = proxy
        .call(
            "GetSecrets",
            &(
                wifi_connection(),
                owned_path(CONNECTION_PATH),
                WIFI,
                vec!["psk".to_owned()],
                0x1u32,
            ),
        )
        .await;
    assert_eq!(
        error_name(&cancelled.unwrap_err()),
        format!("{NM_ERROR_PREFIX}UserCanceled")
    );
}

#[tokio::test]
async fn cancel_get_secrets_closes_the_open_dialog_and_fails_the_request() {
    let (presenter, host) = spawn_host(|_| None);
    let fixture = network_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = secret_proxy(&client, &agent).await;

    let pending = {
        let proxy = proxy.clone();
        tokio::spawn(async move {
            proxy
                .call::<_, _, SettingsMap>(
                    "GetSecrets",
                    &(
                        wifi_connection(),
                        owned_path(CONNECTION_PATH),
                        WIFI,
                        vec!["psk".to_owned()],
                        0x1u32,
                    ),
                )
                .await
        })
    };
    wait_until(|| host.shown_count() == 1).await;
    let (dialog_id, _) = host.last_shown();

    let _: () = proxy
        .call("CancelGetSecrets", &(owned_path(CONNECTION_PATH), WIFI))
        .await
        .unwrap();
    let result = pending.await.unwrap();
    assert_eq!(
        error_name(&result.unwrap_err()),
        format!("{NM_ERROR_PREFIX}AgentCanceled")
    );
    wait_until(|| host.closed.lock().unwrap().contains(&dialog_id)).await;
}

#[tokio::test]
async fn network_agent_without_a_host_reports_no_secrets_and_save_and_delete_succeed() {
    let (presenter, receiver) = mpsc::channel::<PromptCommand>(1);
    drop(receiver);
    let fixture = network_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = secret_proxy(&client, &agent).await;

    let result: Result<SettingsMap, zbus::Error> = proxy
        .call(
            "GetSecrets",
            &(
                wifi_connection(),
                owned_path(CONNECTION_PATH),
                WIFI,
                vec!["psk".to_owned()],
                0x1u32,
            ),
        )
        .await;
    assert_eq!(
        error_name(&result.unwrap_err()),
        format!("{NM_ERROR_PREFIX}NoSecrets")
    );

    let _: () = proxy
        .call(
            "SaveSecrets",
            &(wifi_connection(), owned_path(CONNECTION_PATH)),
        )
        .await
        .unwrap();
    let _: () = proxy
        .call(
            "DeleteSecrets",
            &(wifi_connection(), owned_path(CONNECTION_PATH)),
        )
        .await
        .unwrap();
}

// ----- BlueZ fixture --------------------------------------------------------

async fn bluez_fixture(presenter: Presenter) -> Fixture<FakeBluez> {
    let (bus, address) = private_bus();
    let fake = FakeBluez::default();
    let manager_conn = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .serve_at("/org/bluez", fake.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let agent_conn = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    agent_conn
        .object_server()
        .at(
            bluez::AGENT_PATH,
            BluezAgent::new(agent_conn.clone(), presenter),
        )
        .await
        .unwrap();
    bluez::register(&agent_conn).await.unwrap();
    Fixture {
        _bus: bus,
        _manager_conn: manager_conn,
        client,
        agent: agent_conn,
        fake,
    }
}

async fn bluez_proxy(client: &zbus::Connection, agent: &zbus::Connection) -> zbus::Proxy<'static> {
    zbus::Proxy::new(
        client,
        agent.unique_name().unwrap().to_owned(),
        bluez::AGENT_PATH,
        BLUEZ_AGENT_IFACE,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn bluez_agent_registers_as_default_with_keyboard_display_capability() {
    let (presenter, _host) = spawn_host(|_| None);
    let fixture = bluez_fixture(presenter).await;
    let (agent, fake) = (fixture.agent.clone(), fixture.fake.clone());
    let sender = agent.unique_name().unwrap().to_string();
    assert_eq!(
        fake.registrations.lock().unwrap().clone(),
        vec![(
            sender,
            bluez::AGENT_PATH.to_owned(),
            bluez::CAPABILITY.to_owned()
        )]
    );
    assert_eq!(
        fake.defaults.lock().unwrap().clone(),
        vec![bluez::AGENT_PATH.to_owned()]
    );
}

#[tokio::test]
async fn bluez_agent_answers_pin_passkey_confirmation_and_authorization_requests() {
    let (presenter, host) = spawn_host(|spec| match spec.title.as_str() {
        "Pair device" if spec.field.as_ref().is_some_and(|f| f.max_len == 16) => {
            Some(chosen_answer(1, "0000"))
        }
        "Pair device" if spec.field.as_ref().is_some_and(|f| f.max_len == 6) => {
            Some(chosen_answer(1, "123456"))
        }
        "Pair device" if spec.buttons == ["No", "Yes"] => {
            // Confirmation (yes) and authorization (yes) both accept.
            Some(DialogAnswer::Chosen {
                index: 1,
                text: None,
            })
        }
        _ => None,
    });
    let fixture = bluez_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = bluez_proxy(&client, &agent).await;
    let device = owned_path(DEVICE_PATH);

    let pin: String = proxy.call("RequestPinCode", &(&device,)).await.unwrap();
    assert_eq!(pin, "0000");
    let passkey: u32 = proxy.call("RequestPasskey", &(&device,)).await.unwrap();
    assert_eq!(passkey, 123_456);
    let _: () = proxy
        .call("RequestConfirmation", &(&device, 123_456u32))
        .await
        .unwrap();
    let _: () = proxy
        .call("RequestAuthorization", &(&device,))
        .await
        .unwrap();
    assert_eq!(host.shown_count(), 4);
}

#[tokio::test]
async fn bluez_agent_rejects_declined_confirmation_and_service_authorization() {
    let (presenter, _host) = spawn_host(|_| {
        Some(DialogAnswer::Chosen {
            index: 0,
            text: None,
        })
    });
    let fixture = bluez_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = bluez_proxy(&client, &agent).await;
    let device = owned_path(DEVICE_PATH);

    let confirm: Result<(), zbus::Error> =
        proxy.call("RequestConfirmation", &(&device, 1u32)).await;
    assert_eq!(
        error_name(&confirm.unwrap_err()),
        "org.bluez.Error.Rejected"
    );

    let service: Result<(), zbus::Error> = proxy
        .call(
            "AuthorizeService",
            &(&device, "0000110b-0000-1000-8000-00805f9b34fb"),
        )
        .await;
    assert_eq!(
        error_name(&service.unwrap_err()),
        "org.bluez.Error.Rejected"
    );
}

#[tokio::test]
async fn bluez_display_requests_return_at_once_and_stay_open_until_cancel() {
    let (presenter, host) = spawn_host(|_| None);
    let fixture = bluez_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = bluez_proxy(&client, &agent).await;
    let device = owned_path(DEVICE_PATH);

    let _: () = proxy
        .call("DisplayPasskey", &(&device, 123_456u32, 0u16))
        .await
        .unwrap();
    let (dialog_id, spec) = host.last_shown();
    assert!(spec.message.contains("123456"));
    assert_eq!(spec.buttons, ["Close"]);

    let _: () = proxy.call("Cancel", &()).await.unwrap();
    wait_until(|| host.closed.lock().unwrap().contains(&dialog_id)).await;
}

#[tokio::test]
async fn bluez_display_pin_code_is_closed_by_release() {
    let (presenter, host) = spawn_host(|_| None);
    let fixture = bluez_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = bluez_proxy(&client, &agent).await;
    let device = owned_path(DEVICE_PATH);

    let _: () = proxy
        .call("DisplayPinCode", &(&device, "4321"))
        .await
        .unwrap();
    assert!(host.last_shown().1.message.contains("4321"));
    let (dialog_id, _) = host.last_shown();
    let _: () = proxy.call("Release", &()).await.unwrap();
    wait_until(|| host.closed.lock().unwrap().contains(&dialog_id)).await;
}

#[tokio::test]
async fn bluez_cancel_ends_a_pending_pin_request_and_a_second_one_is_refused() {
    let (presenter, host) = spawn_host(|_| None);
    let fixture = bluez_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = bluez_proxy(&client, &agent).await;
    let device = owned_path(DEVICE_PATH);

    let pending = {
        let proxy = proxy.clone();
        let device = device.clone();
        tokio::spawn(async move {
            proxy
                .call::<_, _, String>("RequestPinCode", &(&device,))
                .await
        })
    };
    wait_until(|| host.shown_count() == 1).await;

    // A second interactive request while the first is open is refused.
    let second: Result<String, zbus::Error> = proxy.call("RequestPinCode", &(&device,)).await;
    assert_eq!(error_name(&second.unwrap_err()), "org.bluez.Error.Canceled");

    let _: () = proxy.call("Cancel", &()).await.unwrap();
    let first = pending.await.unwrap();
    assert_eq!(error_name(&first.unwrap_err()), "org.bluez.Error.Canceled");
}

#[tokio::test]
async fn bluez_pairing_without_a_host_is_rejected_and_release_succeeds() {
    let (presenter, receiver) = mpsc::channel::<PromptCommand>(1);
    drop(receiver);
    let fixture = bluez_fixture(presenter).await;
    let (client, agent) = (fixture.client.clone(), fixture.agent.clone());
    let proxy = bluez_proxy(&client, &agent).await;
    let device = owned_path(DEVICE_PATH);

    let result: Result<u32, zbus::Error> = proxy.call("RequestPasskey", &(&device,)).await;
    assert_eq!(error_name(&result.unwrap_err()), "org.bluez.Error.Rejected");
    let _: () = proxy.call("Release", &()).await.unwrap();
}
