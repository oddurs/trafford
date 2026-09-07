//! Markdown tables, measured and drawn.
//!
//! Only preview uses this. The editor shows the pipes, because the editor
//! shows the file — which is what #0017 settled, and why there is no reveal,
//! no toggle and no per-block state here.
//!
//! Measuring is in display columns, never characters: a table of CJK is
//! exactly the case a naive implementation gets wrong, and it gets it wrong
//! silently, by drawing a grid whose rules do not line up.

use super::markdown::{Link, Rendered, Renderer};
use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

/// The narrowest a column may be squeezed before the table is not worth
/// drawing. Two columns of content plus an ellipsis is already unreadable;
/// below that it is noise with borders around it.
const MIN_COLUMN: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub head: Vec<String>,
    pub aligns: Vec<Align>,
    pub rows: Vec<Vec<String>>,
    /// How many source lines the block occupies, header and separator included.
    pub height: usize,
}

/// Split a table row into cells.
///
/// `\|` is an escaped pipe and does not end a cell — which is not a nicety:
/// Obsidian writes `[[Note\|alias]]` inside tables, and splitting on it turns
/// one cell into two and the whole table ragged.
fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cur.push('|');
            }
            '|' => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// `| --- | :--: |` and friends: the row that makes a table a table.
fn separator(line: &str) -> Option<Vec<Align>> {
    if !line.contains('|') {
        return None;
    }
    let parts = cells(line);
    if parts.is_empty() {
        return None;
    }
    let mut aligns = Vec::with_capacity(parts.len());
    for part in &parts {
        let p = part.trim();
        let left = p.starts_with(':');
        let right = p.ends_with(':');
        let dashes = p.trim_matches(':');
        if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
            return None;
        }
        aligns.push(match (left, right) {
            (true, true) => Align::Center,
            (false, true) => Align::Right,
            _ => Align::Left,
        });
    }
    Some(aligns)
}

/// Recognise a table beginning at `at`, or return `None`.
///
/// A ragged row — one with a different number of cells from the header — makes
/// the whole block not a table, so it falls back to its source rather than
/// being guessed at. Guessing means inventing a cell the author did not write.
pub fn parse(lines: &[String], at: usize) -> Option<Table> {
    let header = lines.get(at)?;
    if !header.contains('|') {
        return None;
    }
    let aligns = separator(lines.get(at + 1)?)?;
    let head = cells(header);
    if head.len() != aligns.len() || head.is_empty() {
        return None;
    }

    let mut rows = Vec::new();
    let mut i = at + 2;
    while let Some(line) = lines.get(i) {
        if line.trim().is_empty() || !line.contains('|') {
            break;
        }
        let row = cells(line);
        if row.len() != head.len() {
            return None;
        }
        rows.push(row);
        i += 1;
    }

    Some(Table {
        head,
        aligns,
        rows,
        height: i - at,
    })
}

/// Column widths that fit `width` display columns, or `None` if they cannot.
///
/// Natural widths first; when they do not fit, the widest column gives up a
/// column at a time. Taking it from the widest is what keeps a single long
/// cell from squeezing every other column to nothing — the failure the naive
/// "scale everything proportionally" produces.
fn measure(table: &Table, width: usize) -> Option<Vec<usize>> {
    let n = table.head.len();
    let overhead = 3 * n + 1;
    let budget = width.checked_sub(overhead)?;
    if budget < n * MIN_COLUMN {
        return None;
    }

    let mut widths: Vec<usize> = table
        .head
        .iter()
        .enumerate()
        .map(|(i, h)| {
            table
                .rows
                .iter()
                .filter_map(|r| r.get(i))
                .chain(std::iter::once(h))
                .map(|c| c.width())
                .max()
                .unwrap_or(1)
                .max(1)
        })
        .collect();

    let mut total: usize = widths.iter().sum();
    while total > budget {
        let (widest, _) = widths
            .iter()
            .enumerate()
            .max_by_key(|(i, w)| (**w, std::cmp::Reverse(*i)))?;
        if widths[widest] <= MIN_COLUMN {
            break;
        }
        widths[widest] -= 1;
        total -= 1;
    }
    (total <= budget).then_some(widths)
}

/// Take at most `width` display columns of a rendered cell, marking the cut.
///
/// The ellipsis is the point: a cell that was silently shortened reads as the
/// whole value, and a table of truncated numbers is worse than no table.
fn fit(cell: &Rendered, width: usize) -> (Vec<Span<'static>>, usize) {
    let full = cell.text.width();
    if full <= width {
        return (cell.slice(0, cell.text.chars().count()), full);
    }
    let room = width.saturating_sub(1);
    let mut used = 0;
    let mut chars = 0;
    for c in cell.text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > room {
            break;
        }
        used += w;
        chars += 1;
    }
    let mut spans = cell.slice(0, chars);
    spans.push(Span::raw("…"));
    (spans, used + 1)
}

/// One drawn line of a table: the spans, the text they add up to, and any
/// links, with their offsets moved into the row rather than the cell.
fn row_line(
    cells: &[Rendered],
    widths: &[usize],
    aligns: &[Align],
    edges: (&str, &str, &str),
    style: Style,
) -> Rendered {
    let (left, mid, right) = edges;
    let mut out = Rendered::default();
    let push = |text: &str, style: Style, out: &mut Rendered| {
        out.text.push_str(text);
        out.spans.push(Span::styled(text.to_string(), style));
    };
    push(left, style, &mut out);
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            push(mid, style, &mut out);
        }
        let width = widths[i];
        let (spans, used) = fit(cell, width);
        let slack = width.saturating_sub(used);
        let (before, after) = match aligns.get(i).copied().unwrap_or(Align::Left) {
            Align::Left => (0, slack),
            Align::Right => (slack, 0),
            Align::Center => (slack / 2, slack - slack / 2),
        };
        push(&" ".repeat(before + 1), style, &mut out);
        // Link offsets are relative to the cell; they have to become relative
        // to the row, or a click resolves against the wrong column entirely.
        let base = out.text.chars().count();
        for link in &cell.links {
            if link.start < spans.iter().map(|s| s.content.chars().count()).sum() {
                out.links.push(Link {
                    start: base + link.start,
                    len: link.len,
                    target: link.target.clone(),
                    heading: link.heading.clone(),
                    wiki: link.wiki,
                });
            }
        }
        for span in spans {
            out.text.push_str(&span.content);
            out.spans.push(span);
        }
        push(&" ".repeat(after + 1), style, &mut out);
    }
    push(right, style, &mut out);
    out
}

/// A rule: `┌───┬───┐` and its relatives.
fn rule(widths: &[usize], edges: (&str, &str, &str), style: Style) -> Rendered {
    let (left, mid, right) = edges;
    let mut text = String::from(left);
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            text.push_str(mid);
        }
        text.push_str(&"─".repeat(w + 2));
    }
    text.push_str(right);
    Rendered {
        spans: vec![Span::styled(text.clone(), style)],
        text,
        links: Vec::new(),
    }
}

/// One line of a drawn table.
pub struct Drawn {
    /// The note line this belongs to, so a click lands on the right one. A
    /// table draws two more lines than it occupies, so preview cannot assume
    /// one drawn line per source line.
    pub source: usize,
    /// Whether this line carries the number in the gutter. Rules do not: the
    /// number belongs beside content, and one sitting beside a border reads as
    /// though the border were the line.
    pub numbered: bool,
    pub rendered: Rendered,
}

/// Draw a table, or return `None` when the pane is too narrow to be worth it.
pub fn render(table: &Table, renderer: &Renderer<'_>, width: usize) -> Option<Vec<Drawn>> {
    let widths = measure(table, width)?;
    let theme = renderer.theme;
    let frame = theme.faded();

    let render_cells = |row: &[String]| -> Vec<Rendered> {
        row.iter().map(|c| renderer.render(c, false)).collect()
    };

    let mut out = Vec::with_capacity(table.rows.len() + 4);
    let line = |source: usize, numbered: bool, rendered: Rendered| Drawn {
        source,
        numbered,
        rendered,
    };
    out.push(line(0, false, rule(&widths, ("┌", "┬", "┐"), frame)));
    out.push(line(
        0,
        true,
        row_line(
            &render_cells(&table.head),
            &widths,
            &table.aligns,
            ("│", "│", "│"),
            frame,
        ),
    ));
    // The separator row draws as this rule, and is not content worth numbering.
    out.push(line(1, false, rule(&widths, ("├", "┼", "┤"), frame)));
    for (n, row) in table.rows.iter().enumerate() {
        out.push(line(
            n + 2,
            true,
            row_line(
                &render_cells(row),
                &widths,
                &table.aligns,
                ("│", "│", "│"),
                frame,
            ),
        ));
    }
    // The bottom rule belongs to the last row: there is no source line under it.
    out.push(line(
        table.height.saturating_sub(1),
        false,
        rule(&widths, ("└", "┴", "┘"), frame),
    ));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Theme;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    fn draw(source: &str, width: usize) -> Option<Vec<String>> {
        let theme = Theme::default();
        let resolves = |t: &str| t != "Nowhere";
        let renderer = Renderer {
            theme: &theme,
            resolves: &resolves,
            conceal: true,
        };
        let src = lines(source);
        let table = parse(&src, 0)?;
        let drawn = render(&table, &renderer, width)?;
        Some(drawn.into_iter().map(|d| d.rendered.text).collect())
    }

    #[test]
    fn a_table_is_measured_and_drawn_with_rules() {
        let out = draw("| When | What |\n| --- | --- |\n| Monday | rain |", 40).unwrap();
        assert_eq!(
            out,
            vec![
                "┌────────┬──────┐",
                "│ When   │ What │",
                "├────────┼──────┤",
                "│ Monday │ rain │",
                "└────────┴──────┘",
            ]
        );
    }

    #[test]
    fn every_drawn_row_is_exactly_as_wide_as_the_rules() {
        // The failure this catches is a grid whose rules do not line up, which
        // is what a character count instead of a display width produces.
        for source in [
            "| a | b |\n| --- | --- |\n| 日本語 | x |\n| y | ✨ |",
            "| head | second |\n| ---: | :---: |\n| 1 | 2 |",
        ] {
            for width in [20usize, 30, 44, 80] {
                let Some(out) = draw(source, width) else {
                    continue;
                };
                let first = out[0].width();
                for line in &out {
                    assert_eq!(line.width(), first, "{line:?} in a {width}-column pane");
                }
                assert!(
                    first <= width,
                    "{first} wider than the {width} it was given"
                );
            }
        }
    }

    #[test]
    fn alignment_markers_are_honoured() {
        // The headers are wider than the cells, so there is slack to place.
        let out = draw(
            "| head | second | third |\n| :--- | ---: | :---: |\n| x | y | z |",
            60,
        )
        .unwrap();
        assert_eq!(out[3], "│ x    │      y │   z   │");
    }

    #[test]
    fn a_cell_too_wide_is_cut_visibly() {
        let out = draw(
            "| word | note |\n| --- | --- |\n| x | a sentence far too long for this |",
            24,
        )
        .unwrap();
        assert!(
            out[3].contains('…'),
            "a silently shortened cell reads as the whole value: {:?}",
            out[3]
        );
    }

    #[test]
    fn one_long_cell_does_not_squeeze_the_others_to_nothing() {
        let out = draw(
            "| id | description |\n| --- | --- |\n| 7 | a very long description indeed that runs on |",
            34,
        )
        .unwrap();
        // "id" and "7" both still fit; only the long column gave ground.
        assert!(out[3].starts_with("│ 7  "), "{:?}", out[3]);
    }

    #[test]
    fn inline_markup_inside_a_cell_is_rendered() {
        let out = draw("| a | b |\n| --- | --- |\n| **bold** | [[Note]] |", 40).unwrap();
        assert!(out[3].contains("bold"), "{:?}", out[3]);
        assert!(!out[3].contains('*'), "{:?}", out[3]);
        assert!(out[3].contains("Note"), "{:?}", out[3]);
        assert!(!out[3].contains("[["), "{:?}", out[3]);
    }

    #[test]
    fn an_escaped_pipe_stays_inside_its_cell() {
        // Obsidian writes this inside tables; splitting on it makes the row
        // ragged and the whole table falls back for no reason.
        let table = parse(&lines("| a | b |\n| --- | --- |\n| [[N\\|alias]] | x |"), 0).unwrap();
        assert_eq!(table.rows[0], vec!["[[N|alias]]", "x"]);
    }

    #[test]
    fn a_ragged_row_is_not_a_table() {
        assert!(parse(&lines("| a | b |\n| --- | --- |\n| only one |"), 0).is_none());
    }

    #[test]
    fn a_missing_separator_is_not_a_table() {
        assert!(parse(&lines("| a | b |\n| x | y |"), 0).is_none());
        assert!(parse(&lines("just | a pipe"), 0).is_none());
    }

    #[test]
    fn a_pane_too_narrow_to_draw_in_gives_up_rather_than_producing_noise() {
        assert!(draw(
            "| a | b | c | d |\n| - | - | - | - |\n| 1 | 2 | 3 | 4 |",
            12
        )
        .is_none());
    }

    #[test]
    fn the_block_reports_how_many_source_lines_it_ate() {
        let table = parse(&lines("| a |\n| --- |\n| 1 |\n| 2 |\n\nafter"), 0).unwrap();
        assert_eq!(table.height, 4);
    }

    #[test]
    fn a_link_in_a_cell_keeps_a_range_that_points_at_it() {
        let theme = Theme::default();
        let resolves = |_: &str| true;
        let renderer = Renderer {
            theme: &theme,
            resolves: &resolves,
            conceal: true,
        };
        // The pipe inside the alias has to be escaped, or it ends the cell —
        // which is markdown's rule, and Obsidian's.
        let src = lines("| a | b |\n| --- | --- |\n| [[Note\\|go]] | x |");
        let table = parse(&src, 0).unwrap();
        let drawn = render(&table, &renderer, 40).unwrap();
        let row = &drawn[3].rendered;
        let link = row.links.first().expect("the cell's link survived");
        assert_eq!(link.target, "Note");
        let at: String = row.text.chars().skip(link.start).take(link.len).collect();
        assert_eq!(at, "go", "the range names the drawn text, not the syntax");
    }
}
