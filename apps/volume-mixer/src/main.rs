//! SLOPOS Volume Mixer: one fader and a Mute box for the default output, the
//! default input and every application playing sound, like the Windows XP
//! mixer. The top bar's sound menu opens the same controls as a popover.
//!
//! Every value comes from the sound server through `slopos-mixer`; a missing
//! server is shown as unavailable, never as a made-up level.

use std::sync::{Arc, OnceLock};

use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use slopos_mixer::provider::{Provider, Update};
use slopos_mixer::MixerView;
use slopos_ui::event::{Event, Key};
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::render::RasterSurface;

const APP_ID: &str = "slopos-volume-mixer";
const ROOT_NODE: u64 = 0;

struct VolumeMixer {
    view: MixerView,
    provider: Provider,
}

impl VolumeMixer {
    fn apply_updates(&mut self, control: &mut Control) {
        let mut changed = false;
        while let Ok(update) = self.provider.updates.try_recv() {
            match update {
                Update::State(state) => self.view.set_state(&state),
                Update::Failed(message) => self
                    .view
                    .set_failure(&format!("Could not change the sound: {message}")),
            }
            changed = true;
        }
        if changed {
            control.request_redraw();
        }
    }
}

impl App for VolumeMixer {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::Wake => self.apply_updates(control),
            Event::CloseRequested => control.quit(),
            Event::PointerPressed { .. } | Event::PointerReleased { .. } => {
                let outcome = self.view.handle(event);
                if let Some(command) = outcome.command {
                    self.provider.send(command);
                }
                if outcome.redraw {
                    control.request_redraw();
                }
            }
            // R asks the sound server again, for a mixer opened before the server ran.
            Event::KeyPressed {
                key: Key::Character('r' | 'R'),
                ..
            } => self.provider.retry(),
            _ => {
                let outcome = self.view.handle(event);
                if let Some(command) = outcome.command {
                    self.provider.send(command);
                }
                if outcome.redraw {
                    control.request_redraw();
                }
            }
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let _ = self.view.paint(surface);
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_label("Volume Mixer");
        root.set_children(
            self.view
                .accessibility_children()
                .into_iter()
                .map(NodeId)
                .collect::<Vec<_>>(),
        );
        let mut nodes = vec![(NodeId(ROOT_NODE), root)];
        nodes.extend(
            self.view
                .accessibility_nodes()
                .into_iter()
                .map(|(id, node)| (NodeId(id), node)),
        );
        Some(TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(ROOT_NODE))),
            tree_id: TreeId::ROOT,
            // The focused control, or the window when none has focus.
            focus: NodeId(self.view.focused_node().unwrap_or(ROOT_NODE)),
        })
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    // The provider starts before the window exists, so its wake-ups go
    // through a slot that the window fills in once it has its proxy.
    let proxy_slot: Arc<OnceLock<Proxy>> = Arc::new(OnceLock::new());
    let wake_slot = Arc::clone(&proxy_slot);
    let provider = match Provider::start(move || {
        if let Some(proxy) = wake_slot.get() {
            proxy.wake();
        }
    }) {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!("slopos-volume-mixer: cannot start the sound provider: {error}");
            std::process::exit(1);
        }
    };
    // The window is as wide as the room the channels need, but never wider than
    // the screen; channels that do not fit are counted in the status line.
    let (full_width, height) = MixerView::full_size();
    let screen_width = platform::x11::screen_size()
        .map(|(width, _)| width)
        .unwrap_or(full_width);
    let width = full_width.min(screen_width.saturating_sub(8)).max(1);
    let mut options = WindowOptions::new("Volume Mixer", APP_ID, width, height);
    options.resizable = false;
    let mut view = MixerView::new();
    view.fit_width(width);
    let app = VolumeMixer { view, provider };
    let result = platform::run(options, app, move |proxy| {
        let _ = proxy_slot.set(proxy);
        // Anything the provider sent before the slot was filled.
        if let Some(proxy) = proxy_slot.get() {
            proxy.wake();
        }
    });
    if let Err(error) = result {
        eprintln!("slopos-volume-mixer: {error}");
        std::process::exit(1);
    }
}
