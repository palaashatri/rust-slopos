//! SLOPOS polkit authentication agent.
//!
//! A background connection serves polkit's `AuthenticationAgent` on the system
//! bus. Each password prompt opens a small toolkit window for as long as it is
//! needed; the window closes on submit, cancel or a polkit cancel. The
//! password goes to the polkit helper and nowhere else.
//!
//! Started by the desktop session? No: the session does not start agents yet,
//! so this binary must be launched by the session (or by hand) to be used.

mod prompt;

use std::{
    collections::VecDeque,
    sync::{mpsc, Arc, Mutex},
};

use accesskit::TreeUpdate;
use slopos_services::polkit::{
    agent::{
        AgentService, PromptReply, PromptRequest, PromptSink, UiCommand, AGENT_OBJECT_PATH,
    },
    helper::{PamHelper, HELPER_CANDIDATES},
};
use slopos_ui::event::Event;
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::render::RasterSurface;
use tokio::sync::oneshot;

use prompt::{Outcome, PromptView};

const APP_ID: &str = "slopos-polkit-agent";

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// The agent's side of the UI: a queue plus the window's proxy, so a command
/// wakes the open window.
struct Sink {
    sender: mpsc::Sender<UiCommand>,
    proxy: Mutex<Option<Proxy>>,
}

impl PromptSink for Sink {
    fn send(&self, command: UiCommand) -> bool {
        let sent = self.sender.send(command).is_ok();
        if let Some(proxy) = lock(&self.proxy).as_ref() {
            proxy.wake();
        }
        sent
    }
}

/// Commands the main loop has not handled yet. The open window defers
/// commands it does not own, so they are handled in order after it closes.
struct Inbox {
    receiver: Mutex<mpsc::Receiver<UiCommand>>,
    deferred: Mutex<VecDeque<UiCommand>>,
}

impl Inbox {
    /// The next command, blocking until one arrives. No polling.
    fn next(&self) -> Option<UiCommand> {
        if let Some(command) = lock(&self.deferred).pop_front() {
            return Some(command);
        }
        lock(&self.receiver).recv().ok()
    }
}

struct PromptWindow {
    view: PromptView,
    /// Answers the agent's prompt; taken once.
    reply: Option<oneshot::Sender<PromptReply>>,
    cookie: String,
    inbox: Arc<Inbox>,
}

impl PromptWindow {
    /// Handle what arrived while the window was open: a cancel for this
    /// request closes it; everything else waits in the inbox.
    fn drain_inbox(&mut self, control: &mut Control) {
        let mut cancelled = false;
        loop {
            let Ok(command) = lock(&self.inbox.receiver).try_recv() else {
                break;
            };
            match command {
                UiCommand::Cancel { cookie } if cookie == self.cookie => cancelled = true,
                other => lock(&self.inbox.deferred).push_back(other),
            }
        }
        if cancelled {
            self.reply.take();
            control.quit();
        }
    }

    fn answer(&mut self, reply: PromptReply, control: &mut Control) {
        if let Some(sender) = self.reply.take() {
            let _ = sender.send(reply);
        }
        control.quit();
    }
}

impl App for PromptWindow {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::Wake => self.drain_inbox(control),
            Event::CloseRequested => self.answer(PromptReply::Cancel, control),
            _ => match self.view.handle(event) {
                Outcome::Ignored => {}
                Outcome::Changed => control.request_redraw(),
                Outcome::Submit(password) => self.answer(PromptReply::Password(password), control),
                Outcome::Cancel => self.answer(PromptReply::Cancel, control),
                Outcome::Acknowledged => control.quit(),
            },
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        self.view.paint(surface);
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        Some(self.view.accessibility())
    }
}

/// Show one window until its answer. Returns when it closes.
fn show(
    view: PromptView,
    reply: Option<oneshot::Sender<PromptReply>>,
    cookie: String,
    inbox: &Arc<Inbox>,
    sink: &Arc<Sink>,
) {
    let mut view = view;
    let (width, height) = view.layout();
    let mut options = WindowOptions::new("Authentication Required", APP_ID, width, height);
    options.resizable = false;
    let app = PromptWindow {
        view,
        reply,
        cookie,
        inbox: Arc::clone(inbox),
    };
    let slot = Arc::clone(sink);
    let result = platform::run(options, app, move |proxy| {
        // Commands sent before the window had a proxy are handled now.
        proxy.wake();
        *lock(&slot.proxy) = Some(proxy);
    });
    *lock(&sink.proxy) = None;
    if let Err(error) = result {
        eprintln!("slopos-polkit-agent: {error}");
    }
}

fn main() {
    let (sender, receiver) = mpsc::channel();
    let sink = Arc::new(Sink {
        sender,
        proxy: Mutex::new(None),
    });
    let inbox = Arc::new(Inbox {
        receiver: Mutex::new(receiver),
        deferred: Mutex::new(VecDeque::new()),
    });
    let helper = PamHelper::installed()
        .unwrap_or_else(|| PamHelper::new(HELPER_CANDIDATES[0]));
    let connection = match futures::executor::block_on(zbus::Connection::system()) {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("slopos-polkit-agent: cannot reach the system bus: {error}");
            std::process::exit(1);
        }
    };
    let agent = futures::executor::block_on(AgentService::start(
        connection,
        sink.clone(),
        Arc::new(helper),
        AGENT_OBJECT_PATH,
    ));
    // Kept alive for the life of the process so the registration stays served.
    let _agent = match agent {
        Ok(agent) => agent,
        Err(error) => {
            eprintln!("slopos-polkit-agent: cannot register with polkit: {error}");
            std::process::exit(1);
        }
    };
    // The agent stays registered for the life of this process; the loop
    // below is the UI and blocks on the inbox.
    while let Some(command) = inbox.next() {
        match command {
            // A cancel with no open window has nothing to close.
            UiCommand::Cancel { .. } => {}
            UiCommand::Prompt(PromptRequest {
                cookie,
                action_id,
                message,
                username,
                wrong_password,
                reply,
            }) => {
                let view = PromptView::password(&message, &action_id, &username, wrong_password);
                show(view, Some(reply), cookie, &inbox, &sink);
            }
            UiCommand::Error(message) => {
                show(PromptView::error(&message), None, String::new(), &inbox, &sink);
            }
        }
    }
}
