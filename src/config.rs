use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// User configuration, read from `<vault>/.trafford/config.toml` if present,
/// otherwise from `~/.config/trafford/config.toml`, otherwise defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// A built-in theme, a theme file, or a Ghostty theme by name or path.
    pub theme: String,
    /// Directory (relative to vault root) new notes are created in.
    pub new_note_dir: String,
    /// Date format used by the daily-note command.
    pub daily_note_format: String,
    /// Directory (relative to vault root) daily notes live in.
    pub daily_note_dir: String,
    /// Template a new daily note starts from, relative to the vault.
    pub daily_note_template: String,
    /// The same three, for the week.
    pub weekly_note_format: String,
    pub weekly_note_dir: String,
    pub weekly_note_template: String,
    /// Anthropic model used by the assistant pane.
    pub model: String,
    /// How many retrieved notes to feed the assistant as context.
    pub context_notes: usize,
    /// Auto-commit the vault after this many seconds of inactivity. 0 disables.
    pub autocommit_secs: u64,
    /// Soft-wrap long lines in the editor rather than scrolling sideways.
    pub wrap: bool,
    /// Where to wrap. 0 means the pane width; a number holds prose to a
    /// readable measure on a wide terminal. Ignored when `wrap` is false.
    pub wrap_column: u16,
    /// Where a deleted note goes: `local` moves it to `.trash/` inside the
    /// vault, `none` unlinks it. Obsidian writes the same word in `app.json`.
    pub trash: String,
    /// Columns prose is held to while reading. 0 means the pane width, which is
    /// what Obsidian's `readableLineLength: false` asks for.
    pub reading_measure: u16,
    /// While previewing, hide the side panes and the line-number gutter and
    /// hold prose to a measure. Set false to keep the editor's chrome.
    pub reading_focus: bool,
    /// Template used for a new note, relative to the vault. Empty means a
    /// heading and nothing else, which is what trafford did before templates.
    pub new_note_template: String,
    /// Show the sidebar on startup.
    pub sidebar: bool,
    /// Sidebar width in columns. A deep vault wants more room for the tree.
    pub sidebar_width: u16,
    /// Show the right-hand context pane on startup.
    pub context_pane: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: crate::ui::theme::DEFAULT.into(),
            new_note_dir: String::new(),
            daily_note_format: "%Y-%m-%d".into(),
            daily_note_dir: "journal".into(),
            daily_note_template: String::new(),
            weekly_note_format: "%Y-W%V".into(),
            weekly_note_dir: "journal".into(),
            weekly_note_template: String::new(),
            model: "claude-sonnet-5".into(),
            context_notes: 6,
            autocommit_secs: 0,
            // On by default. A hard-wrapped vault has short lines already and
            // sees no difference; a soft-wrapped one is unreadable without it,
            // and prose is what this is for.
            wrap: true,
            wrap_column: 0,
            // Deleting is not undoing. The vault this was built for is set
            // to Obsidian's "local" and already has a `.trash/` in it.
            trash: "local".into(),
            reading_measure: 72,
            reading_focus: true,
            new_note_template: String::new(),
            sidebar: true,
            sidebar_width: 32,
            context_pane: true,
        }
    }
}

/// The parts of Obsidian's `app.json` trafford has an answer for.
///
/// Deliberately not all of it. A setting is read here only when trafford has
/// something to do with it — reading a key it then ignores would suggest a
/// promise it is not keeping.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Obsidian {
    /// `"folder"` means new files go to `new_file_folder_path`; anything else
    /// means beside the current note or at the root, neither of which trafford
    /// can honour without more machinery than this is worth.
    new_file_location: String,
    new_file_folder_path: String,
    /// `"local"` is the vault's own `.trash/`, `"none"` unlinks. `"system"` —
    /// the desktop trash — is treated as `local`, because a note recoverable
    /// inside the vault is closer to what was asked for than one that is gone.
    trash_option: String,
    /// Obsidian's "readable line length". False means prose fills the pane.
    readable_line_length: Option<bool>,
}

impl Obsidian {
    fn read(vault_root: &Path) -> Option<Obsidian> {
        let text = std::fs::read_to_string(vault_root.join(".obsidian/app.json")).ok()?;
        // A half-written settings file is not worth failing to open a vault
        // over; the defaults are all still there.
        serde_json::from_str(&text).ok()
    }

    /// Apply what the vault says, for anything `config.toml` left unsaid.
    fn fill_in(&self, cfg: &mut Config, spoken_for: &HashSet<String>) {
        if !spoken_for.contains("new_note_dir")
            && self.new_file_location == "folder"
            && !self.new_file_folder_path.is_empty()
        {
            cfg.new_note_dir = self.new_file_folder_path.clone();
        }
        if !spoken_for.contains("trash") {
            match self.trash_option.as_str() {
                "none" => cfg.trash = "none".into(),
                "local" | "system" => cfg.trash = "local".into(),
                _ => {}
            }
        }
        if !spoken_for.contains("reading_measure") {
            if let Some(false) = self.readable_line_length {
                cfg.reading_measure = 0;
            }
        }
    }
}

impl Config {
    pub fn load(vault_root: &Path) -> Self {
        let local = vault_root.join(".trafford/config.toml");
        let (mut cfg, spoken_for) = Self::read(&local)
            .or_else(|| {
                let dir = dirs::config_dir()?;
                Self::read(&dir.join("trafford/config.toml"))
            })
            .unwrap_or_else(|| (Self::default(), HashSet::new()));

        // Most of this has already been said, in Obsidian's own words, in a
        // file that travels with the vault. Fill in whatever trafford was not
        // asked about explicitly — the reader's own file always wins, so this
        // is a default rather than an override. The same move trafford makes
        // for Ghostty themes: read the configuration they already keep.
        if let Some(obsidian) = Obsidian::read(vault_root) {
            obsidian.fill_in(&mut cfg, &spoken_for);
        }
        cfg
    }

    /// The config, and the names of the keys the file actually mentioned.
    ///
    /// `#[serde(default)]` cannot tell "set to the empty string" from "never
    /// written down", and that difference is the whole precedence rule.
    fn read(path: &Path) -> Option<(Self, HashSet<String>)> {
        let text = std::fs::read_to_string(path).ok()?;
        let cfg: Self = toml::from_str(&text).ok()?;
        let raw: toml::Table = toml::from_str(&text).ok()?;
        Some((cfg, raw.keys().cloned().collect()))
    }

    /// Write a starter config into the vault so it is versioned alongside the notes.
    pub fn write_default(vault_root: &Path) -> Result<PathBuf> {
        let dir = vault_root.join(".trafford");
        std::fs::create_dir_all(&dir).context("creating .trafford directory")?;
        let path = dir.join("config.toml");
        if !path.exists() {
            let body = toml::to_string_pretty(&Config::default())?;
            std::fs::write(&path, body).context("writing config.toml")?;
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault with the two config files written as given.
    fn vault(obsidian: Option<&str>, trafford: Option<&str>) -> crate::testing::TempDir {
        let dir = crate::testing::TempDir::with_files(&[("Note.md", "# Note\n")]);
        if let Some(json) = obsidian {
            std::fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
            std::fs::write(dir.path().join(".obsidian/app.json"), json).unwrap();
        }
        if let Some(toml) = trafford {
            std::fs::create_dir_all(dir.path().join(".trafford")).unwrap();
            std::fs::write(dir.path().join(".trafford/config.toml"), toml).unwrap();
        }
        dir
    }

    const THEIRS: &str = r#"{
      "newFileLocation": "folder",
      "newFileFolderPath": "00-inbox",
      "trashOption": "local",
      "readableLineLength": false
    }"#;

    #[test]
    fn the_vault_answers_what_trafford_was_not_asked() {
        let dir = vault(Some(THEIRS), None);
        let cfg = Config::load(dir.path());
        assert_eq!(cfg.new_note_dir, "00-inbox");
        assert_eq!(cfg.trash, "local");
        assert_eq!(cfg.reading_measure, 0, "readableLineLength: false");
    }

    #[test]
    fn anything_written_in_config_toml_wins() {
        // The precedence rule, and the reason key presence is tracked at all:
        // `new_note_dir = ""` is a decision, not an absence.
        let dir = vault(
            Some(THEIRS),
            Some("new_note_dir = \"\"\ntrash = \"none\"\nreading_measure = 90\n"),
        );
        let cfg = Config::load(dir.path());
        assert_eq!(
            cfg.new_note_dir, "",
            "an explicit empty string is an answer"
        );
        assert_eq!(cfg.trash, "none");
        assert_eq!(cfg.reading_measure, 90);
    }

    #[test]
    fn a_partly_written_config_takes_the_vault_for_the_rest() {
        let dir = vault(Some(THEIRS), Some("theme = \"gotham\"\ntrash = \"none\"\n"));
        let cfg = Config::load(dir.path());
        assert_eq!(cfg.theme, "gotham");
        assert_eq!(cfg.trash, "none", "asked for");
        assert_eq!(cfg.new_note_dir, "00-inbox", "not asked for");
    }

    #[test]
    fn a_vault_with_no_obsidian_config_is_unchanged() {
        let dir = vault(None, None);
        let cfg = Config::load(dir.path());
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn a_half_written_settings_file_is_ignored_rather_than_fatal() {
        // Obsidian was mid-write, or something truncated it. Opening the vault
        // matters more than the settings do.
        let dir = vault(Some("{ \"newFileLocation\": \"fol"), None);
        let cfg = Config::load(dir.path());
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn keys_trafford_has_no_answer_for_are_left_alone() {
        // Obsidian carries far more than this. Reading a setting and then
        // ignoring it would suggest a promise trafford is not keeping.
        let dir = vault(
            Some(r#"{"vimMode": true, "spellcheck": false, "showLineNumber": false}"#),
            None,
        );
        let cfg = Config::load(dir.path());
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn new_files_go_beside_the_current_note_only_when_a_folder_is_named() {
        // `newFileLocation` of "current" or "root" is not something trafford
        // can honour, so it is not pretended at.
        let dir = vault(
            Some(r#"{"newFileLocation": "current", "newFileFolderPath": "00-inbox"}"#),
            None,
        );
        assert_eq!(Config::load(dir.path()).new_note_dir, "");
    }

    #[test]
    fn the_system_trash_is_treated_as_the_vaults_own() {
        // A note recoverable inside the vault is closer to what was asked for
        // than one that is gone.
        let dir = vault(Some(r#"{"trashOption": "system"}"#), None);
        assert_eq!(Config::load(dir.path()).trash, "local");
    }
}
