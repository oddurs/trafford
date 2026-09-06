use super::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

/// Renders markdown as styled spans **without changing the characters**, so the
/// same renderer can back both the preview pane and the editor, where the
/// cursor column must line up with the underlying buffer.
pub struct Renderer<'a> {
    pub theme: &'a Theme,
    /// Returns true when a `[[wikilink]]` target exists in the vault.
    pub resolves: &'a dyn Fn(&str) -> bool,
}

/// True when a line opens or closes a fenced code block.
pub fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

impl<'a> Renderer<'a> {
    pub fn line(&self, text: &str, in_code_block: bool) -> Vec<Span<'static>> {
        let t = self.theme;

        if in_code_block || is_fence(text) {
            return vec![Span::styled(text.to_string(), Style::default().fg(t.code))];
        }

        let trimmed = text.trim_start();
        let indent_len = text.len() - trimmed.len();

        // Headings: dim the hashes, brighten the text.
        if trimmed.starts_with('#') {
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            if hashes <= 6 && trimmed.chars().nth(hashes) == Some(' ') {
                let mut spans = vec![Span::styled(
                    format!("{}{}", &text[..indent_len], "#".repeat(hashes)),
                    t.faded(),
                )];
                spans.extend(self.inline(&trimmed[hashes..], t.heading_style(hashes as u8)));
                return spans;
            }
        }

        // Horizontal rules.
        if matches!(trimmed, "---" | "***" | "___") {
            return vec![Span::styled(text.to_string(), t.faded())];
        }

        // Block quotes.
        if let Some(quoted) = trimmed.strip_prefix('>') {
            let mut spans = vec![Span::styled(text[..indent_len].to_string(), t.faded())];
            spans.push(Span::styled(">".to_string(), Style::default().fg(t.accent)));
            spans.extend(self.inline(
                quoted,
                Style::default().fg(t.dim).add_modifier(Modifier::ITALIC),
            ));
            return spans;
        }

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut rest = text;

        // Leading list marker or task checkbox, coloured but never rewritten.
        if let Some((marker, tail)) = split_list_marker(text) {
            let style = if marker.trim_start().starts_with("- [x]") {
                Style::default().fg(t.add)
            } else {
                Style::default().fg(t.accent)
            };
            spans.push(Span::styled(marker.to_string(), style));
            rest = tail;
        }

        let body_style = if text.trim_start().starts_with("- [x]") {
            Style::default()
                .fg(t.dim)
                .add_modifier(Modifier::CROSSED_OUT)
        } else {
            Style::default().fg(t.fg)
        };
        spans.extend(self.inline(rest, body_style));
        spans
    }

    /// Scan inline markup. Every input character ends up in exactly one span.
    fn inline(&self, text: &str, base: Style) -> Vec<Span<'static>> {
        let t = self.theme;
        let chars: Vec<char> = text.chars().collect();
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut plain = String::new();
        let mut i = 0;

        macro_rules! flush {
            () => {
                if !plain.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut plain), base));
                }
            };
        }

        while i < chars.len() {
            // [[wikilink]]
            if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
                if let Some(end) = find_pair(&chars, i + 2, ']') {
                    let inner: String = chars[i + 2..end].iter().collect();
                    let target = inner
                        .split(['|', '#'])
                        .next()
                        .unwrap_or(&inner)
                        .trim()
                        .to_string();
                    let colour = if (self.resolves)(&target) {
                        t.link
                    } else {
                        t.link_broken
                    };
                    flush!();
                    spans.push(Span::styled(
                        chars[i..end + 2].iter().collect::<String>(),
                        Style::default()
                            .fg(colour)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
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
                            flush!();
                            spans.push(Span::styled(
                                chars[i..=close].iter().collect::<String>(),
                                Style::default().fg(t.link),
                            ));
                            spans.push(Span::styled(
                                chars[close + 1..=paren].iter().collect::<String>(),
                                t.faded(),
                            ));
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
                    flush!();
                    spans.push(Span::styled(
                        chars[i..=end].iter().collect::<String>(),
                        Style::default().fg(t.code),
                    ));
                    i = end + 1;
                    continue;
                }
            }

            // **bold**
            if chars[i] == '*' && chars.get(i + 1) == Some(&'*') {
                if let Some(end) = find_run(&chars, i + 2, '*', 2) {
                    flush!();
                    spans.push(Span::styled(
                        chars[i..end + 2].iter().collect::<String>(),
                        base.add_modifier(Modifier::BOLD),
                    ));
                    i = end + 2;
                    continue;
                }
            }

            // ==highlight==
            if chars[i] == '=' && chars.get(i + 1) == Some(&'=') {
                if let Some(end) = find_run(&chars, i + 2, '=', 2) {
                    flush!();
                    spans.push(Span::styled(
                        chars[i..end + 2].iter().collect::<String>(),
                        Style::default().fg(t.bg).bg(t.accent),
                    ));
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
                        flush!();
                        spans.push(Span::styled(
                            chars[i..=end].iter().collect::<String>(),
                            base.add_modifier(Modifier::ITALIC),
                        ));
                        i = end + 1;
                        continue;
                    }
                }
            }

            // #tag at a word boundary
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
                    flush!();
                    spans.push(Span::styled(
                        chars[i..end].iter().collect::<String>(),
                        Style::default().fg(t.tag),
                    ));
                    i = end;
                    continue;
                }
            }

            plain.push(chars[i]);
            i += 1;
        }
        flush!();
        spans
    }
}

/// Split a leading list marker (`- `, `* `, `1. `, `- [ ] `) from a line.
fn split_list_marker(line: &str) -> Option<(&str, &str)> {
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
        }
    }

    fn text_of(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// The critical invariant: styling never adds or removes characters, or the
    /// editor cursor would drift away from the buffer.
    fn assert_char_preserving(input: &str) {
        let theme = Theme::night();
        let spans = renderer(&theme).line(input, false);
        assert_eq!(text_of(&spans), input, "renderer changed the text");
    }

    #[test]
    fn rendering_preserves_every_character() {
        for line in [
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
        ] {
            assert_char_preserving(line);
        }
    }

    #[test]
    fn broken_links_are_coloured_differently_from_live_ones() {
        let theme = Theme::night();
        let r = renderer(&theme);
        let live = r.line("[[Real]]", false);
        let dead = r.line("[[Missing]]", false);
        assert_eq!(live[0].style.fg, Some(theme.link));
        assert_eq!(dead[0].style.fg, Some(theme.link_broken));
    }

    #[test]
    fn aliased_and_headed_links_resolve_on_the_target_only() {
        let theme = Theme::night();
        let r = renderer(&theme);
        let spans = r.line("[[Missing#Section|alias]]", false);
        assert_eq!(spans[0].style.fg, Some(theme.link_broken));
    }

    #[test]
    fn headings_dim_their_hashes() {
        let theme = Theme::night();
        let spans = renderer(&theme).line("## Title", false);
        assert_eq!(spans[0].content.as_ref(), "##");
        assert_eq!(spans[0].style.fg, Some(theme.faint));
    }

    #[test]
    fn completed_tasks_are_struck_through() {
        let theme = Theme::night();
        let spans = renderer(&theme).line("- [x] done", false);
        let body = spans.last().unwrap();
        assert!(body.style.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn code_blocks_render_as_a_single_code_span() {
        let theme = Theme::night();
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
        let theme = Theme::night();
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
