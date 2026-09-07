pub mod markdown;
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
    draw_editor(f, app, editor_area);
    app.panes.editor = inner_of(editor_area);
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
    ) -> PreviewView {
        let mut lines = Vec::with_capacity(source.len());
        let mut in_code = false;
        for raw in source {
            let opens = markdown::is_fence(raw);
            lines.push(renderer.render(raw, in_code || opens));
            if opens {
                in_code = !in_code;
            }
        }
        let texts: Vec<String> = lines.iter().map(|r| r.text.clone()).collect();
        let layout = crate::layout::Layout::new(&texts, width, true);
        PreviewView { lines, layout }
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
    let gutter = gutter_width(app.editor.buf.len());
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

    let view = app
        .preview
        .then(|| PreviewView::build(&app.editor.buf.lines, &renderer, text_width));

    // Scroll against whichever rows are actually on screen. In preview that is
    // the rendered fold, so the top of the cursor's line is the anchor — there
    // is no caret there to keep any finer promise to.
    let (top_row, total_rows) = match &view {
        Some(v) => (v.layout.first_of(app.editor.buf.row), v.layout.len()),
        None => (cursor_visual.0, app.editor.layout.len()),
    };
    app.editor
        .sync_scroll_visual(top_row, total_rows, inner.height as usize);

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
            if first {
                format!("{:>width$} ", row + 1, width = gutter as usize - 1)
            } else {
                " ".repeat(gutter as usize)
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
        for visual in app.editor.scroll..view.layout.len() {
            if lines.len() >= inner.height as usize {
                break;
            }
            let Some(vrow) = view.layout.row(visual) else {
                break;
            };
            let Some(rendered) = view.lines.get(vrow.line) else {
                break;
            };
            let mut spans = vec![gutter_span(
                vrow.line,
                vrow.is_first(),
                vrow.line == app.editor.buf.row,
            )];
            if vrow.indent > 0 {
                spans.push(Span::raw(" ".repeat(vrow.indent)));
            }
            spans.extend(rendered.slice(vrow.start, vrow.len));
            lines.push(highlight(Line::from(spans), vrow.line));
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

        for visual in app.editor.scroll..app.editor.layout.len() {
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

/// A heading picked out of the open buffer, for the outline.
struct OutlineEntry {
    text: String,
    level: usize,
    row: usize,
}

/// Headings in the live buffer, so the outline updates as you type.
fn outline_of(buf: &crate::editor::Buffer) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    let mut in_code = false;
    for (row, raw) in buf.lines.iter().enumerate() {
        if markdown::is_fence(raw) {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let trimmed = raw.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if level > 6 || trimmed.chars().nth(level) != Some(' ') {
            continue;
        }
        out.push(OutlineEntry {
            text: trimmed[level..].trim().to_string(),
            level,
            row,
        });
    }
    out
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
            lines.push(Line::from(vec![
                Span::styled(indent.clone(), theme.faded()),
                Span::styled(
                    fit(&entry.text, width.saturating_sub(indent.len())),
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
        let theme = Theme::default();
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let resolves = |t: &str| t != "Nowhere";
        let renderer = Renderer {
            theme: &theme,
            resolves: &resolves,
            conceal: true,
        };
        let view = PreviewView::build(&owned, &renderer, width);
        (theme, view)
    }

    #[test]
    fn preview_folds_what_it_draws_not_what_was_written() {
        // The source is 44 characters and would take two rows at this width;
        // rendered it is 24 and takes one. Folding the source would leave a
        // ragged break in the middle of a line that fits.
        let (_t, view) = preview_of(&["see [[Some/Long/Path/Note|a note]] there"], 30);
        assert_eq!(view.lines[0].text, "see a note there");
        assert_eq!(view.layout.len(), 1, "it fits once the syntax is gone");
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
        assert!(view.layout.len() > 1, "it has to actually fold");
        let texts = view.texts();
        let mut found = 0;
        for visual in 0..view.layout.len() {
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
        let buf = crate::editor::Buffer::from_str(
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
