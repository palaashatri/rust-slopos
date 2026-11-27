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

use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection,
};

use super::{
    close, next_prompt_id, register_while_owned, show, AgentError, DialogAnswer, DialogSpec,
    FieldKind, FieldSpec, Presenter, PromptOutcome, Secret, AGENT_IDENTIFIER,
};

pub const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const MANAGER_SERVICE: &str = "org.freedesktop.NetworkManager";
const MANAGER_PATH: &str = "/org/freedesktop/NetworkManager/AgentManager";
const MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager.AgentManager";

pub const WIRELESS_SECURITY: &str = "802-11-wireless-security";
pub const VPN: &str = "vpn";
const PSK_KEY: &str = "psk";
const VPN_PASSWORD_KEY: &str = "password";

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
        Self {
            connection_path: connection_path.to_owned(),
            connection_id,
            setting: setting.to_owned(),
            hints,
            flags,
        }
    }
}

/// The kind of secret a request asks for, when the agent can supply it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    WifiPsk,
    VpnPassword,
}

/// Why the agent does not prompt for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretRefusal {
    /// NetworkManager did not allow interaction.
    NoInteraction,
    /// The setting or its hints name a secret this agent cannot supply
    /// (for example 802.1X certificates or WEP keys).
    Unsupported,
}

/// Map a request to the dialog that asks for its secret.
pub fn prompt_for(request: &SecretRequest) -> Result<(SecretKind, DialogSpec), SecretRefusal> {
    let kind = secret_kind(&request.setting, &request.hints).ok_or(SecretRefusal::Unsupported)?;
    if !request.flags.allow_interaction() {
        return Err(SecretRefusal::NoInteraction);
    }
    let name = &request.connection_id;
    let spec = match kind {
        SecretKind::WifiPsk => DialogSpec {
            title: "Wi-Fi password".into(),
            message: format!("Enter the password for the Wi-Fi network \"{name}\"."),
            buttons: vec!["Cancel".into(), "Connect".into()],
            field: Some(FieldSpec {
                label: "Password".into(),
                kind: FieldKind::Secret,
                min_len: 8,
                max_len: 64,
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

fn secret_kind(setting: &str, hints: &[String]) -> Option<SecretKind> {
    let hinted = |name: &str| hints.is_empty() || hints.iter().any(|hint| hint == name);
    match setting {
        WIRELESS_SECURITY if hinted(PSK_KEY) => Some(SecretKind::WifiPsk),
        VPN if hinted(VPN_PASSWORD_KEY) => Some(SecretKind::VpnPassword),
        _ => None,
    }
}

/// Whether `text` is an acceptable answer for `kind`: a WPA passphrase of
/// 8 to 63 characters or 64 hex digits, or a non-empty VPN password.
pub fn valid_answer(kind: SecretKind, text: &str) -> bool {
    match kind {
        SecretKind::WifiPsk => crate::network::valid_wpa_passphrase(text),
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
        SecretKind::WifiPsk => {
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

/// Requests still waiting for a dialog answer, one per connection and
/// setting.
#[derive(Debug, Default)]
pub struct SecretRequests {
    pending: HashMap<(String, String), u64>,
}

impl SecretRequests {
    /// Record a request. Returns false when one is already open for the same
    /// connection and setting.
    pub fn begin(&mut self, connection_path: &str, setting: &str, id: u64) -> bool {
        let key = (connection_path.to_owned(), setting.to_owned());
        if self.pending.contains_key(&key) {
            return false;
        }
        self.pending.insert(key, id);
        true
    }

    /// Forget dialog `id` once it has been answered or closed. A stale id
    /// (superseded by a newer request) changes nothing.
    pub fn finish(&mut self, connection_path: &str, setting: &str, id: u64) {
        let key = (connection_path.to_owned(), setting.to_owned());
        if self.pending.get(&key) == Some(&id) {
            self.pending.remove(&key);
        }
    }

    /// Remove the request for CancelGetSecrets and return its dialog id to
    /// close, if there was one.
    pub fn cancel(&mut self, connection_path: &str, setting: &str) -> Option<u64> {
        self.pending
            .remove(&(connection_path.to_owned(), setting.to_owned()))
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
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl NetworkSecretAgent {
    async fn get_secrets(
        &self,
        connection: SettingsMap,
        connection_path: OwnedObjectPath,
        setting_name: String,
        hints: Vec<String>,
        flags: u32,
    ) -> Result<SettingsMap, SecretAgentError> {
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
        let opened = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin(&request.connection_path, &request.setting, id);
        if !opened {
            return Err(SecretAgentError::InternalError(
                "a request for this connection is already open".into(),
            ));
        }
        let outcome = show(&self.presenter, id, spec).await;
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish(&request.connection_path, &request.setting, id);
        let secret = secret_outcome(kind, outcome)?;
        secrets_reply(kind, &secret)
            .map_err(|error| SecretAgentError::InternalError(error.to_string()))
    }

    async fn cancel_get_secrets(
        &self,
        connection_path: OwnedObjectPath,
        setting_name: String,
    ) -> Result<(), SecretAgentError> {
        let closing = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cancel(connection_path.as_str(), &setting_name);
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
/// NetworkManager has an owner.
pub async fn serve(system: &Connection, presenter: Presenter) -> Result<(), AgentError> {
    system
        .object_server()
        .at(AGENT_PATH, NetworkSecretAgent::new(presenter))
        .await?;
    register_while_owned(system, MANAGER_SERVICE, || register(system)).await
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
