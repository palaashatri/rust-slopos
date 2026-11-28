//! The dialogs behind the shared operations. Files and the desktop both
//! build them here, so the same question gets the same words, buttons and
//! defaults wherever it is asked.
//!
//! Each builder returns a laid-out [`Dialog`]; the caller keeps it, routes
//! events to it, and maps the chosen button index with the constants or
//! [`conflict_policy`] below.

use std::path::Path;

use slopos_ui::controls::dialog::Dialog;

use crate::ops::Conflict;

/// Button index of “Rename” in [`rename`] (“Cancel” is 0).
pub const RENAME_CONFIRM: usize = 1;

/// Button index of “Skip” in [`conflict`].
pub const CONFLICT_SKIP: usize = 1;
/// Button index of “Keep Both” in [`conflict`], the default.
pub const CONFLICT_KEEP_BOTH: usize = 2;
/// Button index of “Replace” in [`conflict`].
pub const CONFLICT_REPLACE: usize = 3;

/// The policy a [`conflict`] button stands for; `None` for Cancel.
pub fn conflict_policy(index: usize) -> Option<Conflict> {
    match index {
        CONFLICT_SKIP => Some(Conflict::Skip),
        CONFLICT_KEEP_BOTH => Some(Conflict::KeepBoth),
        CONFLICT_REPLACE => Some(Conflict::Replace),
        _ => None,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// “Rename” with the name in a field. The name without its extension is
/// selected, as people usually keep the extension.
pub fn rename(id: u64, path: &Path, window: (u32, u32)) -> Dialog {
    let name = file_name(path);
    let mut dialog = Dialog::new(
        id,
        "Rename",
        &format!("Enter a new name for “{name}”."),
        &["Cancel", "Rename"],
    )
    .with_field("Name", &name);
    if let Some(field) = &mut dialog.field {
        if let Some(dot) = name.rfind('.').filter(|&dot| dot > 0) {
            field.select(0, dot);
        }
    }
    dialog.layout(window);
    dialog
}

/// “An item with this name already exists”, for the item at `target`.
/// Enter keeps both; the buttons are Cancel, Skip, Keep Both, Replace.
pub fn conflict(id: u64, target: &Path, window: (u32, u32)) -> Dialog {
    let name = file_name(target);
    let folder = target
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();
    let mut dialog = Dialog::new(
        id,
        "An item with this name already exists",
        &format!(
            "“{name}” already exists in “{folder}”. Replace it, keep both, or skip the items that conflict?"
        ),
        &["Cancel", "Skip", "Keep Both", "Replace"],
    );
    dialog.set_default(CONFLICT_KEEP_BOTH);
    dialog.layout(window);
    dialog
}

/// A message with one OK button, for a failed operation.
pub fn message(id: u64, title: &str, text: &str, window: (u32, u32)) -> Dialog {
    let mut dialog = Dialog::new(id, title, text, &["OK"]);
    dialog.layout(window);
    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_buttons_map_to_policies() {
        assert_eq!(conflict_policy(0), None);
        assert_eq!(conflict_policy(CONFLICT_SKIP), Some(Conflict::Skip));
        assert_eq!(
            conflict_policy(CONFLICT_KEEP_BOTH),
            Some(Conflict::KeepBoth)
        );
        assert_eq!(conflict_policy(CONFLICT_REPLACE), Some(Conflict::Replace));
        assert_eq!(conflict_policy(4), None);
    }

    #[test]
    fn rename_has_the_name_field_and_confirm_button() {
        let dialog = rename(7, Path::new("/home/u/Desktop/report.final.txt"), (800, 600));
        assert_eq!(dialog.title, "Rename");
        assert_eq!(dialog.field_text(), Some("report.final.txt"));
        assert_eq!(dialog.buttons.len(), 2);
        assert_eq!(dialog.buttons[RENAME_CONFIRM].label, "Rename");
    }

    #[test]
    fn conflict_defaults_to_keep_both() {
        let dialog = conflict(9, Path::new("/x/Folder/a.txt"), (800, 600));
        assert_eq!(dialog.default, CONFLICT_KEEP_BOTH);
        assert_eq!(dialog.buttons.len(), 4);
        assert!(dialog.message.contains("“a.txt”"));
        assert!(dialog.message.contains("/x/Folder"));
    }

    #[test]
    fn message_has_one_button() {
        let dialog = message(3, "Can't rename", "busy", (800, 600));
        assert_eq!(dialog.buttons.len(), 1);
        assert_eq!(dialog.title, "Can't rename");
    }
}
