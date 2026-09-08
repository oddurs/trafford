//! Where a buffer position is on screen, and which buffer position a screen
//! point refers to.
//!
//! Without wrapping these are trivial — one buffer line is one screen row —
//! and the editor computed them inline in six places. Once a line can fold
//! into several rows they stop being trivial, and six inline computations
//! become six chances to disagree. `CLAUDE.md` already says why that matters,
//! learned from a click landing a row off:
//!
//! > Never compute a second, parallel idea of the layout — it will diverge
//! > silently and clicks will land one row off.
//!
//! So there is one of them. With `wrap` off it produces exactly the mapping
//! the editor had before, which is what lets it replace that code rather than
//! sit beside it.

use unicode_width::UnicodeWidthChar;

/// One row of screen, and the slice of a buffer line it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRow {
    /// The buffer line this row is part of.
    pub line: usize,
    /// Character offset into that line where this row starts.
    pub start: usize,
    /// How many characters of the line this row shows.
    pub len: usize,
    /// Display columns this row is pushed right by, so a wrapped list item
    /// keeps its text aligned under the text above rather than under the
    /// marker.
    pub indent: usize,
}

impl VisualRow {
    /// The offset one past the last character on this row.
    pub fn end(&self) -> usize {
        self.start + self.len
    }

    /// Whether this is the first row of its buffer line.
    pub fn is_first(&self) -> bool {
        self.start == 0
    }
}

/// The screen rows a buffer produces at a given width.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    rows: Vec<VisualRow>,
    /// For each buffer line, the index of its first row in `rows`.
    first: Vec<usize>,
}

impl Layout {
    /// Lay `lines` out for a pane `width` columns wide.
    ///
    /// `width` is the space available to text, with the gutter already taken
    /// off. A width of zero, or `wrap` false, gives one row per line — the
    /// behaviour the editor had before this existed.
    pub fn new(lines: &[String], width: usize, wrap: bool) -> Layout {
        Layout::with_widths(lines, &|_| width, wrap)
    }

    /// The same, with a width chosen per line.
    ///
    /// The reading view needs this: prose is held to a measure because prose is
    /// unreadable stretched wide, and a table is not prose. Folding a table at
    /// the measure does not wrap it, it *truncates* it — the cells are already
    /// sized — so the same rule that helps a paragraph destroys a table.
    pub fn with_widths(lines: &[String], width_of: &dyn Fn(usize) -> usize, wrap: bool) -> Layout {
        let mut rows = Vec::with_capacity(lines.len());
        let mut first = Vec::with_capacity(lines.len());
        for (line, text) in lines.iter().enumerate() {
            first.push(rows.len());
            let width = width_of(line);
            if !wrap || width == 0 {
                rows.push(VisualRow {
                    line,
                    start: 0,
                    len: text.chars().count(),
                    indent: 0,
                });
                continue;
            }
            let indent = continuation_indent(text).min(width.saturating_sub(1));
            for (n, (start, len)) in fold(text, width, indent).into_iter().enumerate() {
                rows.push(VisualRow {
                    line,
                    start,
                    len,
                    indent: if n == 0 { 0 } else { indent },
                });
            }
        }
        Layout { rows, first }
    }

    /// Every row, in screen order. Used by the tests and by anything that
    /// wants to walk the fold rather than ask about one position.
    #[cfg(test)]
    pub fn rows(&self) -> &[VisualRow] {
        &self.rows
    }

    /// How many screen rows the buffer occupies once wrapped. Never zero,
    /// since a buffer is never zero lines — hence no `is_empty`.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn row(&self, visual: usize) -> Option<&VisualRow> {
        self.rows.get(visual)
    }

    /// The first screen row of a buffer line.
    pub fn first_of(&self, line: usize) -> usize {
        self.first.get(line).copied().unwrap_or(0)
    }

    /// Where a buffer position is drawn: which screen row, and how many
    /// display columns into it.
    ///
    /// Takes the lines because widths are a property of the text, not of the
    /// fold — the layout knows where the breaks are and nothing else.
    pub fn visual_of(&self, lines: &[String], line: usize, col: usize) -> (usize, usize) {
        let from = self.first_of(line);
        let mut chosen = from;
        for (i, row) in self.rows.iter().enumerate().skip(from) {
            if row.line != line {
                break;
            }
            chosen = i;
            // A cursor one past the last character belongs to this row rather
            // than the start of the next.
            if col < row.end() || (col == row.end() && self.is_last_of_line(i)) {
                break;
            }
        }
        let Some(row) = self.rows.get(chosen) else {
            return (0, 0);
        };
        let text = lines.get(row.line).map(|s| s.as_str()).unwrap_or("");
        let within = col.saturating_sub(row.start).min(row.len);
        let width: usize = text
            .chars()
            .skip(row.start)
            .take(within)
            .filter_map(|c| c.width())
            .sum();
        (chosen, row.indent + width)
    }

    fn is_last_of_line(&self, visual: usize) -> bool {
        match self.rows.get(visual + 1) {
            Some(next) => next.line != self.rows[visual].line,
            None => true,
        }
    }

    /// The buffer position a screen point refers to.
    pub fn source_of(&self, lines: &[String], visual: usize, column: usize) -> (usize, usize) {
        let Some(row) = self.rows.get(visual.min(self.rows.len().saturating_sub(1))) else {
            return (0, 0);
        };
        let text = lines.get(row.line).map(|s| s.as_str()).unwrap_or("");
        let target = column.saturating_sub(row.indent);
        let mut used = 0usize;
        let mut offset = 0usize;
        for c in text.chars().skip(row.start).take(row.len) {
            let w = c.width().unwrap_or(0);
            if used + w > target {
                break;
            }
            used += w;
            offset += 1;
        }
        (row.line, row.start + offset)
    }
}

/// Where a line's continuation rows should start, so a wrapped bullet or quote
/// lines up under its own text instead of under its marker.
fn continuation_indent(line: &str) -> usize {
    let leading: usize = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    let rest = line.trim_start();
    // `▎ ` is the bar preview draws down the left of a callout. It behaves
    // like `> ` for this purpose: the text after it is the content, and a
    // wrapped line should line up under that rather than under the bar.
    for marker in ["- [ ] ", "- [x] ", "- ", "* ", "+ ", "> ", "▎ "] {
        if rest.starts_with(marker) {
            return leading + marker.chars().count();
        }
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && rest[digits..].starts_with(". ") {
        return leading + digits + 2;
    }
    leading
}

/// Break a line into `(start, len)` character runs that each fit `width`
/// display columns, preferring word boundaries.
///
/// The first run gets the full width; later runs lose `indent` to the hanging
/// indent. A run is never empty, so a line always produces at least one row and
/// the fold cannot loop.
fn fold(text: &str, width: usize, indent: usize) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![(0, 0)];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let room = if out.is_empty() {
            width
        } else {
            width.saturating_sub(indent).max(1)
        };
        let mut used = 0usize;
        let mut at = start;
        let mut last_break = None;
        while at < chars.len() {
            let w = chars[at].width().unwrap_or(0);
            if used + w > room {
                break;
            }
            used += w;
            at += 1;
            if chars[at - 1] == ' ' {
                last_break = Some(at);
            }
        }
        if at >= chars.len() {
            out.push((start, chars.len() - start));
            return out;
        }
        // Break after the last space that fits, unless that would make no
        // progress — a single word wider than the pane is split where it is.
        let end = match last_break {
            Some(b) if b > start => b,
            _ => at.max(start + 1),
        };
        out.push((start, end - start));
        start = end;
    }
    if out.is_empty() {
        out.push((0, 0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(|s| s.to_string()).collect()
    }

    /// The property the whole type exists for. If a position survives a trip
    /// to the screen and back, the caret and the mouse cannot disagree.
    #[test]
    fn every_position_round_trips() {
        let lines = lines(
            "# A heading\n\
             short\n\
             a much longer line of ordinary prose that will certainly need folding at these widths\n\
             - a list item whose text runs on far enough to wrap more than once, twice even\n\
             日本語のテキストは全角なので幅が二倍になります\n\
             mixed 日本 and ascii ✨ with emoji\n\
             \n\
             supercalifragilisticexpialidociousandthensomemorewithoutanyspacesatall",
        );
        for width in [8usize, 13, 20, 40, 200] {
            for wrap in [false, true] {
                let layout = Layout::new(&lines, width, wrap);
                for (line, text) in lines.iter().enumerate() {
                    for col in 0..=text.chars().count() {
                        let (visual, column) = layout.visual_of(&lines, line, col);
                        let back = layout.source_of(&lines, visual, column);
                        assert_eq!(
                            back,
                            (line, col),
                            "width {width} wrap {wrap}: ({line},{col}) ->                              visual ({visual},{column}) -> {back:?}"
                        );
                    }
                }
            }
        }
    }

    /// With wrapping off the layout must be exactly what the editor had
    /// before, or replacing that code changes behaviour by accident.
    #[test]
    fn without_wrapping_each_line_is_one_row() {
        let lines = lines("one\ntwo is longer\nthree");
        let layout = Layout::new(&lines, 4, false);
        assert_eq!(layout.row_count(), 3);
        for (i, row) in layout.rows().iter().enumerate() {
            assert_eq!(row.line, i);
            assert_eq!(row.start, 0);
            assert_eq!(row.indent, 0);
            assert_eq!(row.len, lines[i].chars().count());
        }
    }

    #[test]
    fn a_long_line_folds_on_word_boundaries() {
        let lines = lines("the quick brown fox jumps");
        let layout = Layout::new(&lines, 10, true);
        let text: Vec<String> = layout
            .rows()
            .iter()
            .map(|r| lines[r.line].chars().skip(r.start).take(r.len).collect())
            .collect();
        assert!(text.len() > 1);
        for row in &text {
            assert!(row.chars().count() <= 10, "{row:?} is too wide");
        }
        assert_eq!(text.concat(), "the quick brown fox jumps");
    }

    /// A word wider than the pane has to be split somewhere, or the fold makes
    /// no progress and loops.
    #[test]
    fn a_word_wider_than_the_pane_is_split() {
        let lines = lines(&"x".repeat(25));
        let layout = Layout::new(&lines, 10, true);
        assert_eq!(layout.row_count(), 3);
        assert_eq!(layout.rows()[0].len, 10);
    }

    #[test]
    fn a_wrapped_list_item_hangs_under_its_text() {
        let lines = lines("- an item long enough to wrap onto another row");
        let layout = Layout::new(&lines, 20, true);
        assert!(layout.row_count() > 1);
        assert_eq!(
            layout.rows()[0].indent,
            0,
            "the first row carries the marker"
        );
        assert_eq!(layout.rows()[1].indent, 2, "continuations clear '- '");
    }

    #[test]
    fn an_empty_line_still_produces_a_row() {
        let lines = lines("a\n\nb");
        let layout = Layout::new(&lines, 10, true);
        assert_eq!(layout.row_count(), 3);
        assert_eq!(layout.rows()[1].len, 0);
        // And the cursor can sit on it.
        assert_eq!(layout.visual_of(&lines, 1, 0), (1, 0));
    }

    #[test]
    fn wide_characters_are_measured_in_columns() {
        let lines = lines("日本語");
        let layout = Layout::new(&lines, 40, true);
        // Three characters, six columns.
        assert_eq!(layout.visual_of(&lines, 0, 3), (0, 6));
        assert_eq!(layout.source_of(&lines, 0, 6), (0, 3));
        // A click halfway into a wide character lands on it, not past it.
        assert_eq!(layout.source_of(&lines, 0, 3), (0, 1));
    }

    #[test]
    fn a_pane_with_no_room_does_not_loop() {
        let lines = lines("some text");
        for width in [0usize, 1] {
            let layout = Layout::new(&lines, width, true);
            assert!(layout.row_count() > 0);
            assert!(layout.row_count() <= "some text".len() + 1);
        }
    }

    #[test]
    fn first_of_points_at_the_first_row_of_each_line() {
        let lines = lines("a long line that folds several times over\nshort");
        let layout = Layout::new(&lines, 10, true);
        let second = layout.first_of(1);
        assert_eq!(layout.rows()[second].line, 1);
        assert_eq!(layout.rows()[second].start, 0);
    }
}
