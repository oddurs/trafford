//! Test-only helpers shared across modules.

use std::path::{Path, PathBuf};

/// A throwaway directory that deletes itself. Avoids a dev-dependency for
/// something this small.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "trafford-test-{}-{}",
            std::process::id(),
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
