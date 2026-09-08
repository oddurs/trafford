/// A line-oriented text buffer with char-indexed cursor and undo history.
/// All column indices are character offsets, never bytes.
#[derive(Debug, Clone)]
pub struct Buffer {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    /// Column the cursor wants to return to during vertical motion.
    pub goal_col: usize,
    pub dirty: bool,
    /// The file used CRLF endings, so writing it back must too. Stripping them
    /// on load keeps the lone `\r` out of the renderer, where it would reset
    /// the terminal cursor to column zero and tear the layout apart.
    crlf: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Set while an insert session is open so the whole session undoes at once.
    insert_open: bool,
}

#[derive(Debug, Clone)]
struct Snapshot {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

impl Default for Buffer {
    fn default() -> Self {
        Buffer::from_text("")
    }
}

impl Buffer {
    pub fn from_text(text: &str) -> Buffer {
        // A file counts as CRLF if every one of its line breaks is one.
        let breaks = text.matches('\n').count();
        let crlf = breaks > 0 && text.matches("\r\n").count() == breaks;
        let mut lines: Vec<String> = text
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Buffer {
            lines,
            row: 0,
            col: 0,
            goal_col: 0,
            dirty: false,
            crlf,
            undo: Vec::new(),
            redo: Vec::new(),
            insert_open: false,
        }
    }

    /// The buffer as it should be written to disk, in the file's own endings.
    pub fn text(&self) -> String {
        self.lines.join(if self.crlf { "\r\n" } else { "\n" })
    }

    pub fn line(&self, row: usize) -> &str {
        self.lines.get(row).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.line(row).chars().count()
    }

    /// How many lines the buffer holds. Never zero: an empty buffer is one
    /// empty line, which is why there is no `is_empty` to go with it.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn word_count(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.split_whitespace().count())
            .sum()
    }

    // ---- history -------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
        }
    }

    /// Record a checkpoint before a mutation. Consecutive insert-mode edits
    /// collapse into the single checkpoint taken when insert mode opened.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > 512 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn begin_insert(&mut self) {
        if !self.insert_open {
            self.checkpoint();
            self.insert_open = true;
        }
    }

    pub fn end_insert(&mut self) {
        self.insert_open = false;
    }

    pub fn undo(&mut self) -> bool {
        self.insert_open = false;
        match self.undo.pop() {
            Some(snap) => {
                self.redo.push(self.snapshot());
                self.restore(snap);
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(snap) => {
                self.undo.push(self.snapshot());
                self.restore(snap);
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    fn restore(&mut self, snap: Snapshot) {
        self.lines = snap.lines;
        self.row = snap.row.min(self.lines.len().saturating_sub(1));
        self.col = snap.col.min(self.line_len(self.row));
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    // ---- cursor --------------------------------------------------------

    /// Clamp the cursor into the buffer. In normal mode the cursor rests on a
    /// character, so it stops one short of the end of the line.
    pub fn clamp(&mut self, insert_mode: bool) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = self.row.min(self.lines.len() - 1);
        let len = self.line_len(self.row);
        let max = if insert_mode {
            len
        } else {
            len.saturating_sub(1)
        };
        self.col = self.col.min(max);
    }

    pub fn move_left(&mut self) {
        self.col = self.col.saturating_sub(1);
        self.goal_col = self.col;
    }

    pub fn move_right(&mut self, insert_mode: bool) {
        let len = self.line_len(self.row);
        let max = if insert_mode {
            len
        } else {
            len.saturating_sub(1)
        };
        if self.col < max {
            self.col += 1;
        }
        self.goal_col = self.col;
    }

    pub fn move_up(&mut self, insert_mode: bool) {
        if self.row > 0 {
            self.row -= 1;
            self.snap_to_goal(insert_mode);
        }
    }

    pub fn move_down(&mut self, insert_mode: bool) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.snap_to_goal(insert_mode);
        }
    }

    fn snap_to_goal(&mut self, insert_mode: bool) {
        let len = self.line_len(self.row);
        let max = if insert_mode {
            len
        } else {
            len.saturating_sub(1)
        };
        self.col = self.goal_col.min(max);
    }

    pub fn line_start(&mut self) {
        self.col = 0;
        self.goal_col = 0;
    }

    pub fn first_non_blank(&mut self) {
        let n = self
            .line(self.row)
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        self.col = n.min(self.line_len(self.row).saturating_sub(1));
        self.goal_col = self.col;
    }

    pub fn line_end(&mut self, insert_mode: bool) {
        let len = self.line_len(self.row);
        self.col = if insert_mode {
            len
        } else {
            len.saturating_sub(1)
        };
        self.goal_col = self.col;
    }

    pub fn goto_line(&mut self, row: usize) {
        self.row = row.min(self.lines.len().saturating_sub(1));
        self.col = 0;
        self.goal_col = 0;
    }

    /// Move to the start of the next word, wrapping across lines.
    pub fn next_word(&mut self) {
        let mut row = self.row;
        let mut col = self.col;
        let class = |c: char| {
            if c.is_whitespace() {
                0
            } else if c.is_alphanumeric() || c == '_' {
                1
            } else {
                2
            }
        };
        let chars: Vec<char> = self.line(row).chars().collect();
        let start_class = chars.get(col).copied().map(class).unwrap_or(0);
        // Skip the rest of the current run, then any whitespace.
        while col < chars.len() && class(chars[col]) == start_class && start_class != 0 {
            col += 1;
        }
        loop {
            let chars: Vec<char> = self.line(row).chars().collect();
            while col < chars.len() && chars[col].is_whitespace() {
                col += 1;
            }
            if col < chars.len() {
                break;
            }
            if row + 1 >= self.lines.len() {
                col = chars.len().saturating_sub(1);
                break;
            }
            row += 1;
            col = 0;
            if !self.line(row).is_empty() && !self.line(row).starts_with(char::is_whitespace) {
                break;
            }
        }
        self.row = row;
        self.col = col.min(self.line_len(row).saturating_sub(1));
        self.goal_col = self.col;
    }

    /// Move to the start of the previous word.
    pub fn prev_word(&mut self) {
        let mut row = self.row;
        let mut col = self.col;
        loop {
            if col == 0 {
                if row == 0 {
                    break;
                }
                row -= 1;
                col = self.line_len(row);
                if col == 0 {
                    break;
                }
            }
            let chars: Vec<char> = self.line(row).chars().collect();
            let mut i = col;
            while i > 0 && chars.get(i - 1).map(|c| c.is_whitespace()).unwrap_or(false) {
                i -= 1;
            }
            if i == 0 {
                col = 0;
                continue;
            }
            let alnum = chars[i - 1].is_alphanumeric() || chars[i - 1] == '_';
            while i > 0 {
                let c = chars[i - 1];
                let same = (c.is_alphanumeric() || c == '_') == alnum && !c.is_whitespace();
                if !same {
                    break;
                }
                i -= 1;
            }
            col = i;
            break;
        }
        self.row = row;
        self.col = col;
        self.goal_col = col;
    }

    /// Character offset of the end of the current word, for `dw`-style ops.
    pub fn word_end_col(&self) -> usize {
        let chars: Vec<char> = self.line(self.row).chars().collect();
        let mut i = self.col;
        if i >= chars.len() {
            return i;
        }
        let alnum = chars[i].is_alphanumeric() || chars[i] == '_';
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() || ((c.is_alphanumeric() || c == '_') != alnum) {
                break;
            }
            i += 1;
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        i
    }

    // ---- editing -------------------------------------------------------

    pub fn insert_char(&mut self, c: char) {
        let byte = self.byte_at(self.row, self.col);
        self.lines[self.row].insert(byte, c);
        self.col += 1;
        self.goal_col = self.col;
        self.dirty = true;
    }

    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            if c == '\n' {
                self.insert_newline();
            } else {
                self.insert_char(c);
            }
        }
    }

    pub fn insert_newline(&mut self) {
        let byte = self.byte_at(self.row, self.col);
        let rest = self.lines[self.row].split_off(byte);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
        self.goal_col = 0;
        self.dirty = true;
    }

    /// Newline that carries list markers and indentation forward, so writing
    /// bullet lists in a note does not need manual re-indenting.
    pub fn insert_newline_smart(&mut self) {
        let prefix = continuation_prefix(self.line(self.row));
        // Enter on an empty list item ends the list: the marker is cleared in
        // place rather than a fresh empty bullet being opened below it.
        if !prefix.is_empty() && self.line(self.row).trim() == prefix.trim() {
            self.lines[self.row].clear();
            self.col = 0;
            self.goal_col = 0;
            self.dirty = true;
            return;
        }
        self.insert_newline();
        if !prefix.is_empty() {
            self.insert_str(&prefix);
        }
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            let byte = self.byte_at(self.row, self.col - 1);
            self.lines[self.row].remove(byte);
            self.col -= 1;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.line_len(self.row);
            self.lines[self.row].push_str(&current);
        } else {
            return;
        }
        self.goal_col = self.col;
        self.dirty = true;
    }

    /// Delete the character under the cursor (`x`). Returns what was removed.
    pub fn delete_char(&mut self) -> Option<char> {
        if self.col >= self.line_len(self.row) {
            return None;
        }
        let byte = self.byte_at(self.row, self.col);
        let c = self.lines[self.row].remove(byte);
        self.dirty = true;
        Some(c)
    }

    /// Delete whole lines starting at `row`. Returns the removed text.
    pub fn delete_lines(&mut self, row: usize, count: usize) -> String {
        let end = (row + count).min(self.lines.len());
        if row >= self.lines.len() {
            return String::new();
        }
        let removed: Vec<String> = self.lines.drain(row..end).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = row.min(self.lines.len() - 1);
        self.col = 0;
        self.dirty = true;
        removed.join("\n")
    }

    /// Delete from the cursor to `to_col` on the current line.
    pub fn delete_to_col(&mut self, to_col: usize) -> String {
        let from = self.col.min(to_col);
        let to = self.col.max(to_col).min(self.line_len(self.row));
        if from == to {
            return String::new();
        }
        let start = self.byte_at(self.row, from);
        let end = self.byte_at(self.row, to);
        let removed = self.lines[self.row][start..end].to_string();
        self.lines[self.row].replace_range(start..end, "");
        self.col = from;
        self.dirty = true;
        removed
    }

    /// Delete from the cursor to the end of the line (`D`).
    pub fn delete_to_line_end(&mut self) -> String {
        let end = self.line_len(self.row);
        self.delete_to_col(end)
    }

    pub fn open_below(&mut self) {
        let prefix = continuation_prefix(self.line(self.row));
        self.lines.insert(self.row + 1, prefix.clone());
        self.row += 1;
        self.col = prefix.chars().count();
        self.goal_col = self.col;
        self.dirty = true;
    }

    pub fn open_above(&mut self) {
        let prefix = continuation_prefix(self.line(self.row));
        self.lines.insert(self.row, prefix.clone());
        self.col = prefix.chars().count();
        self.goal_col = self.col;
        self.dirty = true;
    }

    pub fn yank_lines(&self, row: usize, count: usize) -> String {
        let end = (row + count).min(self.lines.len());
        if row >= end {
            return String::new();
        }
        self.lines[row..end].join("\n")
    }

    /// Paste `text` after the current line if it is line-wise, else inline.
    pub fn paste(&mut self, text: &str, linewise: bool, after: bool) {
        if text.is_empty() {
            return;
        }
        if linewise {
            let new_lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
            let at = if after { self.row + 1 } else { self.row };
            for (i, l) in new_lines.iter().enumerate() {
                self.lines.insert(at + i, l.clone());
            }
            self.row = at;
            self.col = 0;
        } else {
            if after && self.col < self.line_len(self.row) {
                self.col += 1;
            }
            self.insert_str(text);
        }
        self.goal_col = self.col;
        self.dirty = true;
    }

    /// Indent or outdent the given line range by two spaces.
    pub fn shift_lines(&mut self, from: usize, to: usize, right: bool) {
        for row in from..=to.min(self.lines.len().saturating_sub(1)) {
            if right {
                self.lines[row].insert_str(0, "  ");
            } else {
                let trimmed = self.lines[row]
                    .strip_prefix("  ")
                    .or_else(|| self.lines[row].strip_prefix('\t'))
                    .or_else(|| self.lines[row].strip_prefix(' '))
                    .map(|s| s.to_string());
                if let Some(t) = trimmed {
                    self.lines[row] = t;
                }
            }
        }
        self.dirty = true;
    }

    /// Toggle a `- [ ]` / `- [x]` checkbox on the current line.
    pub fn toggle_task(&mut self) -> bool {
        let line = self.lines[self.row].clone();
        let Some(pos) = line.find("- [") else {
            return false;
        };
        let marker: Vec<char> = line[pos..].chars().take(5).collect();
        if marker.len() < 5 || marker[4] != ']' {
            return false;
        }
        let replacement = if marker[3] == ' ' { "- [x]" } else { "- [ ]" };
        let end = pos
            + line[pos..]
                .char_indices()
                .nth(5)
                .map(|(i, _)| i)
                .unwrap_or(5);
        self.lines[self.row].replace_range(pos..end, replacement);
        self.dirty = true;
        true
    }

    fn byte_at(&self, row: usize, col: usize) -> usize {
        self.lines[row]
            .char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(self.lines[row].len())
    }
}

/// The prefix a new line should inherit: leading whitespace plus any list
/// marker (`- `, `* `, `1. `, `- [ ] `, `> `).
fn continuation_prefix(line: &str) -> String {
    let indent: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let rest = &line[indent.len()..];
    for marker in ["- [ ] ", "- [x] ", "- ", "* ", "+ ", "> "] {
        if rest.starts_with(marker) {
            // A checked box continues as an unchecked one.
            let marker = if marker == "- [x] " { "- [ ] " } else { marker };
            return format!("{indent}{marker}");
        }
    }
    // Ordered lists: increment the number.
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && rest[digits.len()..].starts_with(". ") {
        if let Ok(n) = digits.parse::<usize>() {
            return format!("{indent}{}. ", n + 1);
        }
    }
    indent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserting_and_undoing_round_trips() {
        let mut b = Buffer::from_text("hello");
        b.col = 5;
        b.begin_insert();
        b.insert_str(" world");
        assert_eq!(b.text(), "hello world");
        b.end_insert();
        assert!(b.undo());
        assert_eq!(b.text(), "hello");
        assert!(b.redo());
        assert_eq!(b.text(), "hello world");
    }

    #[test]
    fn insert_session_undoes_as_one_unit() {
        let mut b = Buffer::from_text("");
        b.begin_insert();
        for c in "abc".chars() {
            b.insert_char(c);
        }
        b.end_insert();
        b.undo();
        assert_eq!(b.text(), "");
    }

    #[test]
    fn newline_continues_list_markers() {
        let mut b = Buffer::from_text("  - item");
        b.row = 0;
        b.col = 8;
        b.insert_newline_smart();
        assert_eq!(b.line(1), "  - ");
    }

    #[test]
    fn newline_on_empty_list_item_clears_it() {
        let mut b = Buffer::from_text("- ");
        b.col = 2;
        b.insert_newline_smart();
        assert_eq!(b.text(), "");
        assert_eq!(b.line_count(), 1);
        assert_eq!(b.col, 0);
    }

    #[test]
    fn ordered_lists_increment() {
        let mut b = Buffer::from_text("3. third");
        b.col = 8;
        b.insert_newline_smart();
        assert_eq!(b.line(1), "4. ");
    }

    #[test]
    fn crlf_files_load_without_the_carriage_returns() {
        let b = Buffer::from_text("one\r\ntwo\r\n");
        assert_eq!(b.line(0), "one");
        assert_eq!(b.line(1), "two");
        assert_eq!(b.line_len(0), 3, "the \\r must not count as a column");
    }

    #[test]
    fn crlf_files_are_written_back_with_crlf() {
        let text = "one\r\ntwo\r\n";
        assert_eq!(
            Buffer::from_text(text).text(),
            text,
            "round trip must be exact"
        );
    }

    #[test]
    fn lf_files_stay_lf() {
        let text = "one\ntwo\n";
        assert_eq!(Buffer::from_text(text).text(), text);
    }

    #[test]
    fn a_mixed_file_is_normalised_to_lf() {
        // Mixed endings are already broken; picking one is better than
        // preserving the mess, and LF is the one git wants.
        let b = Buffer::from_text("one\r\ntwo\nthree\r\n");
        assert_eq!(b.text(), "one\ntwo\nthree\n");
    }

    #[test]
    fn editing_a_crlf_file_keeps_its_endings() {
        let mut b = Buffer::from_text("one\r\ntwo\r\n");
        b.row = 0;
        b.col = 3;
        b.insert_str("!");
        assert_eq!(b.text(), "one!\r\ntwo\r\n");
    }

    #[test]
    fn unicode_columns_are_character_based() {
        let mut b = Buffer::from_text("héllo");
        b.col = 2;
        b.insert_char('X');
        assert_eq!(b.text(), "héXllo");
        b.col = 0;
        assert_eq!(b.delete_char(), Some('h'));
    }

    #[test]
    fn word_motions_cross_lines() {
        let mut b = Buffer::from_text("alpha beta\ngamma");
        b.next_word();
        assert_eq!((b.row, b.col), (0, 6));
        b.next_word();
        assert_eq!(b.row, 1);
        b.prev_word();
        assert_eq!((b.row, b.col), (0, 6));
    }

    #[test]
    fn delete_lines_returns_text_and_keeps_buffer_nonempty() {
        let mut b = Buffer::from_text("a\nb");
        let removed = b.delete_lines(0, 2);
        assert_eq!(removed, "a\nb");
        assert_eq!(b.line_count(), 1);
        assert_eq!(b.text(), "");
    }

    #[test]
    fn paste_linewise_inserts_after_current_row() {
        let mut b = Buffer::from_text("one\ntwo");
        b.row = 0;
        b.paste("new", true, true);
        assert_eq!(b.text(), "one\nnew\ntwo");
    }

    #[test]
    fn toggle_task_flips_the_checkbox() {
        let mut b = Buffer::from_text("- [ ] do it");
        assert!(b.toggle_task());
        assert_eq!(b.line(0), "- [x] do it");
        assert!(b.toggle_task());
        assert_eq!(b.line(0), "- [ ] do it");
    }

    #[test]
    fn shift_lines_indents_and_outdents() {
        let mut b = Buffer::from_text("a\nb");
        b.shift_lines(0, 1, true);
        assert_eq!(b.text(), "  a\n  b");
        b.shift_lines(0, 1, false);
        assert_eq!(b.text(), "a\nb");
    }

    #[test]
    fn clamp_respects_normal_mode_last_column() {
        let mut b = Buffer::from_text("abc");
        b.col = 99;
        b.clamp(false);
        assert_eq!(b.col, 2);
        b.col = 99;
        b.clamp(true);
        assert_eq!(b.col, 3);
    }
}
