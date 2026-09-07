//! Mouse handling.
//!
//! Every pane records where it was drawn (see [`crate::app::Panes`]), so a
//! click is resolved against the frame the user is actually looking at rather
//! than against a model of where things ought to be. Anything that can be
//! reached with a key can be reached with the mouse.

use crate::app::{App, ContextTarget, Focus, Overlay, SidebarTab};
use crate::editor::Mode;
use crate::tree::Entry;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

/// Rows a wheel notch moves a list. Three matches most terminals' own scroll.
const WHEEL: usize = 3;

impl App {
    pub fn on_mouse(&mut self, event: MouseEvent) {
        let (c, r) = (event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click(c, r, event),
            MouseEventKind::Down(MouseButton::Right) => self.right_click(c, r),
            MouseEventKind::Drag(MouseButton::Left) => self.drag(c, r, event),
            MouseEventKind::ScrollUp => self.scroll(c, r, -(WHEEL as isize)),
            MouseEventKind::ScrollDown => self.scroll(c, r, WHEEL as isize),
            _ => {}
        }
    }

    fn click(&mut self, c: u16, r: u16, event: MouseEvent) {
        // An overlay covers the panes beneath it, so it gets first refusal.
        if self.overlay.is_some() {
            self.click_overlay(c, r);
            return;
        }
        if self.panes.sidebar_hit(c, r) {
            self.focus = Focus::Sidebar;
            self.click_sidebar(c, r);
        } else if self.panes.editor_hit(c, r) {
            self.focus = Focus::Editor;
            if matches!(self.editor.mode, Mode::Visual | Mode::VisualLine) {
                self.editor.mode = Mode::Normal;
            }
            self.click_editor(c, r, event);
        } else if self.panes.context_hit(c, r) {
            self.click_context(c, r);
        } else if self.panes.assistant_hit(c, r) {
            self.focus = Focus::Assistant;
        }
    }

    /// Open the menu for whatever the focused pane has selected, without a
    /// mouse.
    ///
    /// The point is computed from the selection and then handed to the same
    /// `menu_for` a click uses. Building a second path for the keyboard is how
    /// the two drift into offering different things.
    pub fn open_menu_at_selection(&mut self) {
        let Some((c, r)) = self.selection_point() else {
            self.set_status("nothing to act on here");
            return;
        };
        match self.menu_for(c, r) {
            Some((title, items)) => self.open_menu(title, items, c, r),
            None => self.set_status("nothing to act on here"),
        }
    }

    /// Where on screen the focused pane's selection is drawn.
    fn selection_point(&self) -> Option<(u16, u16)> {
        match self.focus {
            Focus::Sidebar => {
                let inner = self.panes.sidebar;
                if inner.height == 0 {
                    return None;
                }
                let visible = (inner.height as usize).saturating_sub(1);
                let offset =
                    crate::ui::scroll_offset(self.sidebar_cursor, self.sidebar_len(), visible);
                let row = self.sidebar_cursor.checked_sub(offset)?;
                let y = inner.y + 1 + u16::try_from(row).ok()?;
                (y < inner.bottom()).then_some((inner.x + 2, y))
            }
            Focus::Editor => {
                let inner = self.panes.editor;
                if inner.height == 0 {
                    return None;
                }
                let row = self.editor.buf.row.checked_sub(self.editor.scroll)?;
                let y = inner.y + u16::try_from(row).ok()?;
                let gutter = crate::ui::gutter_width(self.editor.buf.len());
                (y < inner.bottom()).then_some((inner.x + gutter, y))
            }
            Focus::Assistant => {
                let inner = self.panes.assistant;
                (inner.height > 0).then_some((inner.x + 1, inner.y + 1))
            }
        }
    }

    /// Right-click offers what can be done to the thing under the pointer.
    /// The menu is built from what was actually clicked rather than being one
    /// fixed list, so it never offers "rename" over empty space.
    fn right_click(&mut self, c: u16, r: u16) {
        // A menu is already open: a second right-click dismisses it.
        if matches!(self.overlay, Some(Overlay::Menu(_))) {
            self.overlay = None;
            return;
        }
        // An overlay covers the panes beneath it, so it answers first.
        let found = if self.overlay.is_some() {
            self.overlay_menu(c, r)
        } else {
            self.menu_for(c, r)
        };
        if let Some((title, items)) = found {
            self.open_menu(title, items, c, r);
        }
    }

    /// What a right-click on a pane should offer, or `None` when there is
    /// nothing there — in which case the click is ignored rather than opening
    /// an empty menu.
    fn menu_for(&mut self, c: u16, r: u16) -> Option<(String, Vec<crate::app::MenuItem>)> {
        use crate::app::{MenuAction as A, MenuItem};
        let item = |label: &str, action: A| MenuItem::new(label, action);

        let (title, items) = if self.panes.sidebar_hit(c, r) {
            self.focus = Focus::Sidebar;
            match self.sidebar_target(r) {
                Some(SidebarTarget::Note(id)) => {
                    let only_note = self.vault.notes.len() <= 1;
                    let tracked = self.repo.is_some();
                    (
                        short_name(&id),
                        vec![
                            item("Open", A::OpenNote(id.clone())),
                            item("Insert a link to this", A::LinkToNote(id.clone()))
                                .unless(self.current.is_none(), "no note open"),
                            item(
                                "Copy a link to this",
                                A::Copy {
                                    what: "link",
                                    text: wikilink(&id),
                                },
                            ),
                            item(
                                "Copy the path",
                                A::Copy {
                                    what: "path",
                                    text: id.clone(),
                                },
                            ),
                            item("Move to…", A::MoveNote(id.clone())),
                            item("Open in Obsidian", obsidian(&self.vault, &id))
                                .unless(!has_obsidian(&self.vault), "no .obsidian in this vault"),
                            item("Open in $EDITOR", editor_action(&self.vault, &id))
                                .unless(std::env::var("EDITOR").is_err(), "$EDITOR is not set"),
                            item("Reveal in the file manager", reveal(&self.vault, &id)).unless(
                                cfg!(not(any(target_os = "macos", target_os = "linux"))),
                                "not supported here",
                            ),
                            item("Duplicate", A::DuplicateNote(id.clone())),
                            item("Rename…", A::RenameNote(id.clone())),
                            item("History", A::HistoryOf(id.clone()))
                                .unless(!tracked, "not a git repository"),
                            item("Delete…", A::DeleteNote(id)).unless(only_note, "the only note"),
                        ],
                    )
                }
                Some(SidebarTarget::Dir(path)) => (
                    short_name(&path),
                    vec![
                        // There is no "New folder": a vault is indexed from
                        // its notes, so a folder with nothing in it has
                        // nowhere to live. Naming `folder/note` here makes
                        // both at once, which is the same gesture with an
                        // honest label.
                        item("New note here…", A::NewNoteIn(path.clone())),
                        item("Expand everything under", A::ExpandUnder(path.clone())),
                        item("Collapse", A::CollapseDir(path)),
                    ],
                ),
                None => match self.sidebar_tab {
                    // The tags tab: the row under the pointer is a tag.
                    SidebarTab::Tags => {
                        let index = self.sidebar_index(r)?;
                        let tags = self.vault.all_tags();
                        let (tag, count) = tags.get(index)?.clone();
                        (
                            format!("#{tag}"),
                            vec![
                                MenuItem::new(
                                    format!("Show the {count} notes with this tag"),
                                    A::FilterByTag(tag),
                                ),
                                item("Back to the tree", A::Command("collapse-all")),
                            ],
                        )
                    }
                    SidebarTab::Notes => (
                        "Vault".to_string(),
                        vec![
                            item("New note…", A::Command("new-note")),
                            item("Expand all", A::Command("expand-all")),
                            item("Collapse all", A::Command("collapse-all")),
                            item("Change theme…", A::Command("theme")),
                            item("Clear the tag filter", A::Command("clear-tag-filter"))
                                .unless(self.tag_filter.is_none(), "no filter set"),
                        ],
                    ),
                },
            }
        } else if self.panes.assistant_hit(c, r) {
            self.focus = Focus::Assistant;
            let answered = self.chat.last_answer().is_some();
            (
                "Assistant".to_string(),
                vec![
                    item("Insert the last answer", A::Command("insert-answer"))
                        .unless(!answered, "nothing answered yet"),
                    item("Save the answer as a note…", A::Command("save-answer"))
                        .unless(!answered, "nothing answered yet"),
                    item("Ask about this note", A::Command("ask-note"))
                        .unless(self.current.is_none(), "no note open"),
                    item("Clear the conversation", A::Command("clear-chat"))
                        .unless(self.chat.messages.is_empty(), "nothing to clear"),
                ],
            )
        } else if self.panes.context_hit(c, r) {
            let index = r.saturating_sub(self.panes.context.y) as usize;
            match self.context_targets.get(index).cloned().flatten() {
                Some(ContextTarget::Heading(row)) => {
                    ("Heading".into(), vec![item("Go to it", A::GoToLine(row))])
                }
                Some(ContextTarget::Note(id)) => (
                    short_name(&id),
                    vec![
                        item("Open", A::OpenNote(id.clone())),
                        item("History", A::HistoryOf(id)),
                    ],
                ),
                Some(ContextTarget::Backlink(id, line)) => (
                    short_name(&id),
                    vec![
                        item("Open at the mention", A::OpenNoteAt(id.clone(), line)),
                        item("Open the note", A::OpenNote(id)),
                    ],
                ),
                Some(ContextTarget::Unwritten(target)) => (
                    target.clone(),
                    vec![item("Write this note…", A::CreateNote(target))],
                ),
                None => return None,
            }
        } else if self.panes.editor_hit(c, r) {
            self.focus = Focus::Editor;
            // Put the cursor where the click was, so the menu acts on it.
            let event = MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: c,
                row: r,
                modifiers: crossterm::event::KeyModifiers::NONE,
            };
            self.click_editor(c, r, event);
            let mut items = Vec::new();
            if let Some(link) = self.editor.link_under_cursor() {
                let target = link.target.clone();
                match self.vault.resolve_target(&target) {
                    Some(idx) => {
                        let id = self.vault.notes[idx].id.clone();
                        items.push(item("Follow this link", A::OpenNote(id)));
                    }
                    None => items.push(item("Write this note…", A::CreateNote(target))),
                }
            }
            // The one moment the interface knows exactly what you mean is
            // when something is selected, so those actions lead.
            if let Some(selection) = self.editor.selected_text() {
                let lines = selection.lines().count();
                items.push(item(
                    &format!("Copy the {lines} selected line(s)"),
                    A::Copy {
                        what: "selection",
                        text: selection,
                    },
                ));
                items.push(item("Make a note from this…", A::ExtractSelection));
                items.push(item(
                    "Ask the assistant about this",
                    A::Command("ask-selection"),
                ));
                items.push(item("Indent", A::Command("indent-selection")));
                items.push(item("Outdent", A::Command("outdent-selection")));
                items.push(item("Delete these lines", A::Command("delete-selection")));
            }
            if let Some(id) = self.current.clone() {
                items.push(item(
                    "Copy a link to this note",
                    A::Copy {
                        what: "link",
                        text: wikilink(&id),
                    },
                ));
            }
            items.push(item("Insert a link…", A::Command("insert-link")));
            items.push(
                item("Save", A::Command("save"))
                    .unless(!self.editor.buf.dirty, "no unsaved changes"),
            );
            items.push(item("Toggle preview", A::Command("toggle-preview")));
            if self.current.is_some() {
                items.push(
                    item("History of this note", A::Command("git-history"))
                        .unless(self.repo.is_none(), "not a git repository"),
                );
                items.push(item("Rename…", A::Command("rename")));
            }
            let title = self
                .current
                .as_deref()
                .map(short_name)
                .unwrap_or_else(|| "Editor".into());
            (title, items)
        } else {
            return None;
        };
        Some((title, items))
    }

    fn open_menu(&mut self, title: String, items: Vec<crate::app::MenuItem>, c: u16, r: u16) {
        use crate::app::Menu;
        if items.is_empty() {
            return;
        }
        let mut menu = Menu {
            title,
            items,
            cursor: 0,
            at: (c, r),
        };
        // Opening onto a greyed entry would make enter do nothing.
        menu.select_first_enabled();
        self.overlay = Some(Overlay::Menu(menu));
    }

    /// The menu for a right-click inside an open overlay. `None` when the
    /// overlay has nothing to offer there, so the click is simply ignored
    /// rather than opening an empty menu.
    fn overlay_menu(&mut self, c: u16, r: u16) -> Option<(String, Vec<crate::app::MenuItem>)> {
        use crate::app::{MenuAction as A, MenuItem as I};
        if !self.panes.overlay_hit(c, r) {
            return None;
        }
        let top = self.panes.overlay.y;
        let height = self.panes.overlay.height as usize;
        match self.overlay.as_ref()? {
            // The git pane's actions are single letters nobody remembers,
            // which is the case for a menu if ever there was one.
            Overlay::Git(pane) => {
                let count = pane.snapshot.changes.len().min(12);
                let index = list_index(r, top + 2, count, pane.cursor, count)?;
                let change = pane.snapshot.changes.get(index)?.clone();
                let path = change.path.clone();
                let is_note = self.vault.get(&path).is_some();
                Some((
                    short_name(&path),
                    vec![
                        if change.staged {
                            I::new("Unstage", A::GitUnstage(path.clone()))
                        } else {
                            I::new("Stage", A::GitStage(path.clone()))
                        },
                        I::new("Show the diff", A::GitDiff(path.clone())),
                        I::new("Open the note", A::OpenNote(path.clone()))
                            .unless(!is_note, "not a note in this vault"),
                        I::new("Discard changes…", A::GitDiscard(path.clone())).unless(
                            change.status == crate::git::Status::Untracked,
                            "never tracked",
                        ),
                    ],
                ))
            }
            Overlay::MoveTo { .. } | Overlay::Themes(_) => None,
            Overlay::Search(pane) => {
                let index = list_index(r, top + 1, pane.hits.len(), pane.cursor, height - 1)?;
                let hit = pane.hits.get(index)?.clone();
                Some((
                    short_name(&hit.id),
                    vec![
                        I::new("Open at this line", A::OpenNoteAt(hit.id.clone(), hit.line)),
                        I::new("Open the note", A::OpenNote(hit.id.clone())),
                        I::new("Insert a link to this", A::LinkToNote(hit.id)),
                    ],
                ))
            }
            Overlay::Switcher(picker) | Overlay::Backlinks(picker) => {
                let index =
                    list_index(r, top + 1, picker.matches.len(), picker.cursor, height - 1)?;
                let (item_index, _) = picker.matches.get(index)?;
                let id = picker.items.get(*item_index)?.key.clone();
                // A backlink's key carries a line; the note id is the head.
                let note = id
                    .rsplit_once(':')
                    .map(|(n, _)| n.to_string())
                    .unwrap_or(id);
                Some((
                    short_name(&note),
                    vec![
                        I::new("Open", A::OpenNote(note.clone())),
                        I::new("Insert a link to this", A::LinkToNote(note.clone())),
                        I::new("Rename…", A::RenameNote(note)),
                    ],
                ))
            }
            _ => None,
        }
    }

    /// Dragging in the editor selects. The editor's operators are line-wise,
    /// so the selection is too — dragging then pressing `y` or `d` does what
    /// the highlight showed, rather than something subtly narrower.
    fn drag(&mut self, c: u16, r: u16, event: MouseEvent) {
        if self.overlay.is_some() || !self.panes.editor_hit(c, r) {
            return;
        }
        if self.editor.mode != Mode::VisualLine {
            self.editor.anchor = (self.editor.buf.row, self.editor.buf.col);
            self.editor.mode = Mode::VisualLine;
        }
        self.click_editor(c, r, event);
    }

    fn scroll(&mut self, c: u16, r: u16, delta: isize) {
        if self.overlay.is_some() && self.panes.overlay_hit(c, r) {
            self.scroll_overlay(delta);
            return;
        }
        if self.overlay.is_some() {
            return;
        }
        if self.panes.sidebar_hit(c, r) {
            let len = self.sidebar_len();
            self.sidebar_cursor = step(self.sidebar_cursor, delta, len);
        } else if self.panes.editor_hit(c, r) {
            // Move the view; the cursor follows only as far as it must to stay
            // on screen, which is how a wheel behaves everywhere else.
            let height = self.editor_height.max(1);
            let max = self.editor.buf.len().saturating_sub(1);
            self.editor.scroll = step(self.editor.scroll, delta, max + 1).min(max);
            let top = self.editor.scroll;
            let bottom = (top + height).saturating_sub(1);
            self.editor.buf.row = self.editor.buf.row.clamp(top, bottom.min(max));
            self.editor.buf.clamp(self.editor.mode.is_insert());
        } else if self.panes.assistant_hit(c, r) {
            self.chat.scroll = if delta < 0 {
                self.chat.scroll.saturating_add(WHEEL as u16)
            } else {
                self.chat.scroll.saturating_sub(WHEEL as u16)
            };
        }
    }

    fn sidebar_len(&self) -> usize {
        match self.sidebar_tab {
            SidebarTab::Notes => self.tree_rows().len(),
            SidebarTab::Tags => self.vault.all_tags().len(),
        }
    }

    /// Which list index sits on screen row `r`, accounting for the one-line
    /// header the sidebar draws and wherever the list is scrolled to.
    fn sidebar_index(&self, r: u16) -> Option<usize> {
        let inner = self.panes.sidebar;
        let first = inner.y.checked_add(1)?; // the header line
        if r < first {
            return None;
        }
        let visible = (inner.height as usize).saturating_sub(1);
        let len = self.sidebar_len();
        let offset = crate::ui::scroll_offset(self.sidebar_cursor, len, visible);
        let index = offset + (r - first) as usize;
        (index < len).then_some(index)
    }

    /// What the sidebar row at screen row `r` refers to.
    fn sidebar_target(&self, r: u16) -> Option<SidebarTarget> {
        let index = self.sidebar_index(r)?;
        match self.sidebar_tab {
            SidebarTab::Notes => match self.tree_rows().get(index)?.entry.clone() {
                Entry::Note { id, .. } => Some(SidebarTarget::Note(id)),
                Entry::Dir { path, .. } => Some(SidebarTarget::Dir(path)),
            },
            SidebarTab::Tags => None,
        }
    }

    fn click_sidebar(&mut self, _c: u16, r: u16) {
        let Some(index) = self.sidebar_index(r) else {
            return;
        };
        match self.sidebar_tab {
            SidebarTab::Notes => {
                let rows = self.tree_rows();
                let Some(row) = rows.get(index) else {
                    return;
                };
                self.sidebar_cursor = index;
                match row.entry.clone() {
                    Entry::Dir { path, expanded, .. } => {
                        if expanded {
                            self.expanded.remove(&path);
                        } else {
                            self.expanded.insert(path);
                        }
                    }
                    Entry::Note { id, .. } => self.open_note(&id, true),
                }
            }
            SidebarTab::Tags => {
                let tags = self.vault.all_tags();
                let Some((tag, _)) = tags.get(index) else {
                    return;
                };
                self.sidebar_cursor = index;
                self.tag_filter = Some(tag.clone());
                self.sidebar_tab = SidebarTab::Notes;
                self.sidebar_cursor = 0;
                self.expand_all();
                self.set_status(format!("filtering by #{tag}"));
            }
        }
    }

    fn click_editor(&mut self, c: u16, r: u16, event: MouseEvent) {
        let inner = self.panes.editor;
        let gutter = crate::ui::gutter_width(self.editor.buf.len());
        let row = self.editor.scroll + (r.saturating_sub(inner.y)) as usize;
        if row >= self.editor.buf.len() {
            return;
        }
        self.editor.buf.row = row;

        let text_x = inner.x + gutter;
        if c < text_x {
            // The gutter: put the cursor at the start of the line.
            self.editor.buf.col = 0;
            self.editor.buf.goal_col = 0;
            return;
        }
        let hscroll = crate::ui::editor_hscroll(&self.editor, inner.width, gutter, self.preview);
        let target = (c - text_x) as usize;
        let line = self.editor.buf.line(row).to_string();
        self.editor.buf.col = crate::ui::column_at(&line, hscroll, target);
        self.editor.buf.goal_col = self.editor.buf.col;
        self.editor.buf.clamp(self.editor.mode.is_insert());

        // Following a link on a plain click would make it impossible to put
        // the cursor inside one, so source mode wants a modifier and the
        // read-only preview does not — which is what Obsidian does.
        let modified = event.modifiers.intersects(
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SUPER,
        );
        if (self.preview || modified) && self.editor.link_under_cursor().is_some() {
            self.follow_link();
        }
    }

    fn click_context(&mut self, _c: u16, r: u16) {
        let index = r.saturating_sub(self.panes.context.y) as usize;
        let Some(Some(target)) = self.context_targets.get(index).cloned() else {
            return;
        };
        match target {
            ContextTarget::Heading(row) => {
                self.focus = Focus::Editor;
                self.editor.buf.goto_line(row);
            }
            ContextTarget::Note(id) => self.open_note(&id, true),
            ContextTarget::Backlink(id, line) => {
                self.open_note(&id, true);
                self.editor.buf.goto_line(line);
            }
            ContextTarget::Unwritten(target) => {
                self.prompt_new_note_from_link(&target);
            }
        }
    }

    fn click_overlay(&mut self, c: u16, r: u16) {
        if !self.panes.overlay_hit(c, r) {
            // Clicking outside an overlay dismisses it, as a dialog should.
            self.overlay = None;
            return;
        }
        let top = self.panes.overlay.y;
        let Some(overlay) = self.overlay.take() else {
            return;
        };
        match overlay {
            // Pickers and search draw a query line first, then the results.
            Overlay::Palette(mut p) => {
                if let Some(i) = list_index(
                    r,
                    top + 1,
                    p.matches.len(),
                    p.cursor,
                    visible(self.panes.overlay.height, 1),
                ) {
                    p.cursor = i;
                    let chosen = p.selected().map(|it| it.key.clone());
                    if let Some(key) = chosen {
                        self.run_command(&key);
                        return;
                    }
                }
                self.overlay = Some(Overlay::Palette(p));
            }
            Overlay::Switcher(mut p) => {
                if let Some(i) = list_index(
                    r,
                    top + 1,
                    p.matches.len(),
                    p.cursor,
                    visible(self.panes.overlay.height, 1),
                ) {
                    p.cursor = i;
                    if let Some(item) = p.selected() {
                        let id = item.key.clone();
                        self.open_note(&id, true);
                        return;
                    }
                }
                self.overlay = Some(Overlay::Switcher(p));
            }
            Overlay::Backlinks(mut p) => {
                if let Some(i) = list_index(
                    r,
                    top + 1,
                    p.matches.len(),
                    p.cursor,
                    visible(self.panes.overlay.height, 1),
                ) {
                    p.cursor = i;
                    if let Some(item) = p.selected() {
                        let key = item.key.clone();
                        self.jump_to_backlink(&key);
                        return;
                    }
                }
                self.overlay = Some(Overlay::Backlinks(p));
            }
            Overlay::LinkPicker(mut p) => {
                if let Some(i) = list_index(
                    r,
                    top + 1,
                    p.matches.len(),
                    p.cursor,
                    visible(self.panes.overlay.height, 1),
                ) {
                    p.cursor = i;
                    if let Some(item) = p.selected() {
                        let id = item.key.clone();
                        self.insert_link_to(&id);
                        return;
                    }
                }
                self.overlay = Some(Overlay::LinkPicker(p));
            }
            Overlay::Search(mut pane) => {
                if let Some(i) = list_index(
                    r,
                    top + 1,
                    pane.hits.len(),
                    pane.cursor,
                    visible(self.panes.overlay.height, 1),
                ) {
                    pane.cursor = i;
                    if let Some(hit) = pane.hits.get(i).cloned() {
                        self.open_note(&hit.id, true);
                        self.editor.buf.goto_line(hit.line);
                        return;
                    }
                }
                self.overlay = Some(Overlay::Search(pane));
            }
            // The git pane's actions are keys, so a click only selects.
            Overlay::Git(mut pane) => {
                let count = pane.snapshot.changes.len().min(12);
                if let Some(i) = list_index(r, top + 2, count, pane.cursor, count) {
                    pane.cursor = i;
                }
                self.overlay = Some(Overlay::Git(pane));
            }
            Overlay::Menu(mut menu) => {
                let first = self.panes.overlay.y;
                if r >= first {
                    // The same offset the renderer used; a menu long enough to
                    // scroll would otherwise resolve clicks to the wrong entry.
                    let visible = self.panes.overlay.height as usize;
                    let offset = crate::ui::scroll_offset(menu.cursor, menu.items.len(), visible);
                    let index = offset + (r - first) as usize;
                    match menu.items.get(index) {
                        // A greyed entry takes the click and does nothing, so
                        // it cannot be run by aiming badly.
                        Some(entry) if entry.is_enabled() => {
                            let action = entry.action.clone();
                            menu.cursor = index;
                            self.run_menu_action(action);
                            return;
                        }
                        _ => {}
                    }
                }
                self.overlay = Some(Overlay::Menu(menu));
            }
            other => self.overlay = Some(other),
        }
    }

    fn scroll_overlay(&mut self, delta: isize) {
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        match overlay {
            Overlay::Palette(p)
            | Overlay::Switcher(p)
            | Overlay::LinkPicker(p)
            | Overlay::Backlinks(p)
            | Overlay::Themes(p) => {
                p.cursor = step(p.cursor, delta, p.matches.len());
            }
            Overlay::Search(pane) => pane.cursor = step(pane.cursor, delta, pane.hits.len()),
            Overlay::Menu(menu) => menu.cursor = step(menu.cursor, delta, menu.items.len()),
            Overlay::Git(pane) => {
                pane.cursor = step(pane.cursor, delta, pane.snapshot.changes.len())
            }
            Overlay::Diff { scroll, .. } => {
                *scroll = if delta < 0 {
                    scroll.saturating_sub(WHEEL as u16)
                } else {
                    scroll.saturating_add(WHEEL as u16)
                };
            }
            _ => {}
        }
    }
}

enum SidebarTarget {
    Note(String),
    Dir(String),
}

/// Whether this vault is also an Obsidian vault. Offering to open a note in
/// Obsidian for a plain folder of markdown would be an entry that fails when
/// chosen, which is worse than one that is not there.
fn has_obsidian(vault: &crate::vault::Vault) -> bool {
    vault.root.join(".obsidian").is_dir()
}

/// The URL Obsidian answers, with the vault and file as query parameters.
fn obsidian(vault: &crate::vault::Vault, id: &str) -> crate::app::MenuAction {
    let name = vault
        .root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let url = format!(
        "obsidian://open?vault={}&file={}",
        urlencode(&name),
        urlencode(id.trim_end_matches(".md"))
    );
    crate::app::MenuAction::Spawn {
        program: opener().into(),
        args: vec![url],
    }
}

fn reveal(vault: &crate::vault::Vault, id: &str) -> crate::app::MenuAction {
    let path = vault.path_for(id).to_string_lossy().to_string();
    let args = if cfg!(target_os = "macos") {
        // -R selects the file rather than opening it.
        vec!["-R".to_string(), path]
    } else {
        vec![path]
    };
    crate::app::MenuAction::Spawn {
        program: opener().into(),
        args,
    }
}

fn editor_action(vault: &crate::vault::Vault, id: &str) -> crate::app::MenuAction {
    // $EDITOR may carry flags, as in "code -w" or "emacs -nw".
    let raw = std::env::var("EDITOR").unwrap_or_default();
    let mut parts = raw.split_whitespace().map(str::to_string);
    let program = parts.next().unwrap_or_else(|| "vi".into());
    let mut args: Vec<String> = parts.collect();
    args.push(vault.path_for(id).to_string_lossy().to_string());
    crate::app::MenuAction::Suspend { program, args }
}

fn opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Percent-encode what a URL query cannot carry literally. Small on purpose:
/// note names are the only input, and pulling in a crate for this would be
/// more dependency than the job needs.
fn urlencode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A note id as the link you would paste into another note.
fn wikilink(id: &str) -> String {
    format!("[[{}]]", short_name(id))
}

/// The last path segment, which is what a menu title should say.
fn short_name(id: &str) -> String {
    id.trim_end_matches(".md")
        .rsplit('/')
        .next()
        .unwrap_or(id)
        .to_string()
}

/// Move `cursor` by `delta`, clamped to `len`.
fn step(cursor: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = cursor as isize + delta;
    next.clamp(0, len as isize - 1) as usize
}

fn visible(height: u16, header: u16) -> usize {
    (height.saturating_sub(header)) as usize
}

/// The list index drawn on screen row `r`, where the list starts at `first`.
fn list_index(r: u16, first: u16, len: usize, cursor: usize, visible: usize) -> Option<usize> {
    if r < first || len == 0 {
        return None;
    }
    let offset = crate::ui::scroll_offset(cursor, len, visible);
    let index = offset + (r - first) as usize;
    (index < len).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_clamps_at_both_ends() {
        assert_eq!(step(0, -3, 10), 0);
        assert_eq!(step(9, 3, 10), 9);
        assert_eq!(step(5, -3, 10), 2);
        assert_eq!(step(5, 3, 10), 8);
    }

    #[test]
    fn step_on_an_empty_list_stays_put() {
        assert_eq!(step(0, 3, 0), 0);
    }

    #[test]
    fn list_index_maps_a_screen_row_to_an_unscrolled_list() {
        assert_eq!(list_index(10, 10, 5, 0, 20), Some(0));
        assert_eq!(list_index(12, 10, 5, 0, 20), Some(2));
        // Past the end of a short list.
        assert_eq!(list_index(20, 10, 5, 0, 20), None);
        // Above the list.
        assert_eq!(list_index(9, 10, 5, 0, 20), None);
    }

    #[test]
    fn list_index_accounts_for_scrolling() {
        // 100 items, 10 visible, cursor at 50: the window starts at 45.
        let offset = crate::ui::scroll_offset(50, 100, 10);
        assert_eq!(offset, 45);
        assert_eq!(list_index(10, 10, 100, 50, 10), Some(45));
        assert_eq!(list_index(15, 10, 100, 50, 10), Some(50));
    }

    #[test]
    fn urls_encode_what_a_query_cannot_carry() {
        assert_eq!(urlencode("plain"), "plain");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("03-resources/ai-ml"), "03-resources%2Fai-ml");
        assert_eq!(urlencode("Karpathy's"), "Karpathy%27s");
        // Non-ASCII goes out as its utf-8 bytes, each percent-encoded.
        assert_eq!(urlencode("日"), "%E6%97%A5");
        // Unreserved characters must be left alone, or Obsidian will not match.
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn list_index_on_an_empty_list_is_none() {
        assert_eq!(list_index(10, 10, 0, 0, 20), None);
    }
}
