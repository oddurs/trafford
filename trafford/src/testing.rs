//! Test-only helpers shared across modules.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes directories created within the same process. A timestamp is
/// not enough: the test harness runs tests on parallel threads, two of them
/// can read the same clock tick, and the pair then shares one directory until
/// the first to finish deletes the other's files.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A throwaway directory that deletes itself. Avoids a dev-dependency for
/// something this small.
pub struct TempDir(PathBuf);

impl Default for TempDir {
    fn default() -> TempDir {
        TempDir::new()
    }
}

impl TempDir {
    pub fn new() -> TempDir {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "trafford-test-{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        TempDir(base)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Write `files` into the directory, creating parents as needed.
    pub fn with_files(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new();
        for (name, body) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        dir
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn directories_are_unique_even_within_one_clock_tick() {
        // The failure this guards against needs two threads to collide, so make
        // a batch as fast as possible and check none of them share a path.
        let dirs: Vec<TempDir> = (0..256).map(|_| TempDir::new()).collect();
        let paths: HashSet<&Path> = dirs.iter().map(|d| d.path()).collect();
        assert_eq!(paths.len(), dirs.len(), "two temp dirs shared a path");
    }

    #[test]
    fn the_directory_is_removed_on_drop() {
        let path = {
            let dir = TempDir::with_files(&[("a/b.md", "x")]);
            assert!(dir.path().join("a/b.md").exists());
            dir.path().to_path_buf()
        };
        assert!(!path.exists());
    }
}
