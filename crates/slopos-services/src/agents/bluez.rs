//! BlueZ pairing agent (contract §18.11, GAP-SEC-04).
//!
//! BlueZ calls [`BluezAgent`] during pairing. PIN and passkey entry, and
//! confirmation and authorization, are answered in a dialog. A passkey that
//! must be typed on the device is shown in a dialog that stays open until
//! BlueZ calls `Cancel` (or the user closes it), until a newer request
//! replaces it, or until the BlueZ owner that opened it goes away.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::oneshot;
use zbus::{zvariant::OwnedObjectPath, Connection};

use super::{
    await_answer, close, get, next_prompt_id, publish_show, publish_show_detached,
    register_while_owned, reserve, sender_of, AgentError, DialogAnswer, DialogSpec, FieldKind,
    FieldSpec, Presenter, PromptOutcome, Secret,
};

pub const AGENT_PATH: &str = "/org/slopos/BluezAgent";
/// BlueZ capability: the agent can show a passkey and take input.
pub const CAPABILITY: &str = "KeyboardDisplay";
const SERVICE: &str = "org.bluez";
const MANAGER_PATH: &str = "/org/bluez";
const MANAGER_INTERFACE: &str = "org.bluez.AgentManager1";
const DEVICE_INTERFACE: &str = "org.bluez.Device1";
/// BlueZ refuses a second `RegisterAgent` for an agent it already has.
const ALREADY_EXISTS: &str = "org.bluez.Error.AlreadyExists";
const LABEL_TIMEOUT: Duration = Duration::from_secs(2);
/// BlueZ rejects PIN replies longer than 16 bytes, so the limit is in bytes.
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
            message: format!("Enter the PIN code for {device_label} (1 to 16 bytes)."),
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

/// Validate a typed PIN: 1 to 16 bytes.
pub fn valid_pin(text: &str) -> bool {
    (1..=PIN_MAX_LEN).contains(&text.len())
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenDialog {
    id: u64,
    interaction: Interaction,
    /// The unique bus name of the BlueZ owner that opened the dialog.
    owner: String,
}

/// Which dialog is open. BlueZ runs one pairing at a time, so the agent
/// keeps at most one.
#[derive(Debug, Default)]
pub struct PairingDialogs {
    open: Option<OpenDialog>,
    /// The newest id that was accepted. Ids come from one counter, so an id
    /// below this is a request that a newer one has already superseded.
    latest: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Busy;

impl PairingDialogs {
    /// Claim the slot for dialog `id`. A second interactive dialog while one
    /// is open is refused, and so is any request older than the newest
    /// accepted one. A display dialog or a newer interactive request replaces
    /// whatever is open. Returns the id of a dialog to close because it was
    /// replaced.
    pub fn begin(
        &mut self,
        id: u64,
        interaction: Interaction,
        owner: &str,
    ) -> Result<Option<u64>, Busy> {
        if id < self.latest {
            return Err(Busy);
        }
        let answer_open = matches!(
            self.open,
            Some(OpenDialog {
                interaction: Interaction::Answer,
                ..
            })
        );
        if answer_open && interaction == Interaction::Answer {
            return Err(Busy);
        }
        let replaced = self.open.take().map(|open| open.id);
        self.open = Some(OpenDialog {
            id,
            interaction,
            owner: owner.to_owned(),
        });
        self.latest = id;
        Ok(replaced)
    }

    /// Whether dialog `id` still holds the slot. An agent checks this before
    /// it shows the dialog, because a lookup may have been overtaken by
    /// Cancel, Release or a newer request.
    pub fn owns(&self, id: u64) -> bool {
        self.open.as_ref().is_some_and(|open| open.id == id)
    }

    /// Forget dialog `id` once it is answered. Returns false when the dialog
    /// no longer holds the slot, which means it was cancelled or replaced, so
    /// a late answer must be refused.
    pub fn finish(&mut self, id: u64) -> bool {
        if self.owns(id) {
            self.open = None;
            true
        } else {
            false
        }
    }

    /// Take the open dialog's id so it can be closed (BlueZ `Cancel` or
    /// `Release`).
    pub fn cancel(&mut self) -> Option<u64> {
        self.open.take().map(|open| open.id)
    }

    /// Take the open dialog's id when `owner` opened it, for an owner that
    /// went away or was replaced.
    pub fn cancel_owner(&mut self, owner: &str) -> Option<u64> {
        if self.open.as_ref().is_some_and(|open| open.owner == owner) {
            self.cancel()
        } else {
            None
        }
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
    ///
    /// The slot is claimed before the label lookup, so Cancel and Release
    /// that arrive during the lookup find this request and end it. The dialog
    /// is published only while the request still owns the slot.
    async fn ask(&self, owner: &str, request: PairingRequest) -> Result<PairingReply, BluezError> {
        let id = next_prompt_id();
        let replaced = self
            .lock()
            .begin(id, Interaction::Answer, owner)
            .map_err(|Busy| BluezError::Canceled("another pairing dialog is open".into()))?;
        if let Some(replaced) = replaced {
            close(&self.presenter, replaced).await;
        }
        let label = self.device_label(device_of(&request)).await;
        let spec = pairing_dialog(&request, &label);
        let Some(permit) = reserve(&self.presenter).await else {
            self.lock().finish(id);
            return Err(BluezError::Rejected("no dialog is available".into()));
        };
        let answer = {
            let dialogs = self.lock();
            if !dialogs.owns(id) {
                return Err(BluezError::Canceled("the pairing was cancelled".into()));
            }
            publish_show(permit, id, spec)
        };
        let outcome = await_answer(answer).await;
        if !self.lock().finish(id) {
            // Cancelled or replaced while the dialog was open: a late answer
            // is refused.
            return Err(BluezError::Canceled("the pairing was cancelled".into()));
        }
        pairing_reply(&request, outcome).map_err(BluezError::from)
    }

    /// Show a display-only request. Its dialog stays open until `Cancel`, a
    /// newer request replaces it, or its owner goes away. The call returns
    /// once the dialog is published; it does not wait for the dialog to close.
    async fn display(&self, owner: &str, request: PairingRequest) -> Result<(), BluezError> {
        let id = next_prompt_id();
        let replaced = match self.lock().begin(id, Interaction::Display, owner) {
            Ok(replaced) => replaced,
            // A newer display update has already started, so this one is
            // superseded and there is nothing to show.
            Err(Busy) => return Ok(()),
        };
        if let Some(replaced) = replaced {
            close(&self.presenter, replaced).await;
        }
        let label = self.device_label(device_of(&request)).await;
        let spec = pairing_dialog(&request, &label);
        let Some(permit) = reserve(&self.presenter).await else {
            // No host can draw the value. Release the slot only if it is
            // still this request's, and tell BlueZ the display failed.
            self.lock().finish(id);
            return Err(BluezError::Rejected("no dialog is available".into()));
        };
        {
            let dialogs = self.lock();
            if dialogs.owns(id) {
                publish_show_detached(permit, id, spec);
            }
        }
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

    async fn request_pin_code(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        device: OwnedObjectPath,
    ) -> Result<String, BluezError> {
        let owner = sender_of(&header);
        match self
            .ask(
                &owner,
                PairingRequest::PinCode {
                    device: device.to_string(),
                },
            )
            .await?
        {
            PairingReply::Pin(pin) => Ok(pin.expose().to_owned()),
            _ => Err(BluezError::Rejected("unexpected reply".into())),
        }
    }

    async fn display_pin_code(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        device: OwnedObjectPath,
        pincode: String,
    ) -> Result<(), BluezError> {
        let owner = sender_of(&header);
        self.display(
            &owner,
            PairingRequest::DisplayPinCode {
                device: device.to_string(),
                pin: pincode,
            },
        )
        .await
    }

    async fn request_passkey(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        device: OwnedObjectPath,
    ) -> Result<u32, BluezError> {
        let owner = sender_of(&header);
        match self
            .ask(
                &owner,
                PairingRequest::Passkey {
                    device: device.to_string(),
                },
            )
            .await?
        {
            PairingReply::Passkey(passkey) => Ok(passkey),
            _ => Err(BluezError::Rejected("unexpected reply".into())),
        }
    }

    async fn display_passkey(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        device: OwnedObjectPath,
        passkey: u32,
        entered: u16,
    ) -> Result<(), BluezError> {
        let owner = sender_of(&header);
        self.display(
            &owner,
            PairingRequest::DisplayPasskey {
                device: device.to_string(),
                passkey,
                entered,
            },
        )
        .await
    }

    async fn request_confirmation(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        device: OwnedObjectPath,
        passkey: u32,
    ) -> Result<(), BluezError> {
        let owner = sender_of(&header);
        self.ask(
            &owner,
            PairingRequest::Confirmation {
                device: device.to_string(),
                passkey,
            },
        )
        .await
        .map(|_| ())
    }

    async fn request_authorization(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        device: OwnedObjectPath,
    ) -> Result<(), BluezError> {
        let owner = sender_of(&header);
        self.ask(
            &owner,
            PairingRequest::Authorization {
                device: device.to_string(),
            },
        )
        .await
        .map(|_| ())
    }

    async fn authorize_service(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        device: OwnedObjectPath,
        uuid: String,
    ) -> Result<(), BluezError> {
        let owner = sender_of(&header);
        self.ask(
            &owner,
            PairingRequest::AuthorizeService {
                device: device.to_string(),
                uuid,
            },
        )
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
/// whenever BlueZ has an owner. An open dialog is closed when that owner goes
/// away or is replaced.
///
/// The watch ends when `stop` fires or its sender is dropped. Then the open
/// dialog is closed, an accepted registration is undone, and the agent object
/// is removed, so a later call can serve again.
pub async fn serve(
    system: &Connection,
    presenter: Presenter,
    mut stop: oneshot::Receiver<()>,
) -> Result<(), AgentError> {
    let agent = BluezAgent::new(system.clone(), presenter.clone());
    let dialogs = agent.dialogs.clone();
    if !system.object_server().at(AGENT_PATH, agent).await? {
        return Err(AgentError::Bus(zbus::Error::Failure(format!(
            "{AGENT_PATH} is already served"
        ))));
    }
    let registered = Mutex::new(None);
    let owner_lost_dialogs = dialogs.clone();
    let owner_lost_presenter = presenter.clone();
    let result = register_while_owned(
        system,
        SERVICE,
        |_owner: String| register(system),
        move |owner: String| {
            let (dialogs, presenter) = (owner_lost_dialogs.clone(), owner_lost_presenter.clone());
            async move {
                let closing = dialogs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .cancel_owner(&owner);
                if let Some(id) = closing {
                    close(&presenter, id).await;
                }
            }
        },
        &registered,
        &mut stop,
    )
    .await;

    // Teardown runs on every exit from the watch, including a failed one.
    let closing = dialogs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .cancel();
    if let Some(id) = closing {
        close(&presenter, id).await;
    }
    if get(&registered).is_some() {
        // Undo the accepted registration. BlueZ may already have gone away,
        // so a failure here is not reported over the watch's own result.
        let _ = unregister(system).await;
    }
    let _ = system
        .object_server()
        .remove::<BluezAgent, _>(AGENT_PATH)
        .await;
    result
}

/// Call `AgentManager1.UnregisterAgent` for this agent.
pub async fn unregister(system: &Connection) -> zbus::Result<()> {
    let agent = OwnedObjectPath::try_from(AGENT_PATH)?;
    system
        .call_method(
            Some(SERVICE),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "UnregisterAgent",
            &(&agent,),
        )
        .await
        .map(|_| ())
}

/// Call `AgentManager1.RegisterAgent` and `RequestDefaultAgent`.
///
/// An agent BlueZ already has is not an error, and the default step still
/// runs. A failed step is returned, so the caller can retry it.
pub async fn register(system: &Connection) -> zbus::Result<()> {
    let agent = OwnedObjectPath::try_from(AGENT_PATH)?;
    match system
        .call_method(
            Some(SERVICE),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "RegisterAgent",
            &(&agent, CAPABILITY),
        )
        .await
    {
        Ok(_) => {}
        Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == ALREADY_EXISTS => {}
        Err(error) => return Err(error),
    }
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
