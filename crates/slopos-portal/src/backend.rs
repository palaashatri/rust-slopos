//! Runs a chooser for one request.
//!
//! [`ProcessBackend`] starts the dialog program (`slopos-file-chooser`, from
//! `slopos_core::process::FILE_CHOOSERS`) with piped stdin and stdout through
//! the shared launch helper, sends the request as JSON and reads the reply as
//! JSON. [`FirstFileBackend`] is the test mode: it answers with the first
//! matching entry of the folder and never shows a window.

use std::io::Write;
use std::path::{Path, PathBuf};

use slopos_core::process::{launch_captured, FILE_CHOOSERS};
use slopos_ui::controls::file_chooser::list_entries;

use crate::spec::{ChooserReply, ChooserSpec, Kind};

/// Runs one chooser. `started` is called with the child's process id as
/// soon as a child exists, so the caller can close the request.
pub trait Backend: Send + Sync {
    fn choose(&self, spec: &ChooserSpec, started: &dyn Fn(u32)) -> ChooserReply;
}

/// Candidate programs in preference order, in the shape of
/// `slopos_core::process::TERMINALS`.
pub type Candidates = &'static [(&'static str, &'static [&'static str])];

/// Runs the dialog program: the request goes in as JSON on stdin and the
/// reply comes back as JSON on stdout.
#[derive(Debug, Clone, Copy)]
pub struct ProcessBackend {
    candidates: Candidates,
}

impl Default for ProcessBackend {
    fn default() -> Self {
        Self {
            candidates: FILE_CHOOSERS,
        }
    }
}

impl ProcessBackend {
    /// A backend that runs the first installed program of `candidates`.
    pub fn with_candidates(candidates: Candidates) -> Self {
        Self { candidates }
    }
}

impl Backend for ProcessBackend {
    fn choose(&self, spec: &ChooserSpec, started: &dyn Fn(u32)) -> ChooserReply {
        let request = match serde_json::to_vec(spec) {
            Ok(request) => request,
            Err(error) => {
                return ChooserReply::Failed {
                    message: format!("could not encode the request: {error}"),
                }
            }
        };
        // The chooser's reply is the answer, so it runs as the portal's child
        // and is waited for below; the launch helper reaps it either way.
        let mut child = match launch_captured(self.candidates) {
            Ok(child) => child,
            Err(message) => {
                return ChooserReply::Failed {
                    message: format!("could not start the file chooser: {message}"),
                }
            }
        };
        started(child.id());
        if let Some(mut stdin) = child.take_stdin() {
            // A chooser that exits early closes stdin; its reply decides.
            let _ = stdin.write_all(&request);
        }
        match child.wait_with_output() {
            Ok(output) if output.status.success() => serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|error| ChooserReply::Failed {
                    message: format!("unreadable chooser reply: {error}"),
                }),
            Ok(output) => ChooserReply::Failed {
                message: format!("the file chooser exited with {}", output.status),
            },
            Err(error) => ChooserReply::Failed {
                message: format!("waiting for the file chooser failed: {error}"),
            },
        }
    }
}

/// Test mode: answers without a window. Open picks the first accepted file
/// in the folder, open-folder picks the folder itself, and save uses the
/// suggested name in the folder.
#[derive(Debug, Default, Clone, Copy)]
pub struct FirstFileBackend;

impl Backend for FirstFileBackend {
    fn choose(&self, spec: &ChooserSpec, _started: &dyn Fn(u32)) -> ChooserReply {
        let folder = spec
            .folder
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        first_choice(spec, &folder).unwrap_or_else(|message| ChooserReply::Failed { message })
    }
}

fn first_choice(spec: &ChooserSpec, folder: &Path) -> Result<ChooserReply, String> {
    match spec.kind {
        Kind::OpenFolder => Ok(ChooserReply::Chosen {
            paths: vec![folder.to_path_buf()],
        }),
        Kind::Save => {
            let name = spec
                .name
                .as_deref()
                .ok_or_else(|| "no name to save as".to_owned())?;
            Ok(ChooserReply::Chosen {
                paths: vec![folder.join(name)],
            })
        }
        Kind::Open => {
            let (entries, _) = list_entries(folder, &|name| spec.accepts(name), false)
                .map_err(|error| format!("{}: {error}", folder.display()))?;
            match entries.into_iter().find(|entry| !entry.is_dir) {
                Some(entry) => Ok(ChooserReply::Chosen {
                    paths: vec![folder.join(entry.name)],
                }),
                None => Err(format!("no file in {}", folder.display())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{FilterSpec, Pattern};
    use std::cell::Cell;

    const MISSING: Candidates = &[("/nonexistent/slopos-file-chooser", &[])];

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Folder")).unwrap();
        std::fs::write(dir.path().join("b.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("a.png"), b"x").unwrap();
        dir
    }

    #[test]
    fn first_file_backend_picks_the_first_accepted_file() {
        let dir = fixture();
        let mut spec = ChooserSpec::new(Kind::Open, "Open");
        spec.folder = Some(dir.path().to_path_buf());
        assert_eq!(
            FirstFileBackend.choose(&spec, &|_| {}),
            ChooserReply::Chosen {
                paths: vec![dir.path().join("a.png")]
            }
        );
        spec.filters.push(FilterSpec {
            name: "Text".into(),
            patterns: vec![Pattern::Glob("*.txt".into())],
        });
        assert_eq!(
            FirstFileBackend.choose(&spec, &|_| {}),
            ChooserReply::Chosen {
                paths: vec![dir.path().join("b.txt")]
            }
        );
    }

    #[test]
    fn first_file_backend_handles_folders_saves_and_empty_folders() {
        let dir = fixture();
        let mut folder = ChooserSpec::new(Kind::OpenFolder, "Folder");
        folder.folder = Some(dir.path().to_path_buf());
        assert_eq!(
            FirstFileBackend.choose(&folder, &|_| {}),
            ChooserReply::Chosen {
                paths: vec![dir.path().to_path_buf()]
            }
        );
        let mut save = ChooserSpec::new(Kind::Save, "Save");
        save.folder = Some(dir.path().to_path_buf());
        assert!(matches!(
            FirstFileBackend.choose(&save, &|_| {}),
            ChooserReply::Failed { .. }
        ));
        save.name = Some("new.txt".into());
        assert_eq!(
            FirstFileBackend.choose(&save, &|_| {}),
            ChooserReply::Chosen {
                paths: vec![dir.path().join("new.txt")]
            }
        );
        let empty = tempfile::tempdir().unwrap();
        let mut open = ChooserSpec::new(Kind::Open, "Open");
        open.folder = Some(empty.path().to_path_buf());
        assert!(matches!(
            FirstFileBackend.choose(&open, &|_| {}),
            ChooserReply::Failed { .. }
        ));
    }

    #[test]
    fn a_missing_program_is_a_failed_reply_not_a_panic() {
        let backend = ProcessBackend::with_candidates(MISSING);
        let reply = backend.choose(&ChooserSpec::new(Kind::Open, "x"), &|_| {});
        match reply {
            ChooserReply::Failed { message } => {
                assert!(
                    message.contains("could not start the file chooser"),
                    "{message}"
                )
            }
            other => panic!("expected a failed reply, got {other:?}"),
        }
    }

    #[test]
    fn the_reply_is_read_from_the_captured_chooser_and_its_pid_is_reported() {
        // A stand-in chooser: it reads the request on stdin and prints a reply.
        let reply = ChooserReply::Chosen {
            paths: vec![PathBuf::from("/picked/file.txt")],
        };
        let json = serde_json::to_string(&reply).unwrap();
        let script: &'static str =
            Box::leak(format!("cat >/dev/null; printf '%s' '{json}'").into_boxed_str());
        let args: &'static [&'static str] = Box::leak(vec!["-c", script].into_boxed_slice());
        let candidates: Candidates = Box::leak(vec![("/bin/sh", args)].into_boxed_slice());

        let pid = Cell::new(0u32);
        let got = ProcessBackend::with_candidates(candidates)
            .choose(&ChooserSpec::new(Kind::Open, "Open"), &|started| {
                pid.set(started)
            });
        assert_eq!(got, reply);
        assert_ne!(
            pid.get(),
            0,
            "the chooser's pid is reported before the wait"
        );
    }
}
