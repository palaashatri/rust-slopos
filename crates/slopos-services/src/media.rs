use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::stream::{self, StreamExt};
use slopos_core::capability::Availability;
use thiserror::Error;
use tokio::{sync::mpsc, time::timeout};
use zbus::{names::OwnedBusName, zvariant::OwnedValue, Connection, Proxy};

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const DBUS_SERVICE: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const PLAYER_READ_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CONCURRENT_PLAYER_READS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPlayer {
    pub bus_name: String,
    pub identity: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub playback_status: PlaybackStatus,
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSnapshot {
    pub revision: u64,
    pub availability: Availability,
    pub players: Vec<MediaPlayer>,
    pub player_failures: Vec<MediaPlayerFailure>,
    pub current_player: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPlayerFailure {
    pub bus_name: String,
    pub availability: Availability,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("MPRIS operation timed out")]
    Timeout,
    #[error("session D-Bus or MPRIS service is unavailable: {0}")]
    MissingService(#[source] zbus::Error),
    #[error("MPRIS operation denied: {0}")]
    PermissionDenied(#[source] zbus::Error),
    #[error("MPRIS player is unavailable: {0}")]
    PlayerUnavailable(String),
    #[error("invalid MPRIS player bus name: {0}")]
    InvalidPlayerName(String),
    #[error("MPRIS player does not support {action}: {player}")]
    UnsupportedAction {
        player: String,
        action: &'static str,
    },
    #[error("invalid MPRIS property: {0}")]
    InvalidProperty(String),
    #[error("MPRIS D-Bus error: {0}")]
    Bus(#[source] zbus::Error),
}

impl MediaError {
    fn kind(&self) -> MediaErrorKind {
        match self {
            Self::Timeout => MediaErrorKind::Timeout,
            Self::MissingService(_) => MediaErrorKind::MissingService,
            Self::PermissionDenied(_) => MediaErrorKind::PermissionDenied,
            Self::PlayerUnavailable(_) => MediaErrorKind::PlayerUnavailable,
            Self::InvalidPlayerName(_) => MediaErrorKind::InvalidPlayerName,
            Self::UnsupportedAction { .. } => MediaErrorKind::UnsupportedAction,
            Self::InvalidProperty(_) => MediaErrorKind::InvalidProperty,
            Self::Bus(_) => MediaErrorKind::Bus,
        }
    }

    pub fn availability(&self) -> Availability {
        match self {
            Self::Timeout
            | Self::InvalidPlayerName(_)
            | Self::UnsupportedAction { .. }
            | Self::InvalidProperty(_)
            | Self::Bus(_) => Availability::Failed,
            Self::MissingService(_) => Availability::MissingService,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::PlayerUnavailable(_) => Availability::Disconnected,
        }
    }
}

impl From<zbus::Error> for MediaError {
    fn from(error: zbus::Error) -> Self {
        let name = match &error {
            zbus::Error::MethodError(name, _, _) => Some(name.as_str()),
            zbus::Error::FDO(fdo) => match fdo.as_ref() {
                zbus::fdo::Error::ServiceUnknown(_) => {
                    Some("org.freedesktop.DBus.Error.ServiceUnknown")
                }
                zbus::fdo::Error::NameHasNoOwner(_) => {
                    Some("org.freedesktop.DBus.Error.NameHasNoOwner")
                }
                zbus::fdo::Error::AccessDenied(_) => {
                    Some("org.freedesktop.DBus.Error.AccessDenied")
                }
                _ => None,
            },
            _ => None,
        };
        match name {
            Some("org.freedesktop.DBus.Error.ServiceUnknown")
            | Some("org.freedesktop.DBus.Error.NameHasNoOwner") => Self::MissingService(error),
            Some("org.freedesktop.DBus.Error.AccessDenied")
            | Some("org.bluez.Error.NotAuthorized") => Self::PermissionDenied(error),
            _ => Self::Bus(error),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MediaErrorKind {
    Timeout,
    MissingService,
    PermissionDenied,
    PlayerUnavailable,
    InvalidPlayerName,
    UnsupportedAction,
    InvalidProperty,
    Bus,
}

#[derive(Clone)]
pub struct MediaClient {
    connection: Connection,
    revision: Arc<AtomicU64>,
    action_lock: Arc<tokio::sync::Mutex<()>>,
}

pub struct MediaEventStream {
    receiver: mpsc::Receiver<Result<MediaSnapshot, MediaError>>,
}

impl MediaEventStream {
    pub async fn next(&mut self) -> Option<Result<MediaSnapshot, MediaError>> {
        self.receiver.recv().await
    }
}

impl MediaClient {
    pub async fn connect() -> Result<Self, MediaError> {
        let connection = timeout(OPERATION_TIMEOUT, Connection::session())
            .await
            .map_err(|_| MediaError::Timeout)??;
        Ok(Self {
            connection,
            revision: Arc::new(AtomicU64::new(0)),
            action_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub async fn snapshot(&self) -> Result<MediaSnapshot, MediaError> {
        timeout(OPERATION_TIMEOUT, self.snapshot_inner())
            .await
            .map_err(|_| MediaError::Timeout)?
    }

    pub fn watch(&self) -> MediaEventStream {
        let client = self.clone();
        let (sender, receiver) = mpsc::channel(4);
        tokio::spawn(async move {
            let mut previous: Option<MediaSnapshot> = None;
            let mut previous_error = None;
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                if sender.is_closed() {
                    break;
                }
                tick.tick().await;
                if sender.is_closed() {
                    break;
                }
                let result = client.snapshot().await;
                if sender.is_closed() {
                    break;
                }
                let changed = match (&previous, &result) {
                    (_, Err(error)) => previous_error != Some(error.kind()),
                    (None, Ok(_)) => true,
                    (Some(old), Ok(new)) => {
                        previous_error.is_some()
                            || old.availability != new.availability
                            || old.current_player != new.current_player
                            || old.players != new.players
                            || old.player_failures != new.player_failures
                    }
                };
                if changed {
                    previous_error = result.as_ref().err().map(MediaError::kind);
                    if let Ok(snapshot) = &result {
                        previous = Some(snapshot.clone());
                    }
                    if sender.send(result).await.is_err() {
                        break;
                    }
                }
            }
        });
        MediaEventStream { receiver }
    }

    pub async fn play_pause(&self, player_name: &str) -> Result<MediaSnapshot, MediaError> {
        self.control(player_name, MediaAction::PlayPause).await
    }

    pub async fn next(&self, player_name: &str) -> Result<MediaSnapshot, MediaError> {
        self.control(player_name, MediaAction::Next).await
    }

    pub async fn previous(&self, player_name: &str) -> Result<MediaSnapshot, MediaError> {
        self.control(player_name, MediaAction::Previous).await
    }

    async fn control(
        &self,
        player_name: &str,
        action: MediaAction,
    ) -> Result<MediaSnapshot, MediaError> {
        validate_player_name(player_name)?;
        timeout(OPERATION_TIMEOUT, async {
            let _guard = self.action_lock.lock().await;
            let snapshot = self.snapshot_inner().await?;
            let player = snapshot
                .players
                .iter()
                .find(|player| player.bus_name == player_name)
                .ok_or_else(|| MediaError::PlayerUnavailable(player_name.to_owned()))?;
            if !player.can_control {
                return Err(MediaError::UnsupportedAction {
                    player: player_name.to_owned(),
                    action: action.label(),
                });
            }
            let (method, allowed) = match action {
                MediaAction::PlayPause => (
                    "PlayPause",
                    player.can_pause
                        && (player.playback_status == PlaybackStatus::Playing || player.can_play),
                ),
                MediaAction::Next => ("Next", player.can_go_next),
                MediaAction::Previous => ("Previous", player.can_go_previous),
            };
            if !allowed {
                return Err(MediaError::UnsupportedAction {
                    player: player_name.to_owned(),
                    action: action.label(),
                });
            }
            let proxy = match self.player_proxy(player_name).await {
                Err(MediaError::MissingService(_)) => {
                    return Err(MediaError::PlayerUnavailable(player_name.to_owned()))
                }
                result => result?,
            };
            match proxy.call::<_, _, ()>(method, &()).await {
                Err(error) if player_owner_lost(&error) => {
                    return Err(MediaError::PlayerUnavailable(player_name.to_owned()))
                }
                Err(error) => return Err(error.into()),
                Ok(()) => {}
            }
            self.snapshot_inner().await
        })
        .await
        .map_err(|_| MediaError::Timeout)?
    }

    async fn snapshot_inner(&self) -> Result<MediaSnapshot, MediaError> {
        let dbus = Proxy::new(&self.connection, DBUS_SERVICE, DBUS_PATH, DBUS_INTERFACE).await?;
        let names: Vec<String> = dbus.call("ListNames", &()).await?;
        let names: Vec<_> = names
            .into_iter()
            .filter(|name| name.starts_with(MPRIS_PREFIX))
            .filter(|name| validate_player_name(name).is_ok())
            .collect();
        let reads = stream::iter(names.into_iter().map(|name| async move {
            let result = timeout(PLAYER_READ_TIMEOUT, self.read_player(&name)).await;
            (name, result)
        }))
        .buffer_unordered(MAX_CONCURRENT_PLAYER_READS)
        .collect::<Vec<_>>()
        .await;
        let mut players = Vec::new();
        let mut player_failures = Vec::new();
        for (name, result) in reads {
            match result {
                Ok(Ok(player)) => players.push(player),
                Ok(Err(MediaError::MissingService(_))) => {}
                Ok(Err(error)) => player_failures.push(MediaPlayerFailure {
                    bus_name: name,
                    availability: error.availability(),
                    reason: error.to_string(),
                }),
                Err(_) => player_failures.push(MediaPlayerFailure {
                    bus_name: name,
                    availability: Availability::Failed,
                    reason: "MPRIS player property read timed out".to_owned(),
                }),
            }
        }
        players.sort_by(|left, right| left.bus_name.cmp(&right.bus_name));
        player_failures.sort_by(|left, right| left.bus_name.cmp(&right.bus_name));
        let current_player = players
            .iter()
            .find(|player| player.playback_status == PlaybackStatus::Playing)
            .or_else(|| players.first())
            .map(|player| player.bus_name.clone());
        let availability = if !players.is_empty() {
            Availability::Available
        } else if !player_failures.is_empty() {
            Availability::Failed
        } else {
            Availability::Disconnected
        };
        Ok(MediaSnapshot {
            revision: self.revision.fetch_add(1, Ordering::Relaxed) + 1,
            availability,
            players,
            player_failures,
            current_player,
        })
    }

    async fn read_player(&self, bus_name: &str) -> Result<MediaPlayer, MediaError> {
        let root = Proxy::new(&self.connection, bus_name, OBJECT_PATH, ROOT_INTERFACE).await?;
        let player = self.player_proxy(bus_name).await?;
        let identity: String = root.get_property("Identity").await?;
        let status: String = player.get_property("PlaybackStatus").await?;
        let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").await?;
        Ok(MediaPlayer {
            bus_name: bus_name.to_owned(),
            identity,
            title: optional_string_field(&metadata, "xesam:title")?,
            artist: optional_string_list_field(&metadata, "xesam:artist")?
                .filter(|artists| !artists.is_empty())
                .map(|artists| artists.join(", ")),
            playback_status: PlaybackStatus::from_str(&status)?,
            can_control: player.get_property("CanControl").await?,
            can_play: player.get_property("CanPlay").await?,
            can_pause: player.get_property("CanPause").await?,
            can_go_next: player.get_property("CanGoNext").await?,
            can_go_previous: player.get_property("CanGoPrevious").await?,
        })
    }

    async fn player_proxy<'a>(&'a self, bus_name: &'a str) -> Result<Proxy<'a>, MediaError> {
        validate_player_name(bus_name)?;
        Ok(Proxy::new(&self.connection, bus_name, OBJECT_PATH, PLAYER_INTERFACE).await?)
    }
}

#[derive(Clone, Copy)]
enum MediaAction {
    PlayPause,
    Next,
    Previous,
}

impl MediaAction {
    fn label(self) -> &'static str {
        match self {
            Self::PlayPause => "play/pause",
            Self::Next => "next track",
            Self::Previous => "previous track",
        }
    }
}

impl PlaybackStatus {
    fn from_str(value: &str) -> Result<Self, MediaError> {
        match value {
            "Playing" => Ok(Self::Playing),
            "Paused" => Ok(Self::Paused),
            "Stopped" => Ok(Self::Stopped),
            _ => Err(MediaError::InvalidProperty(format!(
                "PlaybackStatus={value}"
            ))),
        }
    }
}

fn validate_player_name(name: &str) -> Result<(), MediaError> {
    let parsed = OwnedBusName::try_from(name.to_owned())
        .map_err(|_| MediaError::InvalidPlayerName(name.to_owned()))?;
    if !parsed.as_str().starts_with(MPRIS_PREFIX) || parsed.as_str().len() == MPRIS_PREFIX.len() {
        return Err(MediaError::InvalidPlayerName(name.to_owned()));
    }
    Ok(())
}

fn player_owner_lost(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
                || name.as_str() == "org.freedesktop.DBus.Error.NameHasNoOwner"
        }
        zbus::Error::FDO(fdo) => matches!(
            fdo.as_ref(),
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)
        ),
        _ => false,
    }
}

fn optional_string_field(
    fields: &HashMap<String, OwnedValue>,
    name: &str,
) -> Result<Option<String>, MediaError> {
    fields
        .get(name)
        .map(|value| {
            value
                .try_clone()
                .and_then(String::try_from)
                .map_err(|_| MediaError::InvalidProperty(name.to_owned()))
        })
        .transpose()
}

fn optional_string_list_field(
    fields: &HashMap<String, OwnedValue>,
    name: &str,
) -> Result<Option<Vec<String>>, MediaError> {
    fields
        .get(name)
        .map(|value| {
            value
                .try_clone()
                .and_then(Vec::<String>::try_from)
                .map_err(|_| MediaError::InvalidProperty(name.to_owned()))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;
    use std::{
        collections::HashMap,
        io::BufRead,
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };
    use zbus::{zvariant::OwnedValue, Connection};

    const PLAYER_NAME: &str = "org.mpris.MediaPlayer2.fixture";

    struct BusDaemon(Child);

    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[derive(Clone)]
    struct PlayerFixture {
        status: Arc<Mutex<String>>,
        title: Arc<Mutex<String>>,
        denied: Arc<AtomicBool>,
        can_control: Arc<AtomicBool>,
        can_play: Arc<AtomicBool>,
        can_pause: Arc<AtomicBool>,
        can_next: Arc<AtomicBool>,
        can_previous: Arc<AtomicBool>,
        status_reads: Arc<AtomicU64>,
        play_pause_calls: Arc<AtomicU64>,
        flip_status_on_read: Arc<AtomicBool>,
        status_error: Arc<AtomicU64>,
        vanished: Arc<AtomicBool>,
    }

    struct PlayerRoot;

    impl PlayerFixture {
        fn new(status: &str) -> Self {
            Self {
                status: Arc::new(Mutex::new(status.to_owned())),
                title: Arc::new(Mutex::new("Fixture Track".to_owned())),
                denied: Arc::new(AtomicBool::new(false)),
                can_control: Arc::new(AtomicBool::new(true)),
                can_play: Arc::new(AtomicBool::new(true)),
                can_pause: Arc::new(AtomicBool::new(true)),
                can_next: Arc::new(AtomicBool::new(true)),
                can_previous: Arc::new(AtomicBool::new(true)),
                status_reads: Arc::new(AtomicU64::new(0)),
                play_pause_calls: Arc::new(AtomicU64::new(0)),
                flip_status_on_read: Arc::new(AtomicBool::new(false)),
                status_error: Arc::new(AtomicU64::new(0)),
                vanished: Arc::new(AtomicBool::new(false)),
            }
        }

        fn check_permission(&self) -> zbus::fdo::Result<()> {
            if self.denied.load(Ordering::Relaxed) {
                Err(zbus::fdo::Error::AccessDenied("fixture denied".into()))
            } else {
                Ok(())
            }
        }
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl PlayerRoot {
        #[zbus(property)]
        fn identity(&self) -> String {
            "Fixture Player".to_owned()
        }
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl PlayerFixture {
        #[zbus(property)]
        fn playback_status(&self) -> zbus::fdo::Result<String> {
            self.status_reads.fetch_add(1, Ordering::Relaxed);
            match self.status_error.load(Ordering::Relaxed) {
                1 => return Err(zbus::fdo::Error::AccessDenied("fixture denied".into())),
                2 => return Err(zbus::fdo::Error::Failed("fixture failed".into())),
                _ => {}
            }
            let mut status = self.status.lock().unwrap();
            let reported = status.clone();
            if self.flip_status_on_read.swap(false, Ordering::Relaxed) {
                *status = "Playing".to_owned();
            }
            Ok(reported)
        }

        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, OwnedValue> {
            HashMap::from([
                (
                    "xesam:title".to_owned(),
                    OwnedValue::try_from(zbus::zvariant::Value::from(
                        self.title.lock().unwrap().clone(),
                    ))
                    .unwrap(),
                ),
                (
                    "xesam:artist".to_owned(),
                    OwnedValue::try_from(zbus::zvariant::Value::from(vec![
                        "Fixture Artist".to_owned()
                    ]))
                    .unwrap(),
                ),
            ])
        }

        #[zbus(property)]
        fn can_control(&self) -> bool {
            self.can_control.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        fn can_play(&self) -> bool {
            self.can_play.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        fn can_pause(&self) -> bool {
            self.can_pause.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            self.can_next.load(Ordering::Relaxed)
        }

        #[zbus(property)]
        fn can_go_previous(&self) -> bool {
            self.can_previous.load(Ordering::Relaxed)
        }

        fn play(&self) -> zbus::fdo::Result<()> {
            self.check_permission()?;
            *self.status.lock().unwrap() = "Playing".to_owned();
            Ok(())
        }

        fn pause(&self) -> zbus::fdo::Result<()> {
            self.check_permission()?;
            *self.status.lock().unwrap() = "Paused".to_owned();
            Ok(())
        }

        fn play_pause(&self) -> zbus::fdo::Result<()> {
            self.check_permission()?;
            if self.vanished.load(Ordering::Relaxed) {
                return Err(zbus::fdo::Error::NameHasNoOwner(PLAYER_NAME.to_owned()));
            }
            self.play_pause_calls.fetch_add(1, Ordering::Relaxed);
            let mut status = self.status.lock().unwrap();
            *status = if *status == "Playing" {
                "Paused".to_owned()
            } else {
                "Playing".to_owned()
            };
            Ok(())
        }

        fn next(&self) -> zbus::fdo::Result<()> {
            self.check_permission()?;
            *self.title.lock().unwrap() = "Next Track".to_owned();
            Ok(())
        }

        fn previous(&self) -> zbus::fdo::Result<()> {
            self.check_permission()?;
            *self.title.lock().unwrap() = "Previous Track".to_owned();
            Ok(())
        }
    }

    async fn service(address: &str, name: &str, fixture: PlayerFixture) -> Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .name(name)
            .unwrap()
            .serve_at(OBJECT_PATH, PlayerRoot)
            .unwrap()
            .serve_at(OBJECT_PATH, fixture)
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    async fn client(address: &str) -> MediaClient {
        MediaClient {
            connection: zbus::connection::Builder::address(address)
                .unwrap()
                .build()
                .await
                .unwrap(),
            revision: Arc::new(AtomicU64::new(0)),
            action_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
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
        std::io::BufReader::new(daemon.0.stdout.as_mut().unwrap())
            .read_line(&mut address)
            .unwrap();
        (daemon, address.trim().to_owned())
    }

    #[test]
    fn mpris_names_are_validated_at_the_action_boundary() {
        assert!(validate_player_name(PLAYER_NAME).is_ok());
        assert!(validate_player_name("org.mpris.MediaPlayer2.").is_err());
        assert!(validate_player_name("org.freedesktop.DBus").is_err());
        assert!(validate_player_name("org.mpris.MediaPlayer2.bad-name").is_ok());
        assert!(validate_player_name("org.mpris.MediaPlayer2.bad/name").is_err());
    }

    #[tokio::test]
    async fn discovers_metadata_and_routes_capability_checked_actions() {
        let (_daemon, address) = private_bus();
        let fixture = PlayerFixture::new("Paused");
        let service = service(&address, PLAYER_NAME, fixture.clone()).await;
        let client = client(&address).await;

        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.players.len(), 1);
        assert_eq!(snapshot.current_player.as_deref(), Some(PLAYER_NAME));
        assert_eq!(snapshot.players[0].identity, "Fixture Player");
        assert_eq!(snapshot.players[0].title.as_deref(), Some("Fixture Track"));
        assert_eq!(
            snapshot.players[0].artist.as_deref(),
            Some("Fixture Artist")
        );
        assert_eq!(snapshot.players[0].playback_status, PlaybackStatus::Paused);

        let playing = client.play_pause(PLAYER_NAME).await.unwrap();
        assert_eq!(playing.players[0].playback_status, PlaybackStatus::Playing);
        assert_eq!(fixture.play_pause_calls.load(Ordering::Relaxed), 1);
        let paused = client.play_pause(PLAYER_NAME).await.unwrap();
        assert_eq!(paused.players[0].playback_status, PlaybackStatus::Paused);
        assert_eq!(fixture.play_pause_calls.load(Ordering::Relaxed), 2);

        fixture.flip_status_on_read.store(true, Ordering::Relaxed);
        let raced = client.play_pause(PLAYER_NAME).await.unwrap();
        assert_eq!(raced.players[0].playback_status, PlaybackStatus::Paused);
        assert_eq!(fixture.play_pause_calls.load(Ordering::Relaxed), 3);
        let next = client.next(PLAYER_NAME).await.unwrap();
        assert_eq!(next.players[0].title.as_deref(), Some("Next Track"));
        let previous = client.previous(PLAYER_NAME).await.unwrap();
        assert_eq!(previous.players[0].title.as_deref(), Some("Previous Track"));

        fixture.can_control.store(false, Ordering::Relaxed);
        assert!(matches!(
            client.next(PLAYER_NAME).await,
            Err(MediaError::UnsupportedAction { .. })
        ));
        fixture.can_control.store(true, Ordering::Relaxed);
        *fixture.status.lock().unwrap() = "Paused".to_owned();
        fixture.can_pause.store(false, Ordering::Relaxed);
        assert!(matches!(
            client.play_pause(PLAYER_NAME).await,
            Err(MediaError::UnsupportedAction { .. })
        ));
        *fixture.status.lock().unwrap() = "Playing".to_owned();
        assert!(matches!(
            client.play_pause(PLAYER_NAME).await,
            Err(MediaError::UnsupportedAction { .. })
        ));
        fixture.can_pause.store(true, Ordering::Relaxed);
        fixture.can_next.store(false, Ordering::Relaxed);
        assert!(matches!(
            client.next(PLAYER_NAME).await,
            Err(MediaError::UnsupportedAction { .. })
        ));
        fixture.denied.store(true, Ordering::Relaxed);
        assert!(matches!(
            client.play_pause(PLAYER_NAME).await,
            Err(MediaError::PermissionDenied(_))
        ));
        assert!(matches!(
            client.next("org.mpris.MediaPlayer2.unknown").await,
            Err(MediaError::PlayerUnavailable(_))
        ));
        fixture.denied.store(false, Ordering::Relaxed);
        fixture.vanished.store(true, Ordering::Relaxed);
        assert!(matches!(
            client.play_pause(PLAYER_NAME).await,
            Err(MediaError::PlayerUnavailable(name)) if name == PLAYER_NAME
        ));
        service.close().await.unwrap();
    }

    #[tokio::test]
    async fn a_broken_player_does_not_hide_a_healthy_player() {
        let (_daemon, address) = private_bus();
        let broken_name = "org.mpris.MediaPlayer2.a-broken";
        let broken_fixture = PlayerFixture::new("InvalidStatus");
        let broken_service = service(&address, broken_name, broken_fixture).await;
        let healthy_name = "org.mpris.MediaPlayer2.z-healthy";
        let healthy_service = service(&address, healthy_name, PlayerFixture::new("Playing")).await;
        let client = client(&address).await;

        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.players.len(), 1);
        assert_eq!(snapshot.players[0].bus_name, healthy_name);
        assert_eq!(snapshot.player_failures.len(), 1);
        assert_eq!(snapshot.player_failures[0].bus_name, broken_name);

        broken_service.close().await.unwrap();
        healthy_service.close().await.unwrap();
    }

    #[tokio::test]
    async fn watch_reports_player_removal_and_reappearance() {
        let (_daemon, address) = private_bus();
        let client = client(&address).await;
        let idle = client.snapshot().await.unwrap();
        assert!(idle.players.is_empty());
        assert_eq!(idle.availability, Availability::Disconnected);
        assert_eq!(idle.current_player, None);

        let first_service = service(&address, PLAYER_NAME, PlayerFixture::new("Playing")).await;
        let mut events = client.watch();
        let first = events.next().await.unwrap().unwrap();
        assert_eq!(first.players.len(), 1);

        first_service.close().await.unwrap();
        let removed = tokio::time::timeout(Duration::from_secs(3), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(removed.players.is_empty());
        assert_eq!(removed.availability, Availability::Disconnected);

        let replacement = service(&address, PLAYER_NAME, PlayerFixture::new("Paused")).await;
        let reappeared = tokio::time::timeout(Duration::from_secs(3), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(reappeared.players.len(), 1);
        assert_eq!(
            reappeared.players[0].playback_status,
            PlaybackStatus::Paused
        );
        replacement.close().await.unwrap();
    }

    #[tokio::test]
    async fn watch_stops_polling_when_its_stream_is_dropped() {
        let (_daemon, address) = private_bus();
        let fixture = PlayerFixture::new("Playing");
        let service = service(&address, PLAYER_NAME, fixture.clone()).await;
        let client = client(&address).await;
        let mut events = client.watch();
        events.next().await.unwrap().unwrap();
        let reads_after_initial_snapshot = fixture.status_reads.load(Ordering::Relaxed);
        drop(events);

        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert_eq!(
            fixture.status_reads.load(Ordering::Relaxed),
            reads_after_initial_snapshot
        );
        service.close().await.unwrap();
    }

    #[tokio::test]
    async fn watch_emits_error_category_transitions() {
        let (_daemon, address) = private_bus();
        let fixture = PlayerFixture::new("Playing");
        fixture.status_error.store(1, Ordering::Relaxed);
        let service = service(&address, PLAYER_NAME, fixture.clone()).await;
        let client = client(&address).await;
        let mut events = client.watch();

        let denied = tokio::time::timeout(Duration::from_secs(3), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            denied.player_failures[0].availability,
            Availability::PermissionDenied
        );
        let denied_reason = denied.player_failures[0].reason.clone();
        fixture.status_error.store(2, Ordering::Relaxed);
        let failed = tokio::time::timeout(Duration::from_secs(3), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(failed.player_failures[0].availability, Availability::Failed);
        assert_ne!(failed.player_failures[0].reason, denied_reason);
        service.close().await.unwrap();
    }
}
