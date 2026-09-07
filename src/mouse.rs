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

    /// Right-click offers what can be done to the thing under the pointer.
    /// The menu is built from what was actually clicked rather than being one
    /// fixed list, so it never offers "rename" over empty space.
    fn right_click(&mut self, c: u16, r: u16) {
        use crate::app::{Menu, MenuAction as A, MenuItem};
        let item = |label: &str, action: A| MenuItem {
            label: label.to_string(),
            action,
        };

        // A menu is already open: a second right-click dismisses it.
        if self.overlay.is_some() {
            self.overlay = None;
            return;
        }

        let (title, items) = if self.panes.sidebar_hit(c, r) {
            self.focus = Focus::Sidebar;
            match self.sidebar_target(r) {
                Some(SidebarTarget::Note(id)) => (
                    short_name(&id),
                    vec![
                        item("Open", A::OpenNote(id.clone())),
                        item("Insert a link to this", A::LinkToNote(id.clone())),
                        item("Rename…", A::RenameNote(id.clone())),
                        item("History", A::HistoryOf(id.clone())),
                        item("Delete…", A::DeleteNote(id)),
                    ],
                ),
                Some(SidebarTarget::Dir(path)) => (
                    short_name(&path),
                    vec![
                        item("New note here…", A::NewNoteIn(path.clone())),
                        item("Expand everything under", A::ExpandUnder(path.clone())),
                        item("Collapse", A::CollapseDir(path)),
                    ],
                ),
                None => (
                    "Vault".to_string(),
                    vec![
                        item("New note…", A::Command("new-note")),
                        item("Expand all", A::Command("expand-all")),
                        item("Collapse all", A::Command("collapse-all")),
                        item("Change theme…", A::Command("theme")),
                    ],
                ),
            }
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
                None => return,
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
            items.push(item("Insert a link…", A::Command("insert-link")));
            items.push(item("Save", A::Command("save")));
            items.push(item("Toggle preview", A::Command("toggle-preview")));
            if self.current.is_some() {
                items.push(item("History of this note", A::Command("git-history")));
                items.push(item("Rename…", A::Command("rename")));
            }
            let title = self
                .current
                .as_deref()
                .map(short_name)
                .unwrap_or_else(|| "Editor".into());
            (title, items)
        } else {
            return;
        };

        if items.is_empty() {
            return;
        }
        self.overlay = Some(Overlay::Menu(Menu {
            title,
            items,
            cursor: 0,
            at: (c, r),
        }));
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
                    if let Some(chosen) = menu.items.get(index).cloned() {
                        menu.cursor = index;
                        self.run_menu_action(chosen.action);
                        return;
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
    fn list_index_on_an_empty_list_is_none() {
        assert_eq!(list_index(10, 10, 0, 0, 20), None);
    }
}
