use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// User configuration, read from `<vault>/.trafford/config.toml` if present,
/// otherwise from `~/.config/trafford/config.toml`, otherwise defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// While previewing, hide the side panes and the line-number gutter and
    /// hold prose to a measure. Set false to keep the editor's chrome.
    pub reading_focus: bool,
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
            model: "claude-sonnet-5".into(),
            context_notes: 6,
            autocommit_secs: 0,
            // On by default. A hard-wrapped vault has short lines already and
            // sees no difference; a soft-wrapped one is unreadable without it,
            // and prose is what this is for.
            wrap: true,
            wrap_column: 0,
            reading_focus: true,
            sidebar: true,
            sidebar_width: 32,
            context_pane: true,
        }
    }
}

impl Config {
    pub fn load(vault_root: &Path) -> Self {
        let local = vault_root.join(".trafford/config.toml");
        if let Some(cfg) = Self::read(&local) {
            return cfg;
        }
        if let Some(dir) = dirs::config_dir() {
            if let Some(cfg) = Self::read(&dir.join("trafford/config.toml")) {
                return cfg;
            }
        }
        Self::default()
    }

    fn read(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str(&text).ok()
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
