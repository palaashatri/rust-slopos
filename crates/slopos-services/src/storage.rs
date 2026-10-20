use std::{
    collections::HashMap,
    ffi::OsString,
    future::Future,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::StreamExt;
use slopos_core::{capability::Availability, ipc::IpcObjectPath};
use thiserror::Error;
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinSet,
    time::timeout,
};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection, Proxy,
};

const SERVICE: &str = "org.freedesktop.UDisks2";
const ROOT: &str = "/org/freedesktop/UDisks2";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const BLOCK_INTERFACE: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM_INTERFACE: &str = "org.freedesktop.UDisks2.Filesystem";
const DRIVE_INTERFACE: &str = "org.freedesktop.UDisks2.Drive";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const ACTION_TIMEOUT: Duration = Duration::from_secs(30);
const READBACK_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INITIAL_DELAY: Duration = Duration::from_millis(250);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(5);

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
type Properties = HashMap<String, OwnedValue>;

enum RefreshOutcome {
    Snapshot,
    Unavailable(Availability),
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBlock {
    pub path: IpcObjectPath,
    pub id: Option<String>,
    pub device: PathBuf,
    pub preferred_device: PathBuf,
    pub size: u64,
    pub label: Option<String>,
    pub filesystem_type: Option<String>,
    pub uuid: Option<String>,
    pub usage: Option<String>,
    pub hint_ignore: bool,
    pub hint_system: bool,
    pub drive_path: Option<IpcObjectPath>,
    pub filesystem: Option<StorageFilesystem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFilesystem {
    pub mount_points: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageDrive {
    pub path: IpcObjectPath,
    pub id: Option<String>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub size: u64,
    pub media_available: bool,
    pub media_removable: bool,
    pub removable: bool,
    pub ejectable: bool,
    pub can_power_off: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSnapshot {
    pub revision: u64,
    pub availability: Availability,
    pub blocks: Vec<StorageBlock>,
    pub drives: Vec<StorageDrive>,
}

#[derive(Debug)]
pub enum StorageEvent {
    Snapshot(StorageSnapshot),
    Unavailable { revision: u64, error: StorageError },
}

impl StorageEvent {
    fn revision(&self) -> u64 {
        match self {
            Self::Snapshot(snapshot) => snapshot.revision,
            Self::Unavailable { revision, .. } => *revision,
        }
    }
}

pub struct StorageEventStream {
    receiver: mpsc::Receiver<StorageEvent>,
    task: tokio::task::JoinHandle<()>,
    revision: u64,
}

impl StorageEventStream {
    pub async fn next(&mut self) -> Option<StorageEvent> {
        let event = self.receiver.recv().await?;
        self.revision = event.revision();
        Some(event)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

impl Drop for StorageEventStream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("UDisks2 operation timed out")]
    Timeout,
    #[error("UDisks2 {operation} timed out; state must be re-read before retrying: {path}")]
    ActionOutcomeUnknown {
        operation: &'static str,
        path: String,
    },
    #[error("another UDisks2 mutation is already active: {operation} {path}")]
    ActionInProgress {
        operation: &'static str,
        path: String,
    },
    #[error("UDisks2 {operation} worker stopped; state must be re-read: {path}")]
    ActionWorkerStopped {
        operation: &'static str,
        path: String,
    },
    #[error("UDisks2 target not found for {operation}: {path}")]
    TargetNotFound {
        operation: &'static str,
        path: String,
    },
    #[error("UDisks2 {interface} interface is absent at {path}")]
    InterfaceNotFound {
        interface: &'static str,
        path: String,
    },
    #[error("UDisks2 {operation} is unsupported for target: {path}")]
    UnsupportedOperation {
        operation: &'static str,
        path: String,
    },
    #[error("UDisks2 {operation} is invalid for the current state of target: {path}")]
    InvalidState {
        operation: &'static str,
        path: String,
    },
    #[error("UDisks2 {operation} did not verify through authoritative read-back: {path}")]
    ReadBackMismatch {
        operation: &'static str,
        path: String,
    },
    #[error("UDisks2 service is unavailable: {0}")]
    MissingService(#[source] zbus::Error),
    #[error("UDisks2 operation denied: {0}")]
    PermissionDenied(#[source] zbus::Error),
    #[error("UDisks2 D-Bus connection disconnected: {0}")]
    Disconnected(#[source] zbus::Error),
    #[error("invalid UDisks2 object path: {0}")]
    InvalidObjectPath(String),
    #[error("invalid UDisks2 property: {0}")]
    InvalidProperty(String),
    #[error("UDisks2 snapshot revision exhausted")]
    RevisionExhausted,
    #[error("UDisks2 D-Bus error: {0}")]
    Bus(#[source] zbus::Error),
}

impl StorageError {
    pub fn availability(&self) -> Availability {
        match self {
            Self::MissingService(_) => Availability::MissingService,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::Disconnected(_) => Availability::Disconnected,
            Self::Timeout
            | Self::ActionOutcomeUnknown { .. }
            | Self::ActionInProgress { .. }
            | Self::ActionWorkerStopped { .. }
            | Self::TargetNotFound { .. }
            | Self::InterfaceNotFound { .. }
            | Self::UnsupportedOperation { .. }
            | Self::InvalidState { .. }
            | Self::ReadBackMismatch { .. }
            | Self::InvalidObjectPath(_)
            | Self::InvalidProperty(_)
            | Self::RevisionExhausted
            | Self::Bus(_) => Availability::Failed,
        }
    }
}

impl From<zbus::Error> for StorageError {
    fn from(error: zbus::Error) -> Self {
        match io_error_availability(&error) {
            Some(Availability::Disconnected) => return Self::Disconnected(error),
            Some(Availability::PermissionDenied) => return Self::PermissionDenied(error),
            _ => {}
        }
        let name = dbus_error_name(&error);
        match name {
            Some("org.freedesktop.DBus.Error.ServiceUnknown")
            | Some("org.freedesktop.DBus.Error.NameHasNoOwner") => Self::MissingService(error),
            Some(
                "org.freedesktop.DBus.Error.AccessDenied"
                | "org.freedesktop.UDisks2.Error.NotAuthorized"
                | "org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain"
                | "org.freedesktop.UDisks2.Error.NotAuthorizedDismissed",
            ) => Self::PermissionDenied(error),
            Some("org.freedesktop.UDisks2.Error.TimedOut") => Self::Timeout,
            _ => Self::Bus(error),
        }
    }
}

fn dbus_error_name(error: &zbus::Error) -> Option<&str> {
    match error {
        zbus::Error::MethodError(name, _, _) => Some(name.as_str()),
        zbus::Error::FDO(fdo) => match fdo.as_ref() {
            zbus::fdo::Error::ServiceUnknown(_) => {
                Some("org.freedesktop.DBus.Error.ServiceUnknown")
            }
            zbus::fdo::Error::NameHasNoOwner(_) => {
                Some("org.freedesktop.DBus.Error.NameHasNoOwner")
            }
            zbus::fdo::Error::AccessDenied(_) => Some("org.freedesktop.DBus.Error.AccessDenied"),
            zbus::fdo::Error::ZBus(error) => dbus_error_name(error),
            _ => None,
        },
        _ => None,
    }
}

fn io_error_availability(error: &zbus::Error) -> Option<Availability> {
    match error {
        zbus::Error::InputOutput(error) => match error.kind() {
            std::io::ErrorKind::PermissionDenied => Some(Availability::PermissionDenied),
            std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof => Some(Availability::Disconnected),
            _ => None,
        },
        zbus::Error::FDO(error) => match error.as_ref() {
            zbus::fdo::Error::ZBus(error) => io_error_availability(error),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone)]
pub struct StorageClient {
    connection: Connection,
    revision: Arc<AtomicU64>,
    mutation: Arc<Mutex<()>>,
    action_timeout: Duration,
}

impl StorageClient {
    pub async fn connect() -> Result<Self, StorageError> {
        let connection = timeout(OPERATION_TIMEOUT, Connection::system())
            .await
            .map_err(|_| StorageError::Timeout)??;
        Ok(Self {
            connection,
            revision: Arc::new(AtomicU64::new(0)),
            mutation: Arc::new(Mutex::new(())),
            action_timeout: ACTION_TIMEOUT,
        })
    }

    #[cfg(test)]
    fn from_connection(connection: Connection) -> Self {
        Self {
            connection,
            revision: Arc::new(AtomicU64::new(0)),
            mutation: Arc::new(Mutex::new(())),
            action_timeout: ACTION_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_action_timeout(mut self, action_timeout: Duration) -> Self {
        self.action_timeout = action_timeout;
        self
    }

    async fn execute_action<T, F, Fut>(
        &self,
        operation: &'static str,
        path: &IpcObjectPath,
        action: F,
    ) -> Result<T, StorageError>
    where
        T: Send + 'static,
        F: FnOnce(StorageClient, IpcObjectPath) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, StorageError>> + Send + 'static,
    {
        let path_string = path.as_str().to_owned();
        let guard =
            self.mutation
                .clone()
                .try_lock_owned()
                .map_err(|_| StorageError::ActionInProgress {
                    operation,
                    path: path_string.clone(),
                })?;
        let client = self.clone();
        let path = path.clone();
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let _guard = guard;
            let result = action(client, path).await;
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| StorageError::ActionWorkerStopped {
                operation,
                path: path_string,
            })?
    }

    async fn action_timed_out(
        &self,
        operation: &'static str,
        path: &IpcObjectPath,
    ) -> StorageError {
        let _ = timeout(READBACK_TIMEOUT, self.authoritative_snapshot()).await;
        StorageError::ActionOutcomeUnknown {
            operation,
            path: path.as_str().to_owned(),
        }
    }

    pub async fn snapshot(&self) -> Result<StorageSnapshot, StorageError> {
        self.authoritative_snapshot().await
    }

    async fn authoritative_snapshot(&self) -> Result<StorageSnapshot, StorageError> {
        let mut snapshot = timeout(OPERATION_TIMEOUT, self.snapshot_inner())
            .await
            .map_err(|_| StorageError::Timeout)??;
        snapshot.revision = self.next_revision()?;
        Ok(snapshot)
    }

    pub async fn mount(&self, block_path: &IpcObjectPath) -> Result<PathBuf, StorageError> {
        self.execute_action("mount", block_path, |client, path| async move {
            client.mount_inner(&path).await
        })
        .await
    }

    async fn mount_inner(&self, block_path: &IpcObjectPath) -> Result<PathBuf, StorageError> {
        let snapshot = self.authoritative_snapshot().await?;
        let block = require_block(&snapshot, block_path, "mount")?;
        let filesystem =
            block
                .filesystem
                .as_ref()
                .ok_or_else(|| StorageError::InterfaceNotFound {
                    interface: FILESYSTEM_INTERFACE,
                    path: block_path.as_str().to_owned(),
                })?;
        if block.hint_system || block.hint_ignore || !filesystem.mount_points.is_empty() {
            return Err(StorageError::InvalidState {
                operation: "mount",
                path: block_path.as_str().to_owned(),
            });
        }

        let proxy = timeout(
            OPERATION_TIMEOUT,
            Proxy::new(
                &self.connection,
                SERVICE,
                block_path.as_str(),
                FILESYSTEM_INTERFACE,
            ),
        )
        .await
        .map_err(|_| StorageError::Timeout)??;
        let options = HashMap::<String, OwnedValue>::new();
        let returned_path: String =
            match timeout(self.action_timeout, proxy.call("Mount", &(options,))).await {
                Ok(result) => result?,
                Err(_) => return Err(self.action_timed_out("mount", block_path).await),
            };
        let mount_path = PathBuf::from(returned_path);
        if !mount_path.is_absolute() {
            return Err(StorageError::InvalidProperty("MountPath".to_owned()));
        }
        self.wait_for_mount_state(block_path, Some(&mount_path))
            .await?;
        Ok(mount_path)
    }

    pub async fn unmount(&self, block_path: &IpcObjectPath) -> Result<(), StorageError> {
        self.execute_action("unmount", block_path, |client, path| async move {
            client.unmount_inner(&path).await
        })
        .await
    }

    async fn unmount_inner(&self, block_path: &IpcObjectPath) -> Result<(), StorageError> {
        let snapshot = self.authoritative_snapshot().await?;
        let block = require_block(&snapshot, block_path, "unmount")?;
        let filesystem =
            block
                .filesystem
                .as_ref()
                .ok_or_else(|| StorageError::InterfaceNotFound {
                    interface: FILESYSTEM_INTERFACE,
                    path: block_path.as_str().to_owned(),
                })?;
        if filesystem.mount_points.is_empty() || block.hint_system || block.hint_ignore {
            return Err(StorageError::InvalidState {
                operation: "unmount",
                path: block_path.as_str().to_owned(),
            });
        }

        let proxy = timeout(
            OPERATION_TIMEOUT,
            Proxy::new(
                &self.connection,
                SERVICE,
                block_path.as_str(),
                FILESYSTEM_INTERFACE,
            ),
        )
        .await
        .map_err(|_| StorageError::Timeout)??;
        let options = HashMap::<String, OwnedValue>::new();
        match timeout(
            self.action_timeout,
            proxy.call::<_, _, ()>("Unmount", &(options,)),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => return Err(self.action_timed_out("unmount", block_path).await),
        }
        self.wait_for_mount_state(block_path, None).await?;
        Ok(())
    }

    pub async fn eject(&self, drive_path: &IpcObjectPath) -> Result<(), StorageError> {
        self.execute_action("eject", drive_path, |client, path| async move {
            client.eject_inner(&path).await
        })
        .await
    }

    async fn eject_inner(&self, drive_path: &IpcObjectPath) -> Result<(), StorageError> {
        let snapshot = self.authoritative_snapshot().await?;
        let drive = require_drive(&snapshot, drive_path, "eject")?;
        if !drive.ejectable {
            return Err(StorageError::UnsupportedOperation {
                operation: "eject",
                path: drive_path.as_str().to_owned(),
            });
        }
        if drive_has_protected_filesystem(&snapshot, &drive.path) {
            return Err(StorageError::InvalidState {
                operation: "eject",
                path: drive_path.as_str().to_owned(),
            });
        }

        let proxy = timeout(
            OPERATION_TIMEOUT,
            Proxy::new(
                &self.connection,
                SERVICE,
                drive_path.as_str(),
                DRIVE_INTERFACE,
            ),
        )
        .await
        .map_err(|_| StorageError::Timeout)??;
        let options = HashMap::<String, OwnedValue>::new();
        match timeout(
            self.action_timeout,
            proxy.call::<_, _, ()>("Eject", &(options,)),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => return Err(self.action_timed_out("eject", drive_path).await),
        }
        Ok(())
    }

    pub async fn power_off(&self, drive_path: &IpcObjectPath) -> Result<(), StorageError> {
        self.execute_action("power off", drive_path, |client, path| async move {
            client.power_off_inner(&path).await
        })
        .await
    }

    async fn power_off_inner(&self, drive_path: &IpcObjectPath) -> Result<(), StorageError> {
        let snapshot = self.authoritative_snapshot().await?;
        let drive = require_drive(&snapshot, drive_path, "power off")?;
        if !drive.can_power_off {
            return Err(StorageError::UnsupportedOperation {
                operation: "power off",
                path: drive_path.as_str().to_owned(),
            });
        }
        if drive_has_protected_filesystem(&snapshot, &drive.path) {
            return Err(StorageError::InvalidState {
                operation: "power off",
                path: drive_path.as_str().to_owned(),
            });
        }

        let proxy = timeout(
            OPERATION_TIMEOUT,
            Proxy::new(
                &self.connection,
                SERVICE,
                drive_path.as_str(),
                DRIVE_INTERFACE,
            ),
        )
        .await
        .map_err(|_| StorageError::Timeout)??;
        let options = HashMap::<String, OwnedValue>::new();
        match timeout(
            self.action_timeout,
            proxy.call::<_, _, ()>("PowerOff", &(options,)),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => return Err(self.action_timed_out("power off", drive_path).await),
        }
        Ok(())
    }

    async fn wait_for_mount_state(
        &self,
        block_path: &IpcObjectPath,
        expected_mount_path: Option<&std::path::Path>,
    ) -> Result<(), StorageError> {
        let deadline = tokio::time::Instant::now() + READBACK_TIMEOUT;
        loop {
            let snapshot = tokio::time::timeout_at(deadline, self.authoritative_snapshot())
                .await
                .map_err(|_| StorageError::ReadBackMismatch {
                    operation: if expected_mount_path.is_some() {
                        "mount"
                    } else {
                        "unmount"
                    },
                    path: block_path.as_str().to_owned(),
                })??;
            let observed = snapshot
                .blocks
                .iter()
                .find(|block| block.path == *block_path)
                .and_then(|block| block.filesystem.as_ref())
                .is_some_and(|filesystem| match expected_mount_path {
                    Some(path) => filesystem.mount_points.iter().any(|point| point == path),
                    None => filesystem.mount_points.is_empty(),
                });
            if observed {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(StorageError::ReadBackMismatch {
                    operation: if expected_mount_path.is_some() {
                        "mount"
                    } else {
                        "unmount"
                    },
                    path: block_path.as_str().to_owned(),
                });
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + Duration::from_millis(100)).min(deadline),
            )
            .await;
        }
    }

    pub fn watch(&self) -> StorageEventStream {
        let client = self.clone();
        let (sender, receiver) = mpsc::channel(1);
        let task = tokio::spawn(async move { client.watch_task(sender).await });
        StorageEventStream {
            receiver,
            task,
            revision: 0,
        }
    }

    async fn snapshot_inner(&self) -> Result<StorageSnapshot, StorageError> {
        parse_snapshot(&self.managed_objects().await?)
    }

    async fn managed_objects(&self) -> Result<ManagedObjects, StorageError> {
        let proxy = Proxy::new(&self.connection, SERVICE, ROOT, OBJECT_MANAGER).await?;
        Ok(proxy.call("GetManagedObjects", &()).await?)
    }

    async fn watch_task(self, sender: mpsc::Sender<StorageEvent>) {
        let manager = match Proxy::new(&self.connection, SERVICE, ROOT, OBJECT_MANAGER).await {
            Ok(manager) => manager,
            Err(error) => {
                send_unavailable(&self, &sender, error.into()).await;
                return;
            }
        };
        let mut added = match manager.receive_signal("InterfacesAdded").await {
            Ok(stream) => stream,
            Err(error) => {
                send_unavailable(&self, &sender, error.into()).await;
                return;
            }
        };
        let mut removed = match manager.receive_signal("InterfacesRemoved").await {
            Ok(stream) => stream,
            Err(error) => {
                send_unavailable(&self, &sender, error.into()).await;
                return;
            }
        };
        let mut owner_changes = match manager.receive_owner_changed().await {
            Ok(stream) => stream,
            Err(error) => {
                send_unavailable(&self, &sender, error.into()).await;
                return;
            }
        };
        let (change_sender, mut changes) = mpsc::channel(1);
        let mut object_watchers = JoinSet::new();
        let mut watched_paths = Vec::new();
        let mut retrying = false;
        let mut retry_delay = RETRY_INITIAL_DELAY;
        let mut retry_at = tokio::time::Instant::now() + retry_delay;

        let initial = refresh_snapshot(
            &self,
            &sender,
            &mut object_watchers,
            &mut watched_paths,
            &change_sender,
        )
        .await;
        if !schedule_snapshot_retry(initial, &mut retrying, &mut retry_delay, &mut retry_at) {
            stop_watchers(&mut object_watchers).await;
            return;
        }

        loop {
            tokio::select! {
                _ = sender.closed() => break,
                _ = tokio::time::sleep_until(retry_at), if retrying => {
                    let outcome = refresh_snapshot(&self, &sender, &mut object_watchers, &mut watched_paths, &change_sender).await;
                    if !schedule_snapshot_retry(outcome, &mut retrying, &mut retry_delay, &mut retry_at) { break; }
                }
                event = added.next() => {
                    match event {
                        Some(event) if object_added_is_relevant(&event) => {
                            let outcome = refresh_snapshot(&self, &sender, &mut object_watchers, &mut watched_paths, &change_sender).await;
                            if !schedule_snapshot_retry(outcome, &mut retrying, &mut retry_delay, &mut retry_at) {
                                break;
                            }
                        }
                        Some(_) => {}
                        None => {
                            send_unavailable(&self, &sender, disconnected_error()).await;
                            break;
                        }
                    }
                }
                event = removed.next() => {
                    match event {
                        Some(event) if object_removed_is_relevant(&event) => {
                            let outcome = refresh_snapshot(&self, &sender, &mut object_watchers, &mut watched_paths, &change_sender).await;
                            if !schedule_snapshot_retry(outcome, &mut retrying, &mut retry_delay, &mut retry_at) {
                                break;
                            }
                        }
                        Some(_) => {}
                        None => {
                            send_unavailable(&self, &sender, disconnected_error()).await;
                            break;
                        }
                    }
                }
                event = changes.recv() => {
                    if event.is_none() {
                        break;
                    }
                    let outcome = refresh_snapshot(&self, &sender, &mut object_watchers, &mut watched_paths, &change_sender).await;
                    if !schedule_snapshot_retry(outcome, &mut retrying, &mut retry_delay, &mut retry_at) {
                        break;
                    }
                }
                owner = owner_changes.next() => {
                    match owner {
                        Some(Some(_)) => {
                            stop_watchers(&mut object_watchers).await;
                            watched_paths.clear();
                            let outcome = refresh_snapshot(&self, &sender, &mut object_watchers, &mut watched_paths, &change_sender).await;
                            if !schedule_snapshot_retry(outcome, &mut retrying, &mut retry_delay, &mut retry_at) {
                                break;
                            }
                        }
                        Some(None) => {
                            stop_watchers(&mut object_watchers).await;
                            watched_paths.clear();
                            let error = StorageError::from(zbus::Error::FDO(Box::new(
                                zbus::fdo::Error::NameHasNoOwner(SERVICE.to_owned()),
                            )));
                            let outcome = send_unavailable(&self, &sender, error).await;
                            if !schedule_snapshot_retry(outcome, &mut retrying, &mut retry_delay, &mut retry_at) {
                                break;
                            }
                        }
                        None => {
                            send_unavailable(&self, &sender, disconnected_error()).await;
                            break;
                        }
                    }
                }
            }
        }
        stop_watchers(&mut object_watchers).await;
    }

    fn next_revision(&self) -> Result<u64, StorageError> {
        self.revision
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |revision| {
                revision.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| StorageError::RevisionExhausted)
    }
}

fn require_block<'a>(
    snapshot: &'a StorageSnapshot,
    path: &IpcObjectPath,
    operation: &'static str,
) -> Result<&'a StorageBlock, StorageError> {
    if let Some(block) = snapshot.blocks.iter().find(|block| block.path == *path) {
        return Ok(block);
    }
    if snapshot.drives.iter().any(|drive| drive.path == *path) {
        return Err(StorageError::InterfaceNotFound {
            interface: BLOCK_INTERFACE,
            path: path.as_str().to_owned(),
        });
    }
    Err(StorageError::TargetNotFound {
        operation,
        path: path.as_str().to_owned(),
    })
}

fn require_drive<'a>(
    snapshot: &'a StorageSnapshot,
    path: &IpcObjectPath,
    operation: &'static str,
) -> Result<&'a StorageDrive, StorageError> {
    if let Some(drive) = snapshot.drives.iter().find(|drive| drive.path == *path) {
        return Ok(drive);
    }
    if snapshot.blocks.iter().any(|block| block.path == *path) {
        return Err(StorageError::InterfaceNotFound {
            interface: DRIVE_INTERFACE,
            path: path.as_str().to_owned(),
        });
    }
    Err(StorageError::TargetNotFound {
        operation,
        path: path.as_str().to_owned(),
    })
}

fn drive_has_protected_filesystem(snapshot: &StorageSnapshot, drive_path: &IpcObjectPath) -> bool {
    snapshot.blocks.iter().any(|block| {
        block.drive_path.as_ref() == Some(drive_path)
            && (block.hint_system
                || block.hint_ignore
                || block
                    .filesystem
                    .as_ref()
                    .is_some_and(|filesystem| !filesystem.mount_points.is_empty()))
    })
}

fn parse_snapshot(managed: &ManagedObjects) -> Result<StorageSnapshot, StorageError> {
    let mut blocks = Vec::new();
    let mut drives = Vec::new();
    for (object_path, interfaces) in managed {
        if let Some(properties) = interfaces.get(BLOCK_INTERFACE) {
            blocks.push(parse_block(object_path, interfaces, properties)?);
        }
        if let Some(properties) = interfaces.get(DRIVE_INTERFACE) {
            drives.push(parse_drive(object_path, properties)?);
        }
    }
    blocks.sort_by(|left: &StorageBlock, right| left.path.cmp(&right.path));
    drives.sort_by(|left: &StorageDrive, right| left.path.cmp(&right.path));
    Ok(StorageSnapshot {
        revision: 0,
        availability: Availability::Available,
        blocks,
        drives,
    })
}

async fn refresh_snapshot(
    client: &StorageClient,
    sender: &mpsc::Sender<StorageEvent>,
    watchers: &mut JoinSet<()>,
    watched_paths: &mut Vec<String>,
    changes: &mpsc::Sender<()>,
) -> RefreshOutcome {
    let managed = match timeout(OPERATION_TIMEOUT, client.managed_objects()).await {
        Ok(Ok(managed)) => managed,
        Ok(Err(error)) => {
            if matches!(
                error.availability(),
                Availability::MissingService | Availability::Disconnected
            ) {
                stop_watchers(watchers).await;
                watched_paths.clear();
            }
            return send_unavailable(client, sender, error).await;
        }
        Err(_) => return send_unavailable(client, sender, StorageError::Timeout).await,
    };
    let paths = relevant_object_paths(&managed);
    let changed_paths = paths != *watched_paths;
    if changed_paths {
        if let Err(error) = replace_watchers(client, paths, watchers, watched_paths, changes).await
        {
            stop_watchers(watchers).await;
            watched_paths.clear();
            return send_unavailable(client, sender, error).await;
        }
    }
    let managed = if changed_paths {
        match timeout(OPERATION_TIMEOUT, client.managed_objects()).await {
            Ok(Ok(managed)) => managed,
            Ok(Err(error)) => return send_unavailable(client, sender, error).await,
            Err(_) => return send_unavailable(client, sender, StorageError::Timeout).await,
        }
    } else {
        managed
    };
    let mut snapshot = match parse_snapshot(&managed) {
        Ok(snapshot) => snapshot,
        Err(error) => return send_unavailable(client, sender, error).await,
    };
    snapshot.revision = match client.next_revision() {
        Ok(revision) => revision,
        Err(_) => return RefreshOutcome::Closed,
    };
    if sender.send(StorageEvent::Snapshot(snapshot)).await.is_ok() {
        RefreshOutcome::Snapshot
    } else {
        RefreshOutcome::Closed
    }
}

fn schedule_snapshot_retry(
    outcome: RefreshOutcome,
    retrying: &mut bool,
    retry_delay: &mut Duration,
    retry_at: &mut tokio::time::Instant,
) -> bool {
    match outcome {
        RefreshOutcome::Snapshot => {
            *retrying = false;
            *retry_delay = RETRY_INITIAL_DELAY;
            true
        }
        RefreshOutcome::Unavailable(Availability::Failed) => {
            *retrying = true;
            *retry_at = tokio::time::Instant::now() + *retry_delay;
            *retry_delay = retry_delay.saturating_mul(2).min(RETRY_MAX_DELAY);
            true
        }
        RefreshOutcome::Unavailable(_) => {
            *retrying = false;
            *retry_delay = RETRY_INITIAL_DELAY;
            true
        }
        RefreshOutcome::Closed => false,
    }
}

async fn replace_watchers(
    client: &StorageClient,
    paths: Vec<String>,
    watchers: &mut JoinSet<()>,
    watched_paths: &mut Vec<String>,
    changes: &mpsc::Sender<()>,
) -> Result<(), StorageError> {
    stop_watchers(watchers).await;
    let mut ready = Vec::with_capacity(paths.len());
    for path in &paths {
        let (ready_sender, ready_receiver) = oneshot::channel();
        watchers.spawn(watch_object_properties(
            client.clone(),
            path.clone(),
            changes.clone(),
            ready_sender,
        ));
        ready.push(ready_receiver);
    }
    *watched_paths = paths;
    let registrations = futures::future::join_all(ready.into_iter().map(|registration| async {
        timeout(OPERATION_TIMEOUT, registration)
            .await
            .map_err(|_| StorageError::Timeout)?
            .map_err(|_| {
                StorageError::Disconnected(zbus::Error::InputOutput(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "storage watcher stopped",
                ))))
            })?
    }))
    .await;
    registrations.into_iter().collect()
}

async fn stop_watchers(watchers: &mut JoinSet<()>) {
    watchers.abort_all();
    while watchers.join_next().await.is_some() {}
}

async fn watch_object_properties(
    client: StorageClient,
    path: String,
    changes: mpsc::Sender<()>,
    ready: oneshot::Sender<Result<(), StorageError>>,
) {
    let proxy = match Proxy::new(&client.connection, SERVICE, path, PROPERTIES_INTERFACE).await {
        Ok(proxy) => proxy,
        Err(error) => {
            let _ = ready.send(Err(error.into()));
            return;
        }
    };
    let mut signals = match proxy.receive_signal("PropertiesChanged").await {
        Ok(signals) => signals,
        Err(error) => {
            let _ = ready.send(Err(error.into()));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    while let Some(signal) = signals.next().await {
        if !property_signal_is_relevant(&signal) {
            continue;
        }
        match changes.try_send(()) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(())) => {}
            Err(mpsc::error::TrySendError::Closed(())) => break,
        }
    }
}

async fn send_unavailable(
    client: &StorageClient,
    sender: &mpsc::Sender<StorageEvent>,
    error: StorageError,
) -> RefreshOutcome {
    let revision = match client.next_revision() {
        Ok(revision) => revision,
        Err(_) => return RefreshOutcome::Closed,
    };
    let availability = error.availability();
    if sender
        .send(StorageEvent::Unavailable { revision, error })
        .await
        .is_ok()
    {
        RefreshOutcome::Unavailable(availability)
    } else {
        RefreshOutcome::Closed
    }
}

fn disconnected_error() -> StorageError {
    StorageError::Disconnected(zbus::Error::InputOutput(Arc::new(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "D-Bus connection closed",
    ))))
}

fn relevant_object_paths(managed: &ManagedObjects) -> Vec<String> {
    let mut paths: Vec<_> = managed
        .iter()
        .filter(|(_, interfaces)| {
            interfaces
                .keys()
                .any(|interface| is_storage_interface(interface))
        })
        .map(|(path, _)| path.to_string())
        .collect();
    paths.sort();
    paths
}

fn is_storage_interface(interface: &str) -> bool {
    matches!(
        interface,
        BLOCK_INTERFACE | FILESYSTEM_INTERFACE | DRIVE_INTERFACE
    )
}

fn object_added_is_relevant(message: &zbus::Message) -> bool {
    message
        .body()
        .deserialize::<(OwnedObjectPath, HashMap<String, Properties>)>()
        .map(|(_, interfaces)| {
            interfaces
                .keys()
                .any(|interface| is_storage_interface(interface))
        })
        .unwrap_or(true)
}

fn object_removed_is_relevant(message: &zbus::Message) -> bool {
    message
        .body()
        .deserialize::<(OwnedObjectPath, Vec<String>)>()
        .map(|(_, interfaces)| {
            interfaces
                .iter()
                .any(|interface| is_storage_interface(interface))
        })
        .unwrap_or(true)
}

fn property_signal_is_relevant(message: &zbus::Message) -> bool {
    message
        .body()
        .deserialize::<(String, Properties, Vec<String>)>()
        .map(|(interface, _, _)| is_storage_interface(&interface))
        .unwrap_or(true)
}

fn parse_block(
    object_path: &OwnedObjectPath,
    interfaces: &HashMap<String, Properties>,
    properties: &Properties,
) -> Result<StorageBlock, StorageError> {
    let path = ipc_path(object_path, "ObjectPath")?;
    let device = os_path(byte_array_property(properties, "Device")?, "Device")?;
    let preferred_device = os_path(
        byte_array_property(properties, "PreferredDevice")?,
        "PreferredDevice",
    )?;
    let size = u64_property(properties, "Size")?;
    let raw_drive_path = object_path_property(properties, "Drive")?;
    let drive_path = if raw_drive_path.as_str() == "/" {
        None
    } else {
        Some(ipc_path(&raw_drive_path, "Drive")?)
    };
    let filesystem = match interfaces.get(FILESYSTEM_INTERFACE) {
        Some(filesystem) => Some(StorageFilesystem {
            mount_points: byte_arrays_property(filesystem, "MountPoints")?
                .into_iter()
                .map(|point| os_path(point, "MountPoints"))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        None => None,
    };
    Ok(StorageBlock {
        path,
        id: optional_string_property(properties, "Id")?,
        device,
        preferred_device,
        size,
        label: optional_string_property(properties, "IdLabel")?,
        filesystem_type: optional_string_property(properties, "IdType")?,
        uuid: optional_string_property(properties, "IdUUID")?,
        usage: optional_string_property(properties, "IdUsage")?,
        hint_ignore: bool_property(properties, "HintIgnore")?,
        hint_system: bool_property(properties, "HintSystem")?,
        drive_path,
        filesystem,
    })
}

fn parse_drive(
    object_path: &OwnedObjectPath,
    properties: &Properties,
) -> Result<StorageDrive, StorageError> {
    Ok(StorageDrive {
        path: ipc_path(object_path, "Drive")?,
        id: optional_string_property(properties, "Id")?,
        vendor: optional_string_property(properties, "Vendor")?,
        model: optional_string_property(properties, "Model")?,
        size: u64_property(properties, "Size")?,
        media_available: bool_property(properties, "MediaAvailable")?,
        media_removable: bool_property(properties, "MediaRemovable")?,
        removable: bool_property(properties, "Removable")?,
        ejectable: bool_property(properties, "Ejectable")?,
        can_power_off: bool_property(properties, "CanPowerOff")?,
    })
}

fn ipc_path(path: &OwnedObjectPath, name: &str) -> Result<IpcObjectPath, StorageError> {
    IpcObjectPath::new(path.to_string())
        .map_err(|_| StorageError::InvalidObjectPath(name.to_owned()))
}

fn property<'a>(properties: &'a Properties, name: &str) -> Result<&'a OwnedValue, StorageError> {
    properties
        .get(name)
        .ok_or_else(|| StorageError::InvalidProperty(name.to_owned()))
}

fn u64_property(properties: &Properties, name: &str) -> Result<u64, StorageError> {
    property(properties, name)?
        .try_clone()
        .and_then(u64::try_from)
        .map_err(|_| StorageError::InvalidProperty(name.to_owned()))
}

fn bool_property(properties: &Properties, name: &str) -> Result<bool, StorageError> {
    property(properties, name)?
        .try_clone()
        .and_then(bool::try_from)
        .map_err(|_| StorageError::InvalidProperty(name.to_owned()))
}

fn optional_string_property(
    properties: &Properties,
    name: &str,
) -> Result<Option<String>, StorageError> {
    let value = property(properties, name)?
        .try_clone()
        .and_then(String::try_from)
        .map_err(|_| StorageError::InvalidProperty(name.to_owned()))?;
    Ok((!value.is_empty()).then_some(value))
}

fn object_path_property(
    properties: &Properties,
    name: &str,
) -> Result<OwnedObjectPath, StorageError> {
    property(properties, name)?
        .try_clone()
        .and_then(OwnedObjectPath::try_from)
        .map_err(|_| StorageError::InvalidProperty(name.to_owned()))
}

fn byte_array_property(properties: &Properties, name: &str) -> Result<Vec<u8>, StorageError> {
    property(properties, name)?
        .try_clone()
        .and_then(Vec::<u8>::try_from)
        .map_err(|_| StorageError::InvalidProperty(name.to_owned()))
}

fn byte_arrays_property(properties: &Properties, name: &str) -> Result<Vec<Vec<u8>>, StorageError> {
    property(properties, name)?
        .try_clone()
        .and_then(Vec::<Vec<u8>>::try_from)
        .map_err(|_| StorageError::InvalidProperty(name.to_owned()))
}

fn os_path(bytes: Vec<u8>, name: &str) -> Result<PathBuf, StorageError> {
    if bytes.len() < 2 || bytes.last() != Some(&0) {
        return Err(StorageError::InvalidProperty(name.to_owned()));
    }
    let path = &bytes[..bytes.len() - 1];
    if path[0] != b'/' || path.contains(&0) {
        return Err(StorageError::InvalidProperty(name.to_owned()));
    }
    Ok(PathBuf::from(OsString::from_vec(path.to_vec())))
}

#[cfg(test)]
mod tests {
    use super::{
        StorageClient, StorageError, StorageEvent, StorageEventStream, StorageSnapshot,
        OBJECT_MANAGER,
    };
    use slopos_core::{capability::Availability, ipc::IpcObjectPath};
    use std::{
        collections::HashMap,
        io::{BufRead, BufReader},
        os::unix::ffi::OsStrExt,
        path::PathBuf,
        process::{Child, Command, Stdio},
        sync::Arc,
        time::Duration,
    };
    use zbus::{
        zvariant::{OwnedObjectPath, OwnedValue, Str, Value},
        Connection, Proxy,
    };

    const SERVICE: &str = "org.freedesktop.UDisks2";
    const ROOT: &str = "/org/freedesktop/UDisks2";
    const BLOCK: &str = "org.freedesktop.UDisks2.Block";
    const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
    const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
    const ROOT_BLOCK_PATH: &str = "/org/freedesktop/UDisks2/block_devices/vda1";
    const ACTION_BLOCK_PATH: &str = "/org/freedesktop/UDisks2/block_devices/action1";
    const ACTION_DRIVE_PATH: &str = "/org/freedesktop/UDisks2/drives/action";
    const ACTION_MOUNT_PATH: &str = "/run/media/test-volume";
    type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

    struct BusDaemon(Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[derive(Clone)]
    struct BlockFixture {
        path: String,
        device: Vec<u8>,
        preferred_device: Option<Vec<u8>>,
        size: Option<u64>,
        id: String,
        label: String,
        filesystem_type: String,
        uuid: String,
        usage: String,
        hint_ignore: bool,
        hint_system: bool,
        drive_path: String,
        mount_points: Option<Vec<Vec<u8>>>,
    }

    #[derive(Clone)]
    struct DriveFixture {
        path: String,
        id: String,
        vendor: String,
        model: String,
        size: u64,
        media_available: bool,
        media_removable: bool,
        removable: bool,
        ejectable: bool,
        can_power_off: bool,
        missing_property: Option<String>,
    }

    struct FixtureState {
        blocks: Vec<BlockFixture>,
        drives: Vec<DriveFixture>,
        denied: bool,
        next_snapshot_delay: Option<Duration>,
        denied_action: Option<String>,
        next_action_delay: Option<Duration>,
        action_calls: Vec<String>,
    }

    impl FixtureState {
        fn root_block() -> BlockFixture {
            BlockFixture {
                path: "/org/freedesktop/UDisks2/block_devices/vda1".to_owned(),
                device: b"/dev/vda1\0".to_vec(),
                preferred_device: Some(b"/dev/vda1\0".to_vec()),
                size: Some(1234),
                id: "by-id-vda1".to_owned(),
                label: "cloudimg-rootfs".to_owned(),
                filesystem_type: "ext4".to_owned(),
                uuid: "fixture-uuid".to_owned(),
                usage: "filesystem".to_owned(),
                hint_ignore: false,
                hint_system: true,
                drive_path: "/org/freedesktop/UDisks2/drives/virtio".to_owned(),
                mount_points: Some(vec![b"/\0".to_vec()]),
            }
        }

        fn new(blocks: Vec<BlockFixture>, drives: Vec<DriveFixture>) -> Self {
            Self {
                blocks,
                drives,
                denied: false,
                next_snapshot_delay: None,
                denied_action: None,
                next_action_delay: None,
                action_calls: Vec::new(),
            }
        }
    }

    struct UDisksFixture(FixtureState);

    #[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
    impl UDisksFixture {
        fn get_managed_objects(&self) -> zbus::fdo::Result<ManagedObjects> {
            if self.0.denied {
                return Err(zbus::fdo::Error::AccessDenied("fixture denied".to_owned()));
            }
            Ok(fixture_managed_objects(&self.0))
        }
    }

    fn block_properties(block: &BlockFixture) -> HashMap<String, OwnedValue> {
        let mut properties = HashMap::from([
            ("Size".to_owned(), block.size.map(OwnedValue::from)),
            ("IdLabel".to_owned(), Some(text(&block.label))),
            ("Id".to_owned(), Some(text(&block.id))),
            ("IdType".to_owned(), Some(text(&block.filesystem_type))),
            ("IdUUID".to_owned(), Some(text(&block.uuid))),
            ("IdUsage".to_owned(), Some(text(&block.usage))),
            (
                "HintIgnore".to_owned(),
                Some(OwnedValue::from(block.hint_ignore)),
            ),
            (
                "HintSystem".to_owned(),
                Some(OwnedValue::from(block.hint_system)),
            ),
            ("Drive".to_owned(), Some(object_path(&block.drive_path))),
            ("Device".to_owned(), Some(bytes(&block.device))),
            (
                "PreferredDevice".to_owned(),
                block.preferred_device.as_deref().map(bytes),
            ),
        ]);
        properties.retain(|_, value| value.is_some());
        properties
            .into_iter()
            .map(|(name, value)| (name, value.unwrap()))
            .collect()
    }

    fn block_interfaces(block: &BlockFixture) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut interfaces = HashMap::from([(BLOCK.to_owned(), block_properties(block))]);
        if let Some(mount_points) = &block.mount_points {
            interfaces.insert(
                FILESYSTEM.to_owned(),
                HashMap::from([("MountPoints".to_owned(), byte_arrays(mount_points))]),
            );
        }
        interfaces
    }

    fn drive_properties(drive: &DriveFixture) -> HashMap<String, OwnedValue> {
        let mut properties = HashMap::from([
            ("Id".to_owned(), text(&drive.id)),
            ("Vendor".to_owned(), text(&drive.vendor)),
            ("Model".to_owned(), text(&drive.model)),
            ("Size".to_owned(), OwnedValue::from(drive.size)),
            (
                "MediaAvailable".to_owned(),
                OwnedValue::from(drive.media_available),
            ),
            (
                "MediaRemovable".to_owned(),
                OwnedValue::from(drive.media_removable),
            ),
            ("Removable".to_owned(), OwnedValue::from(drive.removable)),
            ("Ejectable".to_owned(), OwnedValue::from(drive.ejectable)),
            (
                "CanPowerOff".to_owned(),
                OwnedValue::from(drive.can_power_off),
            ),
        ]);
        if let Some(missing_property) = &drive.missing_property {
            properties.remove(missing_property);
        }
        properties
    }

    fn fixture_managed_objects(state: &FixtureState) -> ManagedObjects {
        let mut objects = HashMap::new();
        for block in &state.blocks {
            objects.insert(object_path_key(&block.path), block_interfaces(block));
        }
        for drive in &state.drives {
            objects.insert(
                object_path_key(&drive.path),
                HashMap::from([(DRIVE.to_owned(), drive_properties(drive))]),
            );
        }
        objects
    }

    struct WatchFixture(Arc<tokio::sync::RwLock<FixtureState>>);

    #[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
    impl WatchFixture {
        async fn get_managed_objects(&self) -> zbus::fdo::Result<ManagedObjects> {
            let delay = {
                let mut state = self.0.write().await;
                if state.denied {
                    return Err(zbus::fdo::Error::AccessDenied("fixture denied".to_owned()));
                }
                state.next_snapshot_delay.take()
            };
            if let Some(delay) = delay {
                std::thread::sleep(delay);
            }
            let state = self.0.read().await;
            Ok(fixture_managed_objects(&state))
        }

        async fn add_block(
            &self,
            #[zbus(signal_context)] context: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            let mut block = FixtureState::root_block();
            block.path = "/org/freedesktop/UDisks2/block_devices/vdb1".to_owned();
            block.device = b"/dev/vdb1\0".to_vec();
            block.preferred_device = Some(b"/dev/vdb1\0".to_vec());
            block.id = "by-id-vdb1".to_owned();
            let mut state = self.0.write().await;
            if state
                .blocks
                .iter()
                .any(|current| current.path == block.path)
            {
                return Err(zbus::fdo::Error::Failed(
                    "fixture block already exists".to_owned(),
                ));
            }
            state.blocks.push(block.clone());
            drop(state);
            let signal = (object_path_key(&block.path), block_interfaces(&block));
            context
                .connection()
                .emit_signal(
                    None::<&str>,
                    ROOT,
                    OBJECT_MANAGER,
                    "InterfacesAdded",
                    &signal,
                )
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }

        async fn remove_root_block(
            &self,
            #[zbus(signal_context)] context: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            let mut state = self.0.write().await;
            let previous_length = state.blocks.len();
            state.blocks.retain(|block| block.path != ROOT_BLOCK_PATH);
            if state.blocks.len() == previous_length {
                return Err(zbus::fdo::Error::Failed(
                    "fixture block is absent".to_owned(),
                ));
            }
            drop(state);
            let signal = (
                object_path_key(ROOT_BLOCK_PATH),
                vec![BLOCK.to_owned(), FILESYSTEM.to_owned()],
            );
            context
                .connection()
                .emit_signal(
                    None::<&str>,
                    ROOT,
                    OBJECT_MANAGER,
                    "InterfacesRemoved",
                    &signal,
                )
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }

        async fn set_root_label(
            &self,
            label: String,
            #[zbus(signal_context)] context: zbus::object_server::SignalContext<'_>,
        ) -> zbus::fdo::Result<()> {
            let mut state = self.0.write().await;
            let block = state
                .blocks
                .iter_mut()
                .find(|block| block.path == ROOT_BLOCK_PATH)
                .ok_or_else(|| zbus::fdo::Error::Failed("fixture block is absent".to_owned()))?;
            block.label = label.clone();
            drop(state);
            let signal = (
                BLOCK.to_owned(),
                HashMap::from([("IdLabel".to_owned(), text(&label))]),
                Vec::<String>::new(),
            );
            context
                .connection()
                .emit_signal(
                    None::<&str>,
                    ROOT_BLOCK_PATH,
                    super::PROPERTIES_INTERFACE,
                    "PropertiesChanged",
                    &signal,
                )
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
        }
    }

    async fn action_fixture_behavior(
        state: &Arc<tokio::sync::RwLock<FixtureState>>,
        action: &str,
    ) -> zbus::fdo::Result<Option<Duration>> {
        let (denied, delay) = {
            let mut state = state.write().await;
            state.action_calls.push(action.to_owned());
            (
                state.denied_action.as_deref() == Some(action),
                state.next_action_delay.take(),
            )
        };
        if denied {
            Err(zbus::fdo::Error::AccessDenied(
                "fixture action denied".to_owned(),
            ))
        } else {
            Ok(delay)
        }
    }

    struct FilesystemActionFixture(Arc<tokio::sync::RwLock<FixtureState>>);

    #[zbus::interface(name = "org.freedesktop.UDisks2.Filesystem")]
    impl FilesystemActionFixture {
        async fn mount(&self, options: HashMap<String, OwnedValue>) -> zbus::fdo::Result<String> {
            if !options.is_empty() {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "fixture accepts only empty options".to_owned(),
                ));
            }
            let delay = action_fixture_behavior(&self.0, "Mount").await?;
            {
                let mut state = self.0.write().await;
                let block = state
                    .blocks
                    .iter_mut()
                    .find(|block| block.path == ACTION_BLOCK_PATH)
                    .ok_or_else(|| zbus::fdo::Error::Failed("block disappeared".to_owned()))?;
                block.mount_points = Some(vec![format!("{ACTION_MOUNT_PATH}\0").into_bytes()]);
            }
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            Ok(ACTION_MOUNT_PATH.to_owned())
        }

        async fn unmount(&self, options: HashMap<String, OwnedValue>) -> zbus::fdo::Result<()> {
            if !options.is_empty() {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "fixture accepts only empty options".to_owned(),
                ));
            }
            let delay = action_fixture_behavior(&self.0, "Unmount").await?;
            {
                let mut state = self.0.write().await;
                let block = state
                    .blocks
                    .iter_mut()
                    .find(|block| block.path == ACTION_BLOCK_PATH)
                    .ok_or_else(|| zbus::fdo::Error::Failed("block disappeared".to_owned()))?;
                block.mount_points = Some(Vec::new());
            }
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            Ok(())
        }
    }

    struct DriveActionFixture(Arc<tokio::sync::RwLock<FixtureState>>);

    #[zbus::interface(name = "org.freedesktop.UDisks2.Drive")]
    impl DriveActionFixture {
        async fn eject(&self, options: HashMap<String, OwnedValue>) -> zbus::fdo::Result<()> {
            if !options.is_empty() {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "fixture accepts only empty options".to_owned(),
                ));
            }
            if let Some(delay) = action_fixture_behavior(&self.0, "Eject").await? {
                tokio::time::sleep(delay).await;
            }
            Ok(())
        }

        async fn power_off(&self, options: HashMap<String, OwnedValue>) -> zbus::fdo::Result<()> {
            if !options.is_empty() {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "fixture accepts only empty options".to_owned(),
                ));
            }
            if let Some(delay) = action_fixture_behavior(&self.0, "PowerOff").await? {
                tokio::time::sleep(delay).await;
            }
            Ok(())
        }
    }

    fn text(value: &str) -> OwnedValue {
        OwnedValue::from(Str::from(value))
    }

    fn bytes(value: &[u8]) -> OwnedValue {
        OwnedValue::try_from(Value::from(value.to_vec())).unwrap()
    }

    fn byte_arrays(value: &[Vec<u8>]) -> OwnedValue {
        OwnedValue::try_from(Value::from(value.to_vec())).unwrap()
    }

    fn object_path(value: &str) -> OwnedValue {
        OwnedValue::try_from(Value::from(
            OwnedObjectPath::try_from(value.to_owned()).unwrap(),
        ))
        .unwrap()
    }

    fn object_path_key(value: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(value.to_owned()).unwrap()
    }

    fn private_bus() -> (BusDaemon, String) {
        let mut daemon = BusDaemon(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let mut address = String::new();
        BufReader::new(daemon.0.stdout.as_mut().unwrap())
            .read_line(&mut address)
            .unwrap();
        (daemon, address.trim().to_owned())
    }

    async fn serve(address: &str, state: FixtureState) -> Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(ROOT, UDisksFixture(state))
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    async fn client(address: &str) -> StorageClient {
        let connection = zbus::connection::Builder::address(address)
            .unwrap()
            .build()
            .await
            .unwrap();
        StorageClient::from_connection(connection)
    }

    async fn fixture(state: FixtureState) -> (BusDaemon, Connection, StorageClient) {
        let (daemon, address) = private_bus();
        let service = serve(&address, state).await;
        let client = client(&address).await;
        (daemon, service, client)
    }

    async fn serve_watch(
        address: &str,
        state: Arc<tokio::sync::RwLock<FixtureState>>,
    ) -> Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(ROOT, WatchFixture(state))
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    async fn watch_fixture(
        state: FixtureState,
    ) -> (
        BusDaemon,
        String,
        Arc<tokio::sync::RwLock<FixtureState>>,
        Connection,
        StorageClient,
    ) {
        let (daemon, address) = private_bus();
        let state = Arc::new(tokio::sync::RwLock::new(state));
        let service = serve_watch(&address, state.clone()).await;
        let client = client(&address).await;
        (daemon, address, state, service, client)
    }

    async fn action_fixture(
        state: FixtureState,
        action_timeout: Duration,
    ) -> (
        BusDaemon,
        Arc<tokio::sync::RwLock<FixtureState>>,
        Connection,
        StorageClient,
    ) {
        let (daemon, address) = private_bus();
        let state = Arc::new(tokio::sync::RwLock::new(state));
        let service = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(ROOT, WatchFixture(state.clone()))
            .unwrap()
            .serve_at(ACTION_BLOCK_PATH, FilesystemActionFixture(state.clone()))
            .unwrap()
            .serve_at(ACTION_DRIVE_PATH, DriveActionFixture(state.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = client(&address).await.with_action_timeout(action_timeout);
        (daemon, state, service, client)
    }

    fn action_block(mount_points: Option<Vec<Vec<u8>>>) -> BlockFixture {
        let mut block = FixtureState::root_block();
        block.path = ACTION_BLOCK_PATH.to_owned();
        block.device = b"/dev/action1\0".to_vec();
        block.preferred_device = Some(b"/dev/action1\0".to_vec());
        block.hint_system = false;
        block.hint_ignore = false;
        block.drive_path = ACTION_DRIVE_PATH.to_owned();
        block.mount_points = mount_points;
        block
    }

    fn action_drive(ejectable: bool, can_power_off: bool) -> DriveFixture {
        DriveFixture {
            path: ACTION_DRIVE_PATH.to_owned(),
            id: "action-drive".to_owned(),
            vendor: "fixture".to_owned(),
            model: "fixture removable drive".to_owned(),
            size: 4096,
            media_available: true,
            media_removable: true,
            removable: true,
            ejectable,
            can_power_off,
            missing_property: None,
        }
    }

    fn action_state(
        mount_points: Option<Vec<Vec<u8>>>,
        ejectable: bool,
        can_power_off: bool,
    ) -> FixtureState {
        FixtureState::new(
            vec![action_block(mount_points)],
            vec![action_drive(ejectable, can_power_off)],
        )
    }

    fn action_object_path(path: &str) -> IpcObjectPath {
        IpcObjectPath::new(path.to_owned()).unwrap()
    }

    fn method_error(name: &str) -> zbus::Error {
        let call = zbus::Message::method("/org/example", "Probe")
            .unwrap()
            .build(&())
            .unwrap();
        let reply = zbus::Message::method_error(&call, name)
            .unwrap()
            .build(&"fixture error")
            .unwrap();
        zbus::Error::MethodError(
            name.try_into().unwrap(),
            Some("fixture error".to_owned()),
            reply,
        )
    }

    #[tokio::test]
    async fn storage_mount_and_unmount_use_safe_options_and_read_back_state() {
        let (_daemon, state, _service, client) = action_fixture(
            action_state(Some(Vec::new()), true, true),
            Duration::from_secs(2),
        )
        .await;
        let block_path = action_object_path(ACTION_BLOCK_PATH);

        let mounted_at = client.mount(&block_path).await.unwrap();
        assert_eq!(mounted_at, PathBuf::from(ACTION_MOUNT_PATH));
        let mounted = client.snapshot().await.unwrap();
        assert_eq!(
            mounted.blocks[0].filesystem.as_ref().unwrap().mount_points,
            vec![mounted_at]
        );

        client.unmount(&block_path).await.unwrap();
        let unmounted = client.snapshot().await.unwrap();
        assert!(unmounted.blocks[0]
            .filesystem
            .as_ref()
            .unwrap()
            .mount_points
            .is_empty());
        assert_eq!(
            state.read().await.action_calls,
            vec!["Mount".to_owned(), "Unmount".to_owned()]
        );
    }

    #[tokio::test]
    async fn storage_actions_reject_missing_unsafe_and_unsupported_targets() {
        let (_daemon, state, _service, client) = action_fixture(
            action_state(Some(Vec::new()), false, false),
            Duration::from_secs(2),
        )
        .await;
        let block_path = action_object_path(ACTION_BLOCK_PATH);
        let drive_path = action_object_path(ACTION_DRIVE_PATH);
        let missing_path = action_object_path("/org/freedesktop/UDisks2/block_devices/missing");

        assert!(matches!(
            client.mount(&missing_path).await,
            Err(StorageError::TargetNotFound { .. })
        ));
        assert!(matches!(
            client.eject(&drive_path).await,
            Err(StorageError::UnsupportedOperation {
                operation: "eject",
                ..
            })
        ));
        assert!(matches!(
            client.power_off(&drive_path).await,
            Err(StorageError::UnsupportedOperation {
                operation: "power off",
                ..
            })
        ));
        assert!(matches!(
            client.unmount(&block_path).await,
            Err(StorageError::InvalidState {
                operation: "unmount",
                ..
            })
        ));
        assert!(state.read().await.action_calls.is_empty());

        let (_daemon, _service, system_client) = fixture(FixtureState::new(
            vec![FixtureState::root_block()],
            vec![virtio_drive()],
        ))
        .await;
        let system_path = action_object_path(ROOT_BLOCK_PATH);
        assert!(matches!(
            system_client.mount(&system_path).await,
            Err(StorageError::InvalidState {
                operation: "mount",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn storage_eject_and_power_off_reject_mounted_media_and_use_safe_options() {
        let (_daemon, state, _service, client) = action_fixture(
            action_state(Some(Vec::new()), true, true),
            Duration::from_secs(2),
        )
        .await;
        let drive_path = action_object_path(ACTION_DRIVE_PATH);
        client.eject(&drive_path).await.unwrap();
        client.power_off(&drive_path).await.unwrap();
        assert_eq!(
            state.read().await.action_calls,
            vec!["Eject".to_owned(), "PowerOff".to_owned()]
        );

        let (_daemon, state, _service, client) = action_fixture(
            action_state(
                Some(vec![format!("{ACTION_MOUNT_PATH}\0").into_bytes()]),
                true,
                true,
            ),
            Duration::from_secs(2),
        )
        .await;
        assert!(matches!(
            client.eject(&drive_path).await,
            Err(StorageError::InvalidState {
                operation: "eject",
                ..
            })
        ));
        assert!(matches!(
            client.power_off(&drive_path).await,
            Err(StorageError::InvalidState {
                operation: "power off",
                ..
            })
        ));
        assert!(state.read().await.action_calls.is_empty());
    }

    #[tokio::test]
    async fn storage_action_denial_is_reported_as_permission_denied() {
        let mut state = action_state(Some(Vec::new()), true, true);
        state.denied_action = Some("Mount".to_owned());
        let (_daemon, _state, _service, client) =
            action_fixture(state, Duration::from_secs(2)).await;
        let error = client
            .mount(&action_object_path(ACTION_BLOCK_PATH))
            .await
            .unwrap_err();
        assert!(matches!(error, StorageError::PermissionDenied(_)));
        assert_eq!(error.availability(), Availability::PermissionDenied);

        for name in [
            "org.freedesktop.UDisks2.Error.NotAuthorized",
            "org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain",
            "org.freedesktop.UDisks2.Error.NotAuthorizedDismissed",
        ] {
            let error = StorageError::from(method_error(name));
            assert!(matches!(error, StorageError::PermissionDenied(_)), "{name}");
        }
    }

    #[tokio::test]
    async fn storage_action_timeout_is_bounded_and_state_is_reconciled() {
        let mut state = action_state(Some(Vec::new()), true, true);
        state.next_action_delay = Some(Duration::from_secs(1));
        let (_daemon, _state, _service, client) =
            action_fixture(state, Duration::from_millis(500)).await;
        let block_path = action_object_path(ACTION_BLOCK_PATH);

        assert!(matches!(
            client.mount(&block_path).await,
            Err(StorageError::ActionOutcomeUnknown {
                operation: "mount",
                ..
            })
        ));
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(
            snapshot.blocks[0].filesystem.as_ref().unwrap().mount_points,
            vec![PathBuf::from(ACTION_MOUNT_PATH)]
        );
    }

    #[tokio::test]
    async fn storage_action_waiter_cancellation_keeps_mutation_serialized() {
        let mut state = action_state(Some(Vec::new()), true, true);
        state.next_action_delay = Some(Duration::from_millis(300));
        let (_daemon, state, _service, client) =
            action_fixture(state, Duration::from_secs(2)).await;
        let block_path = action_object_path(ACTION_BLOCK_PATH);
        let caller_client = client.clone();
        let caller_path = block_path.clone();
        let caller = tokio::spawn(async move { caller_client.mount(&caller_path).await });

        tokio::time::timeout(Duration::from_secs(1), async {
            while state.read().await.action_calls.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("action was not dispatched");
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert!(matches!(
            client.mount(&block_path).await,
            Err(StorageError::ActionInProgress {
                operation: "mount",
                ..
            })
        ));

        tokio::time::sleep(Duration::from_millis(350)).await;
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(
            snapshot.blocks[0].filesystem.as_ref().unwrap().mount_points,
            vec![PathBuf::from(ACTION_MOUNT_PATH)]
        );
    }

    #[tokio::test]
    async fn storage_clones_serialize_conflicting_mount_requests() {
        let (_daemon, state, _service, client) = action_fixture(
            action_state(Some(Vec::new()), true, true),
            Duration::from_secs(2),
        )
        .await;
        let other_client = client.clone();
        let block_path = action_object_path(ACTION_BLOCK_PATH);
        let (first, second) =
            tokio::join!(client.mount(&block_path), other_client.mount(&block_path));
        let successful = usize::from(first.is_ok()) + usize::from(second.is_ok());
        let rejected = usize::from(matches!(
            first,
            Err(StorageError::ActionInProgress {
                operation: "mount",
                ..
            }) | Err(StorageError::InvalidState {
                operation: "mount",
                ..
            })
        )) + usize::from(matches!(
            second,
            Err(StorageError::ActionInProgress {
                operation: "mount",
                ..
            }) | Err(StorageError::InvalidState {
                operation: "mount",
                ..
            })
        ));
        assert_eq!(successful, 1);
        assert_eq!(rejected, 1);
        assert_eq!(state.read().await.action_calls, vec!["Mount".to_owned()]);
    }

    async fn next_event(stream: &mut StorageEventStream) -> StorageEvent {
        tokio::time::timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("storage event timed out")
            .expect("storage event stream closed")
    }

    async fn next_snapshot_where(
        stream: &mut StorageEventStream,
        mut matches: impl FnMut(&StorageSnapshot) -> bool,
    ) -> StorageSnapshot {
        loop {
            match next_event(stream).await {
                StorageEvent::Snapshot(snapshot) if matches(&snapshot) => return snapshot,
                StorageEvent::Snapshot(_) => {}
                StorageEvent::Unavailable { error, .. } => {
                    panic!("storage unexpectedly unavailable: {error}")
                }
            }
        }
    }

    fn virtio_drive() -> DriveFixture {
        DriveFixture {
            path: "/org/freedesktop/UDisks2/drives/virtio".to_owned(),
            id: "virtio-disk-id".to_owned(),
            vendor: "QEMU".to_owned(),
            model: "VirtIO Disk".to_owned(),
            size: 4096,
            media_available: true,
            media_removable: false,
            removable: false,
            ejectable: false,
            can_power_off: false,
            missing_property: None,
        }
    }

    #[tokio::test]
    async fn storage_watch_emits_its_initial_snapshot() {
        let (_daemon, _service, client) = fixture(FixtureState::new(vec![], vec![])).await;
        let mut stream = client.watch();
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap();

        match event {
            StorageEvent::Snapshot(snapshot) => {
                assert_eq!(snapshot.revision, 1);
                assert_eq!(stream.revision(), snapshot.revision);
            }
            StorageEvent::Unavailable { .. } => panic!("fixture service should be available"),
        }
    }

    #[tokio::test]
    async fn storage_watch_reconciles_interfaces_and_property_changes() {
        let (_daemon, _address, _state, _service, client) =
            watch_fixture(FixtureState::new(vec![FixtureState::root_block()], vec![])).await;
        let mut stream = client.watch();
        assert_eq!(stream.revision(), 0);
        let initial = next_snapshot_where(&mut stream, |_| true).await;
        assert_eq!(initial.revision, 1);
        assert_eq!(initial.blocks.len(), 1);
        assert_eq!(stream.revision(), initial.revision);

        let manager = Proxy::new(&client.connection, SERVICE, ROOT, OBJECT_MANAGER)
            .await
            .unwrap();
        manager
            .call::<_, _, ()>("SetRootLabel", &("updated-label".to_owned(),))
            .await
            .unwrap();
        let updated = next_snapshot_where(&mut stream, |snapshot| {
            snapshot.blocks[0].label.as_deref() == Some("updated-label")
        })
        .await;
        assert!(updated.revision > initial.revision);

        manager.call::<_, _, ()>("AddBlock", &()).await.unwrap();
        let added = next_snapshot_where(&mut stream, |snapshot| snapshot.blocks.len() == 2).await;
        assert!(added.revision > updated.revision);

        manager
            .call::<_, _, ()>("RemoveRootBlock", &())
            .await
            .unwrap();
        let removed = next_snapshot_where(&mut stream, |snapshot| {
            snapshot.blocks.len() == 1
                && snapshot.blocks[0].path.as_str() == "/org/freedesktop/UDisks2/block_devices/vdb1"
        })
        .await;
        assert!(removed.revision > added.revision);
        assert_eq!(stream.revision(), removed.revision);
    }

    #[tokio::test]
    async fn storage_watch_survives_service_loss_and_recovery() {
        let (_daemon, address) = private_bus();
        let state = Arc::new(tokio::sync::RwLock::new(FixtureState::new(
            vec![FixtureState::root_block()],
            vec![],
        )));
        let client = client(&address).await;
        let mut stream = client.watch();

        let missing_revision = match next_event(&mut stream).await {
            StorageEvent::Unavailable { revision, error } => {
                assert_eq!(error.availability(), Availability::MissingService);
                revision
            }
            StorageEvent::Snapshot(_) => panic!("missing service must not report a snapshot"),
        };
        assert_eq!(stream.revision(), missing_revision);

        let service = serve_watch(&address, state.clone()).await;
        let available = next_snapshot_where(&mut stream, |snapshot| {
            snapshot.availability == Availability::Available
        })
        .await;
        assert!(available.revision > missing_revision);
        assert_eq!(available.blocks.len(), 1);
        drop(service);

        let lost_revision = match next_event(&mut stream).await {
            StorageEvent::Unavailable { revision, error } => {
                assert_eq!(error.availability(), Availability::MissingService);
                revision
            }
            StorageEvent::Snapshot(_) => panic!("service loss must report unavailable"),
        };
        assert!(lost_revision > available.revision);

        let service = serve_watch(&address, state).await;
        let recovered = next_snapshot_where(&mut stream, |snapshot| {
            snapshot.availability == Availability::Available
        })
        .await;
        assert!(recovered.revision > lost_revision);
        assert_eq!(stream.revision(), recovered.revision);
        drop(service);
    }

    #[tokio::test]
    async fn storage_watch_retries_after_recovery_snapshot_timeout() {
        let (_daemon, address) = private_bus();
        let mut fixture = FixtureState::new(vec![FixtureState::root_block()], vec![]);
        fixture.next_snapshot_delay = Some(Duration::from_secs(6));
        let state = Arc::new(tokio::sync::RwLock::new(fixture));
        let client = client(&address).await;
        let mut stream = client.watch();

        match next_event(&mut stream).await {
            StorageEvent::Unavailable { error, .. } => {
                assert_eq!(error.availability(), Availability::MissingService);
            }
            StorageEvent::Snapshot(_) => panic!("missing service must not report a snapshot"),
        }

        let service = serve_watch(&address, state).await;
        let timeout_event = tokio::time::timeout(Duration::from_secs(8), stream.next())
            .await
            .expect("snapshot timeout was not reported")
            .expect("watch closed after snapshot timeout");
        match timeout_event {
            StorageEvent::Unavailable { error, .. } => {
                assert_eq!(error.availability(), Availability::Failed);
            }
            StorageEvent::Snapshot(_) => panic!("delayed snapshot should time out"),
        }

        let recovered = tokio::time::timeout(
            Duration::from_secs(8),
            next_snapshot_where(&mut stream, |snapshot| {
                snapshot.availability == Availability::Available
            }),
        )
        .await
        .expect("watch did not retry the timed-out snapshot");
        assert_eq!(recovered.blocks.len(), 1);
        assert!(stream.revision() == recovered.revision);
        drop(service);
    }

    #[tokio::test]
    async fn storage_watch_coalesces_slow_consumer_updates_and_cancels_on_drop() {
        let (_daemon, _address, _state, _service, client) =
            watch_fixture(FixtureState::new(vec![FixtureState::root_block()], vec![])).await;
        let mut stream = client.watch();
        let initial = next_snapshot_where(&mut stream, |_| true).await;
        assert_eq!(stream.receiver.max_capacity(), 1);
        let manager = Proxy::new(&client.connection, SERVICE, ROOT, OBJECT_MANAGER)
            .await
            .unwrap();

        for index in 0..32 {
            manager
                .call::<_, _, ()>("SetRootLabel", &(format!("label-{index}"),))
                .await
                .unwrap();
        }
        let latest = next_snapshot_where(&mut stream, |snapshot| {
            snapshot.blocks[0].label.as_deref() == Some("label-31")
        })
        .await;
        assert!(latest.revision > initial.revision);
        assert_eq!(stream.revision(), latest.revision);

        let task = stream.task.abort_handle();
        assert!(!task.is_finished());
        drop(stream);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !task.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("watch task did not stop after stream drop");
    }

    #[tokio::test]
    async fn snapshot_parses_udisks_byte_paths_and_associated_metadata() {
        let mut first_block = FixtureState::root_block();
        first_block.device = b"/dev/disk-\xff\0".to_vec();
        first_block.preferred_device = Some(b"/dev/by-id/disk-\xfe\0".to_vec());
        first_block.mount_points = Some(vec![b"/media/usb-\xfd\0".to_vec(), b"/\0".to_vec()]);
        let mut second_block = FixtureState::root_block();
        second_block.path = "/org/freedesktop/UDisks2/block_devices/vdb1".to_owned();
        second_block.device = b"/dev/vdb1\0".to_vec();
        second_block.preferred_device = Some(b"/dev/vdb1\0".to_vec());
        second_block.mount_points = Some(vec![]);
        let mut third_block = FixtureState::root_block();
        third_block.path = "/org/freedesktop/UDisks2/block_devices/vdc1".to_owned();
        third_block.device = b"/dev/vdc1\0".to_vec();
        third_block.preferred_device = Some(b"/dev/vdc1\0".to_vec());
        third_block.drive_path = "/".to_owned();
        third_block.mount_points = None;
        let orphan_drive = DriveFixture {
            path: "/org/freedesktop/UDisks2/drives/a_orphan".to_owned(),
            id: String::new(),
            vendor: String::new(),
            model: String::new(),
            size: 0,
            media_available: false,
            media_removable: true,
            removable: true,
            ejectable: true,
            can_power_off: true,
            missing_property: None,
        };
        let (_daemon, _service, client) = fixture(FixtureState::new(
            vec![second_block, third_block, first_block],
            vec![virtio_drive(), orphan_drive],
        ))
        .await;

        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.blocks.len(), 3);
        let first = &snapshot.blocks[0];
        assert_eq!(
            first.path.as_str(),
            "/org/freedesktop/UDisks2/block_devices/vda1"
        );
        assert_eq!(first.id.as_deref(), Some("by-id-vda1"));
        assert_eq!(first.device.as_os_str().as_bytes(), b"/dev/disk-\xff");
        assert_eq!(
            first.preferred_device.as_os_str().as_bytes(),
            b"/dev/by-id/disk-\xfe"
        );
        assert_eq!(first.size, 1234);
        assert_eq!(first.label.as_deref(), Some("cloudimg-rootfs"));
        assert_eq!(first.filesystem_type.as_deref(), Some("ext4"));
        assert_eq!(first.uuid.as_deref(), Some("fixture-uuid"));
        assert_eq!(first.usage.as_deref(), Some("filesystem"));
        assert_eq!(
            first.drive_path.as_ref().unwrap().as_str(),
            "/org/freedesktop/UDisks2/drives/virtio"
        );
        assert_eq!(
            first.filesystem.as_ref().unwrap().mount_points[0]
                .as_os_str()
                .as_bytes(),
            b"/media/usb-\xfd"
        );
        assert_eq!(
            first.filesystem.as_ref().unwrap().mount_points[1],
            PathBuf::from("/")
        );
        assert!(!first.hint_ignore);
        assert!(first.hint_system);
        assert_eq!(
            snapshot.blocks[1].path.as_str(),
            "/org/freedesktop/UDisks2/block_devices/vdb1"
        );
        assert!(snapshot.blocks[1]
            .filesystem
            .as_ref()
            .unwrap()
            .mount_points
            .is_empty());
        assert_eq!(
            snapshot.blocks[2].path.as_str(),
            "/org/freedesktop/UDisks2/block_devices/vdc1"
        );
        assert!(snapshot.blocks[2].drive_path.is_none());
        assert!(snapshot.blocks[2].filesystem.is_none());

        assert_eq!(snapshot.drives.len(), 2);
        assert_eq!(
            snapshot.drives[0].path.as_str(),
            "/org/freedesktop/UDisks2/drives/a_orphan"
        );
        assert_eq!(snapshot.drives[0].id, None);
        assert!(!snapshot.drives[0].media_available);
        assert!(snapshot.drives[0].media_removable);
        assert_eq!(
            snapshot.drives[1].path.as_str(),
            "/org/freedesktop/UDisks2/drives/virtio"
        );
        assert_eq!(snapshot.drives[1].id.as_deref(), Some("virtio-disk-id"));
        assert_eq!(snapshot.drives[1].vendor.as_deref(), Some("QEMU"));
        assert_eq!(snapshot.drives[1].model.as_deref(), Some("VirtIO Disk"));
        assert_eq!(snapshot.drives[1].size, 4096);
        assert!(snapshot.drives[1].media_available);
        assert!(!snapshot.drives[1].media_removable);
        assert!(!snapshot.drives[1].removable);
        assert!(!snapshot.drives[1].ejectable);
        assert!(!snapshot.drives[1].can_power_off);
    }

    #[tokio::test]
    async fn absent_optional_interfaces_and_blank_strings_remain_absent() {
        let mut block = FixtureState::root_block();
        block.drive_path = "/".to_owned();
        block.mount_points = None;
        block.label.clear();
        block.filesystem_type.clear();
        block.uuid.clear();
        block.usage.clear();
        let (_daemon, _service, client) = fixture(FixtureState::new(vec![block], vec![])).await;

        let snapshot = client.snapshot().await.unwrap();
        let block = &snapshot.blocks[0];
        assert!(block.filesystem.is_none());
        assert!(block.drive_path.is_none());
        assert_eq!(block.label, None);
        assert_eq!(block.filesystem_type, None);
        assert_eq!(block.uuid, None);
        assert_eq!(block.usage, None);
    }

    #[tokio::test]
    async fn empty_live_service_is_available_without_fabricated_devices() {
        let (_daemon, _service, client) = fixture(FixtureState::new(vec![], vec![])).await;
        let snapshot = client.snapshot().await.unwrap();

        assert_eq!(snapshot.availability, Availability::Available);
        assert!(snapshot.blocks.is_empty());
        assert!(snapshot.drives.is_empty());
    }

    #[tokio::test]
    async fn malformed_required_block_properties_return_typed_errors() {
        let mut block = FixtureState::root_block();
        block.device.pop();
        let (_daemon, _service, client) = fixture(FixtureState::new(vec![block], vec![])).await;
        assert!(matches!(
            client.snapshot().await,
            Err(StorageError::InvalidProperty(name)) if name == "Device"
        ));

        let mut block = FixtureState::root_block();
        block.size = None;
        let (_daemon, _service, client) = fixture(FixtureState::new(vec![block], vec![])).await;
        assert!(matches!(
            client.snapshot().await,
            Err(StorageError::InvalidProperty(name)) if name == "Size"
        ));

        let mut block = FixtureState::root_block();
        block.preferred_device = None;
        let (_daemon, _service, client) = fixture(FixtureState::new(vec![block], vec![])).await;
        assert!(matches!(
            client.snapshot().await,
            Err(StorageError::InvalidProperty(name)) if name == "PreferredDevice"
        ));
    }

    #[tokio::test]
    async fn missing_required_drive_property_returns_typed_error() {
        let mut drive = virtio_drive();
        drive.missing_property = Some("Vendor".to_owned());
        let (_daemon, _service, client) = fixture(FixtureState::new(vec![], vec![drive])).await;

        assert!(matches!(
            client.snapshot().await,
            Err(StorageError::InvalidProperty(name)) if name == "Vendor"
        ));
    }

    #[tokio::test]
    async fn missing_service_and_denied_access_have_distinct_availability() {
        let (_daemon, address) = private_bus();
        let missing_client = client(&address).await;
        let missing = missing_client.snapshot().await.unwrap_err();
        assert!(matches!(&missing, StorageError::MissingService(_)));
        assert_eq!(missing.availability(), Availability::MissingService);

        let mut state = FixtureState::new(vec![], vec![]);
        state.denied = true;
        let (_daemon, _service, denied_client) = fixture(state).await;
        let denied = denied_client.snapshot().await.unwrap_err();
        assert!(matches!(&denied, StorageError::PermissionDenied(_)));
        assert_eq!(denied.availability(), Availability::PermissionDenied);
    }

    #[tokio::test]
    #[ignore = "requires a live UDisks2 service on the system bus"]
    async fn live_udisks_snapshot_reads_inventory_without_mutation() {
        let client = StorageClient::connect().await.unwrap();
        let snapshot = client.snapshot().await.unwrap();

        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.revision, 1);
        println!(
            "UDisks2 inventory: {} blocks, {} drives",
            snapshot.blocks.len(),
            snapshot.drives.len()
        );
    }

    #[test]
    fn object_path_ids_use_the_canonical_core_validator() {
        assert!(IpcObjectPath::new("/org/freedesktop/UDisks2/block_devices/vda1").is_ok());
        assert!(IpcObjectPath::new("org/freedesktop/UDisks2/block_devices/vda1").is_err());
    }

    #[test]
    fn disconnected_dbus_io_errors_have_truthful_availability() {
        let error = StorageError::from(zbus::Error::InputOutput(std::sync::Arc::new(
            std::io::Error::from(std::io::ErrorKind::BrokenPipe),
        )));
        assert!(matches!(&error, StorageError::Disconnected(_)));
        assert_eq!(error.availability(), Availability::Disconnected);
    }
}
