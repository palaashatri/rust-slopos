//! The sound-server connection for a host that has none of its own (the
//! Volume Mixer app). One current-thread Tokio runtime blocks on the snapshot
//! stream and the command channel, runs each command on the server and wakes
//! the host for every answer. Nothing polls: a change arrives as an event
//! from the server, a command as a message from the window, and a new
//! connection only when the host asks for one with [`Provider::retry`].

use std::sync::mpsc;
use std::thread;

use slopos_services::audio::{AudioClient, AudioSnapshot};
use tokio::sync::mpsc as async_mpsc;

use crate::model::{apply, Command};

/// What the provider reports.
#[derive(Debug, Clone)]
pub enum Update {
    /// A snapshot of the sound server, or why there is none.
    State(Result<AudioSnapshot, String>),
    /// A command failed. The state reported before it is still current.
    Failed(String),
}

/// A running provider. Commands go in through [`Provider::send`]; answers
/// come out of `updates`, and the host's wake function is called for each.
pub struct Provider {
    commands: async_mpsc::UnboundedSender<Command>,
    retries: async_mpsc::UnboundedSender<()>,
    pub updates: mpsc::Receiver<Update>,
}

impl Provider {
    /// Start the provider thread. `wake` is called from it for every update.
    pub fn start(wake: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        let (commands, receiver) = async_mpsc::unbounded_channel();
        let (retries, retry_receiver) = async_mpsc::unbounded_channel();
        let (sender, updates) = mpsc::channel();
        thread::Builder::new()
            .name("volume-mixer-provider".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => {
                        runtime.block_on(run(receiver, retry_receiver, sender, wake));
                    }
                    Err(error) => {
                        let _ = sender.send(Update::State(Err(format!("mixer thread: {error}"))));
                        wake();
                    }
                }
            })?;
        Ok(Self {
            commands,
            retries,
            updates,
        })
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Ask for a new connection to the sound server. It is ignored while
    /// connected; after a failed connection it is the only way back.
    pub fn retry(&self) {
        let _ = self.retries.send(());
    }
}

/// Whether the window is still there, after one connection has ended.
enum Served {
    /// The window has gone; the provider stops.
    Gone,
    /// The server stopped answering; the state said so.
    Lost,
}

async fn run(
    mut commands: async_mpsc::UnboundedReceiver<Command>,
    mut retries: async_mpsc::UnboundedReceiver<()>,
    sender: mpsc::Sender<Update>,
    wake: impl Fn() + Send + 'static,
) {
    let send = |update: Update| {
        let _ = sender.send(update);
        wake();
    };
    loop {
        match AudioClient::connect().await {
            Ok(client) => match serve(&client, &mut commands, &mut retries, &send).await {
                Served::Gone => return,
                Served::Lost => {}
            },
            Err(error) => send(Update::State(Err(error.to_string()))),
        }
        // Nothing can change until a retry asks for a connection, so requests
        // are dropped in the meantime (the state already says what is wrong).
        if !wait_for_retry(&mut commands, &mut retries).await {
            return;
        }
    }
}

/// Serve one connection until the server stops answering or the window goes.
async fn serve(
    client: &AudioClient,
    commands: &mut async_mpsc::UnboundedReceiver<Command>,
    retries: &mut async_mpsc::UnboundedReceiver<()>,
    send: &impl Fn(Update),
) -> Served {
    send(Update::State(
        client.snapshot().await.map_err(|e| e.to_string()),
    ));
    let mut events = client.watch();
    let served = loop {
        tokio::select! {
            event = events.next() => match event {
                Some(result) => send(Update::State(result.map_err(|e| e.to_string()))),
                None => {
                    send(Update::State(Err("the sound server stopped reporting".into())));
                    break Served::Lost;
                }
            },
            command = commands.recv() => match command {
                Some(command) => match apply(client, &command).await {
                    Ok(snapshot) => send(Update::State(Ok(snapshot))),
                    Err(error) => {
                        send(Update::Failed(error.to_string()));
                        // Show the server's state again, so the controls do not keep
                        // the value that just failed. If it cannot be read, the view
                        // says the sound is unavailable instead.
                        send(Update::State(
                            client.snapshot().await.map_err(|e| e.to_string()),
                        ));
                    }
                },
                None => break Served::Gone,
            },
            // Already connected, so a retry changes nothing.
            retry = retries.recv() => {
                if retry.is_none() {
                    break Served::Gone;
                }
            }
        }
    };
    events.close().await;
    served
}

/// Drop requests until a retry arrives. Returns false once the window has gone.
async fn wait_for_retry(
    commands: &mut async_mpsc::UnboundedReceiver<Command>,
    retries: &mut async_mpsc::UnboundedReceiver<()>,
) -> bool {
    loop {
        tokio::select! {
            retry = retries.recv() => return retry.is_some(),
            command = commands.recv() => {
                if command.is_none() {
                    return false;
                }
            }
        }
    }
}
