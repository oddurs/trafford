pub mod buffer;

pub use buffer::Buffer;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl Mode {
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }

    pub fn is_insert(&self) -> bool {
        matches!(self, Mode::Insert)
    }
}

/// Something the editor cannot do itself and hands back to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    None,
    Save,
    /// Follow the `[[wikilink]]` under the cursor.
    FollowLink,
    /// Jump back in the navigation history.
    Back,
    /// Open the context menu for the cursor's line.
    Menu,
    Status(String),
}

/// A vim-flavoured modal editor over a [`Buffer`].
pub struct Editor {
    pub buf: Buffer,
    pub mode: Mode,
    pub scroll: usize,
    /// Anchor of the current visual selection, as (row, col).
    pub anchor: (usize, usize),
    /// Pending operator such as `d`, `y`, `c`, or prefix `g`.
    pending: Option<char>,
    /// Numeric count typed before an operator or motion.
    count: Option<usize>,
    register: String,
    register_linewise: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new(Buffer::default())
    }
}

impl Editor {
    pub fn new(buf: Buffer) -> Editor {
        Editor {
            buf,
            mode: Mode::Normal,
            scroll: 0,
            anchor: (0, 0),
            pending: None,
            count: None,
            register: String::new(),
            register_linewise: false,
        }
    }

    pub fn load(&mut self, buf: Buffer) {
        self.buf = buf;
        self.mode = Mode::Normal;
        self.scroll = 0;
        self.pending = None;
        self.count = None;
    }

    /// Pending-key indicator for the status line, e.g. `2d` while typing `2dd`.
    pub fn pending_hint(&self) -> String {
        let mut s = String::new();
        if let Some(n) = self.count {
            s.push_str(&n.to_string());
        }
        if let Some(p) = self.pending {
            s.push(p);
        }
        s
    }

    /// The inclusive row range covered by the visual selection.
    pub fn selection_rows(&self) -> Option<(usize, usize)> {
        match self.mode {
            Mode::Visual | Mode::VisualLine => {
                let a = self.anchor.0;
                let b = self.buf.row;
                Some((a.min(b), a.max(b)))
            }
            _ => None,
        }
    }

    /// Keep the cursor inside a viewport `height` rows tall.
    pub fn sync_scroll(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.buf.row < self.scroll {
            self.scroll = self.buf.row;
        } else if self.buf.row >= self.scroll + height {
            self.scroll = self.buf.row + 1 - height;
        }
        let max_scroll = self.buf.len().saturating_sub(1);
        self.scroll = self.scroll.min(max_scroll);
    }

    pub fn on_key(&mut self, key: KeyEvent) -> EditorAction {
        let action = match self.mode {
            Mode::Insert => self.insert_key(key),
            Mode::Normal => self.normal_key(key),
            Mode::Visual | Mode::VisualLine => self.visual_key(key),
        };
        self.buf.clamp(self.mode.is_insert());
        action
    }

    // ---- insert mode ---------------------------------------------------

    fn insert_key(&mut self, key: KeyEvent) -> EditorAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.buf.end_insert();
                self.mode = Mode::Normal;
                self.buf.move_left();
            }
            KeyCode::Char('s') if ctrl => return EditorAction::Save,
            KeyCode::Char(c) if !ctrl => {
                self.buf.begin_insert();
                self.buf.insert_char(c);
            }
            KeyCode::Enter => {
                self.buf.begin_insert();
                self.buf.insert_newline_smart();
            }
            KeyCode::Backspace => {
                self.buf.begin_insert();
                self.buf.backspace();
            }
            KeyCode::Tab => {
                self.buf.begin_insert();
                self.buf.insert_str("  ");
            }
            KeyCode::Left => self.buf.move_left(),
            KeyCode::Right => self.buf.move_right(true),
            KeyCode::Up => self.buf.move_up(true),
            KeyCode::Down => self.buf.move_down(true),
            KeyCode::Home => self.buf.line_start(),
            KeyCode::End => self.buf.line_end(true),
            _ => {}
        }
        EditorAction::None
    }

    // ---- normal mode ---------------------------------------------------

    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }

    fn normal_key(&mut self, key: KeyEvent) -> EditorAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        if ctrl {
            return match key.code {
                KeyCode::Char('s') => EditorAction::Save,
                KeyCode::Char('r') => {
                    self.buf.redo();
                    EditorAction::None
                }
                KeyCode::Char('d') => {
                    for _ in 0..10 {
                        self.buf.move_down(false);
                    }
                    EditorAction::None
                }
                KeyCode::Char('u') => {
                    for _ in 0..10 {
                        self.buf.move_up(false);
                    }
                    EditorAction::None
                }
                KeyCode::Char('o') => EditorAction::Back,
                _ => EditorAction::None,
            };
        }

        // Operator-pending: the previous key was d/y/c/g/>/<.
        if let Some(op) = self.pending.take() {
            return self.operator_key(op, key);
        }

        match key.code {
            KeyCode::Char(c @ '1'..='9') | KeyCode::Char(c @ '0') => {
                // A leading 0 with no count is the line-start motion.
                if c == '0' && self.count.is_none() {
                    self.buf.line_start();
                } else {
                    let digit = c.to_digit(10).unwrap() as usize;
                    self.count = Some(self.count.unwrap_or(0) * 10 + digit);
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                for _ in 0..self.take_count() {
                    self.buf.move_left();
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                for _ in 0..self.take_count() {
                    self.buf.move_right(false);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                for _ in 0..self.take_count() {
                    self.buf.move_down(false);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                for _ in 0..self.take_count() {
                    self.buf.move_up(false);
                }
            }
            KeyCode::Char('w') => {
                for _ in 0..self.take_count() {
                    self.buf.next_word();
                }
            }
            KeyCode::Char('b') => {
                for _ in 0..self.take_count() {
                    self.buf.prev_word();
                }
            }
            KeyCode::Char('^') | KeyCode::Home => self.buf.first_non_blank(),
            KeyCode::Char('$') | KeyCode::End => self.buf.line_end(false),
            KeyCode::Char('G') => {
                let row = match self.count.take() {
                    Some(n) => n.saturating_sub(1),
                    None => self.buf.len().saturating_sub(1),
                };
                self.buf.goto_line(row);
            }
            KeyCode::Char('g')
            | KeyCode::Char('d')
            | KeyCode::Char('y')
            | KeyCode::Char('c')
            | KeyCode::Char('>')
            | KeyCode::Char('<') => {
                if let KeyCode::Char(c) = key.code {
                    self.pending = Some(c);
                }
            }
            KeyCode::Char('i') => {
                self.mode = Mode::Insert;
                self.buf.begin_insert();
            }
            KeyCode::Char('a') => {
                self.mode = Mode::Insert;
                self.buf.begin_insert();
                self.buf.move_right(true);
            }
            KeyCode::Char('I') => {
                self.buf.first_non_blank();
                self.mode = Mode::Insert;
                self.buf.begin_insert();
            }
            KeyCode::Char('A') => {
                self.buf.line_end(true);
                self.mode = Mode::Insert;
                self.buf.begin_insert();
            }
            KeyCode::Char('o') => {
                self.buf.checkpoint();
                self.buf.open_below();
                self.mode = Mode::Insert;
                self.buf.begin_insert();
            }
            KeyCode::Char('O') => {
                self.buf.checkpoint();
                self.buf.open_above();
                self.mode = Mode::Insert;
                self.buf.begin_insert();
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                self.buf.checkpoint();
                for _ in 0..self.take_count() {
                    self.buf.delete_char();
                }
            }
            KeyCode::Char('D') => {
                self.buf.checkpoint();
                self.register = self.buf.delete_to_line_end();
                self.register_linewise = false;
            }
            KeyCode::Char('C') => {
                self.buf.checkpoint();
                self.register = self.buf.delete_to_line_end();
                self.register_linewise = false;
                self.mode = Mode::Insert;
            }
            KeyCode::Char('p') => {
                self.buf.checkpoint();
                let (text, linewise) = (self.register.clone(), self.register_linewise);
                self.buf.paste(&text, linewise, true);
            }
            KeyCode::Char('P') => {
                self.buf.checkpoint();
                let (text, linewise) = (self.register.clone(), self.register_linewise);
                self.buf.paste(&text, linewise, false);
            }
            KeyCode::Char('u') => {
                if !self.buf.undo() {
                    return EditorAction::Status("already at oldest change".into());
                }
            }
            KeyCode::Char('v') => {
                self.mode = Mode::Visual;
                self.anchor = (self.buf.row, self.buf.col);
            }
            KeyCode::Char('V') => {
                self.mode = Mode::VisualLine;
                self.anchor = (self.buf.row, self.buf.col);
            }
            KeyCode::Enter => return EditorAction::FollowLink,
            KeyCode::Char(' ') => {
                self.buf.checkpoint();
                if !self.buf.toggle_task() {
                    return EditorAction::Status("no task on this line".into());
                }
            }
            _ => {}
        }
        EditorAction::None
    }

    fn operator_key(&mut self, op: char, key: KeyEvent) -> EditorAction {
        let count = self.take_count();
        let KeyCode::Char(c) = key.code else {
            return EditorAction::None;
        };
        match (op, c) {
            ('g', 'g') => {
                let row = count.saturating_sub(1);
                self.buf.goto_line(row);
            }
            ('g', 'd') => return EditorAction::FollowLink,
            ('g', 'm') => return EditorAction::Menu,
            ('d', 'd') => {
                self.buf.checkpoint();
                self.register = self.buf.yank_lines(self.buf.row, count);
                self.register_linewise = true;
                self.buf.delete_lines(self.buf.row, count);
            }
            ('y', 'y') => {
                self.register = self.buf.yank_lines(self.buf.row, count);
                self.register_linewise = true;
                return EditorAction::Status(format!("yanked {count} line(s)"));
            }
            ('d', 'w') | ('c', 'w') => {
                self.buf.checkpoint();
                let end = self.buf.word_end_col();
                self.register = self.buf.delete_to_col(end);
                self.register_linewise = false;
                if op == 'c' {
                    self.mode = Mode::Insert;
                }
            }
            ('d', '$') => {
                self.buf.checkpoint();
                self.register = self.buf.delete_to_line_end();
                self.register_linewise = false;
            }
            ('c', 'c') => {
                self.buf.checkpoint();
                self.register = self.buf.yank_lines(self.buf.row, count);
                self.register_linewise = true;
                let row = self.buf.row;
                self.buf.delete_lines(row, count);
                self.buf.lines.insert(row, String::new());
                self.buf.row = row;
                self.buf.col = 0;
                self.mode = Mode::Insert;
            }
            ('>', '>') => {
                self.buf.checkpoint();
                let row = self.buf.row;
                self.buf.shift_lines(row, row + count - 1, true);
            }
            ('<', '<') => {
                self.buf.checkpoint();
                let row = self.buf.row;
                self.buf.shift_lines(row, row + count - 1, false);
            }
            _ => {}
        }
        EditorAction::None
    }

    // ---- visual mode ---------------------------------------------------

    fn visual_key(&mut self, key: KeyEvent) -> EditorAction {
        let (start, end) = self.selection_rows().unwrap_or((0, 0));
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char('h') | KeyCode::Left => self.buf.move_left(),
            KeyCode::Char('l') | KeyCode::Right => self.buf.move_right(false),
            KeyCode::Char('j') | KeyCode::Down => self.buf.move_down(false),
            KeyCode::Char('k') | KeyCode::Up => self.buf.move_up(false),
            KeyCode::Char('w') => self.buf.next_word(),
            KeyCode::Char('b') => self.buf.prev_word(),
            KeyCode::Char('G') => self.buf.goto_line(self.buf.len()),
            KeyCode::Char('$') => self.buf.line_end(false),
            KeyCode::Char('0') => self.buf.line_start(),
            KeyCode::Char('y') => {
                self.register = self.buf.yank_lines(start, end - start + 1);
                self.register_linewise = true;
                self.mode = Mode::Normal;
                return EditorAction::Status(format!("yanked {} line(s)", end - start + 1));
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                self.buf.checkpoint();
                self.register = self.buf.yank_lines(start, end - start + 1);
                self.register_linewise = true;
                self.buf.delete_lines(start, end - start + 1);
                self.mode = Mode::Normal;
            }
            KeyCode::Char('c') => {
                self.buf.checkpoint();
                self.register = self.buf.yank_lines(start, end - start + 1);
                self.register_linewise = true;
                self.buf.delete_lines(start, end - start + 1);
                self.buf.lines.insert(start, String::new());
                self.buf.row = start;
                self.buf.col = 0;
                self.mode = Mode::Insert;
            }
            KeyCode::Char('>') => {
                self.buf.checkpoint();
                self.buf.shift_lines(start, end, true);
                self.mode = Mode::Normal;
            }
            KeyCode::Char('<') => {
                self.buf.checkpoint();
                self.buf.shift_lines(start, end, false);
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        EditorAction::None
    }

    /// The text currently selected, used when sending a selection to the assistant.
    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_rows()?;
        Some(self.buf.yank_lines(start, end - start + 1))
    }

    /// The `#anchor` of a markdown link under the cursor, if the cursor is
    /// inside one — `[Phase 1](#phase-1)` gives `phase-1`.
    ///
    /// Only same-note anchors. A `[text](path.md)` is a different thing and is
    /// not followed here.
    pub fn anchor_under_cursor(&self) -> Option<String> {
        anchor_at(self.buf.line(self.buf.row), self.buf.col)
    }

    /// The `[[link]]` under the cursor, if any.
    pub fn link_under_cursor(&self) -> Option<crate::vault::WikiLink> {
        let line = self.buf.line(self.buf.row);
        crate::vault::note::parse_wikilinks(line, self.buf.row)
            .into_iter()
            .find(|l| self.buf.col >= l.col && self.buf.col < l.col + l.len)
    }
}

/// Find `[text](#anchor)` spanning character `col`, returning the anchor.
///
/// Deliberately small: a full markdown link parser is not needed to answer
/// "is the cursor inside a link to a heading in this note".
pub fn anchor_at(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '[' {
            i += 1;
            continue;
        }
        // A `[[wikilink]]` is not a markdown link; leave it to the other path.
        if chars.get(i + 1) == Some(&'[') {
            i += 2;
            continue;
        }
        let Some(close) = chars[i..].iter().position(|c| *c == ']').map(|p| p + i) else {
            break;
        };
        if chars.get(close + 1) != Some(&'(') || chars.get(close + 2) != Some(&'#') {
            i = close + 1;
            continue;
        }
        let Some(paren) = chars[close..]
            .iter()
            .position(|c| *c == ')')
            .map(|p| p + close)
        else {
            break;
        };
        if col >= i && col <= paren {
            return Some(chars[close + 3..paren].iter().collect());
        }
        i = paren + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn press(ed: &mut Editor, keys: &str) {
        for c in keys.chars() {
            ed.on_key(key(c));
        }
    }

    fn editor(text: &str) -> Editor {
        Editor::new(Buffer::from_str(text))
    }

    #[test]
    fn dd_deletes_a_line_and_p_puts_it_back() {
        let mut ed = editor("one\ntwo\nthree");
        press(&mut ed, "jdd");
        assert_eq!(ed.buf.text(), "one\nthree");
        press(&mut ed, "P");
        assert_eq!(ed.buf.text(), "one\ntwo\nthree");
    }

    #[test]
    fn counts_apply_to_motions_and_operators() {
        let mut ed = editor("a\nb\nc\nd");
        press(&mut ed, "2j");
        assert_eq!(ed.buf.row, 2);
        press(&mut ed, "gg2dd");
        assert_eq!(ed.buf.text(), "c\nd");
    }

    #[test]
    fn zero_is_line_start_but_still_composes_as_a_count() {
        let mut ed = editor("hello\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk");
        press(&mut ed, "$");
        assert_eq!(ed.buf.col, 4);
        press(&mut ed, "0");
        assert_eq!(ed.buf.col, 0);
        press(&mut ed, "10G");
        assert_eq!(ed.buf.row, 9);
    }

    #[test]
    fn insert_mode_types_and_escapes() {
        let mut ed = editor("");
        press(&mut ed, "ihi");
        assert_eq!(ed.buf.text(), "hi");
        assert_eq!(ed.mode, Mode::Insert);
        ed.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn visual_line_delete_removes_the_range() {
        let mut ed = editor("a\nb\nc");
        press(&mut ed, "Vjd");
        assert_eq!(ed.buf.text(), "c");
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn dw_deletes_a_word() {
        let mut ed = editor("alpha beta");
        press(&mut ed, "dw");
        assert_eq!(ed.buf.text(), "beta");
    }

    #[test]
    fn gm_asks_for_the_context_menu() {
        let mut ed = editor("a line");
        press(&mut ed, "g");
        let action = ed.on_key(key('m'));
        assert_eq!(action, EditorAction::Menu);
    }

    #[test]
    fn enter_requests_link_navigation() {
        let mut ed = editor("go to [[Target]] now");
        press(&mut ed, "8l");
        let action = ed.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, EditorAction::FollowLink);
        assert_eq!(ed.link_under_cursor().unwrap().target, "Target");
    }

    #[test]
    fn an_anchor_link_is_found_from_anywhere_inside_it() {
        let line = "- [Phase 1: Foundations](#phase-1-foundations) and more";
        // Every column from the opening bracket to the closing paren.
        for col in 2..=44 {
            assert_eq!(
                anchor_at(line, col).as_deref(),
                Some("phase-1-foundations"),
                "col {col}"
            );
        }
        assert_eq!(anchor_at(line, 1), None, "before the link");
        assert_eq!(anchor_at(line, 50), None, "after the link");
    }

    #[test]
    fn a_wikilink_is_not_mistaken_for_an_anchor() {
        assert_eq!(anchor_at("see [[Note#Heading]] here", 8), None);
        // A wikilink before a real anchor must not swallow it.
        let line = "[[Note]] then [x](#y)";
        assert_eq!(anchor_at(line, 17).as_deref(), Some("y"));
    }

    #[test]
    fn a_link_to_a_file_is_not_an_anchor() {
        assert_eq!(anchor_at("[the note](other.md)", 5), None);
        assert_eq!(anchor_at("[a site](https://example.com)", 5), None);
    }

    #[test]
    fn unclosed_link_syntax_does_not_hang_or_panic() {
        for line in ["[unclosed", "[a](", "[a](#", "[[", "]("] {
            let _ = anchor_at(line, 0);
            let _ = anchor_at(line, line.chars().count().saturating_sub(1));
        }
    }

    #[test]
    fn cursor_outside_a_link_finds_nothing() {
        let mut ed = editor("[[A]] tail");
        press(&mut ed, "$");
        assert!(ed.link_under_cursor().is_none());
    }

    #[test]
    fn space_toggles_a_task() {
        let mut ed = editor("- [ ] thing");
        press(&mut ed, " ");
        assert_eq!(ed.buf.line(0), "- [x] thing");
    }

    #[test]
    fn shift_operators_indent_lines() {
        let mut ed = editor("a\nb");
        press(&mut ed, ">>");
        assert_eq!(ed.buf.line(0), "  a");
        press(&mut ed, "<<");
        assert_eq!(ed.buf.line(0), "a");
    }

    #[test]
    fn scroll_follows_the_cursor() {
        let mut ed = editor(&"x\n".repeat(50));
        ed.buf.goto_line(40);
        ed.sync_scroll(10);
        assert!(ed.scroll <= 40 && 40 < ed.scroll + 10);
        ed.buf.goto_line(0);
        ed.sync_scroll(10);
        assert_eq!(ed.scroll, 0);
    }
}
