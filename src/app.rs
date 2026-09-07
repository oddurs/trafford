use crate::config::Config;
use crate::editor::{Buffer, Editor};
use crate::git::{self, Repo};
use crate::llm;
use crate::tree;
use crate::ui::theme::Theme;
use crate::vault::Vault;
use anyhow::Result;
use ratatui::layout::{Position, Rect};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Fuzzy matching
// ---------------------------------------------------------------------------

/// Subsequence fuzzy match. Returns a score and the matched character indices,
/// or `None` when `query` is not a subsequence of `text`. Consecutive matches,
/// word-boundary matches, and early matches all score higher.
pub fn fuzzy_match(query: &str, text: &str) -> Option<(i64, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = query.chars().collect();
    let mut indices = Vec::with_capacity(needle.len());
    let mut score = 0i64;
    let mut hi = 0usize;
    let mut last_match: Option<usize> = None;

    for &nc in &needle {
        let nc_lower = nc.to_ascii_lowercase();
        let mut found = None;
        while hi < hay.len() {
            if hay[hi].to_ascii_lowercase() == nc_lower {
                found = Some(hi);
                break;
            }
            hi += 1;
        }
        let idx = found?;
        score += 10;
        if idx == 0 {
            score += 15;
        } else if !hay[idx - 1].is_alphanumeric() {
            score += 12; // start of a word or path segment
        }
        if hay[idx] == nc {
            score += 2; // exact case
        }
        if last_match == Some(idx.saturating_sub(1)) {
            // Contiguity is the strongest signal a fuzzy finder has: an
            // uninterrupted run must outrank the same characters scattered
            // across word boundaries.
            score += 20;
        }
        indices.push(idx);
        last_match = Some(idx);
        hi = idx + 1;
    }
    // Prefer earlier matches, but charge for the position once rather than per
    // matched character. Per-character it compounded, so a long path whose
    // filename matched the query exactly lost to a shorter path that merely
    // contained the letters scattered about.
    score -= (indices[0] as i64) / 4;
    // Shorter haystacks with the same match are tighter matches.
    score -= (hay.len() as i64) / 20;
    Some((score, indices))
}

// ---------------------------------------------------------------------------
// Pickers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PickItem {
    pub label: String,
    pub detail: String,
    /// Opaque payload: a note id for the switcher, a command key for the palette.
    pub key: String,
}

#[derive(Debug, Clone, Default)]
pub struct Picker {
    pub title: String,
    pub query: String,
    pub items: Vec<PickItem>,
    /// Indices into `items`, plus the matched character positions in the label.
    pub matches: Vec<(usize, Vec<usize>)>,
    pub cursor: usize,
}

impl Picker {
    pub fn new(title: &str, items: Vec<PickItem>) -> Picker {
        let mut p = Picker {
            title: title.to_string(),
            items,
            ..Default::default()
        };
        p.refilter();
        p
    }

    pub fn refilter(&mut self) {
        let mut scored: Vec<(i64, usize, Vec<usize>)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let against = if item.detail.is_empty() {
                    item.label.clone()
                } else {
                    format!("{} {}", item.label, item.detail)
                };
                fuzzy_match(&self.query, &against).map(|(s, idx)| {
                    // Only keep highlight indices that fall inside the label.
                    let label_len = item.label.chars().count();
                    let idx = idx.into_iter().filter(|i| *i < label_len).collect();
                    (s, i, idx)
                })
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.matches = scored.into_iter().map(|(_, i, idx)| (i, idx)).collect();
        self.cursor = self.cursor.min(self.matches.len().saturating_sub(1));
    }

    pub fn selected(&self) -> Option<&PickItem> {
        self.matches.get(self.cursor).map(|(i, _)| &self.items[*i])
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as isize;
        self.cursor = (((self.cursor as isize + delta) % len + len) % len) as usize;
    }

    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.cursor = 0;
        self.refilter();
    }

    pub fn pop(&mut self) {
        self.query.pop();
        self.cursor = 0;
        self.refilter();
    }
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    NewNote,
    /// Creating a note because a `[[link]]` pointed at nothing.
    NewNoteFromLink,
    Rename,
    Commit,
    SaveAnswerAs,
}

#[derive(Debug, Clone)]
pub struct Prompt {
    pub kind: PromptKind,
    pub title: String,
    pub input: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteNote(String),
    DiscardChanges(String),
    QuitDirty,
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub kind: ConfirmKind,
    pub message: String,
}

/// What a context-menu entry does. Most defer to an existing palette command;
/// the rest need the thing that was clicked, which a command name cannot carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    Command(&'static str),
    OpenNote(String),
    OpenNoteAt(String, usize),
    RenameNote(String),
    DeleteNote(String),
    LinkToNote(String),
    HistoryOf(String),
    NewNoteIn(String),
    ExpandUnder(String),
    CollapseDir(String),
    GoToLine(usize),
    CreateNote(String),
    /// Git actions carry the path they act on, since the pane's own cursor may
    /// not be where the pointer was.
    GitStage(String),
    GitUnstage(String),
    GitDiff(String),
    GitDiscard(String),
    FilterByTag(String),
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub action: MenuAction,
    /// Why this cannot run right now. `None` means it can.
    ///
    /// An action that exists but does not apply *here* is shown greyed with
    /// the reason, rather than left out: a menu whose shape changes under you
    /// cannot teach what is possible, and an absent entry looks the same as an
    /// action that was never built. Actions that make no sense for the thing
    /// clicked stay absent — a folder never lists "Delete note", greyed or
    /// otherwise.
    pub disabled: Option<String>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, action: MenuAction) -> MenuItem {
        MenuItem {
            label: label.into(),
            action,
            disabled: None,
        }
    }

    /// The same entry, greyed out, with the reason shown beside it.
    pub fn unless(mut self, blocked: bool, reason: &str) -> MenuItem {
        if blocked {
            self.disabled = Some(reason.to_string());
        }
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.disabled.is_none()
    }
}

impl MenuItem {
    /// The key that does the same thing, for the hint beside the entry.
    ///
    /// Read from the command table the help screen uses, so the two cannot
    /// disagree — a menu that teaches the wrong key is worse than one that
    /// teaches none.
    pub fn shortcut(&self) -> Option<&'static str> {
        use MenuAction as A;
        let key = match &self.action {
            A::Command(name) => {
                return crate::keymap::COMMANDS
                    .iter()
                    .find(|(k, _, _)| k == name)
                    .map(|(_, _, shortcut)| *shortcut)
                    .filter(|s| !s.is_empty())
            }
            // These have no command behind them; the key is the one that does
            // the same thing to the row the menu was opened on.
            A::OpenNote(_) | A::OpenNoteAt(..) | A::GoToLine(_) | A::CreateNote(_) => "enter",
            A::ExpandUnder(_) => "E",
            A::CollapseDir(_) => "h",
            _ => return None,
        };
        Some(key)
    }
}

/// A menu anchored to the point that was right-clicked.
#[derive(Debug, Clone)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
    pub cursor: usize,
    pub at: (u16, u16),
}

impl Menu {
    /// Move by `delta`, skipping entries that cannot run. Stays put when
    /// nothing in the menu is selectable, rather than spinning forever.
    pub fn move_cursor(&mut self, delta: isize) {
        if self.items.is_empty() || !self.items.iter().any(|i| i.is_enabled()) {
            return;
        }
        let len = self.items.len() as isize;
        let step = if delta >= 0 { 1 } else { -1 };
        let mut at = self.cursor as isize;
        for _ in 0..len {
            at = (at + step).rem_euclid(len);
            if self.items[at as usize].is_enabled() {
                break;
            }
        }
        self.cursor = at as usize;
    }

    /// Put the cursor on the first entry that can actually run.
    pub fn select_first_enabled(&mut self) {
        if let Some(i) = self.items.iter().position(|i| i.is_enabled()) {
            self.cursor = i;
        }
    }

    pub fn selected(&self) -> Option<&MenuItem> {
        self.items.get(self.cursor).filter(|i| i.is_enabled())
    }
}

#[derive(Debug, Default)]
pub struct SearchPane {
    pub query: String,
    pub hits: Vec<crate::vault::Hit>,
    pub cursor: usize,
}

#[derive(Debug, Default)]
pub struct GitPane {
    pub snapshot: git::Snapshot,
    pub cursor: usize,
    pub log: Vec<git::Commit>,
}

pub enum Overlay {
    Palette(Picker),
    Switcher(Picker),
    /// Same as the switcher, but inserts `[[Target]]` instead of navigating.
    LinkPicker(Picker),
    /// Notes linking to the open note; selecting one jumps to the exact line.
    Backlinks(Picker),
    /// Every theme trafford can find, previewed by applying it live.
    Themes(Picker),
    /// A right-click menu, drawn where the click landed.
    Menu(Menu),
    Search(SearchPane),
    Prompt(Prompt),
    Git(GitPane),
    History(Vec<git::Commit>),
    /// A unified diff, shown from the git pane.
    Diff {
        title: String,
        body: String,
        scroll: u16,
    },
    Confirm(Confirm),
    Help,
}

// ---------------------------------------------------------------------------
// Assistant
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Chat {
    pub messages: Vec<llm::Message>,
    pub input: String,
    pub streaming: bool,
    pub rx: Option<Receiver<llm::Event>>,
    pub scroll: u16,
    /// Notes fed to the model for the current turn, shown as provenance.
    pub context_ids: Vec<String>,
}

/// What draining the stream produced this tick.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// The chat changed and the UI should redraw.
    pub changed: bool,
    /// The turn failed; the message is for the status line.
    pub error: Option<String>,
}

impl Chat {
    /// The most recent assistant reply, for inserting into a note.
    pub fn last_answer(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == llm::Role::Assistant)
            .map(|m| m.text.as_str())
            .filter(|t| !t.trim().is_empty())
    }

    /// Consume whatever the worker thread has produced so far, appending
    /// deltas to the open assistant message. Never blocks.
    pub fn drain(&mut self) -> Drained {
        let Some(rx) = &self.rx else {
            return Drained::default();
        };
        let mut out = Drained::default();
        loop {
            match rx.try_recv() {
                Ok(llm::Event::Delta(text)) => {
                    if let Some(last) = self.messages.last_mut() {
                        last.text.push_str(&text);
                    }
                    out.changed = true;
                }
                Ok(llm::Event::Done) => {
                    self.finish();
                    out.changed = true;
                    break;
                }
                Ok(llm::Event::Error(err)) => {
                    // Drop the placeholder if nothing was streamed into it, so
                    // a failed turn does not leave an empty bubble behind.
                    if self.messages.last().is_some_and(|m| m.text.is_empty()) {
                        self.messages.pop();
                    }
                    self.finish();
                    out.changed = true;
                    out.error = Some(err);
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.finish();
                    out.changed = true;
                    break;
                }
            }
        }
        out
    }

    fn finish(&mut self) {
        self.streaming = false;
        self.rx = None;
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// Where each pane was drawn, so a click can be resolved to the thing under
/// it. Recorded on every draw; the mouse handler reads it and nothing else.
/// These are the *inner* areas, inside the borders.
#[derive(Debug, Clone, Copy, Default)]
pub struct Panes {
    pub sidebar: Rect,
    pub editor: Rect,
    pub context: Rect,
    pub assistant: Rect,
    pub assistant_input: Rect,
    /// The list area of whatever overlay is open.
    pub overlay: Rect,
}

fn hit(area: Rect, column: u16, row: u16) -> bool {
    area.width > 0 && area.height > 0 && area.contains(Position::new(column, row))
}

impl Panes {
    pub fn sidebar_hit(&self, c: u16, r: u16) -> bool {
        hit(self.sidebar, c, r)
    }
    pub fn editor_hit(&self, c: u16, r: u16) -> bool {
        hit(self.editor, c, r)
    }
    pub fn context_hit(&self, c: u16, r: u16) -> bool {
        hit(self.context, c, r)
    }
    pub fn assistant_hit(&self, c: u16, r: u16) -> bool {
        hit(self.assistant, c, r) || hit(self.assistant_input, c, r)
    }
    pub fn overlay_hit(&self, c: u16, r: u16) -> bool {
        hit(self.overlay, c, r)
    }
}

/// Something in the context pane a click can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextTarget {
    /// A heading in the open note, by buffer row.
    Heading(usize),
    /// Another note, opened whole.
    Note(String),
    /// Another note, opened at the line that mentions this one.
    Backlink(String, usize),
    /// An unwritten note, offered for creation.
    Unwritten(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Editor,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Notes,
    Tags,
}

pub struct App {
    pub vault: Vault,
    pub repo: Option<Repo>,
    pub config: Config,
    pub theme: Theme,
    pub editor: Editor,
    pub current: Option<String>,
    /// Back-stack of (note id, cursor row) for `Ctrl-O`.
    pub history: Vec<(String, usize)>,
    pub focus: Focus,
    pub sidebar_tab: SidebarTab,
    /// Index into the rows [`App::tree_rows`] returns, not into the note list.
    pub sidebar_cursor: usize,
    /// Directories the sidebar is showing the contents of.
    pub expanded: std::collections::HashSet<String>,
    pub sidebar_visible: bool,
    pub context_visible: bool,
    pub assistant_visible: bool,
    pub preview: bool,
    pub overlay: Option<Overlay>,
    pub chat: Chat,
    pub status: Option<(String, Instant)>,
    pub git_status: git::Snapshot,
    pub last_git_poll: Instant,
    pub last_edit: Instant,
    pub should_quit: bool,
    /// Height of the editor viewport, updated each draw so paging can use it.
    pub editor_height: usize,
    /// Where the panes were drawn, for hit-testing the mouse.
    pub panes: Panes,
    /// What each row of the context pane points at, parallel to its lines.
    pub context_targets: Vec<Option<ContextTarget>>,
    /// Tag currently filtering the note list, if any.
    pub tag_filter: Option<String>,
    /// Where the current theme came from, for the status line and the picker.
    pub theme_source: String,
    /// The theme in use before the picker started previewing, so esc restores.
    pub theme_before_preview: Option<String>,
}

impl App {
    pub fn new(vault: Vault, config: Config) -> App {
        let repo = Repo::discover(&vault.root);
        let git_status = repo
            .as_ref()
            .and_then(|r| r.snapshot().ok())
            .unwrap_or_default();
        let (theme, theme_source) = Theme::resolve(&config.theme, &vault.root);
        let mut app = App {
            vault,
            repo,
            theme,
            editor: Editor::default(),
            current: None,
            history: Vec::new(),
            focus: Focus::Editor,
            sidebar_tab: SidebarTab::Notes,
            sidebar_cursor: 0,
            expanded: std::collections::HashSet::new(),
            sidebar_visible: config.sidebar,
            context_visible: config.context_pane,
            assistant_visible: false,
            preview: false,
            overlay: None,
            chat: Chat::default(),
            status: None,
            git_status,
            last_git_poll: Instant::now(),
            last_edit: Instant::now(),
            should_quit: false,
            editor_height: 20,
            panes: Panes::default(),
            context_targets: Vec::new(),
            tag_filter: None,
            theme_source,
            theme_before_preview: None,
            config,
        };
        // A theme that could not be found has to say so; silently using a
        // different one looks like the config was ignored.
        if app.theme_source.contains("using") {
            app.set_status(format!("theme {}", app.theme_source.clone()));
        }

        // Open the most recently modified note so the app never starts blank.
        if let Some(id) = app
            .vault
            .notes
            .iter()
            .max_by_key(|n| n.modified)
            .map(|n| n.id.clone())
        {
            app.open_note(&id, false);
        }
        app.sidebar_cursor = 0;
        if let Some(id) = app.current.clone() {
            app.reveal_in_tree(&id);
        }
        app
    }

    // ---- status -------------------------------------------------------

    /// Switch themes, remembering the choice so a later save keeps it.
    pub fn apply_theme(&mut self, spec: &str) {
        let (theme, source) = Theme::resolve(spec, &self.vault.root);
        self.theme = theme;
        self.theme_source = source;
        self.config.theme = spec.to_string();
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    pub fn status_text(&self) -> Option<&str> {
        self.status
            .as_ref()
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(5))
            .map(|(s, _)| s.as_str())
    }

    fn report(&mut self, result: Result<String>) {
        match result {
            Ok(msg) => self.set_status(msg),
            Err(err) => self.set_status(format!("error: {err}")),
        }
    }

    // ---- notes --------------------------------------------------------

    /// The notes the sidebar may show, honouring any tag filter.
    pub fn listed_notes(&self) -> Vec<&crate::vault::Note> {
        match &self.tag_filter {
            Some(tag) => self.vault.notes_with_tag(tag),
            None => self.vault.notes.iter().collect(),
        }
    }

    fn tree_input(&self) -> Vec<(String, String)> {
        self.listed_notes()
            .iter()
            .map(|n| (n.id.clone(), n.title.clone()))
            .collect()
    }

    /// The sidebar's visible rows: directories and notes, in tree order.
    pub fn tree_rows(&self) -> Vec<tree::Row> {
        tree::build(&self.tree_input(), &self.expanded)
    }

    /// Open every directory containing `id`, so the note is visible, and put
    /// the cursor on it.
    pub fn reveal_in_tree(&mut self, id: &str) {
        for dir in tree::ancestors(id) {
            self.expanded.insert(dir);
        }
        if let Some(pos) = self
            .tree_rows()
            .iter()
            .position(|r| r.note_id() == Some(id))
        {
            self.sidebar_cursor = pos;
        }
    }

    pub fn expand_all(&mut self) {
        self.expanded = tree::all_dirs(&self.tree_input());
    }

    /// Collapse every directory, leaving the cursor somewhere useful.
    pub fn collapse_all(&mut self) {
        self.expanded.clear();
        self.sidebar_cursor = 0;
        let Some(id) = self.current.clone() else {
            return;
        };
        let rows = self.tree_rows();
        // The open note is no longer visible, so aim at the outermost folder
        // holding it: you can see where you were, and `l` walks back in.
        let found = match tree::ancestors(&id).into_iter().next() {
            Some(dir) => rows.iter().position(|r| r.dir_path() == Some(dir.as_str())),
            // A note at the vault root stays visible when all is collapsed.
            None => rows.iter().position(|r| r.note_id() == Some(id.as_str())),
        };
        if let Some(pos) = found {
            self.sidebar_cursor = pos;
        }
    }

    pub fn open_note(&mut self, id: &str, record_history: bool) {
        if record_history {
            if let Some(cur) = &self.current {
                self.history.push((cur.clone(), self.editor.buf.row));
                if self.history.len() > 100 {
                    self.history.remove(0);
                }
            }
        }
        let path = self.vault.path_for(id);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.editor.load(Buffer::from_str(&text));
                self.current = Some(id.to_string());
                self.focus = Focus::Editor;
                // Keep the sidebar pointing at whatever is open, opening the
                // directories needed to show it.
                self.reveal_in_tree(id);
            }
            Err(err) => self.set_status(format!("cannot open {id}: {err}")),
        }
    }

    pub fn go_back(&mut self) {
        match self.history.pop() {
            Some((id, row)) => {
                self.open_note(&id, false);
                self.editor.buf.goto_line(row);
            }
            None => self.set_status("no earlier note"),
        }
    }

    pub fn save(&mut self) {
        let Some(id) = self.current.clone() else {
            self.set_status("no note open");
            return;
        };
        let path = self.vault.path_for(&id);
        let text = self.editor.buf.text();
        match std::fs::write(&path, &text) {
            Ok(()) => {
                self.editor.buf.mark_saved();
                // Re-index so links, backlinks and tags reflect what was
                // written. Only this note changed, so the whole vault does not
                // need re-reading.
                if let Err(err) = self.vault.refresh_note(&id) {
                    self.set_status(format!("saved, but reindex failed: {err}"));
                } else {
                    self.set_status(format!("saved {id}"));
                }
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("save failed: {err}")),
        }
    }

    /// Offer to create a note for a link that resolves to nothing.
    pub fn prompt_new_note_from_link(&mut self, target: &str) {
        self.overlay = Some(Overlay::Prompt(Prompt {
            kind: PromptKind::NewNoteFromLink,
            title: "Create note".into(),
            input: target.to_string(),
            hint: "this link points at nothing yet — enter to create".into(),
        }));
    }

    /// Follow the `[[link]]` under the cursor, offering to create it if missing.
    pub fn follow_link(&mut self) {
        let Some(link) = self.editor.link_under_cursor() else {
            self.set_status("no link under cursor");
            return;
        };
        match self.vault.resolve_target(&link.target) {
            Some(idx) => {
                let id = self.vault.notes[idx].id.clone();
                let heading = link.heading.clone();
                self.open_note(&id, true);
                // Jump to the heading when the link named one.
                if let Some(h) = heading {
                    if let Some(note) = self.vault.get(&id) {
                        if let Some(row) = note
                            .headings
                            .iter()
                            .find(|x| x.text.eq_ignore_ascii_case(&h))
                            .map(|x| x.line)
                        {
                            self.editor.buf.goto_line(row);
                        }
                    }
                }
            }
            None => self.prompt_new_note_from_link(&link.target),
        }
    }

    fn new_note_path(&self, name: &str) -> String {
        let dir = self.config.new_note_dir.trim_matches('/');
        if dir.is_empty() || name.contains('/') {
            name.to_string()
        } else {
            format!("{dir}/{name}")
        }
    }

    pub fn create_note(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.set_status("note name is empty");
            return;
        }
        let rel = self.new_note_path(name);
        let title = rel
            .rsplit('/')
            .next()
            .unwrap_or(name)
            .trim_end_matches(".md");
        let body = format!("# {title}\n\n");
        match self.vault.create_note(&rel, &body) {
            Ok(id) => {
                self.open_note(&id, true);
                self.editor.buf.goto_line(1);
                self.set_status(format!("created {id}"));
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("create failed: {err}")),
        }
    }

    pub fn daily_note(&mut self) {
        let name = chrono::Local::now()
            .format(&self.config.daily_note_format)
            .to_string();
        let dir = self.config.daily_note_dir.trim_matches('/');
        let rel = if dir.is_empty() {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        let existing = self.vault.resolve_target(&rel);
        match existing {
            Some(idx) => {
                let id = self.vault.notes[idx].id.clone();
                self.open_note(&id, true);
                self.set_status(format!("opened {id}"));
            }
            None => {
                let body = format!("# {name}\n\n");
                match self.vault.create_note(&rel, &body) {
                    Ok(id) => {
                        self.open_note(&id, true);
                        self.editor.buf.goto_line(1);
                        self.set_status(format!("created {id}"));
                    }
                    Err(err) => self.set_status(format!("create failed: {err}")),
                }
            }
        }
    }

    pub fn rename_current(&mut self, new_name: &str) {
        let Some(id) = self.current.clone() else {
            return;
        };
        match self.vault.rename_note(&id, new_name.trim()) {
            Ok((new_id, touched)) => {
                self.current = Some(new_id.clone());
                self.set_status(format!(
                    "renamed to {new_id}{}",
                    if touched > 0 {
                        format!(" — rewrote links in {touched} note(s)")
                    } else {
                        String::new()
                    }
                ));
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("rename failed: {err}")),
        }
    }

    pub fn delete_note(&mut self, id: &str) {
        let path = self.vault.path_for(id);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                let _ = self.vault.rescan();
                if self.current.as_deref() == Some(id) {
                    self.current = None;
                    self.editor.load(Buffer::from_str(""));
                    if let Some(next) = self.vault.notes.first().map(|n| n.id.clone()) {
                        self.open_note(&next, false);
                    }
                }
                self.set_status(format!("deleted {id}"));
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("delete failed: {err}")),
        }
    }

    // ---- git ----------------------------------------------------------

    pub fn refresh_git(&mut self) {
        if let Some(repo) = &self.repo {
            if let Ok(snap) = repo.snapshot() {
                self.git_status = snap;
            }
        }
        self.last_git_poll = Instant::now();
    }

    /// Commit every change in the vault. Used by both the git pane and autocommit.
    pub fn commit_all(&mut self, message: &str) {
        let Some(repo) = self.repo.clone() else {
            self.set_status("not a git repository");
            return;
        };
        let result = (|| -> Result<String> {
            repo.stage_all()?;
            let subject = repo.commit(message)?;
            Ok(subject)
        })();
        self.report(result);
        self.refresh_git();
    }

    // ---- assistant ----------------------------------------------------

    /// Assemble grounding context and start a streaming turn.
    pub fn ask(&mut self, question: String) {
        if question.trim().is_empty() {
            return;
        }
        if self.chat.streaming {
            self.set_status("assistant is still answering");
            return;
        }
        if llm::api_key().is_none() {
            self.set_status("set ANTHROPIC_API_KEY to use the assistant");
            return;
        }

        let mut context: Vec<llm::ContextNote> = Vec::new();
        let mut ids: Vec<String> = Vec::new();

        // The open note is always the first piece of context.
        if let Some(id) = &self.current {
            if let Some(note) = self.vault.get(id) {
                context.push(llm::ContextNote {
                    id: note.id.clone(),
                    title: note.title.clone(),
                    body: llm::excerpt(&self.editor.buf.text(), 6000),
                });
                ids.push(note.id.clone());
            }
        }
        for note in self.vault.relevant(&question, self.config.context_notes) {
            if ids.contains(&note.id) {
                continue;
            }
            context.push(llm::ContextNote {
                id: note.id.clone(),
                title: note.title.clone(),
                body: llm::excerpt(&note.text, 2500),
            });
            ids.push(note.id.clone());
        }

        let vault_name = self
            .vault
            .root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "vault".into());
        let system = llm::system_prompt(&vault_name, &context);

        self.chat.messages.push(llm::Message {
            role: llm::Role::User,
            text: question,
        });
        self.chat.messages.push(llm::Message {
            role: llm::Role::Assistant,
            text: String::new(),
        });
        self.chat.context_ids = ids;

        let history: Vec<llm::Message> = self
            .chat
            .messages
            .iter()
            .filter(|m| !(m.role == llm::Role::Assistant && m.text.is_empty()))
            .cloned()
            .collect();

        let (tx, rx) = std::sync::mpsc::channel();
        llm::spawn(self.config.model.clone(), system, history, tx);
        self.chat.rx = Some(rx);
        self.chat.streaming = true;
        self.assistant_visible = true;
        self.focus = Focus::Assistant;
    }

    /// Drain any streamed tokens. Called once per event-loop tick.
    pub fn poll_assistant(&mut self) -> bool {
        let drained = self.chat.drain();
        if let Some(err) = drained.error {
            self.set_status(format!("assistant: {err}"));
        }
        drained.changed
    }

    pub fn insert_last_answer(&mut self) {
        let Some(answer) = self.chat.last_answer().map(|s| s.to_string()) else {
            self.set_status("no answer to insert");
            return;
        };
        self.editor.buf.checkpoint();
        self.editor.buf.line_end(true);
        self.editor.buf.insert_newline();
        self.editor.buf.insert_str(&answer);
        self.focus = Focus::Editor;
        self.set_status("inserted the assistant's answer");
    }

    // ---- periodic work -------------------------------------------------

    /// Refresh git state occasionally, and autocommit if configured and idle.
    pub fn tick(&mut self) {
        if self.repo.is_some() && self.last_git_poll.elapsed() > Duration::from_secs(5) {
            self.refresh_git();
        }
        let idle = self.config.autocommit_secs;
        if idle > 0
            && self.repo.is_some()
            && !self.git_status.is_clean()
            && !self.editor.buf.dirty
            && self.last_edit.elapsed() > Duration::from_secs(idle)
        {
            let msg = git::autocommit_message(&self.git_status.changes);
            self.commit_all(&msg);
            self.last_edit = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant_turn() -> (Chat, std::sync::mpsc::Sender<llm::Event>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let chat = Chat {
            messages: vec![
                llm::Message {
                    role: llm::Role::User,
                    text: "what is here?".into(),
                },
                // The empty assistant message deltas stream into.
                llm::Message {
                    role: llm::Role::Assistant,
                    text: String::new(),
                },
            ],
            streaming: true,
            rx: Some(rx),
            ..Default::default()
        };
        (chat, tx)
    }

    #[test]
    fn deltas_accumulate_into_the_open_assistant_message() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Delta("Your ".into())).unwrap();
        tx.send(llm::Event::Delta("vault.".into())).unwrap();
        let out = chat.drain();
        assert!(out.changed);
        assert_eq!(chat.messages[1].text, "Your vault.");
        // Still streaming: no Done arrived.
        assert!(chat.streaming);
    }

    #[test]
    fn draining_an_empty_channel_changes_nothing() {
        let (mut chat, _tx) = assistant_turn();
        assert_eq!(chat.drain(), Drained::default());
    }

    #[test]
    fn done_ends_the_turn_and_keeps_the_text() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Delta("answer".into())).unwrap();
        tx.send(llm::Event::Done).unwrap();
        chat.drain();
        assert!(!chat.streaming);
        assert!(chat.rx.is_none());
        assert_eq!(chat.last_answer(), Some("answer"));
    }

    #[test]
    fn an_error_before_any_text_removes_the_empty_bubble() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Error("no credit".into())).unwrap();
        let out = chat.drain();
        assert_eq!(out.error.as_deref(), Some("no credit"));
        assert_eq!(chat.messages.len(), 1, "the placeholder should be gone");
        assert_eq!(chat.messages[0].role, llm::Role::User);
        assert!(!chat.streaming);
    }

    #[test]
    fn an_error_after_partial_text_keeps_what_arrived() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Delta("half an ans".into())).unwrap();
        tx.send(llm::Event::Error("connection reset".into()))
            .unwrap();
        let out = chat.drain();
        assert_eq!(out.error.as_deref(), Some("connection reset"));
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.last_answer(), Some("half an ans"));
    }

    #[test]
    fn a_worker_that_dies_without_speaking_ends_the_turn() {
        let (mut chat, tx) = assistant_turn();
        drop(tx);
        let out = chat.drain();
        assert!(out.changed);
        assert!(out.error.is_none());
        assert!(!chat.streaming, "a dead worker must not leave a spinner up");
    }

    #[test]
    fn last_answer_ignores_a_blank_placeholder() {
        let (chat, _tx) = assistant_turn();
        assert_eq!(chat.last_answer(), None);
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    fn item(action: MenuAction) -> MenuItem {
        MenuItem::new("whatever", action)
    }

    fn menu(items: Vec<MenuItem>) -> Menu {
        Menu {
            title: "t".into(),
            items,
            cursor: 0,
            at: (0, 0),
        }
    }

    /// The point of reading the command table rather than repeating it: the
    /// menu cannot teach a key the help screen disagrees with.
    #[test]
    fn a_command_entry_takes_its_shortcut_from_the_command_table() {
        let save = crate::keymap::COMMANDS
            .iter()
            .find(|(k, _, _)| *k == "save")
            .map(|(_, _, s)| *s)
            .unwrap();
        assert_eq!(item(MenuAction::Command("save")).shortcut(), Some(save));
        assert_eq!(item(MenuAction::Command("save")).shortcut(), Some("ctrl-s"));
    }

    #[test]
    fn a_command_with_no_binding_shows_no_hint() {
        // `rename` is palette-only; an empty string in the table is "no key",
        // not a key that happens to be blank.
        assert_eq!(item(MenuAction::Command("rename")).shortcut(), None);
    }

    #[test]
    fn an_unknown_command_shows_no_hint_rather_than_guessing() {
        assert_eq!(
            item(MenuAction::Command("no-such-command")).shortcut(),
            None
        );
    }

    #[test]
    fn contextual_entries_name_the_key_that_does_the_same_thing() {
        assert_eq!(
            item(MenuAction::OpenNote("a.md".into())).shortcut(),
            Some("enter")
        );
        assert_eq!(
            item(MenuAction::ExpandUnder("p".into())).shortcut(),
            Some("E")
        );
        assert_eq!(
            item(MenuAction::CollapseDir("p".into())).shortcut(),
            Some("h")
        );
    }

    #[test]
    fn entries_with_no_equivalent_key_show_nothing() {
        assert_eq!(item(MenuAction::DeleteNote("a.md".into())).shortcut(), None);
        assert_eq!(item(MenuAction::RenameNote("a.md".into())).shortcut(), None);
        assert_eq!(item(MenuAction::LinkToNote("a.md".into())).shortcut(), None);
    }

    #[test]
    fn a_disabled_entry_carries_its_reason() {
        let entry = item(MenuAction::Command("save")).unless(true, "no unsaved changes");
        assert!(!entry.is_enabled());
        assert_eq!(entry.disabled.as_deref(), Some("no unsaved changes"));
        // `unless(false, ...)` must leave the entry alone.
        let fine = item(MenuAction::Command("save")).unless(false, "never shown");
        assert!(fine.is_enabled());
    }

    #[test]
    fn the_cursor_steps_over_disabled_entries() {
        let mut m = menu(vec![
            item(MenuAction::Command("save")),
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")),
        ]);
        m.move_cursor(1);
        assert_eq!(m.cursor, 3, "should skip the two greyed entries");
        m.move_cursor(1);
        assert_eq!(m.cursor, 0, "and wrap to the first enabled one");
        m.move_cursor(-1);
        assert_eq!(m.cursor, 3, "backwards too");
    }

    /// Spinning forever looking for an enabled entry would hang the draw loop.
    #[test]
    fn a_menu_of_only_disabled_entries_does_not_move_or_hang() {
        let mut m = menu(vec![
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")).unless(true, "nope"),
        ]);
        m.move_cursor(1);
        assert_eq!(m.cursor, 0);
        assert!(m.selected().is_none(), "nothing is selectable");
    }

    #[test]
    fn a_menu_opens_on_the_first_entry_that_can_run() {
        let mut m = menu(vec![
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")),
        ]);
        m.select_first_enabled();
        assert_eq!(m.cursor, 1);
    }

    #[test]
    fn selected_refuses_to_return_a_disabled_entry() {
        let m = menu(vec![item(MenuAction::Command("save")).unless(true, "nope")]);
        assert!(m.selected().is_none());
    }
}
