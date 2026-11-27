//! Drag and drop (P1 item 4): the pure rules for dropping entries onto a
//! folder. The window turns a pointer drag into a [`Drop`]; the copy or move
//! itself runs through the same transactional `ops` as Copy and Paste, so the
//! overwrite and permission errors are the existing ones.
use std::path::{Path, PathBuf};

/// What a drop does. A plain drop copies, so nothing is lost by accident;
/// holding Shift moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Copy,
    Move,
}

/// A drop that may proceed: the unique sources and the destination folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drop {
    pub sources: Vec<PathBuf>,
    pub dest: PathBuf,
    pub action: Action,
}

/// Plan a drop of `sources` onto `target`. `target_is_dir` is the caller's
/// check on the destination. Refusals carry the message to show.
pub fn plan(
    sources: &[PathBuf],
    target: &Path,
    target_is_dir: bool,
    action: Action,
) -> Result<Drop, &'static str> {
    if sources.is_empty() {
        return Err("Nothing to drop.");
    }
    if !target_is_dir {
        return Err("Items can only be dropped onto a folder.");
    }
    let mut unique: Vec<PathBuf> = Vec::with_capacity(sources.len());
    for source in sources {
        if !unique.contains(source) {
            unique.push(source.clone());
        }
    }
    for source in &unique {
        if target == source || target.starts_with(source) {
            return Err("A folder can't be dropped into itself.");
        }
    }
    if action == Action::Move && unique.iter().all(|source| source.parent() == Some(target)) {
        return Err("Those items are already in that folder.");
    }
    Ok(Drop {
        sources: unique,
        dest: target.to_path_buf(),
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    #[test]
    fn a_plain_drop_copies_and_shift_moves() {
        let drop = plan(
            &[p("/home/a/x.txt")],
            &p("/home/a/Docs"),
            true,
            Action::Copy,
        )
        .unwrap();
        assert_eq!(drop.action, Action::Copy);
        assert_eq!(drop.dest, p("/home/a/Docs"));
        let drop = plan(
            &[p("/home/a/x.txt")],
            &p("/home/a/Docs"),
            true,
            Action::Move,
        )
        .unwrap();
        assert_eq!(drop.action, Action::Move);
    }

    #[test]
    fn dropping_onto_a_file_or_nothing_is_refused() {
        assert_eq!(
            plan(&[p("/a/x")], &p("/a/y.txt"), false, Action::Copy),
            Err("Items can only be dropped onto a folder.")
        );
        assert_eq!(
            plan(&[], &p("/a"), true, Action::Copy),
            Err("Nothing to drop.")
        );
    }

    #[test]
    fn a_folder_cannot_go_into_itself_or_a_descendant() {
        let folder = p("/home/a/Projects");
        assert!(plan(std::slice::from_ref(&folder), &folder, true, Action::Copy).is_err());
        assert!(plan(
            std::slice::from_ref(&folder),
            &folder.join("sub"),
            true,
            Action::Copy
        )
        .is_err());
        // A sibling whose name merely starts with the same text is fine.
        assert!(plan(&[folder], &p("/home/a/Projects-old"), true, Action::Copy).is_ok());
    }

    #[test]
    fn moving_within_the_same_folder_is_a_no_op_refusal() {
        let dest = p("/home/a/Docs");
        assert_eq!(
            plan(&[p("/home/a/Docs/x")], &dest, true, Action::Move),
            Err("Those items are already in that folder.")
        );
        // Copying into the same folder is allowed: the conflict dialog offers Keep Both.
        assert!(plan(&[p("/home/a/Docs/x")], &dest, true, Action::Copy).is_ok());
    }

    #[test]
    fn duplicate_sources_are_dropped_once() {
        let drop = plan(
            &[p("/a/x"), p("/a/x"), p("/a/y")],
            &p("/b"),
            true,
            Action::Copy,
        )
        .unwrap();
        assert_eq!(drop.sources, [p("/a/x"), p("/a/y")]);
    }
}
