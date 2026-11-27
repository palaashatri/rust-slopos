//! NetworkManager secret agent (contract §18.10, GAP-SEC-03).
//!
//! NetworkManager calls [`NetworkSecretAgent`] when a connection needs a
//! password it does not have. The agent asks the host for the password in a
//! dialog and returns it under the setting NetworkManager asked about. A
//! password is never stored: `SaveSecrets` and `DeleteSecrets` succeed
//! without writing anything, because SLOPOS has no secret storage yet
//! (GAP-SEC-05).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::oneshot;
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection,
};

use super::{
    await_answer, close, get, next_prompt_id, publish_show, register_while_owned, reserve,
    sender_of, AgentError, DialogAnswer, DialogSpec, FieldKind, FieldSpec, Presenter,
    PromptOutcome, Secret, AGENT_IDENTIFIER,
};

pub const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const MANAGER_SERVICE: &str = "org.freedesktop.NetworkManager";
const MANAGER_PATH: &str = "/org/freedesktop/NetworkManager/AgentManager";
const MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager.AgentManager";

pub const WIRELESS_SECURITY: &str = "802-11-wireless-security";
pub const VPN: &str = "vpn";
const PSK_KEY: &str = "psk";
const KEY_MGMT: &str = "key-mgmt";
const VPN_PASSWORD_KEY: &str = "password";
/// The Wi-Fi key-management modes that take a pre-shared key. WPA-PSK takes a
/// passphrase under the WPA rule, and SAE takes a password under its own rule.
/// `none` (WEP) and the 802.1X modes are refused.
const WPA_PSK: &str = "wpa-psk";
const SAE: &str = "sae";

/// `NMSecretAgentGetSecretsFlags`.
const FLAG_ALLOW_INTERACTION: u32 = 0x1;

/// NetworkManager's connection settings: setting name to key to value.
pub type SettingsMap = HashMap<String, HashMap<String, OwnedValue>>;

/// The `NMSecretAgentGetSecretsFlags` bits NetworkManager sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretFlags(u32);

impl SecretFlags {
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The agent may show a dialog. Without this bit it must refuse.
    pub fn allow_interaction(self) -> bool {
        self.0 & FLAG_ALLOW_INTERACTION != 0
    }
}

/// One `GetSecrets` call, with the parts the agent uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRequest {
    pub connection_path: String,
    pub connection_id: String,
    pub setting: String,
    pub hints: Vec<String>,
    pub flags: SecretFlags,
    /// The `key-mgmt` of the connection's Wi-Fi security setting, if it has
    /// one. It decides whether the request is for a pre-shared key.
    pub key_mgmt: Option<String>,
}

impl SecretRequest {
    pub fn new(
        connection: &SettingsMap,
        connection_path: &str,
        setting: &str,
        hints: Vec<String>,
        flags: SecretFlags,
    ) -> Self {
        let connection_id = setting_string(connection, "connection", "id")
            .unwrap_or_else(|| connection_path.to_owned());
        let key_mgmt = setting_string(connection, WIRELESS_SECURITY, KEY_MGMT);
        Self {
            connection_path: connection_path.to_owned(),
            connection_id,
            setting: setting.to_owned(),
            hints,
            flags,
            key_mgmt,
        }
    }
}

/// The kind of secret a request asks for, when the agent can supply it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// A WPA-PSK passphrase (`key-mgmt` `wpa-psk`).
    WifiPsk,
    /// A WPA3-SAE password (`key-mgmt` `sae`).
    WifiSae,
    VpnPassword,
}

/// Why the agent does not prompt for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretRefusal {
    /// NetworkManager did not allow interaction.
    NoInteraction,
    /// The setting, its hints or its key management name a secret this agent
    /// cannot supply (for example 802.1X certificates, WEP keys, or a Wi-Fi
    /// profile without a pre-shared key).
    Unsupported,
}

/// Map a request to the dialog that asks for its secret.
pub fn prompt_for(request: &SecretRequest) -> Result<(SecretKind, DialogSpec), SecretRefusal> {
    // The connection's key management decides the kind of Wi-Fi secret. Hints
    // alone cannot tell a WEP profile from a WPA one, so WEP and enterprise
    // profiles are refused here, before any dialog is opened.
    let kind = secret_kind(
        &request.setting,
        &request.hints,
        request.key_mgmt.as_deref(),
    )
    .ok_or(SecretRefusal::Unsupported)?;
    if !request.flags.allow_interaction() {
        return Err(SecretRefusal::NoInteraction);
    }
    let name = &request.connection_id;
    let spec = match kind {
        SecretKind::WifiPsk => DialogSpec {
            title: "Wi-Fi password".into(),
            message: format!(
                "Enter the password for the Wi-Fi network \"{name}\" (8 to 63 bytes, or 64 hex digits)."
            ),
            buttons: vec!["Cancel".into(), "Connect".into()],
            field: Some(FieldSpec {
                label: "Password".into(),
                kind: FieldKind::WpaPassphrase,
                min_len: 8,
                max_len: 64,
            }),
        },
        SecretKind::WifiSae => DialogSpec {
            title: "Wi-Fi password".into(),
            message: format!(
                "Enter the password for the Wi-Fi network \"{name}\" (1 to 255 bytes)."
            ),
            buttons: vec!["Cancel".into(), "Connect".into()],
            field: Some(FieldSpec {
                label: "Password".into(),
                kind: FieldKind::SaePassphrase,
                min_len: 1,
                max_len: 255,
            }),
        },
        SecretKind::VpnPassword => DialogSpec {
            title: "VPN password".into(),
            message: format!("Enter the password for the VPN \"{name}\"."),
            buttons: vec!["Cancel".into(), "Connect".into()],
            field: Some(FieldSpec {
                label: "Password".into(),
                kind: FieldKind::Secret,
                min_len: 1,
                max_len: 256,
            }),
        },
    };
    Ok((kind, spec))
}

fn secret_kind(setting: &str, hints: &[String], key_mgmt: Option<&str>) -> Option<SecretKind> {
    // An empty hint list is valid for a PSK profile.
    let hinted = |name: &str| hints.is_empty() || hints.iter().any(|hint| hint == name);
    match setting {
        WIRELESS_SECURITY if hinted(PSK_KEY) => match key_mgmt {
            Some(WPA_PSK) => Some(SecretKind::WifiPsk),
            Some(SAE) => Some(SecretKind::WifiSae),
            _ => None,
        },
        VPN if hinted(VPN_PASSWORD_KEY) => Some(SecretKind::VpnPassword),
        _ => None,
    }
}

/// Whether `text` is an acceptable answer for `kind`: a WPA passphrase
/// (8 to 63 bytes, or 64 hex digits), an SAE password (1 to 255 bytes), or a
/// non-empty VPN password. Each Wi-Fi rule is the field's own rule, so the
/// host's Connect check and this check cannot disagree.
pub fn valid_answer(kind: SecretKind, text: &str) -> bool {
    match kind {
        SecretKind::WifiPsk => FieldKind::WpaPassphrase.accepts(text),
        SecretKind::WifiSae => FieldKind::SaePassphrase.accepts(text),
        SecretKind::VpnPassword => !text.is_empty(),
    }
}

/// Why an agent call did not return a secret. Names are NetworkManager's
/// `org.freedesktop.NetworkManager.SecretAgent.Error.*` errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretFailure {
    NoSecrets(&'static str),
    UserCanceled,
    AgentCanceled,
}

/// Turn the dialog outcome into the secret, or the failure to report.
pub fn secret_outcome(kind: SecretKind, outcome: PromptOutcome) -> Result<Secret, SecretFailure> {
    match outcome {
        PromptOutcome::Answered(DialogAnswer::Chosen {
            index,
            text: Some(text),
        }) if index != DialogSpec::CANCEL => {
            if valid_answer(kind, text.expose()) {
                Ok(text)
            } else {
                Err(SecretFailure::NoSecrets("the password is not valid"))
            }
        }
        PromptOutcome::Answered(DialogAnswer::Chosen { .. })
        | PromptOutcome::Answered(DialogAnswer::Dismissed) => Err(SecretFailure::UserCanceled),
        PromptOutcome::Cancelled => Err(SecretFailure::AgentCanceled),
        PromptOutcome::Unavailable => Err(SecretFailure::NoSecrets("no dialog is available")),
    }
}

/// The `a{sa{sv}}` reply NetworkManager expects for `kind`.
pub fn secrets_reply(kind: SecretKind, secret: &Secret) -> zbus::zvariant::Result<SettingsMap> {
    let mut reply = SettingsMap::new();
    match kind {
        SecretKind::WifiPsk | SecretKind::WifiSae => {
            reply.insert(
                WIRELESS_SECURITY.into(),
                HashMap::from([(
                    PSK_KEY.to_owned(),
                    OwnedValue::try_from(Value::from(secret.expose()))?,
                )]),
            );
        }
        SecretKind::VpnPassword => {
            // The `vpn` setting carries its secrets as `a{ss}` under `secrets`.
            let secrets =
                HashMap::from([(VPN_PASSWORD_KEY.to_owned(), secret.expose().to_owned())]);
            reply.insert(
                VPN.into(),
                HashMap::from([(
                    "secrets".to_owned(),
                    OwnedValue::try_from(Value::from(secrets))?,
                )]),
            );
        }
    }
    Ok(reply)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingRequest {
    id: u64,
    /// The unique bus name of the NetworkManager owner that asked.
    owner: String,
}

/// Requests still waiting for a dialog answer, one per connection and
/// setting.
#[derive(Debug, Default)]
pub struct SecretRequests {
    pending: HashMap<(String, String), PendingRequest>,
}

impl SecretRequests {
    /// Record a request from `owner`. Returns false when one is already open
    /// for the same connection and setting.
    pub fn begin(&mut self, connection_path: &str, setting: &str, id: u64, owner: &str) -> bool {
        let key = (connection_path.to_owned(), setting.to_owned());
        if self.pending.contains_key(&key) {
            return false;
        }
        self.pending.insert(
            key,
            PendingRequest {
                id,
                owner: owner.to_owned(),
            },
        );
        true
    }

    /// Forget dialog `id` once it has been answered or closed. Returns false
    /// when the request is no longer open (it was cancelled, or its owner
    /// went away), so a late answer must not be returned. A stale id changes
    /// nothing.
    pub fn finish(&mut self, connection_path: &str, setting: &str, id: u64) -> bool {
        let key = (connection_path.to_owned(), setting.to_owned());
        if self.pending.get(&key).is_some_and(|open| open.id == id) {
            self.pending.remove(&key);
            true
        } else {
            false
        }
    }

    /// Remove the request for CancelGetSecrets and return its dialog id to
    /// close, if there was one.
    pub fn cancel(&mut self, connection_path: &str, setting: &str) -> Option<u64> {
        self.pending
            .remove(&(connection_path.to_owned(), setting.to_owned()))
            .map(|open| open.id)
    }

    /// Whether dialog `id` still holds the request for this connection and
    /// setting. An agent checks this before it shows the dialog, because a
    /// CancelGetSecrets or an owner change may have overtaken the request.
    pub fn owns(&self, connection_path: &str, setting: &str, id: u64) -> bool {
        self.pending
            .get(&(connection_path.to_owned(), setting.to_owned()))
            .is_some_and(|open| open.id == id)
    }

    /// Remove every request and return their dialog ids to close. Used when
    /// the agent stops, so no dialog outlives it.
    pub fn cancel_all(&mut self) -> Vec<u64> {
        self.pending.drain().map(|(_, open)| open.id).collect()
    }

    /// Remove every request from `owner` and return their dialog ids to
    /// close. Used when that NetworkManager owner goes away or is replaced.
    pub fn cancel_owner(&mut self, owner: &str) -> Vec<u64> {
        let mut closing = Vec::new();
        self.pending.retain(|_, open| {
            if open.owner == owner {
                closing.push(open.id);
                false
            } else {
                true
            }
        });
        closing
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

fn setting_string(connection: &SettingsMap, setting: &str, key: &str) -> Option<String> {
    let value = connection.get(setting)?.get(key)?.try_clone().ok()?;
    String::try_from(value).ok()
}

/// Error names NetworkManager's secret agent protocol defines.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.NetworkManager.SecretAgent.Error")]
pub enum SecretAgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    NoSecrets(String),
    UserCanceled(String),
    AgentCanceled(String),
    InternalError(String),
}

impl From<SecretFailure> for SecretAgentError {
    fn from(failure: SecretFailure) -> Self {
        match failure {
            SecretFailure::NoSecrets(reason) => Self::NoSecrets(reason.into()),
            SecretFailure::UserCanceled => Self::UserCanceled("the user cancelled".into()),
            SecretFailure::AgentCanceled => Self::AgentCanceled("the request was cancelled".into()),
        }
    }
}

/// The object NetworkManager calls. Its state is the set of open requests.
pub struct NetworkSecretAgent {
    presenter: Presenter,
    pending: Arc<Mutex<SecretRequests>>,
}

impl NetworkSecretAgent {
    pub fn new(presenter: Presenter) -> Self {
        Self {
            presenter,
            pending: Arc::new(Mutex::new(SecretRequests::default())),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SecretRequests> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Publish the dialog for request `id` and wait for its answer. The slot
    /// is checked and the dialog published under the lock, so a
    /// CancelGetSecrets that arrives first leaves no dialog behind. A request
    /// that finds no host reports `Unavailable`.
    async fn show_for(&self, request: &SecretRequest, id: u64, spec: DialogSpec) -> PromptOutcome {
        let Some(permit) = reserve(&self.presenter).await else {
            return PromptOutcome::Unavailable;
        };
        let answer = {
            let open = self.lock();
            if !open.owns(&request.connection_path, &request.setting, id) {
                return PromptOutcome::Cancelled;
            }
            publish_show(permit, id, spec)
        };
        await_answer(answer).await
    }
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl NetworkSecretAgent {
    async fn get_secrets(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        connection: SettingsMap,
        connection_path: OwnedObjectPath,
        setting_name: String,
        hints: Vec<String>,
        flags: u32,
    ) -> Result<SettingsMap, SecretAgentError> {
        let owner = sender_of(&header);
        let request = SecretRequest::new(
            &connection,
            connection_path.as_str(),
            &setting_name,
            hints,
            SecretFlags::from_raw(flags),
        );
        let (kind, spec) = prompt_for(&request).map_err(|refusal| match refusal {
            SecretRefusal::NoInteraction => {
                SecretAgentError::NoSecrets("interaction is not allowed".into())
            }
            SecretRefusal::Unsupported => {
                SecretAgentError::NoSecrets("this secret is not supported".into())
            }
        })?;
        let id = next_prompt_id();
        if !self
            .lock()
            .begin(&request.connection_path, &request.setting, id, &owner)
        {
            return Err(SecretAgentError::InternalError(
                "a request for this connection is already open".into(),
            ));
        }
        let outcome = self.show_for(&request, id, spec).await;
        // A request cancelled meanwhile (CancelGetSecrets, or its owner going
        // away) gets no secret, even if the host answered in time. This also
        // clears the slot when no host could show the dialog.
        let still_open = self
            .lock()
            .finish(&request.connection_path, &request.setting, id);
        let outcome = if still_open {
            outcome
        } else {
            PromptOutcome::Cancelled
        };
        let secret = secret_outcome(kind, outcome)?;
        secrets_reply(kind, &secret)
            .map_err(|error| SecretAgentError::InternalError(error.to_string()))
    }

    async fn cancel_get_secrets(
        &self,
        connection_path: OwnedObjectPath,
        setting_name: String,
    ) -> Result<(), SecretAgentError> {
        let closing = self.lock().cancel(connection_path.as_str(), &setting_name);
        if let Some(id) = closing {
            close(&self.presenter, id).await;
        }
        Ok(())
    }

    /// Nothing is stored: there is no secret storage yet (GAP-SEC-05).
    async fn save_secrets(
        &self,
        _connection: SettingsMap,
        _connection_path: OwnedObjectPath,
    ) -> Result<(), SecretAgentError> {
        Ok(())
    }

    async fn delete_secrets(
        &self,
        _connection: SettingsMap,
        _connection_path: OwnedObjectPath,
    ) -> Result<(), SecretAgentError> {
        Ok(())
    }
}

/// Serve the agent on `system` and register it with NetworkManager whenever
/// NetworkManager has an owner. Open requests are cancelled when that owner
/// goes away or is replaced.
///
/// The watch ends when `stop` fires or its sender is dropped. Then every open
/// request is cancelled, an accepted registration is undone, and the agent
/// object is removed, so a later call can serve again.
pub async fn serve(
    system: &Connection,
    presenter: Presenter,
    mut stop: oneshot::Receiver<()>,
) -> Result<(), AgentError> {
    let agent = NetworkSecretAgent::new(presenter.clone());
    let pending = agent.pending.clone();
    if !system.object_server().at(AGENT_PATH, agent).await? {
        return Err(AgentError::Bus(zbus::Error::Failure(format!(
            "{AGENT_PATH} is already served"
        ))));
    }
    let registered = Mutex::new(None);
    let owner_lost_pending = pending.clone();
    let owner_lost_presenter = presenter.clone();
    let result = register_while_owned(
        system,
        MANAGER_SERVICE,
        |_owner: String| register(system),
        move |owner: String| {
            let (pending, presenter) = (owner_lost_pending.clone(), owner_lost_presenter.clone());
            async move {
                let closing = pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .cancel_owner(&owner);
                for id in closing {
                    close(&presenter, id).await;
                }
            }
        },
        &registered,
        &mut stop,
    )
    .await;

    // Teardown runs on every exit from the watch, including a failed one.
    let closing = pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .cancel_all();
    for id in closing {
        close(&presenter, id).await;
    }
    if get(&registered).is_some() {
        // Undo the accepted registration. NetworkManager may already have
        // gone away, so a failure here is not reported over the watch's result.
        let _ = unregister(system).await;
    }
    let _ = system
        .object_server()
        .remove::<NetworkSecretAgent, _>(AGENT_PATH)
        .await;
    result
}

/// Call `AgentManager.Register` on NetworkManager.
pub async fn register(system: &Connection) -> zbus::Result<()> {
    system
        .call_method(
            Some(MANAGER_SERVICE),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "Register",
            &(AGENT_IDENTIFIER,),
        )
        .await
        .map(|_| ())
}

/// Call `AgentManager.Unregister` on NetworkManager, which removes the agent
/// registered by this connection.
pub async fn unregister(system: &Connection) -> zbus::Result<()> {
    system
        .call_method(
            Some(MANAGER_SERVICE),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "Unregister",
            &(),
        )
        .await
        .map(|_| ())
}
