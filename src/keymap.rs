use crate::app::{
    App, Confirm, ConfirmKind, Focus, GitPane, Overlay, PickItem, Picker, Prompt, PromptKind,
    SearchPane, SidebarTab,
};
use crate::editor::EditorAction;
use crate::ui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Every command reachable from the palette. The third field is the shortcut
/// shown alongside it, and is documentation only — the binding lives in
/// [`global_key`].
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("open", "Open note", "ctrl-p"),
    ("search", "Search vault", "ctrl-f"),
    ("new-note", "New note", "ctrl-n"),
    ("daily-note", "Open today's daily note", ""),
    ("insert-link", "Insert link to a note", "ctrl-l"),
    ("backlinks", "Jump to a note that links here", "ctrl-t"),
    ("save", "Save note", "ctrl-s"),
    ("rename", "Rename note and rewrite links", ""),
    ("delete", "Delete note", ""),
    ("toggle-preview", "Toggle rendered preview", "ctrl-e"),
    ("toggle-sidebar", "Toggle sidebar", "ctrl-b"),
    ("toggle-context", "Toggle context pane", ""),
    ("cycle-theme", "Cycle theme", ""),
    ("reindex", "Reindex vault from disk", ""),
    ("git-panel", "Git: review changes", "ctrl-g"),
    ("git-commit", "Git: commit all changes", ""),
    ("git-push", "Git: push", ""),
    ("git-pull", "Git: pull with rebase", ""),
    ("git-history", "Git: history of this note", ""),
    ("ask", "Assistant: ask a question", "ctrl-j"),
    ("ask-note", "Assistant: explain this note", ""),
    ("ask-related", "Assistant: what connects to this note", ""),
    ("ask-selection", "Assistant: ask about the selection", ""),
    (
        "insert-answer",
        "Assistant: insert last answer at cursor",
        "ctrl-y",
    ),
    (
        "save-answer",
        "Assistant: save last answer as a new note",
        "",
    ),
    ("clear-chat", "Assistant: clear the conversation", ""),
    ("help", "Keyboard reference", "f1"),
    ("quit", "Quit", "ctrl-q"),
];

pub fn palette_items() -> Vec<PickItem> {
    COMMANDS
        .iter()
        .map(|(key, label, shortcut)| PickItem {
            label: label.to_string(),
            detail: shortcut.to_string(),
            key: key.to_string(),
        })
        .collect()
}

impl App {
    fn note_items(&self) -> Vec<PickItem> {
        self.vault
            .notes
            .iter()
            .map(|n| PickItem {
                label: n.title.clone(),
                detail: n.id.clone(),
                key: n.id.clone(),
            })
            .collect()
    }

    pub fn open_switcher(&mut self) {
        self.overlay = Some(Overlay::Switcher(Picker::new(
            "Open note",
            self.note_items(),
        )));
    }

    pub fn open_palette(&mut self) {
        self.overlay = Some(Overlay::Palette(Picker::new("Commands", palette_items())));
    }

    pub fn open_git_pane(&mut self) {
        let Some(repo) = self.repo.clone() else {
            self.set_status("not a git repository — run `git init` in the vault");
            return;
        };
        self.refresh_git();
        let log = repo.log(12).unwrap_or_default();
        self.overlay = Some(Overlay::Git(GitPane {
            snapshot: self.git_status.clone(),
            cursor: 0,
            log,
        }));
    }

    /// Run a palette command by key. Also used by the direct shortcuts.
    pub fn run_command(&mut self, key: &str) {
        match key {
            "open" => self.open_switcher(),
            "search" => {
                self.overlay = Some(Overlay::Search(SearchPane::default()));
            }
            "new-note" => {
                self.overlay = Some(Overlay::Prompt(Prompt {
                    kind: PromptKind::NewNote,
                    title: "New note".into(),
                    input: String::new(),
                    hint: "name, or folder/name".into(),
                }))
            }
            "daily-note" => self.daily_note(),
            "insert-link" => {
                self.overlay = Some(Overlay::LinkPicker(Picker::new(
                    "Insert link",
                    self.note_items(),
                )))
            }
            "backlinks" => {
                let Some(id) = self.current.clone() else {
                    self.set_status("no note open");
                    return;
                };
                let items: Vec<PickItem> = self
                    .vault
                    .backlinks_for(&id)
                    .iter()
                    .map(|bl| {
                        let title = self
                            .vault
                            .get(&bl.from)
                            .map(|n| n.title.clone())
                            .unwrap_or_else(|| bl.from.clone());
                        PickItem {
                            label: title,
                            detail: bl.context.clone(),
                            // The line travels with the pick so we land on it.
                            key: format!("{}:{}", bl.from, bl.line),
                        }
                    })
                    .collect();
                if items.is_empty() {
                    self.set_status("nothing links here yet");
                } else {
                    self.overlay = Some(Overlay::Backlinks(Picker::new("Backlinks", items)));
                }
            }
            "save" => self.save(),
            "rename" => match &self.current {
                Some(id) => {
                    let input = id.trim_end_matches(".md").to_string();
                    self.overlay = Some(Overlay::Prompt(Prompt {
                        kind: PromptKind::Rename,
                        title: "Rename note".into(),
                        input,
                        hint: "incoming [[links]] are rewritten".into(),
                    }));
                }
                None => self.set_status("no note open"),
            },
            "delete" => match self.current.clone() {
                Some(id) => {
                    self.overlay = Some(Overlay::Confirm(Confirm {
                        message: format!("Delete {id}? This cannot be undone."),
                        kind: ConfirmKind::DeleteNote(id),
                    }));
                }
                None => self.set_status("no note open"),
            },
            "toggle-preview" => {
                self.preview = !self.preview;
                self.set_status(if self.preview { "preview" } else { "source" });
            }
            "toggle-sidebar" => self.sidebar_visible = !self.sidebar_visible,
            "toggle-context" => self.context_visible = !self.context_visible,
            "cycle-theme" => {
                let next = match self.config.theme.as_str() {
                    "night" => "paper",
                    "paper" => "mono",
                    _ => "night",
                };
                self.config.theme = next.to_string();
                self.theme = Theme::named(next);
                self.set_status(format!("theme: {next}"));
            }
            "reindex" => match self.vault.rescan() {
                Ok(()) => self.set_status(format!("indexed {} notes", self.vault.notes.len())),
                Err(err) => self.set_status(format!("reindex failed: {err}")),
            },
            "git-panel" => self.open_git_pane(),
            "git-commit" => {
                if self.repo.is_none() {
                    self.set_status("not a git repository");
                } else {
                    let suggestion = crate::git::autocommit_message(&self.git_status.changes);
                    self.overlay = Some(Overlay::Prompt(Prompt {
                        kind: PromptKind::Commit,
                        title: "Commit message".into(),
                        input: suggestion,
                        hint: "stages every change in the vault".into(),
                    }));
                }
            }
            "git-push" => match self.repo.clone() {
                Some(repo) => {
                    let result = repo.push();
                    match result {
                        Ok(msg) => self.set_status(msg),
                        Err(err) => self.set_status(format!("push failed: {err}")),
                    }
                    self.refresh_git();
                }
                None => self.set_status("not a git repository"),
            },
            "git-pull" => match self.repo.clone() {
                Some(repo) => {
                    match repo.pull() {
                        Ok(msg) => {
                            let _ = self.vault.rescan();
                            // Reload the open note; the pull may have changed it.
                            if let Some(id) = self.current.clone() {
                                if !self.editor.buf.dirty {
                                    self.open_note(&id, false);
                                }
                            }
                            self.set_status(msg);
                        }
                        Err(err) => self.set_status(format!("pull failed: {err}")),
                    }
                    self.refresh_git();
                }
                None => self.set_status("not a git repository"),
            },
            "git-history" => match (self.repo.clone(), self.current.clone()) {
                (Some(repo), Some(id)) => {
                    let rel = repo.rel(&self.vault.path_for(&id));
                    match repo.log_for(&rel, 30) {
                        Ok(log) if log.is_empty() => self.set_status("no commits touch this note"),
                        Ok(log) => self.overlay = Some(Overlay::History(log)),
                        Err(err) => self.set_status(format!("history failed: {err}")),
                    }
                }
                (None, _) => self.set_status("not a git repository"),
                (_, None) => self.set_status("no note open"),
            },
            "ask" => {
                self.assistant_visible = true;
                self.focus = Focus::Assistant;
            }
            "ask-note" => match &self.current {
                Some(id) => {
                    let q = format!(
                        "Explain the note [[{}]] and what it is really about.",
                        trim_md(id)
                    );
                    self.ask(q);
                }
                None => self.set_status("no note open"),
            },
            "ask-related" => match &self.current {
                Some(id) => {
                    let q = format!(
                        "Which other notes in the vault connect to [[{}]], and what links am I missing?",
                        trim_md(id)
                    );
                    self.ask(q);
                }
                None => self.set_status("no note open"),
            },
            "ask-selection" => match self.editor.selected_text() {
                Some(sel) => {
                    let q = format!(
                        "About this passage from my notes:\n\n{sel}\n\nWhat should I know?"
                    );
                    self.ask(q);
                }
                None => self.set_status("nothing selected — use V to select lines"),
            },
            "insert-answer" => self.insert_last_answer(),
            "save-answer" => {
                if self.chat.last_answer().is_none() {
                    self.set_status("no answer to save");
                } else {
                    self.overlay = Some(Overlay::Prompt(Prompt {
                        kind: PromptKind::SaveAnswerAs,
                        title: "Save answer as".into(),
                        input: String::new(),
                        hint: "name, or folder/name".into(),
                    }));
                }
            }
            "clear-chat" => {
                self.chat.messages.clear();
                self.chat.context_ids.clear();
                self.set_status("conversation cleared");
            }
            "help" => self.overlay = Some(Overlay::Help),
            "quit" => self.request_quit(),
            other => self.set_status(format!("unknown command: {other}")),
        }
    }

    pub fn request_quit(&mut self) {
        if self.editor.buf.dirty {
            self.overlay = Some(Overlay::Confirm(Confirm {
                kind: ConfirmKind::QuitDirty,
                message: "This note has unsaved changes. Quit anyway?".into(),
            }));
        } else {
            self.should_quit = true;
        }
    }

    // ---- routing -------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if self.overlay.is_some() {
            self.overlay_key(key);
            return;
        }
        if self.global_key(key) {
            return;
        }
        match self.focus {
            Focus::Sidebar => self.sidebar_key(key),
            Focus::Editor => self.editor_key(key),
            Focus::Assistant => self.assistant_key(key),
        }
    }

    /// Shortcuts that work from any pane. Returns true when the key was used.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::F(1) {
            self.overlay = Some(Overlay::Help);
            return true;
        }
        if !ctrl {
            // Tab cycles panes, but only when the editor is not taking text.
            if key.code == KeyCode::Tab && !self.editor.mode.is_insert() {
                self.cycle_focus();
                return true;
            }
            return false;
        }
        // ctrl-d/u/r/o belong to the editor (scroll, redo, back); the rest are global.
        match key.code {
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Char('p') => self.open_switcher(),
            KeyCode::Char('k') => self.open_palette(),
            KeyCode::Char('f') => self.run_command("search"),
            KeyCode::Char('g') => self.run_command("git-panel"),
            KeyCode::Char('n') => self.run_command("new-note"),
            KeyCode::Char('l') => self.run_command("insert-link"),
            KeyCode::Char('t') => self.run_command("backlinks"),
            KeyCode::Char('b') => self.run_command("toggle-sidebar"),
            KeyCode::Char('e') => self.run_command("toggle-preview"),
            KeyCode::Char('y') => self.run_command("insert-answer"),
            KeyCode::Char('j') => {
                self.assistant_visible = !self.assistant_visible;
                self.focus = if self.assistant_visible {
                    Focus::Assistant
                } else {
                    Focus::Editor
                };
            }
            _ => return false,
        }
        true
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Editor if self.sidebar_visible => Focus::Sidebar,
            Focus::Editor if self.assistant_visible => Focus::Assistant,
            Focus::Sidebar if self.assistant_visible => Focus::Assistant,
            Focus::Sidebar => Focus::Editor,
            Focus::Assistant => Focus::Editor,
            other => other,
        };
    }

    fn editor_key(&mut self, key: KeyEvent) {
        self.last_edit = std::time::Instant::now();
        match self.editor.on_key(key) {
            EditorAction::Save => self.save(),
            EditorAction::FollowLink => self.follow_link(),
            EditorAction::Back => self.go_back(),
            EditorAction::Status(msg) => self.set_status(msg),
            EditorAction::None => {}
        }
    }

    fn sidebar_key(&mut self, key: KeyEvent) {
        match self.sidebar_tab {
            SidebarTab::Notes => {
                let len = self.listed_notes().len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            self.sidebar_cursor = (self.sidebar_cursor + 1) % len;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if len > 0 {
                            self.sidebar_cursor = (self.sidebar_cursor + len - 1) % len;
                        }
                    }
                    KeyCode::Char('g') => self.sidebar_cursor = 0,
                    KeyCode::Char('G') => self.sidebar_cursor = len.saturating_sub(1),
                    KeyCode::Enter | KeyCode::Char('l') => {
                        if let Some(id) = self
                            .listed_notes()
                            .get(self.sidebar_cursor)
                            .map(|n| n.id.clone())
                        {
                            self.open_note(&id, true);
                        }
                    }
                    KeyCode::Char('t') => {
                        self.sidebar_tab = SidebarTab::Tags;
                        self.sidebar_cursor = 0;
                    }
                    KeyCode::Char('c') if self.tag_filter.is_some() => {
                        self.tag_filter = None;
                        self.sidebar_cursor = 0;
                        self.set_status("tag filter cleared");
                    }
                    KeyCode::Char('/') => self.open_switcher(),
                    KeyCode::Esc | KeyCode::Char('h') => self.focus = Focus::Editor,
                    _ => {}
                }
            }
            SidebarTab::Tags => {
                let tags = self.vault.all_tags();
                let len = tags.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            self.sidebar_cursor = (self.sidebar_cursor + 1) % len;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if len > 0 {
                            self.sidebar_cursor = (self.sidebar_cursor + len - 1) % len;
                        }
                    }
                    KeyCode::Enter | KeyCode::Char('l') => {
                        if let Some((tag, _)) = tags.get(self.sidebar_cursor) {
                            self.tag_filter = Some(tag.clone());
                            self.sidebar_tab = SidebarTab::Notes;
                            self.sidebar_cursor = 0;
                            self.set_status(format!("filtering by #{tag}"));
                        }
                    }
                    KeyCode::Char('t') | KeyCode::Esc => {
                        self.sidebar_tab = SidebarTab::Notes;
                        self.sidebar_cursor = 0;
                    }
                    _ => {}
                }
            }
        }
    }

    fn assistant_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.focus = Focus::Editor,
            KeyCode::Enter => {
                let q = std::mem::take(&mut self.chat.input);
                self.ask(q);
            }
            KeyCode::Backspace => {
                self.chat.input.pop();
            }
            KeyCode::PageUp => self.chat.scroll = self.chat.scroll.saturating_sub(5),
            KeyCode::PageDown => self.chat.scroll = self.chat.scroll.saturating_add(5),
            KeyCode::Char(c) if !ctrl => self.chat.input.push(c),
            _ => {}
        }
    }

    // ---- overlays ------------------------------------------------------

    fn overlay_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            match escape_target(self.overlay.as_ref()) {
                Escape::Close => self.overlay = None,
                // The diff was opened from the git pane, so esc goes back to
                // it rather than dropping the user all the way to the editor.
                Escape::ReopenGitPane => self.open_git_pane(),
            }
            return;
        }
        let Some(overlay) = self.overlay.take() else {
            return;
        };
        match overlay {
            Overlay::Palette(picker) => self.picker_key(key, picker, PickerKind::Palette),
            Overlay::Switcher(picker) => self.picker_key(key, picker, PickerKind::Switcher),
            Overlay::LinkPicker(picker) => self.picker_key(key, picker, PickerKind::Link),
            Overlay::Backlinks(picker) => self.picker_key(key, picker, PickerKind::Backlink),
            Overlay::Search(pane) => self.search_key(key, pane),
            Overlay::Prompt(prompt) => self.prompt_key(key, prompt),
            Overlay::Git(pane) => self.git_key(key, pane),
            Overlay::History(log) => {
                // Any navigation key keeps it open; anything else closes it.
                if matches!(
                    key.code,
                    KeyCode::Char('j') | KeyCode::Char('k') | KeyCode::Up | KeyCode::Down
                ) {
                    self.overlay = Some(Overlay::History(log));
                }
            }
            Overlay::Diff {
                title,
                body,
                mut scroll,
            } => {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => scroll = scroll.saturating_add(1),
                    KeyCode::Char('k') | KeyCode::Up => scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown => scroll = scroll.saturating_add(15),
                    KeyCode::PageUp => scroll = scroll.saturating_sub(15),
                    KeyCode::Char('g') => scroll = 0,
                    _ => return,
                }
                self.overlay = Some(Overlay::Diff {
                    title,
                    body,
                    scroll,
                });
            }
            Overlay::Confirm(confirm) => self.confirm_key(key, confirm),
            Overlay::Help => {}
        }
    }

    fn picker_key(&mut self, key: KeyEvent, mut picker: Picker, kind: PickerKind) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => {
                let chosen = picker.selected().map(|i| i.key.clone());
                if let Some(chosen) = chosen {
                    match kind {
                        PickerKind::Palette => self.run_command(&chosen),
                        PickerKind::Switcher => self.open_note(&chosen, true),
                        PickerKind::Link => self.insert_link_to(&chosen),
                        PickerKind::Backlink => self.jump_to_backlink(&chosen),
                    }
                }
                return;
            }
            KeyCode::Up => picker.move_cursor(-1),
            KeyCode::Down => picker.move_cursor(1),
            KeyCode::Char('p') if ctrl => picker.move_cursor(-1),
            KeyCode::Char('n') if ctrl => picker.move_cursor(1),
            KeyCode::Backspace => picker.pop(),
            KeyCode::Char(c) if !ctrl => picker.push(c),
            _ => {}
        }
        self.overlay = Some(match kind {
            PickerKind::Palette => Overlay::Palette(picker),
            PickerKind::Switcher => Overlay::Switcher(picker),
            PickerKind::Link => Overlay::LinkPicker(picker),
            PickerKind::Backlink => Overlay::Backlinks(picker),
        });
    }

    /// `key` is `note-id:line`, as packed by the backlinks command.
    fn jump_to_backlink(&mut self, key: &str) {
        let (id, line) = match key.rsplit_once(':') {
            Some((id, line)) => (id, line.parse::<usize>().unwrap_or(0)),
            None => (key, 0),
        };
        let id = id.to_string();
        self.open_note(&id, true);
        self.editor.buf.goto_line(line);
    }

    fn insert_link_to(&mut self, id: &str) {
        let stem = id.trim_end_matches(".md").rsplit('/').next().unwrap_or(id);
        self.editor.buf.checkpoint();
        // Insert after the cursor character, matching `a` rather than `i`.
        if self.editor.buf.col < self.editor.buf.line_len(self.editor.buf.row) {
            self.editor.buf.move_right(true);
        }
        self.editor.buf.insert_str(&format!("[[{stem}]]"));
        self.focus = Focus::Editor;
        self.set_status(format!("linked to {stem}"));
    }

    fn search_key(&mut self, key: KeyEvent, mut pane: SearchPane) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => {
                if let Some(hit) = pane.hits.get(pane.cursor).cloned() {
                    self.open_note(&hit.id, true);
                    self.editor.buf.goto_line(hit.line);
                    return;
                }
            }
            KeyCode::Up => pane.cursor = pane.cursor.saturating_sub(1),
            KeyCode::Down => {
                if pane.cursor + 1 < pane.hits.len() {
                    pane.cursor += 1;
                }
            }
            KeyCode::Char('p') if ctrl => pane.cursor = pane.cursor.saturating_sub(1),
            KeyCode::Char('n') if ctrl => {
                if pane.cursor + 1 < pane.hits.len() {
                    pane.cursor += 1;
                }
            }
            KeyCode::Backspace => {
                pane.query.pop();
                pane.hits = self.vault.search(&pane.query, 200);
                pane.cursor = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                pane.query.push(c);
                pane.hits = self.vault.search(&pane.query, 200);
                pane.cursor = 0;
            }
            _ => {}
        }
        self.overlay = Some(Overlay::Search(pane));
    }

    fn prompt_key(&mut self, key: KeyEvent, mut prompt: Prompt) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => {
                let value = prompt.input.trim().to_string();
                match prompt.kind {
                    PromptKind::NewNote | PromptKind::NewNoteFromLink => self.create_note(&value),
                    PromptKind::Rename => self.rename_current(&value),
                    PromptKind::Commit => self.commit_all(&value),
                    PromptKind::SaveAnswerAs => {
                        let answer = self.chat.last_answer().unwrap_or_default().to_string();
                        match self.vault.create_note(&value, &answer) {
                            Ok(id) => {
                                self.open_note(&id, true);
                                self.set_status(format!("saved answer to {id}"));
                            }
                            Err(err) => self.set_status(format!("save failed: {err}")),
                        }
                    }
                }
                return;
            }
            KeyCode::Backspace => {
                prompt.input.pop();
            }
            KeyCode::Char(c) if !ctrl => prompt.input.push(c),
            _ => {}
        }
        self.overlay = Some(Overlay::Prompt(prompt));
    }

    fn git_key(&mut self, key: KeyEvent, mut pane: GitPane) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let len = pane.snapshot.changes.len();
        let selected = pane.snapshot.changes.get(pane.cursor).cloned();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    pane.cursor = (pane.cursor + 1) % len;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if len > 0 {
                    pane.cursor = (pane.cursor + len - 1) % len;
                }
            }
            KeyCode::Char(' ') => {
                if let Some(change) = &selected {
                    let result = if change.staged {
                        repo.unstage(&change.path)
                    } else {
                        repo.stage(&change.path)
                    };
                    if let Err(err) = result {
                        self.set_status(format!("git: {err}"));
                    }
                    self.refresh_git();
                    pane.snapshot = self.git_status.clone();
                    pane.cursor = pane
                        .cursor
                        .min(pane.snapshot.changes.len().saturating_sub(1));
                }
            }
            KeyCode::Char('a') => {
                if let Err(err) = repo.stage_all() {
                    self.set_status(format!("git: {err}"));
                }
                self.refresh_git();
                pane.snapshot = self.git_status.clone();
            }
            KeyCode::Char('c') => {
                let suggestion = crate::git::autocommit_message(&pane.snapshot.changes);
                self.overlay = Some(Overlay::Prompt(Prompt {
                    kind: PromptKind::Commit,
                    title: "Commit message".into(),
                    input: suggestion,
                    hint: "stages every change in the vault".into(),
                }));
                return;
            }
            KeyCode::Char('P') => {
                match repo.push() {
                    Ok(msg) => self.set_status(msg),
                    Err(err) => self.set_status(format!("push failed: {err}")),
                }
                self.refresh_git();
                pane.snapshot = self.git_status.clone();
            }
            KeyCode::Char('p') => {
                match repo.pull() {
                    Ok(msg) => {
                        let _ = self.vault.rescan();
                        self.set_status(msg);
                    }
                    Err(err) => self.set_status(format!("pull failed: {err}")),
                }
                self.refresh_git();
                pane.snapshot = self.git_status.clone();
            }
            KeyCode::Char('d') => {
                if let Some(change) = &selected {
                    match repo.diff(Some(&change.path)) {
                        Ok(body) if body.trim().is_empty() => {
                            self.set_status(format!("{} has no diff against HEAD", change.path))
                        }
                        Ok(body) => {
                            self.overlay = Some(Overlay::Diff {
                                title: format!("diff · {}", change.path),
                                body,
                                scroll: 0,
                            });
                            return;
                        }
                        Err(err) => self.set_status(format!("diff failed: {err}")),
                    }
                }
            }
            KeyCode::Char('X') => {
                if let Some(change) = &selected {
                    self.overlay = Some(Overlay::Confirm(Confirm {
                        message: format!("Discard all local changes to {}?", change.path),
                        kind: ConfirmKind::DiscardChanges(change.path.clone()),
                    }));
                    return;
                }
            }
            KeyCode::Enter => {
                if let Some(change) = &selected {
                    if change.path.ends_with(".md") {
                        let id = change.path.clone();
                        if self.vault.get(&id).is_some() {
                            self.open_note(&id, true);
                            return;
                        }
                    }
                    self.set_status(format!("{} is not a note in this vault", change.path));
                }
            }
            _ => {}
        }
        self.overlay = Some(Overlay::Git(pane));
    }

    fn confirm_key(&mut self, key: KeyEvent, confirm: Confirm) {
        let yes = matches!(
            key.code,
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
        );
        if !yes {
            return;
        }
        match confirm.kind {
            ConfirmKind::DeleteNote(id) => self.delete_note(&id),
            ConfirmKind::DiscardChanges(path) => {
                if let Some(repo) = self.repo.clone() {
                    match repo.discard(&path) {
                        Ok(()) => {
                            let _ = self.vault.rescan();
                            if self.current.as_deref() == Some(path.as_str()) {
                                let id = path.clone();
                                self.open_note(&id, false);
                            }
                            self.set_status(format!("discarded changes to {path}"));
                        }
                        Err(err) => self.set_status(format!("discard failed: {err}")),
                    }
                    self.refresh_git();
                }
            }
            ConfirmKind::QuitDirty => self.should_quit = true,
        }
    }
}

/// Where `esc` lands when an overlay is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escape {
    Close,
    ReopenGitPane,
}

pub fn escape_target(overlay: Option<&Overlay>) -> Escape {
    match overlay {
        Some(Overlay::Diff { .. }) => Escape::ReopenGitPane,
        _ => Escape::Close,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerKind {
    Palette,
    Switcher,
    Link,
    Backlink,
}

fn trim_md(id: &str) -> &str {
    id.trim_end_matches(".md")
}

/// Rows shown in the help overlay.
pub const HELP: &[(&str, &str)] = &[
    ("", "GLOBAL"),
    ("ctrl-p", "open a note"),
    ("ctrl-k", "command palette"),
    ("ctrl-f", "search the vault"),
    ("ctrl-n", "new note"),
    ("ctrl-l", "insert a link to a note"),
    ("ctrl-t", "jump to a note that links here"),
    ("ctrl-s", "save"),
    ("ctrl-e", "toggle rendered preview"),
    ("ctrl-b", "toggle sidebar"),
    ("ctrl-g", "git panel"),
    ("ctrl-j", "toggle the assistant"),
    ("ctrl-y", "insert the assistant's last answer"),
    ("tab", "cycle panes"),
    ("f1", "this help"),
    ("ctrl-q", "quit"),
    ("", ""),
    ("", "EDITOR (vim-flavoured)"),
    ("i a I A o O", "enter insert mode"),
    ("esc", "back to normal mode"),
    ("h j k l w b 0 ^ $", "motions"),
    ("gg G 5G", "jump to line"),
    ("x dd dw D cc cw C", "delete and change"),
    ("yy p P", "yank and put"),
    ("v V", "visual and visual-line"),
    (">> <<", "indent and outdent"),
    ("u ctrl-r", "undo and redo"),
    ("space", "toggle the task on this line"),
    ("enter", "follow the [[link]] under the cursor"),
    ("ctrl-o", "back to the previous note"),
    ("", ""),
    ("", "SIDEBAR"),
    ("j k g G", "move"),
    ("enter", "open"),
    ("t", "switch between notes and tags"),
    ("c", "clear the tag filter"),
    ("", ""),
    ("", "GIT PANEL"),
    ("space", "stage or unstage"),
    ("a", "stage everything"),
    ("c", "commit"),
    ("P p", "push and pull --rebase"),
    ("d", "show the diff"),
    ("X", "discard a file's changes"),
    ("enter", "open the note"),
];

#[cfg(test)]
mod tests {
    use super::{escape_target, Escape};
    use crate::app::{fuzzy_match, Overlay};

    #[test]
    fn escape_from_the_diff_returns_to_the_git_pane() {
        let diff = Overlay::Diff {
            title: "diff · a.md".into(),
            body: String::new(),
            scroll: 0,
        };
        assert_eq!(escape_target(Some(&diff)), Escape::ReopenGitPane);
    }

    #[test]
    fn escape_from_every_other_overlay_closes_it() {
        assert_eq!(escape_target(Some(&Overlay::Help)), Escape::Close);
        assert_eq!(escape_target(None), Escape::Close);
    }

    #[test]
    fn fuzzy_matches_subsequences_only() {
        assert!(fuzzy_match("abc", "a-b-c").is_some());
        assert!(fuzzy_match("cba", "a-b-c").is_none());
    }

    #[test]
    fn fuzzy_prefers_word_boundaries() {
        let tight = fuzzy_match("dn", "daily-note").unwrap().0;
        let loose = fuzzy_match("dn", "dandelion").unwrap().0;
        assert!(tight > loose, "tight={tight} loose={loose}");
    }

    #[test]
    fn fuzzy_prefers_consecutive_runs() {
        let run = fuzzy_match("note", "note").unwrap().0;
        let scattered = fuzzy_match("note", "n-o-t-e").unwrap().0;
        assert!(run > scattered);
    }

    #[test]
    fn a_contiguous_run_at_a_boundary_beats_everything() {
        let best = fuzzy_match("note", "my note").unwrap().0;
        let split = fuzzy_match("note", "no other term ends").unwrap().0;
        assert!(best > split, "best={best} split={split}");
    }

    /// From a real vault: typing a filename ranked a different note first,
    /// because the positional penalty compounded per matched character and
    /// swamped the exact match sitting late in a long path.
    #[test]
    fn an_exact_filename_wins_however_deep_the_path() {
        let query = "01-route-plan";
        let wanted = "Route Plan: Brooklyn to Palo Alto                       04-archive/01-move-california/01-route-plan.md";
        let other =
            "Technical Plan                      01-projects/05-budgeting-app/04-technical-plan.md";
        let (wanted_score, _) = fuzzy_match(query, wanted).unwrap();
        let (other_score, _) = fuzzy_match(query, other).unwrap();
        assert!(
            wanted_score > other_score,
            "wanted={wanted_score} other={other_score}"
        );
    }

    #[test]
    fn a_deep_path_does_not_beat_a_shallow_one_on_the_same_match() {
        let (shallow, _) = fuzzy_match("notes", "notes.md").unwrap();
        let (deep, _) = fuzzy_match("notes", "a/b/c/d/e/f/notes.md").unwrap();
        assert!(shallow > deep, "shallow={shallow} deep={deep}");
    }

    #[test]
    fn fuzzy_is_case_insensitive_but_rewards_exact_case() {
        let exact = fuzzy_match("Rust", "Rust").unwrap().0;
        let folded = fuzzy_match("rust", "Rust").unwrap().0;
        assert!(exact > folded);
    }

    #[test]
    fn fuzzy_reports_matched_indices() {
        let (_, idx) = fuzzy_match("ac", "abc").unwrap();
        assert_eq!(idx, vec![0, 2]);
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_match("", "anything").unwrap().1.len(), 0);
    }

    #[test]
    fn every_command_key_is_unique() {
        let mut keys: Vec<&str> = super::COMMANDS.iter().map(|(k, _, _)| *k).collect();
        keys.sort();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate command key");
    }
}
