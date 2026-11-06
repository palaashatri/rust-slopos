//! SLOPOS Volume Mixer: one fader and a Mute box for the default output, the
//! default input and every application playing sound, like the Windows XP
//! mixer. The top bar's sound menu opens the same controls as a popover.
//!
//! Every value comes from the sound server through `slopos-mixer`; a missing
//! server is shown as unavailable, never as a made-up level. While it is
//! unavailable, "Try again" (or R) asks the provider to connect once more.

use std::sync::{Arc, OnceLock};

use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use slopos_mixer::provider::{Provider, Update};
use slopos_mixer::MixerView;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::RasterSurface;
use slopos_ui::text::Font;

const APP_ID: &str = "slopos-volume-mixer";
const ROOT_NODE: u64 = 0;
/// The retry button's accessibility id. The mixer's own nodes start at 1000.
const RETRY_NODE: u64 = 900;
const RETRY_HINT: &str = "Press R or Try again to reconnect.";
const RETRY_WIDTH: u32 = 120;
const RETRY_HEIGHT: u32 = 26;
const MARGIN: i32 = 8;

struct VolumeMixer {
    view: MixerView,
    provider: Provider,
    retry: Button,
    font: Font,
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

    /// Whether the event is for the retry button, or asks for a retry through
    /// accessibility. A retry is only offered while the server is unavailable,
    /// so the button and its node exist only then.
    fn retry_event(&mut self, event: &Event, control: &mut Control) -> bool {
        if !self.view.is_unavailable() {
            return false;
        }
        if matches!(
            event,
            Event::Accessibility(AccessibilityRequest::Click(id)) if *id == RETRY_NODE
        ) {
            self.provider.retry();
            return true;
        }
        match self.retry.handle(event) {
            ButtonOutcome::Ignored => false,
            ButtonOutcome::Changed => {
                control.request_redraw();
                true
            }
            ButtonOutcome::Activated => {
                self.provider.retry();
                control.request_redraw();
                true
            }
        }
    }
}

impl App for VolumeMixer {
    fn event(&mut self, event: &Event, control: &mut Control) {
        if self.retry_event(event, control) {
            return;
        }
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
        if self.view.is_unavailable() {
            let _ = self.retry.paint(surface, &self.font);
        }
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_label("Volume Mixer");
        let mut children: Vec<NodeId> = self
            .view
            .accessibility_children()
            .into_iter()
            .map(NodeId)
            .collect();
        let mut nodes = Vec::new();
        if self.view.is_unavailable() {
            children.push(NodeId(RETRY_NODE));
            nodes.push((NodeId(RETRY_NODE), self.retry.accessibility_node()));
        }
        root.set_children(children);
        nodes.insert(0, (NodeId(ROOT_NODE), root));
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

/// The app's state for a window `width` by `height` pixels: the view fitted to
/// the width, and the retry button in the bottom-left corner, where the
/// unavailable status leaves room.
fn new_app(provider: Provider, width: u32, height: u32) -> VolumeMixer {
    let mut view = MixerView::new();
    view.set_retry_hint(Some(RETRY_HINT));
    view.fit_width(width);
    let bounds = Rect {
        x: MARGIN,
        y: height as i32 - RETRY_HEIGHT as i32 - MARGIN,
        width: RETRY_WIDTH,
        height: RETRY_HEIGHT,
    };
    VolumeMixer {
        view,
        provider,
        retry: Button::new(RETRY_NODE, "Try again", ButtonPlateVariant::Default)
            .with_bounds(bounds),
        font: Font::system(12.0),
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
    let app = new_app(provider, width, height);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_again_is_offered_only_while_the_server_is_unavailable() {
        let provider = Provider::start(|| {}).expect("the provider thread starts");
        let mut app = new_app(provider, 860, 250);
        let mut control = Control::default();
        let click = Event::Accessibility(AccessibilityRequest::Click(RETRY_NODE));
        // Before any answer the server is not known to be unavailable, so no retry.
        assert!(!app.retry_event(&click, &mut control));
        assert!(!app
            .accessibility()
            .expect("the app has a tree")
            .nodes
            .iter()
            .any(|(id, _)| *id == NodeId(RETRY_NODE)));
        app.view.set_state(&Err("no PulseAudio server".into()));
        assert!(app.retry_event(&click, &mut control));
        let tree = app.accessibility().expect("the app has a tree");
        assert!(tree.nodes.iter().any(|(id, _)| *id == NodeId(RETRY_NODE)));
    }
}
