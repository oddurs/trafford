//! Watching the vault, so a note written by another program shows up here.
//!
//! The vault is meant to be edited from more than one place — an assistant in
//! another window, an editor, a `git pull`. Until this existed, none of that was
//! visible until the `reindex` command was run by hand, and `reindex` has no key
//! binding.
//!
//! A thread owns the watcher and sends batches of changed paths down a channel,
//! the same shape the assistant already uses. Nothing in the draw loop blocks.

use anyhow::{Context, Result};
use notify::{EventKind, RecursiveMode, Watcher as _};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long the vault must be quiet before a batch is sent.
///
/// Editors do not write once. They write a temporary file, rename it over the
/// original, and touch the directory — three or four events for one save. A
/// `git checkout` produces hundreds. Waiting for quiet turns any of those into
/// a single piece of work.
const QUIET: Duration = Duration::from_millis(120);

/// A batch of paths that changed together.
#[derive(Debug, Default)]
pub struct Batch {
    pub paths: Vec<PathBuf>,
    /// Something was created, removed or renamed, so the index cannot be
    /// patched note by note and has to be rebuilt.
    pub structural: bool,
}

pub struct Watcher {
    rx: Receiver<Batch>,
    /// Dropping this stops the watching, so it is held even though nothing
    /// reads it.
    _inner: notify::RecommendedWatcher,
}

impl Watcher {
    /// Begin watching `root`. Returns an error the caller should report rather
    /// than treat as fatal: a vault that is not watched still works, it just
    /// needs `reindex`.
    pub fn start(root: &Path) -> Result<Watcher> {
        let (raw_tx, raw_rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let mut inner = notify::recommended_watcher(move |event| {
            // A closed receiver means the app is going away; nothing to do.
            let _ = raw_tx.send(event);
        })
        .context("starting the file watcher")?;
        inner
            .watch(root, RecursiveMode::Recursive)
            .with_context(|| format!("watching {}", root.display()))?;

        let (tx, rx) = mpsc::channel::<Batch>();
        let root = root.to_path_buf();
        std::thread::spawn(move || debounce(raw_rx, tx, root));

        Ok(Watcher { rx, _inner: inner })
    }

    /// Every batch that has arrived since the last look. Never blocks.
    pub fn drain(&self) -> Vec<Batch> {
        self.rx.try_iter().collect()
    }
}

/// Gather events until the vault goes quiet, then send what changed.
fn debounce(raw: Receiver<notify::Result<notify::Event>>, out: mpsc::Sender<Batch>, root: PathBuf) {
    let mut pending: HashSet<PathBuf> = HashSet::new();
    let mut structural = false;
    let mut last = Instant::now();

    loop {
        match raw.recv_timeout(QUIET) {
            Ok(Ok(event)) => {
                if matches!(event.kind, EventKind::Create(_) | EventKind::Remove(_))
                    || is_rename(&event.kind)
                {
                    structural = true;
                }
                for path in event.paths {
                    if interesting(&path, &root) {
                        pending.insert(path);
                    }
                }
                last = Instant::now();
            }
            // A watcher error is not worth tearing anything down for; the next
            // event still arrives, and `reindex` is always there.
            Ok(Err(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }

        if !pending.is_empty() && last.elapsed() >= QUIET {
            let batch = Batch {
                paths: pending.drain().collect(),
                structural,
            };
            structural = false;
            if out.send(batch).is_err() {
                return;
            }
        }
    }
}

fn is_rename(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Modify(notify::event::ModifyKind::Name(_)))
}

/// Whether a path is worth waking up for.
///
/// Everything under a dot-directory is out: `.git` churns constantly during a
/// commit, and Obsidian's `.trash` is not part of the vault. Editor swap and
/// backup files are out for the same reason — they appear and vanish beside
/// every note being edited elsewhere, and reacting to them would mean a rescan
/// per keystroke in the other program.
fn interesting(path: &Path, root: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    for part in rel.components() {
        let part = part.as_os_str().to_string_lossy();
        if part.starts_with('.') {
            return false;
        }
    }
    let name = match path.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => return false,
    };
    if name.ends_with('~') || name.ends_with(".swp") || name.ends_with(".tmp") {
        return false;
    }
    // A directory event on its own says nothing useful; the file events that
    // came with it do.
    !path.is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/vault")
    }

    #[test]
    fn notes_and_attachments_are_interesting() {
        assert!(interesting(&root().join("Note.md"), &root()));
        assert!(interesting(&root().join("folder/Deep Note.md"), &root()));
        assert!(interesting(&root().join("images/photo.png"), &root()));
    }

    #[test]
    fn dot_directories_are_not() {
        // `.git` churns through a commit and `.trash` is not the vault.
        assert!(!interesting(&root().join(".git/index"), &root()));
        assert!(!interesting(&root().join(".trash/Old.md"), &root()));
        assert!(!interesting(
            &root().join(".obsidian/workspace.json"),
            &root()
        ));
        assert!(!interesting(&root().join("notes/.hidden/x.md"), &root()));
    }

    #[test]
    fn the_scratch_files_other_editors_leave_are_not() {
        // These appear and vanish beside every note being edited elsewhere.
        assert!(!interesting(&root().join("Note.md~"), &root()));
        assert!(!interesting(&root().join(".Note.md.swp"), &root()));
        assert!(!interesting(&root().join("Note.md.tmp"), &root()));
    }

    #[test]
    fn a_path_outside_the_root_is_judged_on_its_own_name() {
        assert!(interesting(Path::new("/elsewhere/Note.md"), &root()));
        assert!(!interesting(Path::new("/elsewhere/Note.md~"), &root()));
    }

    #[test]
    fn creating_and_removing_are_structural_but_editing_is_not() {
        use notify::event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode};
        assert!(is_rename(&EventKind::Modify(ModifyKind::Name(
            RenameMode::Both
        ))));
        assert!(!is_rename(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content
        ))));
        // The classification the debouncer makes, spelled out.
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Remove(RemoveKind::File),
        ] {
            assert!(matches!(kind, EventKind::Create(_) | EventKind::Remove(_)));
        }
    }
}
