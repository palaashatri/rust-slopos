//! The password dialog's logic, separate from the window so it can be tested
//! by sending events. The password is handed out once, inside `Outcome::Submit`,
//! and is never shown in the accessibility tree (the field is a secret field).

use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use slopos_services::polkit::agent::Zeroizing;
use slopos_ui::controls::dialog::{Dialog, DialogOutcome};
use slopos_ui::event::Event;
use slopos_ui::render::RasterSurface;

/// The dialog's own id; its field and buttons take the ids after it.
pub const DIALOG_ID: u64 = 1;
/// The window's root node, above the dialog.
pub const ROOT_NODE: u64 = 0;
const WIDTH: u32 = 420;
const CANCEL: usize = 0;
const AUTHENTICATE: usize = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The event did nothing here.
    Ignored,
    /// The dialog changed (focus, typing); repaint.
    Changed,
    /// The user submitted this password.
    Submit(Zeroizing<String>),
    /// The user cancelled a password prompt.
    Cancel,
    /// The user dismissed an error message.
    Acknowledged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Password,
    Error,
}

pub struct PromptView {
    dialog: Dialog,
    mode: Mode,
}

impl PromptView {
    /// A password prompt for `message` (polkit's text) to authenticate as
    /// `username` for `action_id`. `wrong_password` adds the retry line.
    pub fn password(message: &str, action_id: &str, username: &str, wrong_password: bool) -> Self {
        let mut text = String::new();
        if wrong_password {
            text.push_str("Wrong password. ");
        }
        text.push_str(message);
        text.push_str(&format!(
            " Action: {action_id}. Authenticate as {username}."
        ));
        let dialog = Dialog::new(
            DIALOG_ID,
            "Authentication Required",
            &text,
            &["Cancel", "Authenticate"],
        )
        .with_secret_field("Password");
        Self {
            dialog,
            mode: Mode::Password,
        }
    }

    /// A message the user must dismiss, for example a helper that is not
    /// installed.
    pub fn error(message: &str) -> Self {
        let dialog = Dialog::new(DIALOG_ID, "Authentication Failed", message, &["OK"]);
        Self {
            dialog,
            mode: Mode::Error,
        }
    }

    #[cfg(test)]
    pub fn message(&self) -> &str {
        &self.dialog.message
    }

    /// Size of the window that shows the dialog, and lay the dialog out in it.
    pub fn layout(&mut self) -> (u32, u32) {
        self.dialog.layout_standalone(WIDTH)
    }

    pub fn handle(&mut self, event: &Event) -> Outcome {
        match self.dialog.handle(event) {
            DialogOutcome::Ignored => Outcome::Ignored,
            // Copy and paste are refused by the secret field, so these
            // should not occur; they are never acted on.
            DialogOutcome::Changed | DialogOutcome::Copy(_) | DialogOutcome::PasteRequested => {
                Outcome::Changed
            }
            DialogOutcome::Chosen { index, text } => match self.mode {
                Mode::Password if index == AUTHENTICATE => {
                    Outcome::Submit(Zeroizing::new(text.unwrap_or_default()))
                }
                Mode::Password if index == CANCEL => Outcome::Cancel,
                Mode::Password => Outcome::Ignored,
                Mode::Error => Outcome::Acknowledged,
            },
        }
    }

    pub fn paint(&mut self, surface: &RasterSurface) {
        let _ = self.dialog.paint(surface);
    }

    /// The window's accessibility tree: a window node over the dialog.
    pub fn accessibility(&self) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_label("Authentication");
        root.set_children(vec![NodeId(DIALOG_ID)]);
        let mut nodes = vec![(NodeId(ROOT_NODE), root)];
        nodes.extend(
            self.dialog
                .accessibility_nodes()
                .into_iter()
                .map(|(id, node)| (NodeId(id), node)),
        );
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(ROOT_NODE))),
            tree_id: TreeId::ROOT,
            focus: NodeId(self.dialog.focus_node()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::{Key, Modifiers};

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn typed(view: &mut PromptView, text: &str) {
        for character in text.chars() {
            view.handle(&key(Key::Character(character)));
        }
    }

    #[test]
    fn enter_submits_the_typed_password_once() {
        let mut view =
            PromptView::password("Install software.", "org.example.install", "root", false);
        view.layout();
        typed(&mut view, "hunter2");
        match view.handle(&key(Key::Enter)) {
            Outcome::Submit(password) => assert_eq!(password.as_str(), "hunter2"),
            other => panic!("expected a submit, got {other:?}"),
        }
    }

    #[test]
    fn escape_cancels_a_password_prompt() {
        let mut view =
            PromptView::password("Install software.", "org.example.install", "root", false);
        view.layout();
        typed(&mut view, "abc");
        assert_eq!(view.handle(&key(Key::Escape)), Outcome::Cancel);
    }

    #[test]
    fn an_error_is_dismissed_by_enter_or_escape() {
        for dismiss in [Key::Enter, Key::Escape] {
            let mut view = PromptView::error("The polkit authentication helper is not installed.");
            view.layout();
            assert_eq!(view.handle(&key(dismiss)), Outcome::Acknowledged);
        }
    }

    #[test]
    fn wrong_password_line_is_shown_only_when_asked() {
        let first = PromptView::password("Install software.", "a.b", "root", false);
        assert!(!first.message().contains("Wrong password"));
        let retry = PromptView::password("Install software.", "a.b", "root", true);
        assert!(retry.message().starts_with("Wrong password."));
        assert!(retry.message().contains("Authenticate as root."));
    }

    #[test]
    fn the_password_never_appears_in_the_accessibility_tree() {
        let mut view = PromptView::password("Install software.", "a.b", "root", false);
        view.layout();
        typed(&mut view, "hunter2");
        let tree = view.accessibility();
        let secret_nodes: Vec<&Node> = tree
            .nodes
            .iter()
            .map(|(_, node)| node)
            .filter(|node| node.role() == Role::PasswordInput)
            .collect();
        assert_eq!(secret_nodes.len(), 1, "one password field");
        assert_eq!(secret_nodes[0].value(), None);
        for (_, node) in &tree.nodes {
            assert!(!format!("{node:?}").contains("hunter2"));
        }
    }

    #[test]
    fn the_window_tree_has_a_root_over_the_dialog() {
        let mut view = PromptView::password("Install software.", "a.b", "root", false);
        view.layout();
        let tree = view.accessibility();
        assert_eq!(tree.nodes[0].0, NodeId(ROOT_NODE));
        assert_eq!(tree.nodes[0].1.children(), &[NodeId(DIALOG_ID)]);
        assert!(tree.nodes.iter().any(|(id, _)| *id == NodeId(DIALOG_ID)));
    }
}
