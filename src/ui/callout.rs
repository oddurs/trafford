//! Obsidian callouts — `> [!note]`, drawn as a callout rather than as a quote
//! with its marker showing.
//!
//! The vault this was built against has 269 of them and exactly three kinds.
//! Preview drew every one as an italic blockquote with `[!note]` sitting in the
//! text as literal characters, which is the same failure as showing the `[[`
//! around a link.
//!
//! Unlike a table, a callout draws one line per source line, so the mapping
//! back to the note is the identity and nothing has to be tracked.

use super::markdown::{Rendered, Renderer};
use super::theme::Theme;
use ratatui::style::{Modifier, Style};

/// The bar down the left of a callout. Solid rather than `│` so it reads as a
/// band of colour rather than as a table border.
const BAR: &str = "▎";

#[derive(Debug, Clone)]
pub struct Callout {
    /// The word between the brackets, lowercased. Kept as written rather than
    /// as an enum: a vault may use a kind this program has never heard of, and
    /// drawing it labelled with what the author typed beats refusing to draw
    /// it at all.
    pub kind: String,
    pub title: Option<String>,
    /// How many source lines the block covers.
    pub height: usize,
}

/// Recognise a callout beginning at `at`.
pub fn parse(lines: &[String], at: usize) -> Option<Callout> {
    let first = lines.get(at)?;
    let rest = first.trim_start().strip_prefix('>')?.trim_start();
    let inner = rest.strip_prefix("[!")?;
    let close = inner.find(']')?;
    let kind = inner[..close].trim().to_lowercase();
    if kind.is_empty() || !kind.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return None;
    }
    // Obsidian's `+`/`-` suffix asks for a callout that starts open or closed.
    // Not supported yet; skipping the character keeps such a callout drawing
    // rather than falling back to a quote.
    let after = inner[close + 1..].trim_start_matches(['+', '-']).trim();
    let title = (!after.is_empty()).then(|| after.to_string());

    let mut height = 1;
    while let Some(line) = lines.get(at + height) {
        if !line.trim_start().starts_with('>') {
            break;
        }
        height += 1;
    }
    Some(Callout {
        kind,
        title,
        height,
    })
}

/// The colour a kind is drawn in.
///
/// Mapped onto roles that already exist rather than adding three more knobs:
/// a note is information, a tip is affirmative, a warning wants the colour a
/// theme already uses for "look at this". A Ghostty import governs all three,
/// which is the point.
fn colour(kind: &str, theme: &Theme) -> ratatui::style::Color {
    match kind {
        "tip" | "success" | "check" | "done" => theme.added,
        "warning" | "caution" | "attention" | "danger" | "error" | "bug" => theme.modified,
        _ => theme.accent,
    }
}

/// Draw a callout: one line out for each line in.
pub fn render(
    callout: &Callout,
    lines: &[String],
    at: usize,
    renderer: &Renderer<'_>,
) -> Vec<Rendered> {
    let theme = renderer.theme;
    let fg = colour(&callout.kind, theme);
    let bar = Style::default().fg(fg);

    let mut label = Rendered::default()
        .suffixed(BAR, bar)
        .suffixed(" ", bar)
        .suffixed(
            &callout.kind.to_uppercase(),
            Style::default().fg(fg).add_modifier(Modifier::BOLD),
        );
    if let Some(title) = &callout.title {
        label = label
            .suffixed(" · ", theme.faded())
            .suffixed(title, Style::default().fg(theme.fg));
    }
    let mut out = vec![label];

    for offset in 1..callout.height {
        let raw = &lines[at + offset];
        // `> body` — drop the marker and the single space after it, which is
        // syntax, and keep any deeper indentation, which is the author's.
        let body = raw.trim_start().strip_prefix('>').unwrap_or("");
        let body = body.strip_prefix(' ').unwrap_or(body);
        let rendered = renderer.render(body, false);
        out.push(rendered.prefixed(&format!("{BAR} "), bar));
    }
    // The bar is part of each line's text, so wrapping would leave it on the
    // first row only. The rail puts it back on every row the line folds onto.
    for line in &mut out {
        line.rail = Some(ratatui::text::Span::styled(BAR.to_string(), bar));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    fn draw(text: &str) -> Vec<String> {
        let theme = Theme::default();
        let resolves = |_: &str| true;
        let renderer = Renderer {
            theme: &theme,
            resolves: &resolves,
            conceal: true,
        };
        let src = lines(text);
        let c = parse(&src, 0).expect("a callout");
        render(&c, &src, 0, &renderer)
            .into_iter()
            .map(|r| r.text)
            .collect()
    }

    #[test]
    fn a_callout_is_labelled_and_barred() {
        assert_eq!(
            draw("> [!note]\n> The body.\n> More."),
            ["▎ NOTE", "▎ The body.", "▎ More."]
        );
    }

    #[test]
    fn a_title_is_used_when_one_is_given() {
        assert_eq!(
            draw("> [!note] Research Scope")[0],
            "▎ NOTE · Research Scope"
        );
    }

    #[test]
    fn the_three_kinds_the_vault_uses_are_coloured_apart() {
        let theme = Theme::default();
        assert_eq!(colour("note", &theme), theme.accent);
        assert_eq!(colour("tip", &theme), theme.added);
        assert_eq!(colour("warning", &theme), theme.modified);
    }

    #[test]
    fn a_kind_nobody_anticipated_still_draws_as_a_callout() {
        // Labelled with what the author wrote, which is more use than refusing
        // to draw it and showing `[!quote]` as text.
        assert_eq!(draw("> [!quote] Someone")[0], "▎ QUOTE · Someone");
        let theme = Theme::default();
        assert_eq!(colour("quote", &theme), theme.accent, "falls back to note");
    }

    #[test]
    fn markup_inside_a_callout_still_renders() {
        let out = draw("> [!tip]\n> See [[Note]] and **this**.");
        assert_eq!(out[1], "▎ See Note and this.");
    }

    #[test]
    fn a_blank_quoted_line_keeps_the_bar() {
        assert_eq!(
            draw("> [!note]\n>\n> After a gap."),
            ["▎ NOTE", "▎ ", "▎ After a gap."],
            "an empty quoted line is still a line of the callout"
        );
    }

    #[test]
    fn a_plain_quote_is_not_a_callout() {
        assert!(parse(&lines("> just a quote"), 0).is_none());
        assert!(parse(&lines("not a quote at all"), 0).is_none());
        assert!(parse(&lines("> [!] empty kind"), 0).is_none());
        assert!(parse(&lines("> [!two words] no"), 0).is_none());
    }

    #[test]
    fn the_block_stops_at_the_first_unquoted_line() {
        let src = lines("> [!note]\n> one\n> two\nafter\n> another quote");
        assert_eq!(parse(&src, 0).unwrap().height, 3);
    }

    #[test]
    fn obsidians_fold_suffix_does_not_stop_it_drawing() {
        assert_eq!(draw("> [!warning]- Collapsed")[0], "▎ WARNING · Collapsed");
    }
}
