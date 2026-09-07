pub mod callout;
pub mod fold;
pub mod markdown;
pub mod table;
pub mod theme;

use crate::app::{App, ContextTarget, Focus, Overlay, Picker, SidebarTab};
use crate::editor::Mode;
use crate::keymap::HELP;
use crate::llm;
use markdown::Renderer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use theme::Theme;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const CONTEXT_WIDTH: u16 = 32;
const ASSISTANT_WIDTH: u16 = 46;

pub fn draw(f: &mut Frame, app: &mut App) {
    let theme = app.theme;
    let area = f.area();
    f.render_widget(Block::default().style(theme.base()), area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let mut cols: Vec<Constraint> = Vec::new();
    if app.sidebar_visible {
        cols.push(Constraint::Length(app.config.sidebar_width.clamp(18, 60)));
    }
    cols.push(Constraint::Min(20));
    let right_width = if app.assistant_visible {
        ASSISTANT_WIDTH
    } else if app.context_visible {
        CONTEXT_WIDTH
    } else {
        0
    };
    if right_width > 0 {
        cols.push(Constraint::Length(right_width));
    }
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(cols)
        .split(rows[0]);

    // Every draw re-records where the panes landed, so a click can be
    // resolved against the frame the user is actually looking at.
    app.panes = crate::app::Panes::default();
    let mut i = 0;
    if app.sidebar_visible {
        draw_sidebar(f, app, panes[i]);
        app.panes.sidebar = inner_of(panes[i]);
        i += 1;
    }
    let editor_area = panes[i];
    i += 1;
    // Set before drawing: the editor may keep a row for the section crumb, and
    // it is the only thing that knows whether it did.
    app.panes.editor = inner_of(editor_area);
    draw_editor(f, app, editor_area);
    if right_width > 0 {
        if app.assistant_visible {
            let (body, input) = draw_assistant(f, app, panes[i]);
            app.panes.assistant = body;
            app.panes.assistant_input = input;
        } else {
            let targets = draw_context(f, app, panes[i]);
            app.context_targets = targets;
            app.panes.context = inner_of(panes[i]);
        }
    }

    draw_status(f, app, rows[1]);

    let overlay_area = match &app.overlay {
        Some(Overlay::Palette(p))
        | Some(Overlay::Switcher(p))
        | Some(Overlay::LinkPicker(p))
        | Some(Overlay::Backlinks(p))
        | Some(Overlay::Themes(p))
        | Some(Overlay::MoveTo { picker: p, .. }) => draw_picker(f, &theme, p, area),
        Some(Overlay::Search(pane)) => draw_search(f, &theme, pane, area),
        Some(Overlay::Prompt(prompt)) => {
            draw_prompt(f, &theme, prompt, area);
            Rect::default()
        }
        Some(Overlay::Git(pane)) => draw_git(f, &theme, pane, area),
        Some(Overlay::History(log)) => draw_history(f, &theme, log, area),
        Some(Overlay::Confirm(c)) => {
            draw_confirm(f, &theme, c, area);
            Rect::default()
        }
        Some(Overlay::Diff {
            title,
            body,
            scroll,
        }) => draw_diff(f, &theme, title, body, *scroll, area),
        Some(Overlay::Menu(menu)) => draw_menu(f, &theme, menu, area),
        Some(Overlay::Peek(peek)) => {
            draw_peek(f, &theme, peek, area);
            Rect::default()
        }
        Some(Overlay::Help) => {
            draw_help(f, &theme, area);
            Rect::default()
        }
        None => Rect::default(),
    };
    app.panes.overlay = overlay_area;
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

/// The drawable area inside a bordered pane.
fn inner_of(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

fn pane_block<'a>(theme: &Theme, title: &'a str, focused: bool) -> Block<'a> {
    // The focused pane sits one shade above the page and carries a dot, so
    // where you are is legible at a glance without a loud border.
    let marker = if focused { " ● " } else { "   " };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused))
        .title(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme.accent)),
            Span::styled(
                title.to_string(),
                if focused {
                    theme.title()
                } else {
                    theme.dimmed()
                },
            ),
            Span::styled(" ", theme.dimmed()),
        ]))
        .style(Style::default().bg(if focused { theme.surface } else { theme.bg }))
}

fn section(theme: &Theme, label: &str) -> Line<'static> {
    // Secondary, not accent: the accent marks focus and the current thing, and
    // loses its meaning if every heading also wears it.
    Line::from(Span::styled(
        label.to_uppercase(),
        Style::default()
            .fg(theme.secondary)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Truncate to `width` display columns, adding an ellipsis when it does not
/// fit. Measured in columns rather than characters, because CJK and emoji
/// occupy two columns each and would otherwise overflow the pane.
fn fit(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    // Leave one column for the ellipsis.
    let budget = width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0usize;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let focused = app.focus == Focus::Sidebar;
    let title = match app.sidebar_tab {
        SidebarTab::Notes => match &app.tag_filter {
            Some(tag) => format!("#{tag}"),
            None => "vault".to_string(),
        },
        SidebarTab::Tags => "tags".to_string(),
    };
    let block = pane_block(&theme, &title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let width = inner.width as usize;
    let height = inner.height as usize;

    let mut lines: Vec<Line> = Vec::new();
    match app.sidebar_tab {
        SidebarTab::Notes => {
            let rows = app.tree_rows();
            let notes = app.listed_notes();
            let words: usize = notes.iter().map(|n| n.words).sum();
            lines.push(Line::from(Span::styled(
                fit(
                    &format!("{} · {}", plural(notes.len(), "note"), compact(words)),
                    width,
                ),
                theme.faded(),
            )));

            let visible = height.saturating_sub(1);
            let offset = scroll_offset(app.sidebar_cursor, rows.len(), visible);
            for (i, row) in rows.iter().enumerate().skip(offset).take(visible) {
                let selected = i == app.sidebar_cursor && focused;
                // Two columns per level. Levels below the first get a faint
                // rule so a deep note can still be traced to its folder once
                // the folder's own row has scrolled away. The first level is
                // plain, or the rule would sit flush against the pane border
                // and read as a doubled edge.
                let indent: String = (0..row.depth)
                    .map(|i| if i == 0 { "  " } else { "│ " })
                    .collect();
                let mut spans = vec![Span::styled(indent.clone(), theme.faded())];
                let used = row.depth * 2;

                match &row.entry {
                    crate::tree::Entry::Dir {
                        name,
                        expanded,
                        notes,
                        ..
                    } => {
                        let count = format!(" {notes}");
                        let room = width.saturating_sub(used + 2 + count.chars().count());
                        let label = fit(name, room);
                        let pad = room.saturating_sub(label.width());
                        spans.push(Span::styled(
                            if *expanded { "▾ " } else { "▸ " },
                            Style::default().fg(theme.accent),
                        ));
                        spans.push(Span::styled(
                            label,
                            if selected {
                                theme.selected()
                            } else {
                                Style::default()
                                    .fg(theme.heading)
                                    .add_modifier(Modifier::BOLD)
                            },
                        ));
                        spans.push(Span::styled(" ".repeat(pad), theme.dimmed()));
                        spans.push(Span::styled(count, theme.faded()));
                    }
                    crate::tree::Entry::Note { id, title } => {
                        let is_open = app.current.as_deref() == Some(id.as_str());
                        spans.push(Span::styled(
                            if is_open { "▌ " } else { "  " },
                            Style::default().fg(theme.accent),
                        ));
                        spans.push(Span::styled(
                            fit(title, width.saturating_sub(used + 2)),
                            if selected {
                                theme.selected()
                            } else if is_open {
                                Style::default().fg(theme.accent)
                            } else {
                                Style::default().fg(theme.fg)
                            },
                        ));
                    }
                }
                let mut line = Line::from(spans);
                if selected {
                    line = line.style(Style::default().bg(theme.selection));
                }
                lines.push(line);
            }
            if rows.is_empty() {
                lines.push(Line::from(Span::styled("  no notes", theme.faded())));
            }
        }
        SidebarTab::Tags => {
            let tags = app.vault.all_tags();
            let offset = scroll_offset(app.sidebar_cursor, tags.len(), height.saturating_sub(1));
            lines.push(Line::from(Span::styled(
                fit(&plural(tags.len(), "tag"), width),
                theme.faded(),
            )));
            for (i, (tag, count)) in tags.iter().enumerate().skip(offset).take(height - 1) {
                let selected = i == app.sidebar_cursor && focused;
                let label = format!("#{tag}");
                let count = format!(" {count}");
                let pad = width.saturating_sub(label.chars().count() + count.chars().count() + 1);
                lines.push(Line::from(vec![
                    Span::styled(" ", theme.dimmed()),
                    Span::styled(
                        fit(&label, width.saturating_sub(2)),
                        if selected {
                            theme.selected()
                        } else {
                            Style::default().fg(theme.tag)
                        },
                    ),
                    Span::styled(" ".repeat(pad), theme.dimmed()),
                    Span::styled(count, theme.faded()),
                ]));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// "1 note", "3 notes" — an `s` only when it belongs.
fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Render a word count compactly: 940, 12.4k, 1.2m.
fn compact(n: usize) -> String {
    match n {
        0..=999 => format!("{n} words"),
        1_000..=999_999 => format!("{:.1}k words", n as f64 / 1_000.0),
        _ => format!("{:.1}m words", n as f64 / 1_000_000.0),
    }
}

/// Width of the line-number gutter, including its trailing space. Shared with
/// the mouse handler: a click has to be measured against the same geometry the
/// renderer drew, or it lands on the wrong column.
pub fn gutter_width(line_count: usize) -> u16 {
    (line_count.to_string().len() + 1).max(4) as u16
}

/// How far the editor is scrolled sideways. Source mode follows the cursor;
/// preview soft-wraps and never scrolls.
/// The note as preview draws it.
///
/// Concealed text is shorter than its source, so it folds at different places
/// and cannot borrow the editor's rows. It carries its own layout over the
/// rendered lines, which is what the drawing walks and what a click resolves
/// against — the same one-model rule as the editor, applied to the other view.
pub struct PreviewView {
    pub lines: Vec<markdown::Rendered>,
    /// The source line each drawn line came from.
    ///
    /// Not the identity, and that is the whole reason it exists: a table draws
    /// two more lines than it occupies, so a click on row three of a drawn
    /// table has to land on the note's row three and not on a rule.
    pub sources: Vec<usize>,
    /// Whether each drawn line carries the gutter number for its source line.
    pub numbered: Vec<bool>,
    pub layout: crate::layout::Layout,
}

impl PreviewView {
    /// Always folds, whatever `wrap` is set to.
    ///
    /// That setting is about the editor, where scrolling sideways through a
    /// line you are editing is a defensible preference. Preview is for
    /// reading, and a reading view that runs its text off the right edge is
    /// not a preference, it is a truncation.
    pub fn build(
        source: &[String],
        renderer: &markdown::Renderer<'_>,
        width: usize,
        folded: Option<&std::collections::HashSet<usize>>,
    ) -> PreviewView {
        let mut lines = Vec::with_capacity(source.len());
        let mut sources = Vec::with_capacity(source.len());
        let mut numbered = Vec::with_capacity(source.len());
        let heads = fold::headings(source);
        let theme = renderer.theme;
        let mut in_code = false;
        let mut i = 0;

        // Frontmatter, collapsed to what a reader uses. 123 of the 129 notes
        // this was built against open with six lines of it — a fifth of the
        // first screen spent on bookkeeping before a sentence appears.
        if let Some((pairs, body)) = crate::vault::note::frontmatter_block(source) {
            for drawn in properties(&pairs, renderer) {
                lines.push(drawn);
                // Every drawn row belongs to the block's first line. There is no
                // sensible mapping from a chip back to the YAML line it came
                // from, and the top of the block is where a click should land.
                sources.push(0);
                numbered.push(false);
            }
            i = body;
        }
        while i < source.len() {
            let raw = &source[i];
            let opens = markdown::is_fence(raw);

            // A heading carries a marker saying whether it can be opened, and
            // what is behind it when it is shut. Rendered first so the marker
            // sits outside the concealment and the link offsets move with it.
            if !in_code && !opens && heads.iter().any(|h| h.row == i) {
                let end = fold::section_end(&heads, i, source.len());
                let hidden = end - i - 1;
                let shut = hidden > 0 && folded.is_some_and(|f| f.contains(&i));
                let marker = if hidden == 0 {
                    "  "
                } else if shut {
                    "▸ "
                } else {
                    "▾ "
                };
                let mut drawn = renderer
                    .render(raw, false)
                    .prefixed(marker, Style::default().fg(theme.accent));
                if shut {
                    let plural = if hidden == 1 { "" } else { "s" };
                    drawn = drawn.suffixed(&format!("  {hidden} line{plural}"), theme.faded());
                }
                lines.push(drawn);
                sources.push(i);
                numbered.push(true);
                i = if shut { end } else { i + 1 };
                continue;
            }

            // A callout is a block too, and one that draws a line for each
            // line it covers — so unlike a table it needs no mapping back.
            if !in_code && !opens {
                if let Some(c) = callout::parse(source, i) {
                    for (n, drawn) in callout::render(&c, source, i, renderer)
                        .into_iter()
                        .enumerate()
                    {
                        lines.push(drawn);
                        sources.push(i + n);
                        numbered.push(true);
                    }
                    i += c.height;
                    continue;
                }
            }

            // A table is a block, not a line. Inside a fence it is text like
            // anything else — pipes in a code sample are not a table.
            if !in_code && !opens {
                if let Some(table) = table::parse(source, i) {
                    if let Some(drawn) = table::render(&table, renderer, width) {
                        for line in drawn {
                            lines.push(line.rendered);
                            sources.push(i + line.source);
                            numbered.push(line.numbered);
                        }
                        i += table.height;
                        continue;
                    }
                    // Too narrow to draw: fall through and show the pipes,
                    // which is still the table, just not a pretty one.
                }
            }

            lines.push(renderer.render(raw, in_code || opens));
            sources.push(i);
            numbered.push(true);
            if opens {
                in_code = !in_code;
            }
            i += 1;
        }
        let texts: Vec<String> = lines.iter().map(|r| r.text.clone()).collect();
        let layout = crate::layout::Layout::new(&texts, width, true);
        PreviewView {
            lines,
            sources,
            numbered,
            layout,
        }
    }

    /// The note line a drawn line belongs to.
    pub fn source(&self, line: usize) -> usize {
        self.sources.get(line).copied().unwrap_or(0)
    }

    /// The screen row that stands for a note line.
    ///
    /// When the line itself is drawn, that row. When it is not — it is inside a
    /// fold — the row that *contains* it, which is the folded heading above it
    /// rather than the next thing after it. A reader who folds the section they
    /// were inside should be looking at that section, not at the one beyond.
    pub fn row_of_source(&self, source: usize) -> usize {
        let exact = |numbered_only: bool| {
            self.sources.iter().enumerate().position(|(i, s)| {
                *s == source && (!numbered_only || self.numbered.get(i).copied().unwrap_or(true))
            })
        };
        let drawn = exact(true)
            // A table's rules share their row's line, so prefer the row that
            // carries the number — content rather than a border.
            .or_else(|| exact(false))
            // Not drawn at all: the row that contains it.
            .or_else(|| self.sources.iter().rposition(|s| *s < source))
            .or_else(|| self.sources.iter().position(|s| *s > source))
            .unwrap_or(self.lines.len().saturating_sub(1));
        self.layout.first_of(drawn)
    }

    /// Whether this drawn line should show its line number.
    ///
    /// A table's rules belong to note lines but are not those lines: the
    /// number goes on the header and the rows, so it sits beside content
    /// rather than beside a border.
    pub fn starts_source(&self, line: usize) -> bool {
        self.numbered.get(line).copied().unwrap_or(true)
    }

    /// The link under a drawn position, if there is one.
    pub fn link_at(&self, line: usize, column: usize) -> Option<&markdown::Link> {
        self.lines.get(line)?.link_at(column)
    }

    /// The drawn text, which is what the layout was built over and what a
    /// click has to be resolved against.
    pub fn texts(&self) -> Vec<String> {
        self.lines.iter().map(|r| r.text.clone()).collect()
    }
}

/// How wide prose is held while reading, when nothing else says.
///
/// `wrap_column` defaults to the pane, which is right for an editor and wrong
/// for a reading view: prose stretched across a hundred and fifty columns is
/// harder to read than prose at seventy, not easier.
const READING_MEASURE: u16 = 72;

/// Rows of context kept beyond the reading cursor, so a line never arrives hard
/// against the edge with nothing after it.
const READING_MARGIN: usize = 3;

/// Where one crumb sits: start column, end column, and the note line it names.
type Crumb = (usize, usize, usize);

/// The heading chain for the top visible line, and the columns each crumb
/// occupies so a click can find it.
///
/// Returns `None` when the chain is empty, or when its deepest heading is
/// already on screen — repeating a heading immediately above itself is noise,
/// and the row is better spent on text.
fn sticky(
    heads: &[fold::Heading],
    total: usize,
    top_line: usize,
    visible: &[usize],
    width: usize,
) -> Option<(Vec<Crumb>, String)> {
    let chain = fold::chain(heads, top_line, total);
    let deepest = chain.last()?;
    if visible.contains(&deepest.row) {
        return None;
    }

    const SEP: &str = "  ›  ";
    // Drop from the front when it will not fit: the deepest heading is the one
    // that says where you are, so it is the last thing to go.
    let mut from = 0;
    let text = loop {
        let mut text = String::new();
        if from > 0 {
            text.push('…');
            text.push_str(SEP);
        }
        for (n, h) in chain[from..].iter().enumerate() {
            if n > 0 {
                text.push_str(SEP);
            }
            text.push_str(&h.text);
        }
        if text.chars().count() <= width || from + 1 >= chain.len() {
            break text;
        }
        from += 1;
    };

    // Where each crumb sits, so the mouse can hit it.
    let mut spots = Vec::new();
    let mut at = if from > 0 { 1 + SEP.chars().count() } else { 0 };
    for (n, h) in chain[from..].iter().enumerate() {
        if n > 0 {
            at += SEP.chars().count();
        }
        let len = h.text.chars().count();
        spots.push((at, at + len, h.row));
        at += len;
    }
    Some((spots, fit(&text, width)))
}

/// Frontmatter drawn as properties: the tags a reader clicks, then whatever
/// else the block held, dimmed.
///
/// At most two rows, and nothing is dropped — a key nobody anticipated still
/// appears, it just appears out of the way.
fn properties(
    pairs: &[(String, String)],
    renderer: &markdown::Renderer<'_>,
) -> Vec<markdown::Rendered> {
    let theme = renderer.theme;
    let mut out = Vec::new();

    let tags: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k == "tags" || k == "tag")
        .flat_map(|(_, v)| v.split(','))
        .map(|t| t.trim().trim_matches('"').trim_start_matches('#'))
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();

    if !tags.is_empty() {
        let mut row = markdown::Rendered::default();
        for (n, tag) in tags.iter().enumerate() {
            if n > 0 {
                row = row.suffixed(" · ", theme.faded());
            }
            row = row.with_link(
                &format!("#{tag}"),
                Style::default().fg(theme.tag),
                tag.clone(),
                markdown::Target::Tag,
            );
        }
        out.push(row);
    }

    // Everything else in the order it was written, so a key that is not
    // understood is still visible rather than silently swallowed.
    let rest: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k != "tags" && k != "tag")
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k} {v}"))
        .collect();
    if !rest.is_empty() {
        out.push(markdown::Rendered::default().suffixed(&rest.join("  ·  "), theme.faded()));
    }
    out
}

/// How wide text may be before it folds.
///
/// `wrap_column` of 0 means the pane, which is the default and what most
/// terminals want. A number holds prose to a readable measure on a wide
/// screen — but never wider than the pane, or the fold would put text where
/// there is none to draw it.
pub fn wrap_width(pane_width: usize, wrap_column: u16) -> usize {
    match wrap_column {
        0 => pane_width,
        n => (n as usize).min(pane_width),
    }
}

pub fn editor_hscroll(
    editor: &crate::editor::Editor,
    inner_width: u16,
    gutter: u16,
    preview: bool,
) -> usize {
    if preview {
        return 0;
    }
    let text_width = inner_width.saturating_sub(gutter) as usize;
    editor
        .buf
        .col
        .saturating_sub(text_width.saturating_sub(4).max(1))
}

/// Keep `cursor` visible inside a window `height` tall over `len` items.
pub fn scroll_offset(cursor: usize, len: usize, height: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    let half = height / 2;
    cursor.saturating_sub(half).min(len - height)
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

fn draw_editor(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let focused = app.focus == Focus::Editor;
    let title = match &app.current {
        Some(id) => format!("{}{}", id, if app.editor.buf.dirty { " ●" } else { "" }),
        None => "no note".to_string(),
    };
    let block = pane_block(&theme, &title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    app.editor_height = inner.height as usize;

    // One layout, read by the drawing below, by the caret, by `j`/`k`, and by
    // the mouse. Everything that has an opinion about where a line is on
    // screen reads this and nothing else.
    // Reading is a different posture: no line numbers, and prose held to a
    // measure rather than stretched across the terminal.
    let reading = app.preview && app.config.reading_focus;
    let gutter = if reading {
        0
    } else {
        gutter_width(app.editor.buf.line_count())
    };
    let inner = if reading {
        let measure = match app.config.wrap_column {
            0 => READING_MEASURE,
            n => n,
        }
        .min(inner.width);
        Rect {
            x: inner.x + (inner.width - measure) / 2,
            width: measure,
            ..inner
        }
    } else {
        inner
    };
    let pane_width = inner.width.saturating_sub(gutter) as usize;
    let wrap = app.config.wrap;
    let text_width = wrap_width(pane_width, app.config.wrap_column);
    // The source layout is kept current whichever view is showing: the cursor
    // lives in the buffer, and a motion that resolved against rendered columns
    // would put it somewhere the file does not agree with.
    app.editor.relayout(text_width, wrap);
    let cursor_visual = app.editor.layout.visual_of(
        &app.editor.buf.lines,
        app.editor.buf.row,
        app.editor.buf.col,
    );

    let vault = &app.vault;
    let resolves = |target: &str| vault.resolves(target);
    let renderer = Renderer {
        theme: &theme,
        resolves: &resolves,
        conceal: app.preview,
    };

    let folded: Option<std::collections::HashSet<usize>> = app
        .current
        .as_deref()
        .and_then(|id| app.folded.of(id))
        .cloned();
    let view = app.preview.then(|| {
        PreviewView::build(
            &app.editor.buf.lines,
            &renderer,
            text_width,
            folded.as_ref(),
        )
    });

    // Scroll against whichever rows are actually on screen. In preview that is
    // the rendered fold, so the top of the cursor's line is the anchor — there
    // is no caret there to keep any finer promise to.
    // Preview keeps its own place. Re-anchoring it to the buffer cursor on
    // every draw is what pinned the wheel: the view was put back before it was
    // seen. The editor still follows its caret, which is what a caret is for.
    match &view {
        Some(v) => {
            let last = v.layout.row_count().saturating_sub(1);
            // A fold changed the document since the last draw; put the reader
            // back over the line they were on rather than over whatever this
            // row index now points at.
            if let Some(source) = app.preview_anchor.take() {
                app.preview_row = v.row_of_source(source);
            }
            app.preview_row = app.preview_row.min(last);
            app.editor.sync_scroll_margin(
                app.preview_row,
                v.layout.row_count(),
                inner.height as usize,
                READING_MARGIN,
            );
            // The buffer cursor follows the reader, so leaving preview lands
            // where they were and `za`, `K` and the crumb act on a line that is
            // actually on screen.
            let source = v.source(v.layout.row(app.preview_row).map(|r| r.line).unwrap_or(0));
            app.editor.buf.row = source.min(app.editor.buf.line_count().saturating_sub(1));
            app.editor.buf.col = 0;
        }
        None => app.editor.sync_scroll_visual(
            cursor_visual.0,
            app.editor.layout.row_count(),
            inner.height as usize,
        ),
    }

    // Which note lines are on screen, so the crumb can hide itself when the
    // heading it names is already visible.
    let rows = |layout: &crate::layout::Layout, of: &dyn Fn(usize) -> usize| {
        (app.editor.scroll..layout.row_count())
            .take(inner.height as usize)
            .filter_map(|v| layout.row(v).map(|r| of(r.line)))
            .collect::<Vec<usize>>()
    };
    let visible: Vec<usize> = match &view {
        Some(v) => rows(&v.layout, &|line| v.source(line)),
        None => rows(&app.editor.layout, &|line| line),
    };
    let heads = fold::headings(&app.editor.buf.lines);
    let crumbs = visible.first().and_then(|top| {
        sticky(
            &heads,
            app.editor.buf.line_count(),
            *top,
            &visible,
            inner.width.saturating_sub(gutter) as usize,
        )
    });
    // The crumb costs a row, so the text gets one fewer and the pane it is
    // hit-tested against has to agree.
    let inner = match &crumbs {
        Some(_) if inner.height > 1 => Rect {
            y: inner.y + 1,
            height: inner.height - 1,
            ..inner
        },
        _ => inner,
    };
    app.editor_height = inner.height as usize;
    app.panes.editor = inner;
    app.sticky = Vec::new();
    if let Some((spots, text)) = &crumbs {
        let y = inner.y - 1;
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text.clone(), theme.faded()))),
            Rect {
                x: inner.x + gutter,
                y,
                width: inner.width.saturating_sub(gutter),
                height: 1,
            },
        );
        app.sticky = spots
            .iter()
            .map(|(a, b, row)| {
                (
                    inner.x + gutter + *a as u16,
                    inner.x + gutter + *b as u16,
                    *row,
                )
            })
            .collect();
        app.panes.sticky_y = Some(y);
    }
    if crumbs.is_none() {
        app.panes.sticky_y = None;
    }

    // Nothing runs off the right edge when it folds, so there is nothing to
    // scroll to. Sideways scrolling survives only as what `wrap = false` gets.
    let hscroll = if wrap {
        0
    } else {
        editor_hscroll(&app.editor, inner.width, gutter, app.preview)
    };

    let selection = app.editor.selection_rows();
    let mut lines: Vec<Line> = Vec::new();

    // The gutter carries the buffer line number, blank on a continuation row:
    // the number belongs to the line, not to each row it folds onto.
    let gutter_span = |row: usize, first: bool, cursor: bool| {
        Span::styled(
            match gutter {
                // Reading mode has no gutter at all, so there is nothing to
                // pad and nothing to number.
                0 => String::new(),
                w if first => format!("{:>width$} ", row + 1, width = w as usize - 1),
                w => " ".repeat(w as usize),
            },
            if cursor {
                Style::default().fg(theme.accent)
            } else {
                theme.faded()
            },
        )
    };
    let highlight = |line: Line<'static>, row: usize| {
        let selected = selection
            .map(|(a, b)| row >= a && row <= b)
            .unwrap_or(false);
        if selected {
            line.style(Style::default().bg(theme.selection))
        } else if row == app.editor.buf.row && focused {
            line.style(Style::default().bg(theme.cursorline))
        } else {
            line
        }
    };

    if let Some(view) = &view {
        // Preview walks its own rows, over text that has already been rendered
        // once. Slicing the spans rather than re-rendering a substring is what
        // keeps a link that straddles a fold looking like one link.
        for visual in app.editor.scroll..view.layout.row_count() {
            if lines.len() >= inner.height as usize {
                break;
            }
            let Some(vrow) = view.layout.row(visual) else {
                break;
            };
            let Some(rendered) = view.lines.get(vrow.line) else {
                break;
            };
            let source = view.source(vrow.line);
            let mut spans = vec![gutter_span(
                source,
                vrow.is_first() && view.starts_source(vrow.line),
                source == app.editor.buf.row,
            )];
            match (&rendered.rail, vrow.is_first(), vrow.indent) {
                // A continuation row of a railed line redraws the rail, then
                // pads to where the text was.
                (Some(rail), false, indent) if indent > 0 => {
                    let width = rail.content.chars().count().min(indent);
                    spans.push(rail.clone());
                    spans.push(Span::raw(" ".repeat(indent - width)));
                }
                (_, _, indent) if indent > 0 => spans.push(Span::raw(" ".repeat(indent))),
                _ => {}
            }
            spans.extend(rendered.slice(vrow.start, vrow.len));
            lines.push(highlight(Line::from(spans), source));
        }
    } else {
        // A fence opened before the viewport must still colour the visible lines.
        let first_line = app
            .editor
            .layout
            .row(app.editor.scroll)
            .map(|r| r.line)
            .unwrap_or(0);
        let mut in_code = app
            .editor
            .buf
            .lines
            .iter()
            .take(first_line)
            .filter(|l| markdown::is_fence(l))
            .count()
            % 2
            == 1;

        for visual in app.editor.scroll..app.editor.layout.row_count() {
            if lines.len() >= inner.height as usize {
                break;
            }
            let Some(vrow) = app.editor.layout.row(visual) else {
                break;
            };
            let row = vrow.line;
            let raw = app.editor.buf.line(row);
            let opens_fence = markdown::is_fence(raw);
            let render_as_code = in_code;
            if opens_fence {
                in_code = !in_code;
            }

            let visible: String = raw
                .chars()
                .skip(vrow.start + hscroll)
                .take(vrow.len.saturating_sub(hscroll))
                .collect();
            let visible = format!("{}{visible}", " ".repeat(vrow.indent));
            let mut spans = vec![gutter_span(row, vrow.is_first(), row == app.editor.buf.row)];
            spans.extend(renderer.line(&visible, render_as_code || opens_fence));
            lines.push(highlight(Line::from(spans), row));
        }
    }

    // Both views fold their own text, with a hanging indent ratatui has no
    // idea about, so nothing here may wrap a second time.
    f.render_widget(Paragraph::new(lines), inner);

    // Kept for the mouse: a click in preview lands on drawn characters, and
    // only this knows what they were before they were drawn.
    app.preview_view = view;

    // Place the real terminal cursor so the terminal's own caret is used.
    // The offset is in display columns: a line of CJK is twice as wide as it
    // is long, and a character-indexed cursor drifts left of its glyph.
    if focused && !app.preview && app.overlay.is_none() {
        // The same layout the rows above were drawn from, so the caret cannot
        // land somewhere the text is not.
        let (visual, column) = cursor_visual;
        let cx = inner.x + gutter + column.saturating_sub(hscroll) as u16;
        let cy = inner.y + (visual.saturating_sub(app.editor.scroll)) as u16;
        if cx < inner.right() && cy < inner.bottom() {
            f.set_cursor_position((cx, cy));
        }
    }
}

// ---------------------------------------------------------------------------
// Context pane: outline, outgoing links, backlinks
// ---------------------------------------------------------------------------

/// Headings in the live buffer, so the outline updates as you type.
fn outline_of(buf: &crate::editor::Buffer) -> Vec<fold::Heading> {
    fold::headings(&buf.lines)
}

/// How many rows the outline may use, given the pane height and how much the
/// sections below it need.
///
/// A long note would otherwise fill the pane with its own headings and push
/// the backlinks — the part you cannot get any other way — off the bottom.
/// Real notes have forty headings; the toy vaults this was built against had
/// four, which is why it looked fine.
fn outline_budget(pane_height: usize, below: usize) -> usize {
    if pane_height <= 1 {
        return 0;
    }
    // One row for the OUTLINE label, and leave room for what follows.
    let available = pane_height.saturating_sub(1);
    let for_outline = available.saturating_sub(below);
    // Always show a few headings, even when the sections below are hungry.
    for_outline.max(3).min(available)
}

/// Draws the context pane and returns, for each rendered row, what a click on
/// it should do — `None` for labels and blank lines.
fn draw_context(f: &mut Frame, app: &App, area: Rect) -> Vec<Option<ContextTarget>> {
    let theme = app.theme;
    let block = pane_block(&theme, "context", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return Vec::new();
    }
    let width = inner.width as usize;
    let height = inner.height as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut targets: Vec<Option<ContextTarget>> = Vec::new();

    let Some(id) = app.current.clone() else {
        lines.push(Line::from(Span::styled("no note open", theme.faded())));
        f.render_widget(Paragraph::new(lines), inner);
        return targets;
    };

    let outgoing = app.vault.outgoing(&id);
    let backlinks = app.vault.backlinks_for(&id);
    let orphans: Vec<&String> = app
        .vault
        .unresolved
        .iter()
        .filter(|(_, refs)| refs.iter().any(|r| r.from == id))
        .map(|(target, _)| target)
        .collect();

    // Work out what the lower sections want before deciding the outline's share.
    let out_shown = outgoing.len().min(6);
    let back_shown = backlinks.len().min(6);
    let orphan_shown = if orphans.is_empty() {
        0
    } else {
        orphans.len().min(4)
    };
    let below = 2 + out_shown.max(1)          // blank + heading + rows
        + 2 + back_shown.max(1) * 2           // backlinks carry a context line
        + if orphan_shown > 0 { 2 + orphan_shown } else { 0 };

    let entries = outline_of(&app.editor.buf);
    let budget = outline_budget(height, below);

    lines.push(section(&theme, "outline"));
    targets.push(None);
    if entries.is_empty() {
        lines.push(Line::from(Span::styled("  no headings", theme.faded())));
        targets.push(None);
    } else {
        // Keep the heading the cursor is under in view, the way the sidebar
        // keeps its selection in view.
        let current = entries
            .iter()
            .rposition(|e| e.row <= app.editor.buf.row)
            .unwrap_or(0);
        let offset = scroll_offset(current, entries.len(), budget);
        let shown = entries.iter().skip(offset).take(budget);
        for entry in shown {
            let indent = "  ".repeat(entry.level.saturating_sub(1));
            let here = entries
                .get(current)
                .map(|c| c.row == entry.row)
                .unwrap_or(false);
            // A folded section is marked here too, so the outline says the same
            // thing about the note as the note does.
            let shut = app.folded.is_folded(&id, entry.row);
            let marker = if shut { "▸ " } else { "" };
            let used = indent.len() + marker.len();
            lines.push(Line::from(vec![
                Span::styled(indent.clone(), theme.faded()),
                Span::styled(marker, Style::default().fg(theme.accent)),
                Span::styled(
                    fit(&entry.text, width.saturating_sub(used)),
                    if here {
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        theme
                            .heading_style(entry.level as u8)
                            .remove_modifier(Modifier::BOLD)
                    },
                ),
            ]));
            targets.push(Some(ContextTarget::Heading(entry.row)));
        }
        let hidden = entries.len().saturating_sub(budget);
        if hidden > 0 {
            lines.push(Line::from(Span::styled(
                fit(&format!("  … {hidden} more"), width),
                theme.faded(),
            )));
            targets.push(None);
        }
    }

    lines.push(Line::from(""));
    lines.push(section(&theme, &format!("links out · {}", outgoing.len())));
    targets.push(None);
    targets.push(None);
    if outgoing.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.faded())));
        targets.push(None);
    }
    for target in outgoing.iter().take(out_shown) {
        let title = app
            .vault
            .get(target)
            .map(|n| n.title.clone())
            .unwrap_or_else(|| target.clone());
        lines.push(Line::from(vec![
            Span::styled("  → ", theme.faded()),
            Span::styled(
                fit(&title, width.saturating_sub(4)),
                Style::default().fg(theme.link),
            ),
        ]));
        targets.push(Some(ContextTarget::Note(target.clone())));
    }

    lines.push(Line::from(""));
    lines.push(section(&theme, &format!("backlinks · {}", backlinks.len())));
    targets.push(None);
    targets.push(None);
    if backlinks.is_empty() {
        lines.push(Line::from(Span::styled("  none yet", theme.faded())));
        targets.push(None);
    }
    for bl in backlinks.iter().take(back_shown) {
        let title = app
            .vault
            .get(&bl.from)
            .map(|n| n.title.clone())
            .unwrap_or_else(|| bl.from.clone());
        lines.push(Line::from(vec![
            Span::styled("  ← ", theme.faded()),
            Span::styled(
                fit(&title, width.saturating_sub(4)),
                Style::default().fg(theme.link),
            ),
        ]));
        let jump = Some(ContextTarget::Backlink(bl.from.clone(), bl.line));
        targets.push(jump.clone());
        if !bl.context.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("     {}", fit(&bl.context, width.saturating_sub(5))),
                theme.faded(),
            )));
            // The quoted line is part of the same target, so clicking either
            // half goes to the same place.
            targets.push(jump);
        }
    }

    // Unresolved links are the vault's growing edge — worth surfacing.
    if !orphans.is_empty() {
        lines.push(Line::from(""));
        lines.push(section(&theme, &format!("unwritten · {}", orphans.len())));
        targets.push(None);
        targets.push(None);
        for target in orphans.iter().take(orphan_shown) {
            lines.push(Line::from(vec![
                Span::styled("  ○ ", theme.faded()),
                Span::styled(
                    fit(target, width.saturating_sub(4)),
                    Style::default().fg(theme.broken),
                ),
            ]));
            targets.push(Some(ContextTarget::Unwritten((*target).clone())));
        }
    }

    debug_assert_eq!(
        lines.len(),
        targets.len(),
        "every context row needs a click target, even if it is None"
    );
    f.render_widget(Paragraph::new(lines), inner);
    targets
}

// ---------------------------------------------------------------------------
// Assistant
// ---------------------------------------------------------------------------

/// Draws the assistant and returns its transcript and input areas, so a
/// click in either can focus it.
fn draw_assistant(f: &mut Frame, app: &App, area: Rect) -> (Rect, Rect) {
    let theme = app.theme;
    let focused = app.focus == Focus::Assistant;
    let title = format!("assistant · {}", app.config.model);
    let block = pane_block(&theme, &title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 4 {
        return (Rect::default(), Rect::default());
    }

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(inner);

    let width = split[0].width as usize;
    let mut lines: Vec<Line> = Vec::new();

    if app.chat.messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "Ask anything about your vault.",
            theme.dimmed(),
        )));
        lines.push(Line::from(""));
        for hint in [
            "The open note is always in context;",
            "related notes are retrieved by keyword.",
            "",
            "ctrl-y inserts the last answer at the cursor.",
        ] {
            lines.push(Line::from(Span::styled(hint, theme.faded())));
        }
        if llm::api_key().is_none() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "ANTHROPIC_API_KEY is not set.",
                Style::default().fg(theme.broken),
            )));
        }
    }

    let resolves = |target: &str| app.vault.resolves(target);
    let renderer = Renderer {
        theme: &theme,
        resolves: &resolves,
        conceal: false,
    };

    for message in &app.chat.messages {
        match message.role {
            llm::Role::User => {
                lines.push(Line::from(Span::styled(
                    "you",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )));
                for line in wrap_text(&message.text, width) {
                    lines.push(Line::from(Span::styled(
                        line,
                        Style::default().fg(theme.fg),
                    )));
                }
            }
            llm::Role::Assistant => {
                lines.push(Line::from(Span::styled(
                    "claude",
                    Style::default().fg(theme.link).add_modifier(Modifier::BOLD),
                )));
                let mut in_code = false;
                for raw in message.text.lines() {
                    let fence = markdown::is_fence(raw);
                    for chunk in wrap_text(raw, width) {
                        lines.push(Line::from(renderer.line(&chunk, in_code || fence)));
                    }
                    if fence {
                        in_code = !in_code;
                    }
                }
            }
        }
        lines.push(Line::from(""));
    }

    if app.chat.streaming {
        lines.push(Line::from(Span::styled("▌ thinking…", theme.faded())));
    }
    if !app.chat.context_ids.is_empty() {
        lines.push(Line::from(Span::styled(
            fit(
                &format!("context: {}", app.chat.context_ids.join(", ")),
                width,
            ),
            theme.faded(),
        )));
    }

    // Pin the view to the bottom so streaming output stays visible.
    let visible = split[0].height as usize;
    let max_scroll = lines.len().saturating_sub(visible);
    let scroll = max_scroll.saturating_sub(app.chat.scroll as usize);
    f.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), split[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused));
    let input_inner = input_block.inner(split[1]);
    f.render_widget(input_block, split[1]);
    let prompt = format!("› {}", app.chat.input);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            fit(&prompt, input_inner.width as usize),
            Style::default().fg(theme.fg),
        ))),
        input_inner,
    );
    if focused && app.overlay.is_none() {
        // Display columns, not characters, or the caret drifts left of the
        // text as soon as anything wide is typed. The pane can also be
        // squeezed to nothing, so there may be no column to sit in.
        let cx = input_inner.x + (prompt.width() as u16).min(input_inner.width.saturating_sub(1));
        f.set_cursor_position((cx, input_inner.y));
    }
    (split[0], input_inner)
}

/// Break `text` into lines no wider than `width`, on word boundaries.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    for raw in text.split('\n') {
        if raw.width() <= width {
            out.push(raw.to_string());
            continue;
        }
        let mut current = String::new();
        for word in raw.split_inclusive(' ') {
            if current.width() + word.width() > width && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            // A single word wider than the pane is hard-split.
            if word.width() > width {
                let mut chunk = String::new();
                for c in word.chars() {
                    // A wide character that would straddle the edge starts the
                    // next line instead of being cut in half.
                    if chunk.width() + c.width().unwrap_or(0) > width {
                        out.push(std::mem::take(&mut chunk));
                    }
                    chunk.push(c);
                }
                current = chunk;
            } else {
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Status line
// ---------------------------------------------------------------------------

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let mode = app.editor.mode;
    let mode_colour = match mode {
        Mode::Normal => theme.mode_normal,
        Mode::Insert => theme.mode_insert,
        Mode::Visual | Mode::VisualLine => theme.mode_visual,
    };

    let mut spans = vec![
        Span::styled(
            format!(" {} ", mode.label()),
            Style::default()
                .fg(theme.bg)
                .bg(mode_colour)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(theme.surface)),
    ];

    // A focused pane explains itself here, so its keys are discoverable
    // without going to the help screen first.
    let hint = match app.focus {
        Focus::Sidebar if app.overlay.is_none() => Some(match app.sidebar_tab {
            SidebarTab::Notes => {
                "l open · h back · space toggle · E/C expand all · . reveal · t tags"
            }
            SidebarTab::Tags => "enter filter by tag · t back to the tree",
        }),
        Focus::Assistant if app.overlay.is_none() => {
            Some("type a question · enter sends · ctrl-y inserts the answer · esc leaves")
        }
        // The editor is where you start, so it has to say how to leave.
        Focus::Editor if app.overlay.is_none() && app.editor.mode == Mode::Normal => {
            Some("tab or click for the file tree · ctrl-k commands · f1 keys")
        }
        _ => None,
    };

    if let Some(status) = app.status_text() {
        spans.push(Span::styled(
            status.to_string(),
            Style::default().fg(theme.fg).bg(theme.surface),
        ));
    } else if let Some(hint) = hint {
        if app.focus == Focus::Editor && app.repo.is_some() {
            // Branch first: it is state, and the hint is only a reminder.
            spans.push(Span::styled(
                format!("⎇ {}   ", app.git_status.branch),
                Style::default().fg(theme.link).bg(theme.surface),
            ));
        }
        spans.push(Span::styled(
            hint.to_string(),
            Style::default().fg(theme.faint).bg(theme.surface),
        ));
    } else {
        let git = &app.git_status;
        if app.repo.is_some() {
            spans.push(Span::styled(
                format!("⎇ {}", git.branch),
                Style::default().fg(theme.link).bg(theme.surface),
            ));
            if !git.is_clean() {
                spans.push(Span::styled(
                    format!("  ●{}", git.changes.len()),
                    Style::default().fg(theme.accent).bg(theme.surface),
                ));
            }
            if git.ahead > 0 {
                spans.push(Span::styled(
                    format!("  ↑{}", git.ahead),
                    Style::default().fg(theme.added).bg(theme.surface),
                ));
            }
            if git.behind > 0 {
                spans.push(Span::styled(
                    format!("  ↓{}", git.behind),
                    Style::default().fg(theme.removed).bg(theme.surface),
                ));
            }
        } else {
            spans.push(Span::styled(
                "no git",
                Style::default().fg(theme.faint).bg(theme.surface),
            ));
        }
        spans.push(Span::styled(
            format!("   {}", plural(app.vault.notes.len(), "note")),
            Style::default().fg(theme.muted).bg(theme.surface),
        ));
    }

    let right = format!(
        "{}  {} words  {}:{} ",
        app.editor.pending_hint(),
        app.editor.buf.word_count(),
        app.editor.buf.row + 1,
        app.editor.buf.col + 1,
    );
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (area.width as usize).saturating_sub(used + right.chars().count());
    spans.push(Span::styled(
        " ".repeat(pad),
        Style::default().bg(theme.surface),
    ));
    spans.push(Span::styled(
        right,
        Style::default().fg(theme.muted).bg(theme.surface),
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

fn centred(area: Rect, width_pct: u16, height: u16) -> Rect {
    let w = (area.width * width_pct / 100).min(area.width.saturating_sub(4));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 3,
        width: w,
        height: h,
    }
}

fn overlay_block<'a>(theme: &Theme, title: String) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(Line::from(vec![
            Span::styled(" ", theme.dimmed()),
            Span::styled(title, theme.title()),
            Span::styled(" ", theme.dimmed()),
        ]))
        .style(Style::default().bg(theme.overlay))
}

fn draw_picker(f: &mut Frame, theme: &Theme, picker: &Picker, area: Rect) -> Rect {
    let rect = centred(area, 60, 20);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, picker.title.clone());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 2 {
        return inner;
    }

    let mut lines = vec![Line::from(vec![
        Span::styled("› ", Style::default().fg(theme.accent)),
        Span::styled(picker.query.clone(), Style::default().fg(theme.fg)),
        Span::styled("▏", Style::default().fg(theme.accent)),
    ])];

    let height = inner.height as usize - 1;
    let offset = scroll_offset(picker.cursor, picker.matches.len(), height);
    for (row, (idx, hits)) in picker.matches.iter().enumerate().skip(offset).take(height) {
        let item = &picker.items[*idx];
        let selected = row == picker.cursor;
        let mut spans = vec![Span::styled(
            if selected { "▌ " } else { "  " },
            Style::default().fg(theme.accent),
        )];
        // Highlight the characters the query actually matched.
        for (i, c) in item.label.chars().enumerate() {
            let matched = hits.contains(&i);
            let style = if matched {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(theme.heading)
            } else {
                Style::default().fg(theme.fg)
            };
            spans.push(Span::styled(c.to_string(), style));
        }
        if !item.detail.is_empty() {
            spans.push(Span::styled(format!("  {}", item.detail), theme.faded()));
        }
        let mut line = Line::from(spans);
        if selected {
            line = line.style(Style::default().bg(theme.selection));
        }
        lines.push(line);
    }
    if picker.matches.is_empty() {
        lines.push(Line::from(Span::styled("  no matches", theme.faded())));
    }
    f.render_widget(Paragraph::new(lines), inner);
    inner
}

fn draw_search(f: &mut Frame, theme: &Theme, pane: &crate::app::SearchPane, area: Rect) -> Rect {
    let rect = centred(area, 74, 22);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, format!("search · {} hits", pane.hits.len()));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 2 {
        return inner;
    }
    let width = inner.width as usize;
    let mut lines = vec![Line::from(vec![
        Span::styled("/ ", Style::default().fg(theme.accent)),
        Span::styled(pane.query.clone(), Style::default().fg(theme.fg)),
        Span::styled("▏", Style::default().fg(theme.accent)),
    ])];

    let height = inner.height as usize - 1;
    let offset = scroll_offset(pane.cursor, pane.hits.len(), height);
    for (i, hit) in pane.hits.iter().enumerate().skip(offset).take(height) {
        let selected = i == pane.cursor;
        let head = format!("{}:{}", hit.title, hit.line + 1);
        let mut spans = vec![
            Span::styled(
                if selected { "▌ " } else { "  " },
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                fit(&head, width / 3),
                if selected {
                    Style::default()
                        .fg(theme.heading)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.link)
                },
            ),
            Span::styled("  ", theme.faded()),
        ];
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        spans.push(Span::styled(
            fit(&hit.context, width.saturating_sub(used)),
            theme.dimmed(),
        ));
        let mut line = Line::from(spans);
        if selected {
            line = line.style(Style::default().bg(theme.selection));
        }
        lines.push(line);
    }
    if pane.query.is_empty() {
        lines.push(Line::from(Span::styled(
            "  type to search every note",
            theme.faded(),
        )));
    } else if pane.hits.is_empty() {
        lines.push(Line::from(Span::styled("  nothing found", theme.faded())));
    }
    f.render_widget(Paragraph::new(lines), inner);
    inner
}

fn draw_prompt(f: &mut Frame, theme: &Theme, prompt: &crate::app::Prompt, area: Rect) -> Rect {
    let rect = centred(area, 50, 5);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, prompt.title.clone());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines = vec![
        Line::from(vec![
            Span::styled("› ", Style::default().fg(theme.accent)),
            Span::styled(prompt.input.clone(), Style::default().fg(theme.fg)),
            Span::styled("▏", Style::default().fg(theme.accent)),
        ]),
        Line::from(Span::styled(prompt.hint.clone(), theme.faded())),
    ];
    f.render_widget(Paragraph::new(lines), inner);
    inner
}

fn draw_git(f: &mut Frame, theme: &Theme, pane: &crate::app::GitPane, area: Rect) -> Rect {
    let rect = centred(area, 74, 24);
    f.render_widget(Clear, rect);
    let snap = &pane.snapshot;
    let block = overlay_block(
        theme,
        format!(
            "git · {} · {} staged of {} change{}{}",
            snap.branch,
            snap.staged_count(),
            snap.changes.len(),
            if snap.changes.len() == 1 { "" } else { "s" },
            if snap.has_remote { "" } else { " · no remote" },
        ),
    );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let width = inner.width as usize;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "space stage · a all · c commit · d diff · enter open · P push · p pull · X discard",
        theme.faded(),
    )));
    lines.push(Line::from(""));

    if snap.changes.is_empty() {
        lines.push(Line::from(Span::styled(
            "  working tree clean",
            Style::default().fg(theme.added),
        )));
    }
    for (i, change) in snap.changes.iter().enumerate().take(12) {
        let selected = i == pane.cursor;
        let colour = match change.status {
            crate::git::Status::Added => theme.added,
            crate::git::Status::Deleted => theme.removed,
            crate::git::Status::Conflicted => theme.broken,
            crate::git::Status::Untracked => theme.muted,
            _ => theme.modified,
        };
        let mut line = Line::from(vec![
            Span::styled(
                if selected { "▌ " } else { "  " },
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                if change.staged {
                    "staged  "
                } else {
                    "        "
                },
                Style::default().fg(theme.added),
            ),
            Span::styled(
                format!("{} ", change.status.glyph()),
                Style::default().fg(colour),
            ),
            Span::styled(
                fit(&change.path, width.saturating_sub(14)),
                Style::default().fg(theme.fg),
            ),
        ]);
        if selected {
            line = line.style(Style::default().bg(theme.selection));
        }
        lines.push(line);
    }

    if !pane.log.is_empty() {
        lines.push(Line::from(""));
        lines.push(section(theme, "recent commits"));
        for commit in pane.log.iter().take(6) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", commit.short),
                    Style::default().fg(theme.link),
                ),
                Span::styled(
                    fit(&commit.subject, width.saturating_sub(24)),
                    Style::default().fg(theme.fg),
                ),
                Span::styled(format!("  {}", commit.when), theme.faded()),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
    inner
}

/// A unified diff, coloured the way `git diff` does.
fn draw_diff(
    f: &mut Frame,
    theme: &Theme,
    title: &str,
    body: &str,
    scroll: u16,
    area: Rect,
) -> Rect {
    let rect = centred(area, 84, 28);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, title.to_string());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = body
        .lines()
        .map(|raw| {
            let style = if raw.starts_with("+++") || raw.starts_with("---") {
                theme.dimmed().add_modifier(Modifier::BOLD)
            } else if raw.starts_with("@@") {
                Style::default().fg(theme.link)
            } else if raw.starts_with('+') {
                Style::default().fg(theme.added)
            } else if raw.starts_with('-') {
                Style::default().fg(theme.removed)
            } else if raw.starts_with("diff ") || raw.starts_with("index ") {
                theme.faded()
            } else {
                Style::default().fg(theme.fg)
            };
            Line::from(Span::styled(raw.to_string(), style))
        })
        .collect();
    // Never scroll past the last screenful.
    let max = lines.len().saturating_sub(inner.height as usize) as u16;
    f.render_widget(Paragraph::new(lines).scroll((scroll.min(max), 0)), inner);
    inner
}

fn draw_history(f: &mut Frame, theme: &Theme, log: &[crate::git::Commit], area: Rect) -> Rect {
    let rect = centred(area, 70, 20);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, format!("history · {} commits", log.len()));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let width = inner.width as usize;
    let lines: Vec<Line> = log
        .iter()
        .map(|c| {
            Line::from(vec![
                Span::styled(format!("  {} ", c.short), Style::default().fg(theme.link)),
                Span::styled(
                    fit(&c.subject, width.saturating_sub(30)),
                    Style::default().fg(theme.fg),
                ),
                Span::styled(format!("  {} · {}", c.author, c.when), theme.faded()),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
    inner
}

fn draw_confirm(f: &mut Frame, theme: &Theme, confirm: &crate::app::Confirm, area: Rect) -> Rect {
    let rect = centred(area, 50, 5);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, "confirm".into());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines = vec![
        Line::from(Span::styled(
            confirm.message.clone(),
            Style::default().fg(theme.fg),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y",
                Style::default()
                    .fg(theme.added)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" yes    ", theme.dimmed()),
            Span::styled(
                "esc",
                Style::default()
                    .fg(theme.removed)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", theme.dimmed()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
    inner
}

/// A context menu, drawn at the point that was right-clicked and nudged back
/// on screen when that point is near an edge.
fn draw_menu(f: &mut Frame, theme: &Theme, menu: &crate::app::Menu, area: Rect) -> Rect {
    // Wide enough for the longest label *and* its shortcut, so a hint never
    // pushes the label it belongs to out of the menu.
    let width = menu
        .items
        .iter()
        .map(|i| {
            let hint = i
                .disabled
                .as_deref()
                .or(i.shortcut())
                .map(|s| s.width() + 2)
                .unwrap_or(0);
            i.label.width() + 4 + hint
        })
        .chain(std::iter::once(menu.title.width() + 6))
        .max()
        .unwrap_or(20)
        // Not `clamp`: on a terminal narrower than the minimum, min would
        // exceed max and clamp panics. The available width always wins.
        .max(16)
        .min(area.width.saturating_sub(2).max(1) as usize) as u16;
    // +2 for the border. Clamped to the area, which is what makes the menu
    // scroll rather than overflow.
    let height = (menu.items.len() as u16 + 2).min(area.height);

    let (cx, cy) = menu.at;
    let x = cx.min(area.right().saturating_sub(width)).max(area.x);
    // Prefer below the pointer; flip above when there is no room below.
    let y = if cy + height <= area.bottom() {
        cy
    } else {
        cy.saturating_sub(height)
    };
    // Whatever the pointer said, the menu has to be inside the area: a rect
    // that leaves it is a panic in the buffer, not a cosmetic problem.
    let y = y.clamp(area.y, area.bottom().saturating_sub(height).max(area.y));
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

    f.render_widget(Clear, rect);
    let block = overlay_block(theme, menu.title.clone());
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    // A menu can outgrow the terminal, so it scrolls like every other list
    // here. The offset must be the one the mouse handler recomputes, or a
    // click lands on a different entry than the one under the pointer.
    let visible = inner.height as usize;
    let offset = scroll_offset(menu.cursor, menu.items.len(), visible);
    let lines: Vec<Line> = menu
        .items
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(i, item)| {
            let selected = i == menu.cursor;
            // A greyed entry shows why instead of its key: the reason is the
            // useful thing, and it cannot be run anyway.
            let hint = item.disabled.as_deref().or(item.shortcut()).unwrap_or("");
            let room = (inner.width as usize).saturating_sub(2 + hint.width());
            let label = fit(&item.label, room);
            // The hint sits against the right edge, so the eye can run down
            // the keys without reading the labels.
            let gap = room.saturating_sub(label.width());
            let mut line = Line::from(vec![
                Span::styled(
                    if selected { "▌ " } else { "  " },
                    Style::default().fg(theme.accent),
                ),
                Span::styled(
                    label,
                    match (selected, item.is_enabled()) {
                        (_, false) => Style::default().fg(theme.faint),
                        (true, _) => theme.selected(),
                        (false, _) => Style::default().fg(theme.fg),
                    },
                ),
                Span::styled(" ".repeat(gap), Style::default()),
                Span::styled(hint.to_string(), theme.faded()),
            ]);
            if selected {
                line = line.style(Style::default().bg(theme.selection));
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
    inner
}

/// What a link points at, drawn over the note rather than instead of it.
///
/// Centred rather than beside the link: a popover that follows the cursor has
/// to decide what to do when the cursor is at the bottom of the screen, and a
/// reader who pressed a key already knows where they pressed it.
fn draw_peek(f: &mut Frame, theme: &Theme, peek: &crate::app::Peek, area: Rect) {
    // A measure rather than a percentage: a summary is prose, and prose is
    // unreadable stretched across a wide terminal.
    let width = area.width.saturating_sub(8).clamp(0, 64).max(20);
    let text_width = width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        fit(&peek.title, text_width),
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
    ))];
    if !peek.detail.is_empty() {
        lines.push(Line::from(Span::styled(
            fit(&peek.detail, text_width),
            theme.faded(),
        )));
    }
    if !peek.body.is_empty() {
        lines.push(Line::from(""));
        for row in wrap_text(&peek.body, text_width).into_iter().take(5) {
            lines.push(Line::from(Span::styled(
                row,
                Style::default().fg(theme.muted),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        match (&peek.open, &peek.create) {
            (Some(_), _) => "enter opens · esc dismisses",
            (None, Some(_)) => "enter writes it · esc dismisses",
            _ => "esc dismisses",
        },
        theme.faded(),
    )));

    let height = (lines.len() as u16 + 2).min(area.height);
    let pct = ((width as u32 * 100) / area.width.max(1) as u32).min(100) as u16;
    let rect = centred(area, pct, height);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focus))
        .style(Style::default().bg(theme.surface))
        .title(Span::styled(" peek ", Style::default().fg(theme.accent)));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_help(f: &mut Frame, theme: &Theme, area: Rect) {
    let rect = centred(area, 62, (HELP.len() as u16 + 3).min(area.height));
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, "trafford · keys".into());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(keys, what)| {
            if keys.is_empty() {
                Line::from(Span::styled(
                    what.to_string(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(format!("  {:<20}", keys), Style::default().fg(theme.link)),
                    Span::styled(what.to_string(), Style::default().fg(theme.fg)),
                ])
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Left), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Confirm, ConfirmKind, GitPane, PickItem, Prompt, PromptKind, SearchPane};
    use crate::config::Config;
    use crate::testing::TempDir;
    use crate::vault::Vault;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn preview_of(lines: &[&str], width: usize) -> (Theme, PreviewView) {
        folded_preview_of(lines, width, &[])
    }

    /// A preview with the headings on `shut` collapsed.
    fn folded_preview_of(lines: &[&str], width: usize, shut: &[usize]) -> (Theme, PreviewView) {
        let theme = Theme::default();
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let resolves = |t: &str| t != "Nowhere";
        let renderer = Renderer {
            theme: &theme,
            resolves: &resolves,
            conceal: true,
        };
        let set: std::collections::HashSet<usize> = shut.iter().copied().collect();
        let view = PreviewView::build(&owned, &renderer, width, Some(&set));
        (theme, view)
    }

    #[test]
    fn preview_folds_what_it_draws_not_what_was_written() {
        // The source is 44 characters and would take two rows at this width;
        // rendered it is 24 and takes one. Folding the source would leave a
        // ragged break in the middle of a line that fits.
        let (_t, view) = preview_of(&["see [[Some/Long/Path/Note|a note]] there"], 30);
        assert_eq!(view.lines[0].text, "see a note there");
        assert_eq!(
            view.layout.row_count(),
            1,
            "it fits once the syntax is gone"
        );
    }

    #[test]
    fn a_click_in_preview_finds_the_link_that_was_drawn() {
        let (_t, view) = preview_of(&["see [[Note|alias]] there"], 40);
        let texts = view.texts();
        // Column 5 is inside "alias" as drawn.
        let (line, col) = view.layout.source_of(&texts, 0, 5);
        let link = view.link_at(line, col).expect("a link there");
        assert_eq!(link.target, "Note");
        // And column 0 is not.
        let (line, col) = view.layout.source_of(&texts, 0, 0);
        assert!(view.link_at(line, col).is_none());
    }

    #[test]
    fn a_link_split_across_a_fold_is_clickable_on_both_rows() {
        // Narrow enough that the link's own text has to break.
        let (_t, view) = preview_of(&["x [[Note|a rather long alias here]] y"], 14);
        assert!(view.layout.row_count() > 1, "it has to actually fold");
        let texts = view.texts();
        let mut found = 0;
        for visual in 0..view.layout.row_count() {
            let row = view.layout.row(visual).unwrap();
            for column in 0..row.len {
                let (line, col) = view.layout.source_of(&texts, visual, column + row.indent);
                if view.link_at(line, col).map(|l| l.target.as_str()) == Some("Note") {
                    found += 1;
                }
            }
        }
        assert_eq!(
            found,
            "a rather long alias here".chars().count(),
            "every drawn character of the link answers to it, on whichever row it landed"
        );
    }

    #[test]
    fn a_heading_link_keeps_the_heading_concealment_threw_away() {
        let (_t, view) = preview_of(&["[[Note#Some Section|go]]"], 40);
        assert_eq!(view.lines[0].text, "go");
        let link = view.link_at(0, 0).unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.heading.as_deref(), Some("Some Section"));
    }

    #[test]
    fn a_fenced_block_keeps_its_syntax_in_preview() {
        let (_t, view) = preview_of(&["```", "**not bold**", "```", "**bold**"], 40);
        assert_eq!(view.lines[1].text, "**not bold**", "inside the fence");
        assert_eq!(view.lines[3].text, "bold", "after it");
    }

    const FRONTMATTER: [&str; 9] = [
        "---",
        "created: 2026-03-22",
        "tags:",
        "  - status/active",
        "  - type/reference",
        "---",
        "",
        "# Title",
        "body",
    ];

    #[test]
    fn frontmatter_collapses_to_the_things_a_reader_uses() {
        let (_t, view) = preview_of(&FRONTMATTER, 60);
        let drawn: Vec<&str> = view.lines.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            drawn,
            [
                "#status/active · #type/reference",
                "created 2026-03-22",
                "",
                "▾ Title",
                "body",
            ],
            "six lines of YAML became two"
        );
    }

    #[test]
    fn a_property_tag_is_clickable_and_names_the_tag() {
        let (_t, view) = preview_of(&FRONTMATTER, 60);
        let link = view.link_at(0, 2).expect("a tag under the pointer");
        assert_eq!(
            link.target, "status/active",
            "no leading hash in the target"
        );
        assert_eq!(link.kind, markdown::Target::Tag);
        // The second chip answers too, and the separator between them does not.
        assert_eq!(
            view.link_at(0, 20).map(|l| l.target.as_str()),
            Some("type/reference")
        );
        assert!(view.link_at(0, 16).is_none(), "the separator is not a tag");
    }

    #[test]
    fn a_key_nobody_anticipated_is_still_shown() {
        let (_t, view) = preview_of(&["---", "banana: yellow", "---", "body"], 60);
        assert_eq!(view.lines[0].text, "banana yellow");
    }

    #[test]
    fn a_note_without_frontmatter_is_untouched() {
        let (_t, view) = preview_of(&["# Title", "body"], 60);
        assert_eq!(view.lines.len(), 2);
        assert_eq!(view.sources, vec![0, 1]);
    }

    #[test]
    fn an_unterminated_block_falls_back_to_its_source() {
        let (_t, view) = preview_of(&["---", "tags: one", "# never closed"], 60);
        assert_eq!(view.lines[0].text, "---", "shown as written");
        assert_eq!(view.lines.len(), 3);
    }

    #[test]
    fn a_click_on_a_property_lands_at_the_top_of_the_block() {
        let (_t, view) = preview_of(&FRONTMATTER, 60);
        assert_eq!(view.source(0), 0);
        assert_eq!(view.source(1), 0);
        assert_eq!(view.source(3), 7, "the heading still names its own line");
    }

    fn crumb(lines: &[&str], top: usize, visible: &[usize], width: usize) -> Option<String> {
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let heads = fold::headings(&owned);
        sticky(&heads, owned.len(), top, visible, width).map(|(_, text)| text)
    }

    const DOC: [&str; 9] = [
        "# Title",
        "intro",
        "## One",
        "a",
        "b",
        "### One A",
        "c",
        "## Two",
        "d",
    ];

    #[test]
    fn the_crumb_names_the_sections_the_top_line_is_inside() {
        assert_eq!(
            crumb(&DOC, 6, &[6, 7, 8], 60).as_deref(),
            Some("Title  ›  One  ›  One A")
        );
    }

    #[test]
    fn the_crumb_hides_when_its_own_heading_is_on_screen() {
        // Line 5 *is* "### One A", so a crumb ending in it would print the
        // same words immediately above themselves.
        assert_eq!(crumb(&DOC, 6, &[5, 6, 7], 60), None);
    }

    #[test]
    fn a_line_under_no_heading_gets_no_crumb() {
        assert_eq!(crumb(&["intro", "# Title"], 0, &[0, 1], 60), None);
    }

    #[test]
    fn a_narrow_pane_drops_the_outer_sections_first() {
        // The deepest heading is the one that says where you are, so it is the
        // last thing to go.
        let out = crumb(&DOC, 6, &[6], 20).unwrap();
        assert!(out.contains("One A"), "{out:?}");
        assert!(!out.contains("Title"), "{out:?}");
        assert!(out.starts_with('…'), "{out:?}");
    }

    #[test]
    fn the_crumb_records_where_each_section_can_be_clicked() {
        let owned: Vec<String> = DOC.iter().map(|s| s.to_string()).collect();
        let heads = fold::headings(&owned);
        let (spots, text) = sticky(&heads, owned.len(), 6, &[6], 60).unwrap();
        assert_eq!(spots.len(), 3);
        for (a, b, row) in &spots {
            let named: String = text.chars().skip(*a).take(b - a).collect();
            let heading = heads.iter().find(|h| h.row == *row).unwrap();
            assert_eq!(named, heading.text, "the columns name the heading");
        }
    }

    #[test]
    fn a_callout_draws_a_line_for_each_line_it_covers() {
        let (_t, view) = preview_of(&["before", "> [!tip] Watch out", "> body", "after"], 60);
        assert_eq!(
            view.lines
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>(),
            ["before", "▎ TIP · Watch out", "▎ body", "after"]
        );
        assert_eq!(
            view.sources,
            vec![0, 1, 2, 3],
            "the mapping is the identity"
        );
    }

    #[test]
    fn a_quote_that_is_not_a_callout_still_draws_as_a_quote() {
        let (_t, view) = preview_of(&["> just quoting"], 60);
        assert_eq!(view.lines[0].text, "> just quoting");
    }

    #[test]
    fn a_callout_inside_a_fence_is_left_alone() {
        let (_t, view) = preview_of(&["```", "> [!note]", "```"], 60);
        assert_eq!(view.lines[1].text, "> [!note]");
    }

    #[test]
    fn a_link_in_a_callout_is_still_clickable_past_the_bar() {
        let (_t, view) = preview_of(&["> [!note]", "> see [[Note]]"], 60);
        assert_eq!(view.lines[1].text, "▎ see Note");
        assert_eq!(
            view.link_at(1, 6).map(|l| l.target.as_str()),
            Some("Note"),
            "the bar moved it along, and the map moved with it"
        );
    }

    const SECTIONED: [&str; 9] = [
        "# Title",
        "intro",
        "## One",
        "a",
        "b",
        "### One A",
        "c",
        "## Two",
        "d",
    ];

    #[test]
    fn every_heading_says_whether_it_can_be_opened() {
        let (_t, view) = preview_of(&SECTIONED, 40);
        assert_eq!(view.lines[0].text, "▾ Title");
        assert_eq!(view.lines[2].text, "▾ One");
        assert_eq!(view.lines[5].text, "▾ One A");
        assert_eq!(
            view.lines[7].text, "▾ Two",
            "a section with one line under it"
        );
        // A heading with nothing under it gets no marker, because there is
        // nothing behind it to promise.
        let (_t, empty) = preview_of(&["## Nothing"], 40);
        assert_eq!(empty.lines[0].text, "  Nothing");
    }

    #[test]
    fn a_folded_section_draws_as_one_line_and_says_what_it_hides() {
        let (_t, view) = folded_preview_of(&SECTIONED, 40, &[2]);
        let drawn: Vec<&str> = view.lines.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(drawn, ["▾ Title", "intro", "▸ One  4 lines", "▾ Two", "d"]);
        assert_eq!(
            view.sources,
            vec![0, 1, 2, 7, 8],
            "the lines that survived still name their own rows"
        );
    }

    #[test]
    fn folding_an_outer_heading_takes_the_inner_ones_with_it() {
        let (_t, view) = folded_preview_of(&SECTIONED, 40, &[0]);
        assert_eq!(
            view.lines
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>(),
            ["▸ Title  8 lines"]
        );
    }

    #[test]
    fn one_hidden_line_is_not_reported_as_lines() {
        let (_t, view) = folded_preview_of(&["## Two", "d"], 40, &[0]);
        assert_eq!(view.lines[0].text, "▸ Two  1 line");
    }

    #[test]
    fn a_fold_marker_does_not_move_a_link_out_from_under_the_pointer() {
        let (_t, view) = preview_of(&["## See [[Note|alias]]", "body"], 40);
        assert_eq!(view.lines[0].text, "▾ See alias");
        // "alias" starts at drawn column 6, two of which are the marker.
        assert_eq!(view.link_at(0, 6).map(|l| l.target.as_str()), Some("Note"));
        assert!(view.link_at(0, 0).is_none(), "the marker is not the link");
    }

    #[test]
    fn a_hash_inside_a_fence_does_not_become_a_foldable_section() {
        let (_t, view) = preview_of(&["```sh", "# not a heading", "```", "after"], 40);
        assert_eq!(view.lines[1].text, "# not a heading");
        assert_eq!(view.lines.len(), 4);
    }

    #[test]
    fn a_line_inside_a_fold_is_stood_for_by_the_heading_that_hides_it() {
        let src = ["# Title", "intro", "## One", "a", "b", "c", "## Two", "d"];
        let (_t, open) = preview_of(&src, 40);
        // Unfolded, line 4 is drawn and stands for itself.
        assert_eq!(
            open.source(open.layout.row(open.row_of_source(4)).unwrap().line),
            4
        );

        // Folded, line 4 is inside "## One" — which is where the reader
        // belongs, not "## Two" beyond it.
        let (_t, shut) = folded_preview_of(&src, 40, &[2]);
        let row = shut.row_of_source(4);
        let line = shut.layout.row(row).unwrap().line;
        assert_eq!(shut.source(line), 2, "the heading that hides it");
        assert!(shut.lines[line].text.starts_with('▸'));
    }

    #[test]
    fn a_table_draws_more_lines_than_it_occupies_and_still_maps_back() {
        let (_t, view) = preview_of(
            &[
                "before",
                "| a | b |",
                "| --- | --- |",
                "| 1 | 2 |",
                "| 3 | 4 |",
                "after",
            ],
            40,
        );
        // Six source lines; the table's four become six drawn ones.
        assert_eq!(view.lines.len(), 8);
        assert_eq!(
            view.sources,
            vec![0, 1, 1, 2, 3, 4, 4, 5],
            "each drawn line names the note line it belongs to"
        );
        // Rules carry no number; the header and rows do.
        assert_eq!(
            view.numbered,
            vec![true, false, true, false, true, true, false, true]
        );
        assert!(view.lines[1].text.starts_with('┌'));
        assert!(view.lines[7].text == "after");
    }

    #[test]
    fn a_table_too_wide_for_the_pane_falls_back_to_its_pipes() {
        let (_t, view) = preview_of(
            &[
                "| a | b | c | d | e |",
                "| - | - | - | - | - |",
                "| 1 | 2 | 3 | 4 | 5 |",
            ],
            14,
        );
        assert_eq!(view.lines.len(), 3, "one drawn line per source line again");
        assert!(
            view.lines[0].text.contains('|'),
            "the pipes are still the table"
        );
        assert_eq!(view.sources, vec![0, 1, 2]);
    }

    #[test]
    fn pipes_inside_a_fence_are_not_a_table() {
        let (_t, view) = preview_of(
            &["```", "| a | b |", "| --- | --- |", "| 1 | 2 |", "```"],
            40,
        );
        assert_eq!(view.lines.len(), 5, "nothing was drawn as a grid");
        assert_eq!(view.lines[1].text, "| a | b |");
    }

    #[test]
    fn scrolling_to_a_note_line_finds_the_row_that_shows_it() {
        let (_t, view) = preview_of(&["before", "| a |", "| --- |", "| 1 |", "after"], 40);
        // Note line 3 is the table's only data row, drawn fourth overall.
        assert_eq!(view.row_of_source(3), 4);
        assert_eq!(view.source(4), 3);
    }

    #[test]
    fn wrap_column_is_a_measure_not_a_promise() {
        assert_eq!(wrap_width(80, 0), 80, "0 means the pane");
        assert_eq!(
            wrap_width(200, 72),
            72,
            "a measure holds prose in on a wide screen"
        );
        // Never wider than the pane: the fold would put characters where there
        // is no room to draw them, and the caret would follow them off-screen.
        assert_eq!(wrap_width(40, 72), 40);
        assert_eq!(wrap_width(0, 72), 0);
    }

    fn app_with_every_pane_open() -> (TempDir, App) {
        let dir = TempDir::with_files(&[
            ("Welcome.md", "---\ntags: [meta]\n---\n# Welcome\n\nSee [[Other]] and [[Ghost]].\n\n- [ ] a task\n\n```rust\nlet x = 1;\n```\n"),
            ("Other.md", "# Other\n\nBack to [[Welcome]].\n"),
        ]);
        let vault = Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, Config::default());
        app.sidebar_visible = true;
        app.context_visible = true;
        app.chat.messages.push(crate::llm::Message {
            role: crate::llm::Role::User,
            text: "a question long enough to need wrapping in a narrow pane".into(),
        });
        app.chat.messages.push(crate::llm::Message {
            role: crate::llm::Role::Assistant,
            text: "an answer citing [[Welcome]] with `code` and a very long unbroken token                    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
        });
        (dir, app)
    }

    /// Whether a drawn row carries a line number in its gutter.
    ///
    /// The gutter sits after a pane border partway along the row, so this looks
    /// for `│`, then padding, then digits, then a space — the padding is what
    /// separates it from the sidebar's `│2 notes · …`.
    fn numbered_row(line: &str) -> bool {
        line.match_indices('│').any(|(i, _)| {
            let rest = &line[i + '│'.len_utf8()..];
            let after = rest.trim_start_matches(' ');
            if after.len() == rest.len() {
                return false;
            }
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            !digits.is_empty() && after[digits.len()..].starts_with(' ')
        })
    }

    /// Draw at `width`x`height` and read the screen back, the way the pty probe
    /// does but without leaving the process.
    fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// An app on a note long enough to scroll, already in the reading view.
    fn reading_a_long_note() -> (TempDir, App) {
        let mut body = String::from("---\ntags: [meta]\n---\n# Long\n\n");
        for section in 1..=6 {
            body.push_str(&format!("## Section {section}\n\n"));
            for line in 1..=12 {
                body.push_str(&format!("Paragraph {section}.{line} of the note.\n\n"));
            }
        }
        let dir = TempDir::with_files(&[("Long.md", &body)]);
        let vault = Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, Config::default());
        app.open_note("Long.md", false);
        app.run_command("toggle-preview");
        screen(&mut app, 90, 14);
        (dir, app)
    }

    #[test]
    fn jumping_into_a_folded_section_opens_it() {
        let (_dir, mut app) = reading_a_long_note();
        app.fold_all();
        screen(&mut app, 90, 14);

        // Somewhere well inside the third section, which is collapsed.
        let heads = crate::ui::fold::headings(&app.editor.buf.lines);
        let third = heads.iter().filter(|h| h.level == 2).nth(2).unwrap().row;
        let target = third + 4;
        assert!(
            app.folded.is_folded("Long.md", third),
            "the section should start out shut"
        );

        app.jump_to(target);
        screen(&mut app, 90, 14);
        assert!(!app.folded.is_folded("Long.md", third), "the fold opened");

        let view = app.preview_view.as_ref().unwrap();
        let drawn: Vec<usize> = view.sources.clone();
        assert!(
            drawn.contains(&target),
            "and the line is actually drawn now"
        );
    }

    #[test]
    fn jumping_leaves_folds_that_are_not_in_the_way_alone() {
        let (_dir, mut app) = reading_a_long_note();
        app.fold_all();
        screen(&mut app, 90, 14);
        let heads = crate::ui::fold::headings(&app.editor.buf.lines);
        let sections: Vec<usize> = heads
            .iter()
            .filter(|h| h.level == 2)
            .map(|h| h.row)
            .collect();

        app.jump_to(sections[1] + 3);
        screen(&mut app, 90, 14);
        assert!(
            !app.folded.is_folded("Long.md", sections[1]),
            "the one in the way"
        );
        assert!(
            app.folded.is_folded("Long.md", sections[3]),
            "a section elsewhere stays as the reader left it"
        );
    }

    #[test]
    fn jumping_to_a_line_that_is_not_hidden_folds_nothing() {
        let (_dir, mut app) = reading_a_long_note();
        screen(&mut app, 90, 14);
        app.jump_to(6);
        assert!(
            app.folded
                .of("Long.md")
                .map(|f| f.is_empty())
                .unwrap_or(true),
            "nothing was folded, so nothing needed opening"
        );
    }

    #[test]
    fn a_redraw_does_not_undo_a_scroll() {
        // The bug this replaces: the view was re-anchored to the buffer cursor
        // on every draw, so the wheel moved it and it was put straight back
        // before anyone saw.
        let (_dir, mut app) = reading_a_long_note();
        assert!(app.scroll_preview(9));
        assert_eq!(app.preview_row, 9);
        screen(&mut app, 90, 14);
        assert_eq!(app.preview_row, 9, "a draw must not move the reader");
        screen(&mut app, 90, 14);
        assert_eq!(app.preview_row, 9);
    }

    #[test]
    fn reading_motions_move_within_the_drawn_document() {
        let (_dir, mut app) = reading_a_long_note();
        let rows = app.preview_view.as_ref().unwrap().layout.row_count();
        assert!(rows > 40, "the fixture should be longer than a screen");

        app.preview_to_end(true);
        assert_eq!(app.preview_row, rows - 1, "G goes to the last drawn row");
        app.preview_to_end(false);
        assert_eq!(app.preview_row, 0, "gg to the first");

        app.preview_page(true);
        let paged = app.preview_row;
        assert!(paged > 0 && paged < rows, "ctrl-d moved by a screenful");
        app.preview_page(false);
        assert_eq!(app.preview_row, 0, "and ctrl-u came back");
    }

    #[test]
    fn reading_motions_stop_at_the_ends() {
        let (_dir, mut app) = reading_a_long_note();
        let rows = app.preview_view.as_ref().unwrap().layout.row_count();
        app.scroll_preview(-5);
        assert_eq!(app.preview_row, 0, "not above the first row");
        app.scroll_preview(rows as isize * 2);
        assert_eq!(app.preview_row, rows - 1, "nor past the last");
    }

    #[test]
    fn the_buffer_cursor_follows_what_is_being_read() {
        let (_dir, mut app) = reading_a_long_note();
        app.preview_page(true);
        app.preview_page(true);
        screen(&mut app, 90, 14);
        let read = app.preview_source();
        assert!(read > 5, "we scrolled somewhere");
        assert_eq!(
            app.editor.buf.row, read,
            "leaving preview must land where the reader was"
        );
    }

    #[test]
    fn folding_under_the_reader_keeps_their_place() {
        let (_dir, mut app) = reading_a_long_note();
        app.preview_page(true);
        app.preview_page(true);
        screen(&mut app, 90, 14);
        let before = app.preview_source();

        app.fold_all();
        screen(&mut app, 90, 14);
        let after = app.preview_source();
        assert!(
            after <= before,
            "folding should not throw the reader forwards: {before} -> {after}"
        );
        // The line they were on is inside the section now standing for it.
        let heads = crate::ui::fold::headings(&app.editor.buf.lines);
        let end = crate::ui::fold::section_end(&heads, after, app.editor.buf.line_count());
        assert!(
            after <= before && before < end,
            "{before} should be inside the section at {after}..{end}"
        );
    }

    #[test]
    fn reading_hides_the_panes_and_puts_them_back() {
        let (_dir, mut app) = app_with_every_pane_open();
        app.open_note("Welcome.md", false);
        assert!(screen(&mut app, 120, 20)
            .iter()
            .any(|l| l.contains("OUTLINE")));

        app.run_command("toggle-preview");
        let reading = screen(&mut app, 120, 20);
        assert!(
            !reading.iter().any(|l| l.contains("OUTLINE")),
            "context pane gone"
        );
        assert!(
            !reading.iter().any(|l| l.contains("notes ·")),
            "sidebar gone"
        );

        app.run_command("toggle-preview");
        assert!(screen(&mut app, 120, 20)
            .iter()
            .any(|l| l.contains("OUTLINE")));
    }

    #[test]
    fn a_pane_toggled_by_hand_while_reading_stays_that_way() {
        let (_dir, mut app) = app_with_every_pane_open();
        app.open_note("Welcome.md", false);
        app.run_command("toggle-preview");
        app.run_command("toggle-sidebar");
        assert!(app.sidebar_visible, "the reader asked for it back");
        app.run_command("toggle-preview");
        assert!(
            app.sidebar_visible,
            "and leaving preview must not argue with them"
        );
    }

    #[test]
    fn reading_drops_the_line_numbers() {
        let (_dir, mut app) = app_with_every_pane_open();
        app.open_note("Welcome.md", false);
        let numbered = |app: &mut App| screen(app, 100, 16).iter().any(|l| numbered_row(l));
        assert!(numbered(&mut app), "the editor numbers its lines");
        app.run_command("toggle-preview");
        assert!(!numbered(&mut app), "reading does not");
    }

    #[test]
    fn reading_holds_prose_to_a_measure_on_a_wide_terminal() {
        let (_dir, mut app) = app_with_every_pane_open();
        app.open_note("Welcome.md", false);
        app.run_command("toggle-preview");
        let wide = screen(&mut app, 160, 16);
        // Text starts well inside the pane rather than against its left edge.
        // Every row opens with the pane's border, so measure past it.
        let indents: Vec<usize> = wide
            .iter()
            .filter(|l| l.contains("Welcome") && !l.contains('╭'))
            .map(|l| {
                let body = l.trim_start_matches('│');
                body.len() - body.trim_start().len()
            })
            .collect();
        assert!(!indents.is_empty(), "the note should be on screen");
        assert!(
            indents.iter().all(|i| *i > 20),
            "prose should be centred, indents were {indents:?}"
        );
    }

    #[test]
    fn the_chrome_can_be_kept_for_anyone_who_wants_it() {
        let (_dir, mut app) = app_with_every_pane_open();
        app.config.reading_focus = false;
        app.open_note("Welcome.md", false);
        app.run_command("toggle-preview");
        let reading = screen(&mut app, 120, 20);
        assert!(
            reading.iter().any(|l| l.contains("OUTLINE")),
            "the panes stayed"
        );
        assert!(
            reading.iter().any(|l| numbered_row(l)),
            "and so did the line numbers"
        );
    }

    /// Sizes that starve the layout, including the ones that used to panic.
    const SIZES: &[(u16, u16)] = &[
        (200, 60),
        (120, 34),
        (80, 24),
        (60, 20),
        (48, 18),
        (40, 15),
        (30, 12),
        (20, 10),
        (12, 6),
        (8, 4),
        (4, 2),
        (1, 1),
    ];

    fn render(app: &mut App, w: u16, h: u16) {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
    }

    /// Draw at every hostile size with each pane focused in turn. Focus
    /// matters: the cursor-placement branches only run for the focused pane,
    /// and those are where the width arithmetic lives.
    fn render_every_focus(app: &mut App, w: u16, h: u16) {
        for focus in [Focus::Editor, Focus::Sidebar, Focus::Assistant] {
            app.focus = focus;
            render(app, w, h);
        }
    }

    /// A pane squeezed to zero columns must not take the app down. The
    /// assistant's input cursor used to subtract from a zero width and panic
    /// on any terminal narrower than about sixty columns.
    #[test]
    fn every_pane_survives_a_starved_layout() {
        let (_dir, mut app) = app_with_every_pane_open();
        for &(w, h) in SIZES {
            for visible in [false, true] {
                app.assistant_visible = visible;
                for preview in [false, true] {
                    app.preview = preview;
                    render_every_focus(&mut app, w, h);
                }
            }
        }
    }

    #[test]
    fn every_overlay_survives_a_starved_layout() {
        let (_dir, mut app) = app_with_every_pane_open();
        let items = vec![PickItem {
            label: "a note".into(),
            detail: "a/note.md".into(),
            key: "a/note.md".into(),
        }];
        let overlays = || -> Vec<Overlay> {
            vec![
                Overlay::Help,
                Overlay::Palette(Picker::new("Commands", items.clone())),
                Overlay::Switcher(Picker::new("Open note", items.clone())),
                Overlay::Search(SearchPane {
                    query: "welcome".into(),
                    hits: vec![],
                    cursor: 0,
                }),
                Overlay::Prompt(Prompt {
                    kind: PromptKind::NewNote,
                    title: "New note".into(),
                    input: "name".into(),
                    hint: "a hint that is wider than a narrow overlay".into(),
                }),
                Overlay::Git(GitPane::default()),
                Overlay::History(vec![]),
                Overlay::Diff {
                    title: "diff · Welcome.md".into(),
                    body: "@@ -1 +1 @@\n-old\n+new\n".into(),
                    scroll: 99,
                },
                Overlay::Confirm(Confirm {
                    kind: ConfirmKind::QuitDirty,
                    message: "unsaved changes, quit anyway?".into(),
                }),
                Overlay::Menu(long_menu(40, 30)),
            ]
        };
        app.assistant_visible = true;
        for overlay in overlays() {
            app.overlay = Some(overlay);
            for &(w, h) in SIZES {
                render_every_focus(&mut app, w, h);
            }
        }
    }

    fn long_menu(items: usize, cursor: usize) -> crate::app::Menu {
        crate::app::Menu {
            title: "a menu with more entries than fit".into(),
            items: (0..items)
                .map(|i| {
                    crate::app::MenuItem::new(
                        format!("entry number {i}"),
                        crate::app::MenuAction::Command("save"),
                    )
                })
                .collect(),
            cursor,
            at: (4, 2),
        }
    }

    /// The row the menu's cursor is on, searched only inside the menu — the
    /// sidebar draws the same marker for the open note, so a search over the
    /// whole screen finds that one and always succeeds.
    fn selected_row(backend: &TestBackend, menu: Rect) -> Option<u16> {
        let buffer = backend.buffer();
        (menu.y..menu.bottom().min(buffer.area.height)).find(|y| {
            (menu.x..menu.right().min(buffer.area.width)).any(|x| buffer[(x, *y)].symbol() == "▌")
        })
    }

    /// A menu longer than the terminal used to draw its first N entries and
    /// silently drop the rest, so moving the cursor down walked it off screen.
    #[test]
    fn a_menu_taller_than_the_terminal_keeps_its_selection_visible() {
        let (_dir, mut app) = app_with_every_pane_open();
        for (rows, cursor) in [(10u16, 0usize), (10, 20), (10, 39), (6, 39), (40, 39)] {
            app.overlay = Some(Overlay::Menu(long_menu(40, cursor)));
            let mut terminal = Terminal::new(TestBackend::new(70, rows)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            assert!(
                selected_row(terminal.backend(), app.panes.overlay).is_some(),
                "entry {cursor} of 40 was off screen at {rows} rows"
            );
        }
    }

    /// The hint has to be drawn, and it has to not eat the label.
    #[test]
    fn a_menu_draws_its_shortcuts_without_truncating_labels() {
        let (_dir, mut app) = app_with_every_pane_open();
        app.overlay = Some(Overlay::Menu(crate::app::Menu {
            title: "a note".into(),
            items: vec![
                crate::app::MenuItem::new("Save", crate::app::MenuAction::Command("save")),
                crate::app::MenuItem::new("Rename…", crate::app::MenuAction::Command("rename")),
            ],
            cursor: 0,
            at: (2, 2),
        }));
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol())
            .collect();
        assert!(text.contains("Save"), "the label was lost");
        assert!(text.contains("ctrl-s"), "the shortcut was not drawn");
        assert!(
            text.contains("Rename…"),
            "an entry with no key lost its label"
        );
    }

    #[test]
    fn a_menu_that_fits_is_not_scrolled() {
        let (_dir, mut app) = app_with_every_pane_open();
        app.overlay = Some(Overlay::Menu(long_menu(4, 0)));
        let mut terminal = Terminal::new(TestBackend::new(70, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol())
            .collect();
        for i in 0..4 {
            assert!(
                text.contains(&format!("entry number {i}")),
                "entry {i} missing"
            );
        }
    }

    #[test]
    fn a_vault_with_no_notes_renders() {
        let dir = TempDir::with_files(&[]);
        let vault = Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, Config::default());
        app.assistant_visible = true;
        for &(w, h) in SIZES {
            render_every_focus(&mut app, w, h);
        }
    }

    #[test]
    fn plural_adds_an_s_only_when_it_belongs() {
        assert_eq!(plural(1, "tag"), "1 tag");
        assert_eq!(plural(0, "tag"), "0 tags");
        assert_eq!(plural(2, "note"), "2 notes");
    }

    #[test]
    fn outline_budget_always_leaves_room_for_a_few_headings() {
        // Even when the sections below want the whole pane.
        assert_eq!(outline_budget(20, 100), 3);
        assert_eq!(outline_budget(6, 100), 3);
    }

    #[test]
    fn outline_budget_yields_to_the_sections_below_it() {
        // 30 rows, one for the label, 20 wanted below: 9 left for the outline.
        assert_eq!(outline_budget(30, 20), 9);
    }

    #[test]
    fn outline_budget_never_exceeds_the_pane() {
        for height in 0..40 {
            for below in 0..40 {
                let budget = outline_budget(height, below);
                assert!(
                    budget < height.max(1) || height <= 1,
                    "height={height} below={below} budget={budget}"
                );
            }
        }
    }

    #[test]
    fn outline_budget_handles_a_pane_with_no_room() {
        assert_eq!(outline_budget(0, 5), 0);
        assert_eq!(outline_budget(1, 5), 0);
    }

    #[test]
    fn outline_collects_headings_and_skips_code_fences() {
        let buf = crate::editor::Buffer::from_text(
            "# One\n\ntext\n## Two\n```\n# not a heading\n```\n### Three\n#no-space\n",
        );
        let entries = outline_of(&buf);
        let got: Vec<(&str, usize)> = entries.iter().map(|e| (e.text.as_str(), e.level)).collect();
        assert_eq!(got, vec![("One", 1), ("Two", 2), ("Three", 3)]);
        assert_eq!(entries[0].row, 0);
    }

    #[test]
    fn compact_scales_word_counts() {
        assert_eq!(compact(940), "940 words");
        assert_eq!(compact(12_400), "12.4k words");
        assert_eq!(compact(2_000_000), "2.0m words");
    }

    #[test]
    fn fit_truncates_with_an_ellipsis() {
        assert_eq!(fit("hello", 10), "hello");
        assert_eq!(fit("hello world", 8), "hello w…");
        assert_eq!(fit("hello", 0), "");
    }

    #[test]
    fn fit_measures_wide_characters_as_two_columns() {
        // Five characters, ten columns: it fits in ten but not in nine.
        assert_eq!(fit("日本語のノ", 10), "日本語のノ");
        let cut = fit("日本語のノ", 9);
        assert!(cut.width() <= 9, "{cut:?} is {} columns", cut.width());
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn fit_never_exceeds_its_column_budget() {
        for text in [
            "ascii text here",
            "日本語のノートです。",
            "mixed 日本 text",
            "✨🌱✨🌱",
        ] {
            for width in 1..20 {
                let out = fit(text, width);
                assert!(
                    out.width() <= width,
                    "fit({text:?}, {width}) = {out:?} is {} columns",
                    out.width()
                );
            }
        }
    }

    #[test]
    fn wrapping_never_exceeds_the_pane_width() {
        for text in [
            "the quick brown fox jumps over the lazy dog",
            "日本語のノートです。日本語のノートです。",
            "a very long unbroken token aaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "ながいながいながいながいながいながいことば",
        ] {
            for width in 2..24 {
                for line in wrap_text(text, width) {
                    assert!(
                        line.width() <= width,
                        "wrap_text({text:?}, {width}) produced {line:?} at {} columns",
                        line.width()
                    );
                }
            }
        }
    }

    #[test]
    fn wrapping_preserves_the_text() {
        let text = "日本語のノートです。and some ascii";
        let joined: String = wrap_text(text, 12).concat();
        assert_eq!(joined.replace(' ', ""), text.replace(' ', ""));
    }

    #[test]
    fn scroll_offset_centres_the_cursor_once_past_the_window() {
        assert_eq!(scroll_offset(0, 100, 10), 0);
        assert_eq!(scroll_offset(50, 100, 10), 45);
        // Never scrolls past the end of the list.
        assert_eq!(scroll_offset(99, 100, 10), 90);
        // A list that fits needs no offset.
        assert_eq!(scroll_offset(3, 5, 10), 0);
    }

    #[test]
    fn wrap_breaks_on_word_boundaries() {
        let out = wrap_text("the quick brown fox", 10);
        assert!(out.iter().all(|l| l.chars().count() <= 10), "{out:?}");
        assert_eq!(out.concat().replace(' ', ""), "thequickbrownfox");
    }

    #[test]
    fn wrap_hard_splits_words_longer_than_the_pane() {
        let out = wrap_text(&"x".repeat(25), 10);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].chars().count(), 10);
    }

    #[test]
    fn wrap_preserves_explicit_newlines() {
        assert_eq!(
            wrap_text("a\nb", 10),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
