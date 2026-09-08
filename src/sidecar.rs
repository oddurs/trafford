//! `.trafford/` — state the vault did not author.
//!
//! Everything v0.7 produces beyond the notes themselves needs somewhere to
//! live: which link suggestions were declined, what a similarity pass scored,
//! anything cached. Both obvious homes are wrong.
//!
//! It cannot go in the notes. The moment derived data is written into a `.md`
//! file the note stops being what its author wrote, another program's edit
//! silently invalidates it, and the portability that lets Obsidian, an agent
//! and trafford share these files is gone.
//!
//! It cannot go in `.obsidian/`. That belongs to Obsidian, and two programs
//! writing one settings file is how settings get lost.
//!
//! So: a sibling directory holding state that is **derived, disposable and
//! never authoritative**. The markdown is the only source of truth; this is a
//! cache with opinions. Deleting it while trafford is running loses nothing but
//! time, which is the property every other item in this milestone leans on.

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};

/// Where derived state lives inside the vault.
///
/// `.trafford/` itself is **not** this directory. It already holds
/// `config.toml`, which is the reader's own configuration and is meant to be
/// versioned with the notes — `cargo run -- init` even scaffolds a `.gitignore`
/// naming `.trafford/cache/`, so the convention was already there. Putting
/// derived state at the top of `.trafford/` and ignoring it would have
/// untracked the reader's config, which is the opposite of what they asked for.
pub const DIR: &str = ".trafford/cache";

/// What this trafford writes. A sidecar stamped with anything else is
/// **discarded rather than migrated**: it is derived, so rebuilding is always
/// cheaper than the risk of misreading it, and a migration path is a promise to
/// keep a format working that nothing depends on.
pub const FORMAT: u32 = 1;

/// The sidecar for one vault.
#[derive(Debug, Clone)]
pub struct Sidecar {
    root: PathBuf,
}

/// Every stored file carries the format it was written with, so a stale one can
/// be recognised without being parsed.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Envelope<T> {
    format: u32,
    data: T,
}

impl Sidecar {
    /// The sidecar beside a vault. Creating this does not touch the disk —
    /// nothing is written until something is stored, so a reader who never uses
    /// a feature that needs one never gets the directory.
    pub fn beside(vault_root: &Path) -> Sidecar {
        Sidecar {
            root: vault_root.join(DIR),
        }
    }

    /// Read a stored value, or `None` if it is absent, unreadable, or was
    /// written by a different format.
    ///
    /// Every failure is the same answer on purpose. A corrupt cache and an
    /// absent one call for identical handling — rebuild — and distinguishing
    /// them would only invite a caller to treat one as an error worth showing.
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        let text = std::fs::read_to_string(self.file(name)).ok()?;
        let envelope: Envelope<T> = serde_json::from_str(&text).ok()?;
        (envelope.format == FORMAT).then_some(envelope.data)
    }

    /// Store a value, creating the directory and its `.gitignore` if needed.
    pub fn store<T: Serialize>(&self, name: &str, data: &T) -> Result<()> {
        self.ensure()?;
        let path = self.file(name);
        let text = serde_json::to_string(&Envelope {
            format: FORMAT,
            data,
        })
        .context("serialising sidecar data")?;
        // Written beside and renamed, so a reader never sees half a file — an
        // interrupted write would otherwise leave something that parses as
        // nothing and looks like corruption.
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, text).with_context(|| format!("writing {}", temp.display()))?;
        std::fs::rename(&temp, &path).with_context(|| format!("replacing {}", path.display()))
    }

    fn file(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.json"))
    }

    /// Create the directory, and tell git to ignore it.
    ///
    /// The `.gitignore` goes *inside the cache* rather than editing the vault's
    /// own: that file belongs to the reader, and a program that appends to it
    /// is a program that will one day append twice. A directory that ignores
    /// itself needs no cooperation from anybody — and scoping it to the cache
    /// leaves `.trafford/config.toml` tracked, which is where the reader put it
    /// on purpose.
    ///
    /// Checked separately from the directory: `.trafford/` may already exist
    /// for the config, and an early return on the directory would skip the
    /// ignore file for every vault that has one.
    fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        let ignore = self.root.join(".gitignore");
        if !ignore.exists() {
            std::fs::write(
                &ignore,
                "# Written by trafford. Everything here is derived from the notes\n\
                 # and can be deleted at any time.\n*\n",
            )
            .with_context(|| format!("writing {}", ignore.display()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Thing {
        declined: Vec<String>,
    }

    fn thing() -> Thing {
        Thing {
            declined: vec!["a.md".into()],
        }
    }

    /// The reader's own config lives at `.trafford/config.toml` and is meant to
    /// be versioned. Derived state goes a level down, so ignoring the cache
    /// never touches it.
    #[test]
    fn the_readers_config_is_not_swept_up_by_the_cache() {
        let dir = TempDir::with_files(&[
            ("a.md", "# A\n"),
            (".trafford/config.toml", "theme = \"gotham\"\n"),
        ]);
        let side = Sidecar::beside(dir.path());
        side.store("queries", &thing()).unwrap();
        assert!(
            !dir.path().join(".trafford/.gitignore").exists(),
            "nothing may ignore the config directory"
        );
        assert!(dir.path().join(".trafford/cache/.gitignore").exists());
        assert!(dir.path().join(".trafford/config.toml").exists());
    }

    #[test]
    fn a_value_survives_a_round_trip() {
        let dir = TempDir::with_files(&[("a.md", "# A\n")]);
        let side = Sidecar::beside(dir.path());
        side.store("suggestions", &thing()).unwrap();
        assert_eq!(side.load::<Thing>("suggestions"), Some(thing()));
    }

    /// Nothing is written until something is stored, so a reader who never uses
    /// a feature that needs one never finds the directory in their vault.
    #[test]
    fn the_directory_is_not_created_just_by_asking_for_it() {
        let dir = TempDir::with_files(&[("a.md", "# A\n")]);
        let side = Sidecar::beside(dir.path());
        assert!(!dir.path().join(DIR).exists());
        assert_eq!(side.load::<Thing>("suggestions"), None);
        assert!(
            !dir.path().join(DIR).exists(),
            "reading must not create it either"
        );
    }

    /// The property everything else leans on: deleting it loses nothing.
    #[test]
    fn deleting_the_whole_directory_is_survivable() {
        let dir = TempDir::with_files(&[("a.md", "# A\n")]);
        let side = Sidecar::beside(dir.path());
        side.store("suggestions", &thing()).unwrap();
        std::fs::remove_dir_all(dir.path().join(DIR)).unwrap();
        assert_eq!(side.load::<Thing>("suggestions"), None);
        // And it comes back without ceremony.
        side.store("suggestions", &thing()).unwrap();
        assert_eq!(side.load::<Thing>("suggestions"), Some(thing()));
    }

    /// A sidecar from another format is discarded, not migrated. Rebuilding
    /// derived data is always cheaper than the risk of misreading it.
    #[test]
    fn a_sidecar_from_another_format_is_discarded_silently() {
        let dir = TempDir::with_files(&[("a.md", "# A\n")]);
        let side = Sidecar::beside(dir.path());
        side.store("suggestions", &thing()).unwrap();
        let path = dir.path().join(DIR).join("suggestions.json");
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("\"format\":1", "\"format\":99")).unwrap();
        assert_eq!(side.load::<Thing>("suggestions"), None);
    }

    /// Corrupt and absent are the same answer, because they call for the same
    /// handling and telling them apart only invites showing one as an error.
    #[test]
    fn a_corrupt_file_reads_as_absent() {
        let dir = TempDir::with_files(&[("a.md", "# A\n")]);
        let side = Sidecar::beside(dir.path());
        side.store("suggestions", &thing()).unwrap();
        std::fs::write(dir.path().join(DIR).join("suggestions.json"), "{ not json").unwrap();
        assert_eq!(side.load::<Thing>("suggestions"), None);
    }

    /// The directory ignores itself, rather than trafford editing the vault's
    /// `.gitignore` — that file belongs to the reader.
    #[test]
    fn the_directory_ignores_itself_and_leaves_the_vaults_gitignore_alone() {
        let dir = TempDir::with_files(&[("a.md", "# A\n"), (".gitignore", "notes-backup/\n")]);
        let side = Sidecar::beside(dir.path());
        side.store("suggestions", &thing()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "notes-backup/\n",
            "the vault's own file is untouched"
        );
        let own = std::fs::read_to_string(dir.path().join(DIR).join(".gitignore")).unwrap();
        assert!(own.contains('*'), "{own}");
    }
}
