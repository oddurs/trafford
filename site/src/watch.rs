//! Noticing that a file changed, by asking.
//!
//! `notify` is the obvious answer and was the plan. Polling won on two counts:
//! a docs tree is dozens of files, so a walk every 200 ms is free; and the
//! site crate then adds *no* dependency the workspace did not already have,
//! which turns "site tooling never ships in the binary" from an argument into
//! something `cargo tree` shows.
//!
//! It also sidesteps the part of filesystem events that actually costs time —
//! an editor writing a file produces several of them, some write a temporary
//! file and rename it over the original, and the platforms disagree about
//! which. A snapshot comparison has one event per settled state by
//! construction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A directory's files, by path, with their size and modification time.
///
/// Size as well as time: two writes inside one filesystem timestamp tick are
/// rare and are exactly what a script that rewrites a file produces.
pub type Snapshot = BTreeMap<PathBuf, (u64, Option<SystemTime>)>;

/// Walk `roots`, skipping hidden entries and anything under `skip`.
pub fn snapshot(roots: &[PathBuf], skip: &[PathBuf]) -> Snapshot {
    let mut out = Snapshot::new();
    let mut stack: Vec<PathBuf> = roots.to_vec();
    while let Some(path) = stack.pop() {
        if skip.iter().any(|s| path.starts_with(s)) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                // Editors leave `.file.swp` and `4913` behind; a build writes
                // into a dot-prefixed staging directory. None of it is source.
                if name.starts_with('.') {
                    continue;
                }
                stack.push(entry.path());
            }
        } else if meta.is_file() {
            out.insert(path, (meta.len(), meta.modified().ok()));
        }
    }
    out
}

/// Watch `roots` and call `on_change` once per settled change.
///
/// Blocks. `should_stop` is checked every tick so the caller can end it.
pub fn watch(
    roots: &[PathBuf],
    skip: &[PathBuf],
    interval: Duration,
    mut should_stop: impl FnMut() -> bool,
    mut on_change: impl FnMut(),
) {
    let mut last = snapshot(roots, skip);
    // A burst of writes — save-all in an editor, a script rewriting three
    // files — should rebuild once. Wait for the tree to stop moving.
    let mut settling = 0u8;
    loop {
        if should_stop() {
            return;
        }
        std::thread::sleep(interval);
        let now = snapshot(roots, skip);
        if now != last {
            last = now;
            settling = 2;
            continue;
        }
        if settling > 0 {
            settling -= 1;
            if settling == 0 {
                on_change();
            }
        }
    }
}

/// Whether two snapshots differ, for a caller that wants to compare its own.
pub fn changed(a: &Snapshot, b: &Snapshot) -> bool {
    a != b
}

/// The paths a build reads, given a docs directory and the crate's own assets.
pub fn sources(docs: &Path, crate_dir: &Path) -> Vec<PathBuf> {
    vec![docs.to_path_buf(), crate_dir.join("assets")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use trafford::testing::TempDir;

    #[test]
    fn a_snapshot_notices_a_write() {
        let dir = TempDir::with_files(&[("a.md", "one")]);
        let roots = vec![dir.path().to_path_buf()];
        let before = snapshot(&roots, &[]);
        std::fs::write(dir.path().join("a.md"), "one and a half").unwrap();
        assert!(changed(&before, &snapshot(&roots, &[])));
    }

    /// The bug this avoids: watching the directory a build writes into means
    /// every build triggers the next one, forever.
    #[test]
    fn the_output_directory_can_be_skipped() {
        let dir = TempDir::with_files(&[("a.md", "one"), ("out/index.html", "x")]);
        let roots = vec![dir.path().to_path_buf()];
        let skip = vec![dir.path().join("out")];
        let before = snapshot(&roots, &skip);
        std::fs::write(dir.path().join("out/index.html"), "rebuilt").unwrap();
        assert!(!changed(&before, &snapshot(&roots, &skip)));
    }

    #[test]
    fn editor_droppings_are_not_source() {
        let dir = TempDir::with_files(&[("a.md", "one"), (".a.md.swp", "junk")]);
        let snap = snapshot(&[dir.path().to_path_buf()], &[]);
        assert_eq!(snap.len(), 1);
    }

    #[test]
    fn a_new_file_is_a_change_and_so_is_a_deleted_one() {
        let dir = TempDir::with_files(&[("a.md", "one")]);
        let roots = vec![dir.path().to_path_buf()];
        let before = snapshot(&roots, &[]);
        std::fs::write(dir.path().join("b.md"), "two").unwrap();
        let after_add = snapshot(&roots, &[]);
        assert!(changed(&before, &after_add));
        std::fs::remove_file(dir.path().join("b.md")).unwrap();
        assert!(changed(&after_add, &snapshot(&roots, &[])));
    }

    /// Three writes in quick succession are one rebuild, not three.
    #[test]
    fn a_burst_of_writes_settles_into_one_change() {
        let dir = TempDir::with_files(&[("a.md", "one")]);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let (c, s, path) = (calls.clone(), stop.clone(), dir.path().to_path_buf());
        let handle = std::thread::spawn(move || {
            watch(
                &[path],
                &[],
                Duration::from_millis(20),
                || s.load(std::sync::atomic::Ordering::SeqCst),
                || {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                },
            );
        });

        for body in ["two", "three", "four"] {
            std::fs::write(dir.path().join("a.md"), body).unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(400));
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        handle.join().unwrap();

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
