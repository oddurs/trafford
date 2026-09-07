use crate::config::Config;
use crate::editor::{Buffer, Editor};
use crate::git::{self, Repo};
use crate::llm;
use crate::ui::theme::Theme;
use crate::vault::Vault;
use anyhow::Result;
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
        score -= (idx as i64) / 8; // prefer earlier matches
        indices.push(idx);
        last_match = Some(idx);
        hi = idx + 1;
    }
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
    pub sidebar_cursor: usize,
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
    /// Tag currently filtering the note list, if any.
    pub tag_filter: Option<String>,
}

impl App {
    pub fn new(vault: Vault, config: Config) -> App {
        let repo = Repo::discover(&vault.root);
        let git_status = repo
            .as_ref()
            .and_then(|r| r.snapshot().ok())
            .unwrap_or_default();
        let theme = Theme::named(&config.theme);
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
            tag_filter: None,
            config,
        };
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
        app
    }

    // ---- status -------------------------------------------------------

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

    /// The note ids currently listed in the sidebar, honouring any tag filter.
    pub fn listed_notes(&self) -> Vec<&crate::vault::Note> {
        match &self.tag_filter {
            Some(tag) => self.vault.notes_with_tag(tag),
            None => self.vault.notes.iter().collect(),
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
                if let Some(pos) = self.listed_notes().iter().position(|n| n.id == id) {
                    self.sidebar_cursor = pos;
                }
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
            None => {
                self.overlay = Some(Overlay::Prompt(Prompt {
                    kind: PromptKind::NewNoteFromLink,
                    title: "Create note".into(),
                    input: link.target.clone(),
                    hint: "this link points at nothing yet — enter to create".into(),
                }));
            }
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
