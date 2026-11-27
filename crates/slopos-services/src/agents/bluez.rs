//! BlueZ pairing agent (contract §18.11, GAP-SEC-04).
//!
//! BlueZ calls [`BluezAgent`] during pairing. PIN and passkey entry, and
//! confirmation and authorization, are answered in a dialog. A passkey that
//! must be typed on the device is shown in a dialog that stays open until
//! BlueZ calls `Cancel` (or the user closes it).

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use zbus::{zvariant::OwnedObjectPath, Connection};

use super::{
    close, next_prompt_id, register_while_owned, show, show_detached, AgentError, DialogAnswer,
    DialogSpec, FieldKind, FieldSpec, Presenter, PromptOutcome, Secret,
};

pub const AGENT_PATH: &str = "/org/slopos/BluezAgent";
/// BlueZ capability: the agent can show a passkey and take input.
pub const CAPABILITY: &str = "KeyboardDisplay";
const SERVICE: &str = "org.bluez";
const MANAGER_PATH: &str = "/org/bluez";
const MANAGER_INTERFACE: &str = "org.bluez.AgentManager1";
const DEVICE_INTERFACE: &str = "org.bluez.Device1";
const LABEL_TIMEOUT: Duration = Duration::from_secs(2);
const PIN_MAX_LEN: usize = 16;
const PASSKEY_DIGITS: usize = 6;

/// A request BlueZ makes of the agent, with the values it passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingRequest {
    PinCode {
        device: String,
    },
    Passkey {
        device: String,
    },
    DisplayPinCode {
        device: String,
        pin: String,
    },
    DisplayPasskey {
        device: String,
        passkey: u32,
        entered: u16,
    },
    Confirmation {
        device: String,
        passkey: u32,
    },
    Authorization {
        device: String,
    },
    AuthorizeService {
        device: String,
        uuid: String,
    },
}

impl PairingRequest {
    /// Display-only requests return at once and stay on screen until BlueZ
    /// cancels them. The others wait for an answer.
    pub fn interaction(&self) -> Interaction {
        match self {
            Self::DisplayPinCode { .. } | Self::DisplayPasskey { .. } => Interaction::Display,
            _ => Interaction::Answer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    /// The agent waits for the user's answer.
    Answer,
    /// The agent only shows a value.
    Display,
}

/// The device name to show: its alias, then its address, then the object
/// path's last segment.
pub fn label_from(alias: Option<&str>, address: Option<&str>, device_path: &str) -> String {
    [alias, address]
        .into_iter()
        .flatten()
        .find(|text| !text.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            device_path
                .rsplit('/')
                .next()
                .unwrap_or(device_path)
                .to_owned()
        })
}

/// Map a request to the dialog that shows or asks for it.
pub fn pairing_dialog(request: &PairingRequest, device_label: &str) -> DialogSpec {
    let title = "Pair device".to_owned();
    match request {
        PairingRequest::PinCode { .. } => DialogSpec {
            title,
            message: format!("Enter the PIN code for {device_label}."),
            buttons: vec!["Cancel".into(), "Pair".into()],
            field: Some(FieldSpec {
                label: "PIN code".into(),
                kind: FieldKind::Secret,
                min_len: 1,
                max_len: PIN_MAX_LEN,
            }),
        },
        PairingRequest::Passkey { .. } => DialogSpec {
            title,
            message: format!("Enter the {PASSKEY_DIGITS}-digit passkey shown on {device_label}."),
            buttons: vec!["Cancel".into(), "Pair".into()],
            field: Some(FieldSpec {
                label: "Passkey".into(),
                kind: FieldKind::Digits,
                min_len: PASSKEY_DIGITS,
                max_len: PASSKEY_DIGITS,
            }),
        },
        PairingRequest::DisplayPinCode { pin, .. } => DialogSpec {
            title,
            message: format!("Type this PIN code on {device_label}: {pin}"),
            buttons: vec!["Close".into()],
            field: None,
        },
        PairingRequest::DisplayPasskey {
            passkey, entered, ..
        } => DialogSpec {
            title,
            message: format!(
                "Type {passkey:0width$} on {device_label}. {entered} digits typed so far.",
                width = PASSKEY_DIGITS
            ),
            buttons: vec!["Close".into()],
            field: None,
        },
        PairingRequest::Confirmation { passkey, .. } => DialogSpec {
            title,
            message: format!(
                "Confirm that {device_label} shows the passkey {passkey:0width$}.",
                width = PASSKEY_DIGITS
            ),
            buttons: vec!["No".into(), "Yes".into()],
            field: None,
        },
        PairingRequest::Authorization { .. } => DialogSpec {
            title,
            message: format!("Allow {device_label} to pair with this computer?"),
            buttons: vec!["No".into(), "Yes".into()],
            field: None,
        },
        PairingRequest::AuthorizeService { uuid, .. } => DialogSpec {
            title,
            message: format!("Allow {device_label} to use the service {uuid}?"),
            buttons: vec!["No".into(), "Yes".into()],
            field: None,
        },
    }
}

/// The value an interactive request returns to BlueZ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingReply {
    Pin(Secret),
    Passkey(u32),
    /// Confirmation or authorization accepted.
    Accepted,
}

/// The BlueZ error an interactive request returns instead of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingFailure {
    /// `org.bluez.Error.Canceled`: the user cancelled or BlueZ cancelled.
    Canceled(&'static str),
    /// `org.bluez.Error.Rejected`: the user declined, or no dialog could ask.
    Rejected(&'static str),
}

/// Validate a typed PIN: 1 to 16 characters.
pub fn valid_pin(text: &str) -> bool {
    (1..=PIN_MAX_LEN).contains(&text.chars().count())
}

/// Parse a passkey: exactly six ASCII digits, value up to 999999.
pub fn parse_passkey(text: &str) -> Option<u32> {
    if text.len() != PASSKEY_DIGITS || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|value| *value <= 999_999)
}

/// Map the dialog outcome of an interactive request to BlueZ's answer.
pub fn pairing_reply(
    request: &PairingRequest,
    outcome: PromptOutcome,
) -> Result<PairingReply, PairingFailure> {
    match outcome {
        PromptOutcome::Cancelled => Err(PairingFailure::Canceled("the pairing was cancelled")),
        PromptOutcome::Unavailable => Err(PairingFailure::Rejected("no dialog is available")),
        PromptOutcome::Answered(DialogAnswer::Dismissed) => {
            Err(PairingFailure::Canceled("the dialog was closed"))
        }
        PromptOutcome::Answered(DialogAnswer::Chosen { index, text }) => {
            let yes = index != DialogSpec::CANCEL;
            match request {
                PairingRequest::PinCode { .. } => match text {
                    Some(pin) if yes && valid_pin(pin.expose()) => Ok(PairingReply::Pin(pin)),
                    _ if !yes => Err(PairingFailure::Canceled("the PIN was cancelled")),
                    _ => Err(PairingFailure::Rejected("the PIN is not valid")),
                },
                PairingRequest::Passkey { .. } => {
                    match text.as_ref().and_then(|t| parse_passkey(t.expose())) {
                        Some(passkey) if yes => Ok(PairingReply::Passkey(passkey)),
                        _ if !yes => Err(PairingFailure::Canceled("the passkey was cancelled")),
                        _ => Err(PairingFailure::Rejected("the passkey is not valid")),
                    }
                }
                PairingRequest::Confirmation { .. } => {
                    if yes {
                        Ok(PairingReply::Accepted)
                    } else {
                        Err(PairingFailure::Rejected("the passkey was not confirmed"))
                    }
                }
                PairingRequest::Authorization { .. } | PairingRequest::AuthorizeService { .. } => {
                    if yes {
                        Ok(PairingReply::Accepted)
                    } else {
                        Err(PairingFailure::Rejected("the pairing was declined"))
                    }
                }
                PairingRequest::DisplayPinCode { .. } | PairingRequest::DisplayPasskey { .. } => {
                    Err(PairingFailure::Canceled("display requests take no answer"))
                }
            }
        }
    }
}

/// Which dialog is open. BlueZ runs one pairing at a time, so the agent
/// keeps at most one.
#[derive(Debug, Default)]
pub struct PairingDialogs {
    open: Option<(u64, Interaction)>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Busy;

impl PairingDialogs {
    /// Open dialog `id`. A second interactive dialog while one is open is
    /// refused. A display dialog replaces whatever is open. Returns the id
    /// of a dialog to close because it was replaced.
    pub fn begin(&mut self, id: u64, interaction: Interaction) -> Result<Option<u64>, Busy> {
        match (self.open, interaction) {
            (Some((_, Interaction::Answer)), Interaction::Answer) => Err(Busy),
            (open, _) => {
                let replaced = open.map(|(previous, _)| previous);
                self.open = Some((id, interaction));
                Ok(replaced)
            }
        }
    }

    /// Forget dialog `id` once it is answered. A stale id changes nothing.
    pub fn finish(&mut self, id: u64) {
        if matches!(self.open, Some((open, _)) if open == id) {
            self.open = None;
        }
    }

    /// Take the open dialog's id so it can be closed (BlueZ `Cancel` or
    /// `Release`).
    pub fn cancel(&mut self) -> Option<u64> {
        self.open.take().map(|(id, _)| id)
    }
}

/// Read the device's alias and address. The lookup runs as a tokio task:
/// agent methods run on zbus's executor, which has no tokio timer, so the
/// timeout must be armed inside the runtime.
async fn lookup_device_names(
    runtime: &tokio::runtime::Handle,
    connection: &Connection,
    device: &str,
) -> (Option<String>, Option<String>) {
    let connection = connection.clone();
    let device = device.to_owned();
    let lookup = runtime.spawn(async move {
        tokio::time::timeout(LABEL_TIMEOUT, async {
            let proxy = zbus::Proxy::new(&connection, SERVICE, device.as_str(), DEVICE_INTERFACE)
                .await
                .ok()?;
            let alias: Option<String> = proxy.get_property("Alias").await.ok();
            let address: Option<String> = proxy.get_property("Address").await.ok();
            Some((alias, address))
        })
        .await
    });
    match lookup.await {
        Ok(Ok(Some(names))) => names,
        _ => (None, None),
    }
}

/// The object BlueZ calls.
pub struct BluezAgent {
    connection: Connection,
    presenter: Presenter,
    runtime: tokio::runtime::Handle,
    dialogs: Arc<Mutex<PairingDialogs>>,
}

impl BluezAgent {
    /// Must be called from within a tokio runtime.
    pub fn new(connection: Connection, presenter: Presenter) -> Self {
        Self {
            connection,
            presenter,
            runtime: tokio::runtime::Handle::current(),
            dialogs: Arc::new(Mutex::new(PairingDialogs::default())),
        }
    }

    async fn device_label(&self, device: &str) -> String {
        let (alias, address) = lookup_device_names(&self.runtime, &self.connection, device).await;
        label_from(alias.as_deref(), address.as_deref(), device)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PairingDialogs> {
        self.dialogs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Ask an interactive request and map the answer to BlueZ's reply.
    async fn ask(&self, request: PairingRequest) -> Result<PairingReply, BluezError> {
        let label = self.device_label(device_of(&request)).await;
        let spec = pairing_dialog(&request, &label);
        let id = next_prompt_id();
        let replaced = self
            .lock()
            .begin(id, Interaction::Answer)
            .map_err(|Busy| BluezError::Canceled("another pairing dialog is open".into()))?;
        if let Some(replaced) = replaced {
            close(&self.presenter, replaced).await;
        }
        let outcome = show(&self.presenter, id, spec).await;
        self.lock().finish(id);
        pairing_reply(&request, outcome).map_err(BluezError::from)
    }

    /// Show a display-only request. Its dialog stays open until `Cancel`.
    async fn display(&self, request: PairingRequest) -> Result<(), BluezError> {
        let label = self.device_label(device_of(&request)).await;
        let spec = pairing_dialog(&request, &label);
        let id = next_prompt_id();
        let replaced = self
            .lock()
            .begin(id, Interaction::Display)
            .map_err(|Busy| BluezError::Canceled("another pairing dialog is open".into()))?;
        if let Some(replaced) = replaced {
            close(&self.presenter, replaced).await;
        }
        show_detached(&self.presenter, id, spec).await;
        Ok(())
    }
}

fn device_of(request: &PairingRequest) -> &str {
    match request {
        PairingRequest::PinCode { device }
        | PairingRequest::Passkey { device }
        | PairingRequest::DisplayPinCode { device, .. }
        | PairingRequest::DisplayPasskey { device, .. }
        | PairingRequest::Confirmation { device, .. }
        | PairingRequest::Authorization { device }
        | PairingRequest::AuthorizeService { device, .. } => device,
    }
}

/// BlueZ's `org.bluez.Error.*` errors for agent replies.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum BluezError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
    Canceled(String),
}

impl From<PairingFailure> for BluezError {
    fn from(failure: PairingFailure) -> Self {
        match failure {
            PairingFailure::Canceled(reason) => Self::Canceled(reason.into()),
            PairingFailure::Rejected(reason) => Self::Rejected(reason.into()),
        }
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl BluezAgent {
    async fn release(&self) {
        let closing = self.lock().cancel();
        if let Some(id) = closing {
            close(&self.presenter, id).await;
        }
    }

    async fn request_pin_code(&self, device: OwnedObjectPath) -> Result<String, BluezError> {
        match self
            .ask(PairingRequest::PinCode {
                device: device.to_string(),
            })
            .await?
        {
            PairingReply::Pin(pin) => Ok(pin.expose().to_owned()),
            _ => Err(BluezError::Rejected("unexpected reply".into())),
        }
    }

    async fn display_pin_code(
        &self,
        device: OwnedObjectPath,
        pincode: String,
    ) -> Result<(), BluezError> {
        self.display(PairingRequest::DisplayPinCode {
            device: device.to_string(),
            pin: pincode,
        })
        .await
    }

    async fn request_passkey(&self, device: OwnedObjectPath) -> Result<u32, BluezError> {
        match self
            .ask(PairingRequest::Passkey {
                device: device.to_string(),
            })
            .await?
        {
            PairingReply::Passkey(passkey) => Ok(passkey),
            _ => Err(BluezError::Rejected("unexpected reply".into())),
        }
    }

    async fn display_passkey(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
        entered: u16,
    ) -> Result<(), BluezError> {
        self.display(PairingRequest::DisplayPasskey {
            device: device.to_string(),
            passkey,
            entered,
        })
        .await
    }

    async fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
    ) -> Result<(), BluezError> {
        self.ask(PairingRequest::Confirmation {
            device: device.to_string(),
            passkey,
        })
        .await
        .map(|_| ())
    }

    async fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), BluezError> {
        self.ask(PairingRequest::Authorization {
            device: device.to_string(),
        })
        .await
        .map(|_| ())
    }

    async fn authorize_service(
        &self,
        device: OwnedObjectPath,
        uuid: String,
    ) -> Result<(), BluezError> {
        self.ask(PairingRequest::AuthorizeService {
            device: device.to_string(),
            uuid,
        })
        .await
        .map(|_| ())
    }

    async fn cancel(&self) {
        let closing = self.lock().cancel();
        if let Some(id) = closing {
            close(&self.presenter, id).await;
        }
    }
}

/// Serve the agent on `system` and register it as BlueZ's default agent
/// whenever BlueZ has an owner.
pub async fn serve(system: &Connection, presenter: Presenter) -> Result<(), AgentError> {
    system
        .object_server()
        .at(AGENT_PATH, BluezAgent::new(system.clone(), presenter))
        .await?;
    register_while_owned(system, SERVICE, || register(system)).await
}

/// Call `AgentManager1.RegisterAgent` and `RequestDefaultAgent`.
pub async fn register(system: &Connection) -> zbus::Result<()> {
    let agent = OwnedObjectPath::try_from(AGENT_PATH)?;
    system
        .call_method(
            Some(SERVICE),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "RegisterAgent",
            &(&agent, CAPABILITY),
        )
        .await?;
    system
        .call_method(
            Some(SERVICE),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "RequestDefaultAgent",
            &(&agent,),
        )
        .await
        .map(|_| ())
}
