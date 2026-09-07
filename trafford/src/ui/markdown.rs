use super::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

/// Renders markdown as styled spans.
///
/// With `conceal` off it **changes no characters**, which is what lets the
/// editor draw through it: the cursor column has to line up with the buffer,
/// and it cannot if the renderer is free to add or drop text. That is the
/// invariant `styling_alone_preserves_every_character` pins, and it is not
/// negotiable on the editor path.
///
/// With `conceal` on it drops the syntax and keeps what the syntax was for —
/// `[[Note|alias]]` becomes `alias`. Only preview turns it on, because preview
/// has no caret to keep honest. What the drawn text maps back to is carried in
/// [`Rendered`] rather than being recoverable by counting, which is the whole
/// reason concealment cannot be a flag on the editor path too.
pub struct Renderer<'a> {
    pub theme: &'a Theme,
    /// Returns true when a `[[wikilink]]` target exists in the vault.
    pub resolves: &'a dyn Fn(&str) -> bool,
    /// Draw markup as what it means rather than as what was typed.
    pub conceal: bool,
}

/// What a clickable run of text points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A note in the vault, named by a `[[wikilink]]`.
    Note,
    /// A URL, or a `#anchor` into this note.
    Url,
    /// A `#tag`. Clicking one filters the vault by it, which is what the tags
    /// tab in the sidebar already does — the tag was just never clickable where
    /// it was written.
    Tag,
}

/// Somewhere a click can go: the drawn characters, and where they point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Character offset into [`Rendered::text`], not into the source line.
    pub start: usize,
    pub len: usize,
    /// A note name for a wikilink, a URL for a markdown link.
    pub target: String,
    /// The `#Heading` part of `[[Note#Heading]]`, which the target drops.
    ///
    /// Carried here because concealment throws the syntax away: without it a
    /// click in preview could open the note but not land on the section the
    /// link actually named.
    pub heading: Option<String>,
    pub kind: Target,
}

impl Link {
    pub fn covers(&self, column: usize) -> bool {
        column >= self.start && column < self.start + self.len
    }
}

/// One rendered line: how it is drawn, what it reads as, and what can be
/// clicked in it.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub spans: Vec<Span<'static>>,
    /// Drawn at the start of every row this line wraps onto.
    ///
    /// A callout's bar is part of the line's text, so it appears on the first
    /// row and nowhere else — and a callout that loses its bar halfway through
    /// a paragraph stops reading as one. Anything with a rail repeats it.
    pub rail: Option<Span<'static>>,
    /// The characters actually drawn. With `conceal` off this is the source
    /// line unchanged; with it on it is shorter, and is what preview folds and
    /// hit-tests against — since it is what is on the screen.
    pub text: String,
    pub links: Vec<Link>,
}

impl Rendered {
    pub fn link_at(&self, column: usize) -> Option<&Link> {
        self.links.iter().find(|l| l.covers(column))
    }

    /// A copy with `text` in front of it, moving the links along.
    ///
    /// The offsets in `links` index the drawn text, so anything put in front of
    /// that text has to move them or a click resolves against the wrong
    /// column. Folding puts a marker in front of every heading, which is why
    /// this exists.
    pub fn prefixed(&self, text: &str, style: Style) -> Rendered {
        let shift = text.chars().count();
        let mut spans = Vec::with_capacity(self.spans.len() + 1);
        spans.push(Span::styled(text.to_string(), style));
        spans.extend(self.spans.iter().cloned());
        Rendered {
            spans,
            rail: self.rail.clone(),
            text: format!("{text}{}", self.text),
            links: self
                .links
                .iter()
                .map(|l| Link {
                    start: l.start + shift,
                    ..l.clone()
                })
                .collect(),
        }
    }

    /// A copy with a clickable run appended, recording where it landed.
    ///
    /// Building a line out of pieces rather than scanning one: the properties
    /// row has no markdown source to parse, only values that should behave like
    /// the tags a reader is used to clicking.
    pub fn with_link(&self, text: &str, style: Style, target: String, kind: Target) -> Rendered {
        let start = self.text.chars().count();
        let mut out = self.suffixed(text, style);
        let len = text.chars().count();
        if len > 0 {
            out.links.push(Link {
                start,
                len,
                target,
                heading: None,
                kind,
            });
        }
        out
    }

    /// A copy with `text` after it. Links are unaffected: they are all in front
    /// of anything appended.
    pub fn suffixed(&self, text: &str, style: Style) -> Rendered {
        let mut out = self.clone();
        out.text.push_str(text);
        out.spans.push(Span::styled(text.to_string(), style));
        out
    }

    /// The spans covering drawn characters `start..start + len`.
    ///
    /// A folded row is a slice of a line, and the spans have to be cut to
    /// match or the styling slides along the text. Splits inside a span rather
    /// than only between them, because a link can straddle a fold.
    pub fn slice(&self, start: usize, len: usize) -> Vec<Span<'static>> {
        let end = start + len;
        let mut out = Vec::new();
        let mut at = 0usize;
        for span in &self.spans {
            let span_len = span.content.chars().count();
            let span_end = at + span_len;
            if span_end > start && at < end {
                let from = start.saturating_sub(at);
                let to = (end - at).min(span_len);
                let text: String = span.content.chars().skip(from).take(to - from).collect();
                if !text.is_empty() {
                    out.push(Span::styled(text, span.style));
                }
            }
            at = span_end;
            if at >= end {
                break;
            }
        }
        out
    }
}

/// True when a line opens or closes a fenced code block.
pub fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// Accumulates a rendered line: the spans to draw, the characters they add up
/// to, and the links found along the way.
///
/// Spans and text are appended together, by construction, so the two cannot
/// drift apart — the bug that would put a click on the wrong link.
#[derive(Default)]
struct Out {
    spans: Vec<Span<'static>>,
    text: String,
    links: Vec<Link>,
    /// Characters pushed so far, which is where the next one starts.
    width: usize,
}

impl Out {
    fn push(&mut self, content: &str, style: Style) {
        if content.is_empty() {
            return;
        }
        self.width += content.chars().count();
        self.text.push_str(content);
        self.spans.push(Span::styled(content.to_string(), style));
    }

    /// Push text that points somewhere, recording the range it occupies.
    fn push_link(
        &mut self,
        content: &str,
        style: Style,
        target: String,
        heading: Option<String>,
        kind: Target,
    ) {
        let start = self.width;
        self.push(content, style);
        if self.width > start {
            self.links.push(Link {
                start,
                len: self.width - start,
                target,
                heading,
                kind,
            });
        }
    }

    fn finish(self) -> Rendered {
        Rendered {
            spans: self.spans,
            rail: None,
            text: self.text,
            links: self.links,
        }
    }
}

impl<'a> Renderer<'a> {
    /// Spans only. The editor draws through this and never conceals.
    pub fn line(&self, text: &str, in_code_block: bool) -> Vec<Span<'static>> {
        self.render(text, in_code_block).spans
    }

    pub fn render(&self, text: &str, in_code_block: bool) -> Rendered {
        let t = self.theme;
        let mut out = Out::default();

        if in_code_block || is_fence(text) {
            out.push(text, Style::default().fg(t.code));
            return out.finish();
        }

        let trimmed = text.trim_start();
        let indent_len = text.len() - trimmed.len();

        // Headings: dim the hashes, brighten the text. Concealed, the level is
        // carried by the style alone, which is what a heading looks like
        // anywhere that is not a text editor.
        if trimmed.starts_with('#') {
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            if hashes <= 6 && trimmed.chars().nth(hashes) == Some(' ') {
                out.push(&text[..indent_len], t.faded());
                if !self.conceal {
                    out.push(&"#".repeat(hashes), t.faded());
                }
                let body = &trimmed[hashes..];
                let body = if self.conceal {
                    body.strip_prefix(' ').unwrap_or(body)
                } else {
                    body
                };
                self.inline(&mut out, body, t.heading_style(hashes as u8));
                return out.finish();
            }
        }

        // Horizontal rules.
        if matches!(trimmed, "---" | "***" | "___") {
            out.push(text, t.faded());
            return out.finish();
        }

        // Block quotes. The marker stays: it is structure, not syntax, and
        // without it a quoted line is just an italic one.
        if let Some(quoted) = trimmed.strip_prefix('>') {
            out.push(&text[..indent_len], t.faded());
            out.push(">", Style::default().fg(t.accent));
            self.inline(
                &mut out,
                quoted,
                Style::default().fg(t.muted).add_modifier(Modifier::ITALIC),
            );
            return out.finish();
        }

        let mut rest = text;

        // Leading list marker or task checkbox, coloured but never rewritten.
        if let Some((marker, tail)) = split_list_marker(text) {
            let style = if marker.trim_start().starts_with("- [x]") {
                Style::default().fg(t.added)
            } else {
                Style::default().fg(t.accent)
            };
            out.push(marker, style);
            rest = tail;
        }

        let body_style = if text.trim_start().starts_with("- [x]") {
            Style::default()
                .fg(t.muted)
                .add_modifier(Modifier::CROSSED_OUT)
        } else {
            Style::default().fg(t.fg)
        };
        self.inline(&mut out, rest, body_style);
        out.finish()
    }

    /// Turn the scanner's pieces into styled spans.
    ///
    /// Every decision about *what is there* was made in [`scan`]; everything
    /// here is about how a terminal draws it. The HTML backend in `site/` is
    /// the same function written against tags instead of styles, which is the
    /// point of the split — there is one inline scanner in this program, for
    /// the same reason there is one heading scanner.
    fn inline(&self, out: &mut Out, text: &str, base: Style) {
        let t = self.theme;
        for piece in scan(text) {
            let shown = |all: &'_ str, inner: &'_ str| -> String {
                if self.conceal { inner } else { all }.to_string()
            };
            match &piece.kind {
                Inline::Text => out.push(&piece.raw, base),
                Inline::Code { inner } => {
                    out.push(&shown(&piece.raw, inner), Style::default().fg(t.code))
                }
                Inline::Strong { inner } => {
                    out.push(&shown(&piece.raw, inner), base.add_modifier(Modifier::BOLD))
                }
                Inline::Emphasis { inner } => out.push(
                    &shown(&piece.raw, inner),
                    base.add_modifier(Modifier::ITALIC),
                ),
                Inline::Highlight { inner } => out.push(
                    &shown(&piece.raw, inner),
                    Style::default().fg(t.bg).bg(t.accent),
                ),
                Inline::Wiki {
                    target,
                    heading,
                    label,
                } => {
                    let colour = if (self.resolves)(target) {
                        t.link
                    } else {
                        t.broken
                    };
                    out.push_link(
                        &shown(&piece.raw, label),
                        Style::default()
                            .fg(colour)
                            .add_modifier(Modifier::UNDERLINED),
                        target.clone(),
                        heading.clone(),
                        Target::Note,
                    );
                }
                Inline::Link { url, label } => {
                    if self.conceal {
                        out.push_link(
                            label,
                            Style::default()
                                .fg(t.link)
                                .add_modifier(Modifier::UNDERLINED),
                            url.clone(),
                            None,
                            Target::Url,
                        );
                    } else {
                        // Source form: the label is the link, the parenthesised
                        // URL is punctuation that happens to be readable.
                        out.push_link(
                            &format!("[{label}]"),
                            Style::default().fg(t.link),
                            url.clone(),
                            None,
                            Target::Url,
                        );
                        out.push(&format!("({url})"), t.faded());
                    }
                }
                Inline::Tag { name } => out.push_link(
                    &piece.raw,
                    Style::default().fg(t.tag),
                    name.clone(),
                    None,
                    Target::Tag,
                ),
            }
        }
    }
}

/// A run of inline markup, described by what it means rather than by how it
/// is drawn.
///
/// `raw` is the source characters the piece covers, exactly — concatenate
/// every piece's `raw` and you have the line back. That is what
/// `scanning_covers_every_character` pins, and it is the property the editor
/// path depends on: styling a piece cannot add or drop a character if the
/// pieces already add up to the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub raw: String,
    pub kind: Inline,
}

/// What a [`Piece`] is. The payload is what survives concealment — the text a
/// reader was meant to see, plus wherever it points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// No markup. `raw` is the text.
    Text,
    /// `` `code` ``
    Code { inner: String },
    /// `**bold**`
    Strong { inner: String },
    /// `*italic*` or `_italic_`
    Emphasis { inner: String },
    /// `==highlight==`
    Highlight { inner: String },
    /// `[[Note#Heading|alias]]`
    Wiki {
        /// The note name, with any `#heading` and `|alias` stripped.
        target: String,
        /// The section named after the `#`, which the target drops.
        heading: Option<String>,
        /// What the author wanted read: the alias, or the whole inner text.
        label: String,
    },
    /// `[label](url)`
    Link { url: String, label: String },
    /// `#tag` — the hash is part of the tag, not syntax wrapped around it.
    Tag { name: String },
}

/// Find the inline markup in one line of a note.
///
/// This is the only inline scanner in the program. The terminal renderer maps
/// its output to styles; the docs site maps the same output to HTML. A second
/// scanner is the same class of bug as a second heading scanner — it agrees
/// with this one until the day it does not, and then a link is drawn in one
/// place and not the other.
///
/// Nothing here knows about code blocks: a fenced line is not scanned at all,
/// which is the caller's decision because only the caller is reading the note
/// in order.
pub fn scan(text: &str) -> Vec<Piece> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Piece> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !plain.is_empty() {
                out.push(Piece {
                    raw: std::mem::take(&mut plain),
                    kind: Inline::Text,
                });
            }
        };
    }

    macro_rules! piece {
        ($from:expr, $to:expr, $kind:expr) => {{
            flush!();
            out.push(Piece {
                raw: chars[$from..$to].iter().collect(),
                kind: $kind,
            });
        }};
    }

    while i < chars.len() {
        // [[wikilink]] and [[wikilink|alias]]
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
            if let Some(end) = find_pair(&chars, i + 2, ']') {
                let inner: String = chars[i + 2..end].iter().collect();
                let target = inner
                    .split(['|', '#'])
                    .next()
                    .unwrap_or(&inner)
                    .trim()
                    .to_string();
                // `[[Note#Heading|alias]]`: the heading sits between the
                // target and any alias.
                let heading = inner
                    .split_once('#')
                    .map(|(_, rest)| rest.split('|').next().unwrap_or(rest).trim())
                    .filter(|h| !h.is_empty())
                    .map(str::to_string);
                // An alias is what the author wanted read; without one the
                // whole inner text is, headings included.
                let label = match inner.split_once('|') {
                    Some((_, alias)) => alias.to_string(),
                    None => inner.clone(),
                };
                piece!(
                    i,
                    end + 2,
                    Inline::Wiki {
                        target,
                        heading,
                        label,
                    }
                );
                i = end + 2;
                continue;
            }
        }

        // [text](url)
        if chars[i] == '[' {
            if let Some(close) = chars[i..].iter().position(|c| *c == ']').map(|p| p + i) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(paren) = chars[close..]
                        .iter()
                        .position(|c| *c == ')')
                        .map(|p| p + close)
                    {
                        let label: String = chars[i + 1..close].iter().collect();
                        let url: String = chars[close + 2..paren].iter().collect();
                        piece!(i, paren + 1, Inline::Link { url, label });
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }

        // `inline code`
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..]
                .iter()
                .position(|c| *c == '`')
                .map(|p| p + i + 1)
            {
                let inner: String = chars[i + 1..end].iter().collect();
                piece!(i, end + 1, Inline::Code { inner });
                i = end + 1;
                continue;
            }
        }

        // **bold**
        if chars[i] == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = find_run(&chars, i + 2, '*', 2) {
                let inner: String = chars[i + 2..end].iter().collect();
                piece!(i, end + 2, Inline::Strong { inner });
                i = end + 2;
                continue;
            }
        }

        // ==highlight==
        if chars[i] == '=' && chars.get(i + 1) == Some(&'=') {
            if let Some(end) = find_run(&chars, i + 2, '=', 2) {
                let inner: String = chars[i + 2..end].iter().collect();
                piece!(i, end + 2, Inline::Highlight { inner });
                i = end + 2;
                continue;
            }
        }

        // *italic* or _italic_
        if (chars[i] == '*' || chars[i] == '_') && chars.get(i + 1) != Some(&chars[i]) {
            let delim = chars[i];
            if let Some(end) = chars[i + 1..]
                .iter()
                .position(|c| *c == delim)
                .map(|p| p + i + 1)
            {
                if end > i + 1 {
                    let inner: String = chars[i + 1..end].iter().collect();
                    piece!(i, end + 1, Inline::Emphasis { inner });
                    i = end + 1;
                    continue;
                }
            }
        }

        // #tag at a word boundary. The hash is part of the tag, not syntax
        // wrapped around it, so it survives concealment.
        if chars[i] == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
            let mut end = i + 1;
            while end < chars.len()
                && (chars[end].is_alphanumeric()
                    || chars[end] == '-'
                    || chars[end] == '_'
                    || chars[end] == '/')
            {
                end += 1;
            }
            if end > i + 1 {
                let name: String = chars[i + 1..end].iter().collect();
                piece!(i, end, Inline::Tag { name });
                i = end;
                continue;
            }
        }

        plain.push(chars[i]);
        i += 1;
    }
    flush!();
    out
}

/// Split a leading list marker (`- `, `* `, `1. `, `- [ ] `) from a line.
///
/// Public because it is a rule about the document, not about the terminal:
/// the docs site has to agree with the editor on what a list item is.
pub fn split_list_marker(line: &str) -> Option<(&str, &str)> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    for marker in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if rest.starts_with(marker) {
            return Some(line.split_at(indent + marker.len()));
        }
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && rest[digits..].starts_with(". ") {
        return Some(line.split_at(indent + digits + 2));
    }
    None
}

/// Find the index of `cc` (a doubled delimiter such as `]]`) starting at `from`.
fn find_pair(chars: &[char], from: usize, delim: char) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == delim && chars[i + 1] == delim {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find a run of `n` copies of `delim` starting at or after `from`.
fn find_run(chars: &[char], from: usize, delim: char, n: usize) -> Option<usize> {
    let mut i = from;
    while i + n <= chars.len() {
        if chars[i..i + n].iter().all(|c| *c == delim) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renderer(theme: &Theme) -> Renderer<'_> {
        Renderer {
            theme,
            resolves: &|target| target != "Missing",
            conceal: false,
        }
    }

    fn concealing(theme: &Theme) -> Renderer<'_> {
        Renderer {
            theme,
            resolves: &|target| target != "Missing",
            conceal: true,
        }
    }

    fn text_of(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// Lines that exercise every branch of the scanner, used by both halves of
    /// the invariant below.
    const SAMPLES: [&str; 10] = [
        "# Heading",
        "plain text",
        "  - [ ] a task with [[Link]] and `code`",
        "**bold** and *italic* and _under_",
        "a #tag and ==mark== and [text](http://x)",
        "> quoted **text**",
        "1. ordered item",
        "---",
        "unclosed [[link and `code",
        "héllo **wörld** ✨",
    ];

    /// The invariant, one level down: the pieces add up to the line.
    ///
    /// This is where `styling_alone_preserves_every_character` now comes from.
    /// The span mapper cannot drop a character if the scanner already covers
    /// every one of them, and the HTML backend in `site/` gets the same
    /// guarantee for free — which is the whole argument for there being one
    /// scanner rather than two.
    #[test]
    fn scanning_covers_every_character() {
        for line in SAMPLES {
            let joined: String = scan(line).iter().map(|p| p.raw.as_str()).collect();
            assert_eq!(joined, *line, "pieces do not add up to the source");
        }
    }

    /// A piece says what it is, not what colour it will be. Pinned because the
    /// HTML backend reads these and nothing else.
    #[test]
    fn a_piece_carries_what_survives_concealment() {
        let pieces = scan("see [[Notes/Deep Work#Rules|the rules]] and #focus/deep");
        let wiki = pieces
            .iter()
            .find(|p| matches!(p.kind, Inline::Wiki { .. }))
            .expect("a wikilink");
        assert_eq!(wiki.raw, "[[Notes/Deep Work#Rules|the rules]]");
        assert_eq!(
            wiki.kind,
            Inline::Wiki {
                target: "Notes/Deep Work".into(),
                heading: Some("Rules".into()),
                label: "the rules".into(),
            }
        );
        let tag = pieces
            .iter()
            .find(|p| matches!(p.kind, Inline::Tag { .. }))
            .expect("a tag");
        assert_eq!(tag.raw, "#focus/deep");
        assert_eq!(
            tag.kind,
            Inline::Tag {
                name: "focus/deep".into()
            }
        );
    }

    /// The invariant, still: styling alone never adds or removes a character.
    ///
    /// This is what lets the editor draw through the renderer — the cursor
    /// column has to line up with the buffer, and it cannot if the renderer is
    /// free to rewrite the line. Concealment is the deliberate exception, and
    /// it is off here because the editor is what this protects.
    #[test]
    fn styling_alone_preserves_every_character() {
        let theme = Theme::default();
        for line in SAMPLES {
            let spans = renderer(&theme).line(line, false);
            assert_eq!(text_of(&spans), line, "renderer changed the text");
        }
    }

    /// The other half, which is why the test above had to be narrowed rather
    /// than deleted: concealment exists precisely to break that equality, and
    /// a rule that is never seen to bite is not being tested.
    #[test]
    fn concealment_is_what_breaks_it_and_only_where_asked() {
        let theme = Theme::default();
        let r = concealing(&theme);
        let changed: Vec<&str> = SAMPLES
            .iter()
            .copied()
            .filter(|line| text_of(&r.line(line, false)) != *line)
            .collect();
        assert!(
            changed.len() >= 4,
            "concealment should visibly change marked-up lines, changed: {changed:?}"
        );
        // Prose with no markup is not touched, concealing or not.
        assert_eq!(text_of(&r.line("plain text", false)), "plain text");
        // Nor is anything inside a fence, where the syntax is the content.
        assert_eq!(
            text_of(&r.line("**not bold here**", true)),
            "**not bold here**"
        );
    }

    #[test]
    fn what_a_reader_sees_instead_of_the_syntax() {
        let theme = Theme::default();
        let r = concealing(&theme);
        for (source, shown) in [
            ("[[Note]]", "Note"),
            ("[[Note|alias]]", "alias"),
            ("[[Note#Heading]]", "Note#Heading"),
            ("[text](http://x)", "text"),
            ("**bold**", "bold"),
            ("*italic*", "italic"),
            ("==mark==", "mark"),
            ("`code`", "code"),
            ("## Heading", "Heading"),
            ("- [ ] a task", "- [ ] a task"),
            ("> quoted", "> quoted"),
            ("a #tag stays", "a #tag stays"),
        ] {
            assert_eq!(
                text_of(&r.line(source, false)),
                shown,
                "concealing {source:?}"
            );
        }
    }

    #[test]
    fn a_prefix_moves_the_links_it_pushes_along() {
        let theme = Theme::default();
        let rendered = concealing(&theme).render("see [[Note|alias]]", false);
        let before = rendered.link_at(4).unwrap().clone();
        let marked = rendered.prefixed("▸ ", theme.faded());
        assert_eq!(marked.text, "▸ see alias");
        let after = marked
            .link_at(before.start + 2)
            .expect("link moved with it");
        assert_eq!(after.target, before.target);
        assert_eq!(after.len, before.len);
        assert!(
            marked.link_at(before.start).is_none(),
            "and is no longer where it was"
        );
    }

    #[test]
    fn a_suffix_leaves_the_links_where_they_are() {
        let theme = Theme::default();
        let rendered = concealing(&theme).render("[[Note]]", false);
        let tail = rendered.suffixed("   4 lines", theme.faded());
        assert_eq!(tail.text, "Note   4 lines");
        assert_eq!(tail.link_at(0).unwrap().target, "Note");
    }

    #[test]
    fn slicing_a_row_out_of_a_line_keeps_the_styling_on_it() {
        let theme = Theme::default();
        let rendered = concealing(&theme).render("see [[Note|alias]] there", false);
        assert_eq!(rendered.text, "see alias there");
        let whole: String = rendered
            .slice(0, rendered.text.chars().count())
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(whole, rendered.text, "a full slice is the line");

        // A cut through the middle of the link: both halves keep its colour.
        let left = rendered.slice(0, 6);
        let right = rendered.slice(6, 9);
        let text: String = left
            .iter()
            .chain(right.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(text, rendered.text, "the halves put the line back together");
        let link_style = rendered.spans[1].style;
        assert_eq!(left.last().unwrap().style, link_style);
        assert_eq!(right[0].style, link_style);
    }

    #[test]
    fn slicing_past_the_end_yields_what_is_there_and_no_more() {
        let theme = Theme::default();
        let rendered = renderer(&theme).render("short", false);
        let spans = rendered.slice(2, 100);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(text, "ort");
        assert!(rendered.slice(99, 5).is_empty());
    }

    #[test]
    fn a_link_knows_which_drawn_columns_are_its_own() {
        let theme = Theme::default();
        let rendered = concealing(&theme).render("see [[Note|alias]] there", false);
        assert_eq!(rendered.text, "see alias there");
        let link = rendered.link_at(4).expect("a link under column 4");
        assert_eq!(link.target, "Note");
        assert_eq!(link.kind, Target::Note);
        assert_eq!((link.start, link.len), (4, 5), "exactly the drawn alias");
        assert!(rendered.link_at(3).is_none(), "the space before it is not");
        assert!(rendered.link_at(9).is_none(), "nor the space after");
    }

    #[test]
    fn a_markdown_link_points_at_its_url_not_its_label() {
        let theme = Theme::default();
        let rendered = concealing(&theme).render("[text](http://x)", false);
        assert_eq!(rendered.text, "text");
        let link = rendered.link_at(0).unwrap();
        assert_eq!(link.target, "http://x");
        assert_eq!(link.kind, Target::Url);
    }

    #[test]
    fn a_broken_link_is_still_visibly_broken_when_rendered() {
        let theme = Theme::default();
        let r = concealing(&theme);
        let live = r.render("[[Real]]", false);
        let dead = r.render("[[Missing]]", false);
        assert_eq!(live.text, "Real");
        assert_eq!(dead.text, "Missing");
        assert_eq!(live.spans[0].style.fg, Some(theme.link));
        assert_eq!(
            dead.spans[0].style.fg,
            Some(theme.broken),
            "concealing the brackets must not conceal that it goes nowhere"
        );
    }

    #[test]
    fn the_drawn_text_is_exactly_what_the_spans_say() {
        // Out builds both together, and this is the property that makes a click
        // land on the link it looks like it landed on.
        let theme = Theme::default();
        for r in [renderer(&theme), concealing(&theme)] {
            for line in SAMPLES {
                let rendered = r.render(line, false);
                assert_eq!(text_of(&rendered.spans), rendered.text, "on {line:?}");
            }
        }
    }

    #[test]
    fn every_link_range_lands_inside_the_text_it_indexes() {
        let theme = Theme::default();
        for r in [renderer(&theme), concealing(&theme)] {
            for line in SAMPLES {
                let rendered = r.render(line, false);
                let len = rendered.text.chars().count();
                for link in &rendered.links {
                    assert!(
                        link.start + link.len <= len,
                        "{line:?}: link {link:?} runs past {len} characters"
                    );
                }
            }
        }
    }

    #[test]
    fn broken_links_are_coloured_differently_from_live_ones() {
        let theme = Theme::default();
        let r = renderer(&theme);
        let live = r.line("[[Real]]", false);
        let dead = r.line("[[Missing]]", false);
        assert_eq!(live[0].style.fg, Some(theme.link));
        assert_eq!(dead[0].style.fg, Some(theme.broken));
    }

    #[test]
    fn aliased_and_headed_links_resolve_on_the_target_only() {
        let theme = Theme::default();
        let r = renderer(&theme);
        let spans = r.line("[[Missing#Section|alias]]", false);
        assert_eq!(spans[0].style.fg, Some(theme.broken));
    }

    #[test]
    fn headings_dim_their_hashes() {
        let theme = Theme::default();
        let spans = renderer(&theme).line("## Title", false);
        assert_eq!(spans[0].content.as_ref(), "##");
        assert_eq!(spans[0].style.fg, Some(theme.faint));
    }

    #[test]
    fn completed_tasks_are_struck_through() {
        let theme = Theme::default();
        let spans = renderer(&theme).line("- [x] done", false);
        let body = spans.last().unwrap();
        assert!(body.style.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn code_blocks_render_as_a_single_code_span() {
        let theme = Theme::default();
        let spans = renderer(&theme).line("let x = [[not a link]];", true);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.fg, Some(theme.code));
    }

    #[test]
    fn fences_are_detected() {
        assert!(is_fence("```rust"));
        assert!(is_fence("  ~~~"));
        assert!(!is_fence("``inline``"));
    }

    #[test]
    fn list_markers_split_off_the_body() {
        assert_eq!(split_list_marker("  - item"), Some(("  - ", "item")));
        assert_eq!(split_list_marker("12. item"), Some(("12. ", "item")));
        assert_eq!(split_list_marker("- [ ] t"), Some(("- [ ] ", "t")));
        assert_eq!(split_list_marker("no marker"), None);
    }

    #[test]
    fn tags_need_a_word_boundary() {
        let theme = Theme::default();
        let r = renderer(&theme);
        let spans = r.line("word#nottag and #realtag", false);
        let tagged: Vec<&str> = spans
            .iter()
            .filter(|s| s.style.fg == Some(theme.tag))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(tagged, vec!["#realtag"]);
    }
}
