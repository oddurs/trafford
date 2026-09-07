pub mod markdown;
pub mod theme;

use crate::app::{App, Focus, Overlay, Picker, SidebarTab};
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

const SIDEBAR_WIDTH: u16 = 30;
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
        cols.push(Constraint::Length(SIDEBAR_WIDTH));
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

    let mut i = 0;
    if app.sidebar_visible {
        draw_sidebar(f, app, panes[i]);
        i += 1;
    }
    let editor_area = panes[i];
    i += 1;
    draw_editor(f, app, editor_area);
    if right_width > 0 {
        if app.assistant_visible {
            draw_assistant(f, app, panes[i]);
        } else {
            draw_context(f, app, panes[i]);
        }
    }

    draw_status(f, app, rows[1]);

    match &app.overlay {
        Some(Overlay::Palette(p))
        | Some(Overlay::Switcher(p))
        | Some(Overlay::LinkPicker(p))
        | Some(Overlay::Backlinks(p)) => draw_picker(f, &theme, p, area),
        Some(Overlay::Search(pane)) => draw_search(f, &theme, pane, area),
        Some(Overlay::Prompt(prompt)) => draw_prompt(f, &theme, prompt, area),
        Some(Overlay::Git(pane)) => draw_git(f, &theme, pane, area),
        Some(Overlay::History(log)) => draw_history(f, &theme, log, area),
        Some(Overlay::Confirm(c)) => draw_confirm(f, &theme, c, area),
        Some(Overlay::Diff {
            title,
            body,
            scroll,
        }) => draw_diff(f, &theme, title, body, *scroll, area),
        Some(Overlay::Help) => draw_help(f, &theme, area),
        None => {}
    }
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

fn pane_block<'a>(theme: &Theme, title: &'a str, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused))
        .title(Line::from(vec![
            Span::styled(" ", theme.dimmed()),
            Span::styled(
                title.to_string(),
                if focused {
                    theme.title()
                } else {
                    theme.dimmed().add_modifier(Modifier::BOLD)
                },
            ),
            Span::styled(" ", theme.dimmed()),
        ]))
        .style(Style::default().bg(theme.bg))
}

fn section(theme: &Theme, label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_uppercase(),
        Style::default()
            .fg(theme.accent)
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

/// Display columns taken by the first `cols` characters of `text`.
fn width_of_prefix(text: &str, chars: usize) -> usize {
    text.chars().take(chars).filter_map(|c| c.width()).sum()
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
            let notes = app.listed_notes();
            let offset = scroll_offset(app.sidebar_cursor, notes.len(), height.saturating_sub(1));
            let words: usize = notes.iter().map(|n| n.words).sum();
            lines.push(Line::from(Span::styled(
                fit(
                    &format!("{} notes · {}", notes.len(), compact(words)),
                    width,
                ),
                theme.faded(),
            )));
            for (i, note) in notes.iter().enumerate().skip(offset).take(height - 1) {
                let selected = i == app.sidebar_cursor && focused;
                let is_open = app.current.as_deref() == Some(note.id.as_str());
                let marker = if is_open { "▌" } else { " " };
                let style = if selected {
                    theme.selected()
                } else if is_open {
                    Style::default().fg(theme.accent)
                } else {
                    Style::default().fg(theme.fg)
                };
                lines.push(Line::from(vec![
                    Span::styled(marker, Style::default().fg(theme.accent)),
                    Span::styled(fit(&note.title, width.saturating_sub(1)), style),
                ]));
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

/// Keep `cursor` visible inside a window `height` tall over `len` items.
fn scroll_offset(cursor: usize, len: usize, height: usize) -> usize {
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
    app.editor.sync_scroll(inner.height as usize);

    let vault = &app.vault;
    let resolves = |target: &str| vault.resolve_target(target).is_some();
    let renderer = Renderer {
        theme: &theme,
        resolves: &resolves,
    };

    // Gutter width scales with the line count so wide files stay aligned.
    let gutter = (app.editor.buf.len().to_string().len() + 1).max(4) as u16;
    let text_width = inner.width.saturating_sub(gutter) as usize;

    // Horizontal scroll follows the cursor in source mode; preview soft-wraps.
    let hscroll = if app.preview {
        0
    } else {
        app.editor
            .buf
            .col
            .saturating_sub(text_width.saturating_sub(4).max(1))
    };

    // A fence opened before the viewport must still colour the visible lines.
    let mut in_code = app
        .editor
        .buf
        .lines
        .iter()
        .take(app.editor.scroll)
        .filter(|l| markdown::is_fence(l))
        .count()
        % 2
        == 1;

    let selection = app.editor.selection_rows();
    let mut lines: Vec<Line> = Vec::new();

    for row in app.editor.scroll..app.editor.buf.len() {
        if lines.len() >= inner.height as usize {
            break;
        }
        let raw = app.editor.buf.line(row);
        let opens_fence = markdown::is_fence(raw);
        let render_as_code = in_code;
        if opens_fence {
            in_code = !in_code;
        }

        let is_cursor_row = row == app.editor.buf.row;
        let selected = selection
            .map(|(a, b)| row >= a && row <= b)
            .unwrap_or(false);

        let number = Span::styled(
            format!("{:>width$} ", row + 1, width = gutter as usize - 1),
            if is_cursor_row {
                Style::default().fg(theme.accent)
            } else {
                theme.faded()
            },
        );

        let visible: String = raw.chars().skip(hscroll).collect();
        let mut spans = vec![number];
        spans.extend(renderer.line(&visible, render_as_code || opens_fence));

        let mut line = Line::from(spans);
        if selected {
            line = line.style(Style::default().bg(theme.sel));
        } else if is_cursor_row && focused {
            line = line.style(Style::default().bg(theme.cursorline));
        }
        lines.push(line);
    }

    let paragraph = if app.preview {
        Paragraph::new(lines).wrap(Wrap { trim: false })
    } else {
        Paragraph::new(lines)
    };
    f.render_widget(paragraph, inner);

    // Place the real terminal cursor so the terminal's own caret is used.
    // The offset is in display columns: a line of CJK is twice as wide as it
    // is long, and a character-indexed cursor drifts left of its glyph.
    if focused && !app.preview && app.overlay.is_none() {
        let line = app.editor.buf.line(app.editor.buf.row);
        let visible: String = line.chars().skip(hscroll).collect();
        let offset = width_of_prefix(&visible, app.editor.buf.col.saturating_sub(hscroll));
        let cx = inner.x + gutter + offset as u16;
        let cy = inner.y + (app.editor.buf.row.saturating_sub(app.editor.scroll)) as u16;
        if cx < inner.right() && cy < inner.bottom() {
            f.set_cursor_position((cx, cy));
        }
    }
}

// ---------------------------------------------------------------------------
// Context pane: outline, outgoing links, backlinks
// ---------------------------------------------------------------------------

fn draw_context(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let block = pane_block(&theme, "context", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    let Some(id) = app.current.clone() else {
        lines.push(Line::from(Span::styled("no note open", theme.faded())));
        f.render_widget(Paragraph::new(lines), inner);
        return;
    };

    // Outline comes from the live buffer, so it updates as you type.
    lines.push(section(&theme, "outline"));
    let mut in_code = false;
    let mut any_heading = false;
    for (row, raw) in app.editor.buf.lines.iter().enumerate() {
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
        any_heading = true;
        let text = trimmed[level..].trim();
        let indent = "  ".repeat(level.saturating_sub(1));
        let here = row == app.editor.buf.row;
        lines.push(Line::from(vec![
            Span::styled(indent.clone(), theme.faded()),
            Span::styled(
                fit(text, width.saturating_sub(indent.len())),
                if here {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    theme
                        .heading_style(level as u8)
                        .remove_modifier(Modifier::BOLD)
                },
            ),
        ]));
    }
    if !any_heading {
        lines.push(Line::from(Span::styled("  no headings", theme.faded())));
    }

    let outgoing = app.vault.outgoing(&id);
    lines.push(Line::from(""));
    lines.push(section(&theme, &format!("links out · {}", outgoing.len())));
    if outgoing.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.faded())));
    }
    for target in outgoing.iter().take(8) {
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
    }

    let backlinks = app.vault.backlinks_for(&id);
    lines.push(Line::from(""));
    lines.push(section(&theme, &format!("backlinks · {}", backlinks.len())));
    if backlinks.is_empty() {
        lines.push(Line::from(Span::styled("  none yet", theme.faded())));
    }
    for bl in backlinks.iter().take(10) {
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
        if !bl.context.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("     {}", fit(&bl.context, width.saturating_sub(5))),
                theme.faded(),
            )));
        }
    }

    // Unresolved links are the vault's growing edge — worth surfacing.
    let orphans: Vec<&String> = app
        .vault
        .unresolved
        .iter()
        .filter(|(_, refs)| refs.iter().any(|r| r.from == id))
        .map(|(target, _)| target)
        .collect();
    if !orphans.is_empty() {
        lines.push(Line::from(""));
        lines.push(section(&theme, &format!("unwritten · {}", orphans.len())));
        for target in orphans.iter().take(6) {
            lines.push(Line::from(vec![
                Span::styled("  ○ ", theme.faded()),
                Span::styled(
                    fit(target, width.saturating_sub(4)),
                    Style::default().fg(theme.link_broken),
                ),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// Assistant
// ---------------------------------------------------------------------------

fn draw_assistant(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let focused = app.focus == Focus::Assistant;
    let title = format!("assistant · {}", app.config.model);
    let block = pane_block(&theme, &title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 4 {
        return;
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
                Style::default().fg(theme.link_broken),
            )));
        }
    }

    let resolves = |target: &str| app.vault.resolve_target(target).is_some();
    let renderer = Renderer {
        theme: &theme,
        resolves: &resolves,
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
        // The pane can be squeezed to nothing on a narrow terminal, so this
        // must not assume there is a column to put the cursor in.
        let cx = input_inner.x
            + (prompt.chars().count() as u16).min(input_inner.width.saturating_sub(1));
        f.set_cursor_position((cx, input_inner.y));
    }
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
        Mode::Normal => theme.accent,
        Mode::Insert => theme.add,
        Mode::Visual | Mode::VisualLine => theme.link,
    };

    let mut spans = vec![
        Span::styled(
            format!(" {} ", mode.label()),
            Style::default()
                .fg(theme.bg)
                .bg(mode_colour)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(theme.panel)),
    ];

    if let Some(status) = app.status_text() {
        spans.push(Span::styled(
            status.to_string(),
            Style::default().fg(theme.fg).bg(theme.panel),
        ));
    } else {
        let git = &app.git_status;
        if app.repo.is_some() {
            spans.push(Span::styled(
                format!("⎇ {}", git.branch),
                Style::default().fg(theme.link).bg(theme.panel),
            ));
            if !git.is_clean() {
                spans.push(Span::styled(
                    format!("  ●{}", git.changes.len()),
                    Style::default().fg(theme.accent).bg(theme.panel),
                ));
            }
            if git.ahead > 0 {
                spans.push(Span::styled(
                    format!("  ↑{}", git.ahead),
                    Style::default().fg(theme.add).bg(theme.panel),
                ));
            }
            if git.behind > 0 {
                spans.push(Span::styled(
                    format!("  ↓{}", git.behind),
                    Style::default().fg(theme.del).bg(theme.panel),
                ));
            }
        } else {
            spans.push(Span::styled(
                "no git",
                Style::default().fg(theme.faint).bg(theme.panel),
            ));
        }
        spans.push(Span::styled(
            format!("   {}", plural(app.vault.notes.len(), "note")),
            Style::default().fg(theme.dim).bg(theme.panel),
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
        Style::default().bg(theme.panel),
    ));
    spans.push(Span::styled(
        right,
        Style::default().fg(theme.dim).bg(theme.panel),
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
        .style(Style::default().bg(theme.panel))
}

fn draw_picker(f: &mut Frame, theme: &Theme, picker: &Picker, area: Rect) {
    let rect = centred(area, 60, 20);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, picker.title.clone());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 2 {
        return;
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
            line = line.style(Style::default().bg(theme.sel));
        }
        lines.push(line);
    }
    if picker.matches.is_empty() {
        lines.push(Line::from(Span::styled("  no matches", theme.faded())));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_search(f: &mut Frame, theme: &Theme, pane: &crate::app::SearchPane, area: Rect) {
    let rect = centred(area, 74, 22);
    f.render_widget(Clear, rect);
    let block = overlay_block(theme, format!("search · {} hits", pane.hits.len()));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 2 {
        return;
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
            line = line.style(Style::default().bg(theme.sel));
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
}

fn draw_prompt(f: &mut Frame, theme: &Theme, prompt: &crate::app::Prompt, area: Rect) {
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
}

fn draw_git(f: &mut Frame, theme: &Theme, pane: &crate::app::GitPane, area: Rect) {
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
            Style::default().fg(theme.add),
        )));
    }
    for (i, change) in snap.changes.iter().enumerate().take(12) {
        let selected = i == pane.cursor;
        let colour = match change.status {
            crate::git::Status::Added => theme.add,
            crate::git::Status::Deleted => theme.del,
            crate::git::Status::Conflicted => theme.link_broken,
            crate::git::Status::Untracked => theme.dim,
            _ => theme.accent,
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
                Style::default().fg(theme.add),
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
            line = line.style(Style::default().bg(theme.sel));
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
}

/// A unified diff, coloured the way `git diff` does.
fn draw_diff(f: &mut Frame, theme: &Theme, title: &str, body: &str, scroll: u16, area: Rect) {
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
                Style::default().fg(theme.add)
            } else if raw.starts_with('-') {
                Style::default().fg(theme.del)
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
}

fn draw_history(f: &mut Frame, theme: &Theme, log: &[crate::git::Commit], area: Rect) {
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
}

fn draw_confirm(f: &mut Frame, theme: &Theme, confirm: &crate::app::Confirm, area: Rect) {
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
                Style::default().fg(theme.add).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" yes    ", theme.dimmed()),
            Span::styled(
                "esc",
                Style::default().fg(theme.del).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", theme.dimmed()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
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
    fn prefix_width_counts_columns_not_characters() {
        assert_eq!(width_of_prefix("abc", 2), 2);
        assert_eq!(width_of_prefix("日本語", 2), 4);
        assert_eq!(width_of_prefix("a日b", 2), 3);
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
