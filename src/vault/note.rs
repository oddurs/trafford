use std::path::{Path, PathBuf};

/// A wiki-style link found inside a note: `[[Target#heading|alias]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
    /// The source line, trimmed. Kept here because the index only retains a
    /// lowercased copy of the body for searching, which would mangle case.
    pub context: String,
    /// Position within the source line, used for click/jump targeting.
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

#[derive(Debug, Clone)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

/// A single markdown note in the vault.
#[derive(Debug, Clone)]
pub struct Note {
    /// Path relative to the vault root, always using `/` separators.
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    pub links: Vec<WikiLink>,
    pub headings: Vec<Heading>,
    pub words: usize,
    pub modified: std::time::SystemTime,
    /// The note as written. Held so search results and the assistant's context
    /// can be shown with their original case, without re-reading from disk.
    pub text: String,
    /// A lowercased copy of `text` for allocation-free substring search.
    /// `to_lowercase` never changes the number of newlines, so line N here is
    /// always line N of `text`.
    pub haystack: String,
}

impl Note {
    /// The line a `#anchor` names, matching either the heading's text or its
    /// slug — a link may be written either way, and both should work.
    ///
    /// Duplicate headings resolve to the first, as GitHub does.
    pub fn heading_line(&self, anchor: &str) -> Option<usize> {
        let anchor = anchor.trim();
        if anchor.is_empty() {
            return None;
        }
        let wanted = slug(anchor);
        self.headings
            .iter()
            .find(|h| h.text.eq_ignore_ascii_case(anchor) || slug(&h.text) == wanted)
            .map(|h| h.line)
    }

    /// The filename stem — what `[[Bare Links]]` match against.
    pub fn stem(&self) -> &str {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.id)
    }

    pub fn parse(root: &Path, path: &Path, text: &str) -> Note {
        let id = relative_id(root, path);
        let (frontmatter, body_offset) = split_frontmatter(text);
        let mut tags = Vec::new();
        let mut title = None;

        for (key, value) in frontmatter {
            match key.as_str() {
                "title" => title = Some(value.trim_matches('"').to_string()),
                "tags" => tags.extend(parse_tag_list(&value)),
                _ => {}
            }
        }

        let mut links = Vec::new();
        let mut headings = Vec::new();
        let mut words = 0usize;
        let mut in_code_fence = false;

        for (idx, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                in_code_fence = !in_code_fence;
                continue;
            }
            if in_code_fence {
                continue;
            }
            if idx < body_offset {
                continue;
            }
            words += line.split_whitespace().count();

            if let Some(h) = parse_heading(line, idx) {
                // A template's H1 is a placeholder, not a name. Falling back to
                // the filename keeps `_templates/` readable instead of listing
                // three notes all called `<% tp.file.title %>`.
                if title.is_none() && h.level == 1 && !is_placeholder(&h.text) {
                    title = Some(h.text.clone());
                }
                headings.push(h);
            }
            links.extend(parse_wikilinks(line, idx));
            tags.extend(parse_inline_tags(line));
        }

        tags.sort();
        tags.dedup();

        let title = title.unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string()
        });

        Note {
            id,
            path: path.to_path_buf(),
            title,
            tags,
            links,
            headings,
            words,
            modified: std::time::SystemTime::UNIX_EPOCH,
            text: text.to_string(),
            haystack: text.to_lowercase(),
        }
    }
}

/// True when a string is a template expression rather than real text —
/// Templater's `<% ... %>` or Obsidian core's `{{...}}`.
fn is_placeholder(text: &str) -> bool {
    let t = text.trim();
    (t.contains("<%") && t.contains("%>")) || (t.contains("{{") && t.contains("}}"))
}

/// A heading turned into the anchor a link would name it by.
///
/// GitHub's rule, because that is what people's notes are already written
/// against: lowercase, punctuation dropped, spaces to hyphens. Matching it
/// exactly matters more than any improvement on it would.
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_hyphen = true; // trims leading hyphens
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_was_hyphen = false;
        } else if (c.is_whitespace() || c == '-' || c == '_') && !last_was_hyphen {
            out.push('-');
            last_was_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn relative_id(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Returns the key/value pairs of a YAML-ish frontmatter block and the line
/// index at which the body starts. Only flat `key: value` pairs and simple
/// `- item` lists are understood, which covers ordinary Obsidian frontmatter.
fn split_frontmatter(text: &str) -> (Vec<(String, String)>, usize) {
    let mut lines = text.lines();
    if lines.next().map(|l| l.trim_end()) != Some("---") {
        return (Vec::new(), 0);
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut last_key: Option<String> = None;
    for (i, line) in text.lines().enumerate().skip(1) {
        if line.trim_end() == "---" {
            return (pairs, i + 1);
        }
        let trimmed = line.trim();
        if let Some(item) = trimmed.strip_prefix("- ") {
            if let Some(key) = &last_key {
                if let Some(slot) = pairs.iter_mut().find(|(k, _)| k == key) {
                    slot.1.push(',');
                    slot.1.push_str(item.trim());
                }
            }
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            pairs.push((key.clone(), value.trim().to_string()));
            last_key = Some(key);
        }
    }
    (pairs, 0)
}

/// The frontmatter block at the top of a note: its key/value pairs, and the
/// line the body starts on.
///
/// `None` when there is none, or when the block is never closed — an
/// unterminated `---` is not frontmatter, it is a note that begins with a
/// horizontal rule and should be shown as written.
pub fn frontmatter_block(lines: &[String]) -> Option<(Vec<(String, String)>, usize)> {
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return None;
    }
    let text = lines.join("\n");
    let (pairs, body) = split_frontmatter(&text);
    // `split_frontmatter` reports 0 for an unterminated block.
    if body == 0 {
        return None;
    }
    // A `- item` list accumulates onto an empty value, so it comes back with a
    // leading comma. That is an artefact of how it is gathered rather than
    // anything the author wrote, and `parse_tag_list` drops it silently — but
    // anything that shows the value to a reader has to.
    let pairs = pairs
        .into_iter()
        .map(|(k, v)| (k, v.trim_start_matches(',').to_string()))
        .collect();
    Some((pairs, body))
}

fn parse_tag_list(value: &str) -> Vec<String> {
    value
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|t| {
            t.trim()
                .trim_matches('"')
                .trim_start_matches('#')
                .to_string()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

fn parse_heading(line: &str, idx: usize) -> Option<Heading> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level > 6 {
        return None;
    }
    let text = trimmed[level..].trim();
    if text.is_empty() {
        return None;
    }
    Some(Heading {
        level: level as u8,
        text: text.to_string(),
        line: idx,
    })
}

/// Extract `#tag` occurrences, skipping headings and URL fragments.
fn parse_inline_tags(line: &str) -> Vec<String> {
    if line.trim_start().starts_with('#') {
        return Vec::new();
    }
    let bytes: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '#' && (i == 0 || bytes[i - 1].is_whitespace()) {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_alphanumeric()
                    || bytes[end] == '-'
                    || bytes[end] == '_'
                    || bytes[end] == '/')
            {
                end += 1;
            }
            if end > start {
                let tag: String = bytes[start..end].iter().collect();
                // Obsidian requires a non-numeric character, which is what
                // keeps "their #1 barrier" and "Lex Fridman #333" out of the
                // tag list. Without it, ordinary prose becomes tags.
                if tag.chars().any(|c| !c.is_ascii_digit()) {
                    out.push(tag);
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Parse every `[[...]]` on a line. Columns are character offsets.
pub fn parse_wikilinks(line: &str, line_idx: usize) -> Vec<WikiLink> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '[' && chars[i + 1] == '[' {
            if let Some(close) = find_close(&chars, i + 2) {
                let inner: String = chars[i + 2..close].iter().collect();
                if !inner.is_empty() && !inner.contains('[') {
                    out.push(build_link(&inner, line, line_idx, i, close + 2 - i));
                }
                i = close + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn find_close(chars: &[char], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == ']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn build_link(inner: &str, source: &str, line: usize, col: usize, len: usize) -> WikiLink {
    let (target_part, alias) = match inner.split_once('|') {
        Some((t, a)) => (t, Some(a.trim().to_string())),
        None => (inner, None),
    };
    let (target, heading) = match target_part.split_once('#') {
        Some((t, h)) => (t, Some(h.trim().to_string())),
        None => (target_part, None),
    };
    WikiLink {
        target: target.trim().to_string(),
        heading,
        alias,
        context: source.trim().to_string(),
        line,
        col,
        len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_alias_and_heading_links() {
        let links = parse_wikilinks("see [[Alpha]] and [[b/Beta#Intro|the beta]] ok", 3);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Alpha");
        assert_eq!(links[0].line, 3);
        assert_eq!(links[1].target, "b/Beta");
        assert_eq!(links[1].heading.as_deref(), Some("Intro"));
        assert_eq!(links[1].alias.as_deref(), Some("the beta"));
        assert_eq!(
            links[0].context,
            "see [[Alpha]] and [[b/Beta#Intro|the beta]] ok"
        );
    }

    #[test]
    fn link_columns_point_at_the_opening_bracket() {
        let links = parse_wikilinks("xx [[A]]", 0);
        assert_eq!(links[0].col, 3);
        assert_eq!(links[0].len, "[[A]]".chars().count());
    }

    #[test]
    fn a_frontmatter_block_reports_where_the_body_starts() {
        let src: Vec<String> = "---\ntags:\n  - one\n---\n# Body"
            .split('\n')
            .map(str::to_string)
            .collect();
        let (pairs, body) = frontmatter_block(&src).expect("a block");
        assert_eq!(body, 4, "the line after the closing ---");
        assert_eq!(pairs, vec![("tags".to_string(), "one".to_string())]);
    }

    #[test]
    fn an_unterminated_block_is_not_frontmatter() {
        let src: Vec<String> = "---\ntags: one\n# never closed"
            .split('\n')
            .map(str::to_string)
            .collect();
        assert!(frontmatter_block(&src).is_none());
    }

    #[test]
    fn a_rule_partway_down_is_not_frontmatter() {
        let src: Vec<String> = "# Title\n---\nkey: value\n---"
            .split('\n')
            .map(str::to_string)
            .collect();
        assert!(frontmatter_block(&src).is_none());
    }

    #[test]
    fn frontmatter_tags_and_title_are_read() {
        let text = "---\ntitle: Real Title\ntags:\n  - one\n  - two\n---\n# Heading\nbody\n";
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), text);
        assert_eq!(note.title, "Real Title");
        assert_eq!(note.tags, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn a_templated_heading_does_not_become_the_title() {
        for heading in [
            "<% tp.file.title %>",
            "{{title}}",
            "<% tp.date.now(\"YYYY\") %>",
        ] {
            let text = format!("# {heading}\n\nbody\n");
            let note = Note::parse(Path::new("/v"), Path::new("/v/daily-note.md"), &text);
            assert_eq!(note.title, "daily-note", "for heading {heading}");
        }
    }

    #[test]
    fn an_ordinary_heading_is_still_the_title() {
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), "# Real Heading\n");
        assert_eq!(note.title, "Real Heading");
        // Percent signs on their own are not a placeholder.
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), "# 50% Done\n");
        assert_eq!(note.title, "50% Done");
    }

    #[test]
    fn inline_tags_are_collected_but_headings_are_not() {
        let text = "# Not A Tag\nsome #alpha and #beta/nested here\n";
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), text);
        assert_eq!(
            note.tags,
            vec!["alpha".to_string(), "beta/nested".to_string()]
        );
    }

    #[test]
    fn lowercasing_preserves_line_correspondence() {
        // The search index relies on this: line N of `haystack` must always be
        // line N of `text`, even for characters that change length when folded.
        let text = "Ärger\nİstanbul\nSTRASSE\n";
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), text);
        assert_eq!(note.text.lines().count(), note.haystack.lines().count());
    }

    #[test]
    fn purely_numeric_hashes_are_not_tags() {
        let text = "\
cost is their #1 barrier
Lex Fridman #333 — Karpathy
see issue #490 and order #123
but #a1 and #2026-review and #topic/ai are tags
";
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), text);
        assert_eq!(
            note.tags,
            vec![
                "2026-review".to_string(),
                "a1".to_string(),
                "topic/ai".to_string()
            ]
        );
    }

    #[test]
    fn slugs_follow_the_rule_notes_are_written_against() {
        assert_eq!(
            slug("Phase 1: Foundations (Months 1-3)"),
            "phase-1-foundations-months-1-3"
        );
        assert_eq!(
            slug("The three-layer architecture"),
            "the-three-layer-architecture"
        );
        assert_eq!(
            slug("Why this works (the bookkeeping)"),
            "why-this-works-the-bookkeeping"
        );
        assert_eq!(slug("  Leading and trailing  "), "leading-and-trailing");
        assert_eq!(slug("Karpathy's LLM Wiki"), "karpathys-llm-wiki");
        // Runs of punctuation collapse rather than leaving empty segments.
        assert_eq!(slug("A -- B"), "a-b");
        assert_eq!(slug("!!!"), "");
    }

    #[test]
    fn an_anchor_matches_a_heading_by_slug_or_by_text() {
        let text = "# Top\n\n## Phase 1: Foundations\n\nbody\n### Deep Dive\n";
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), text);
        assert_eq!(note.heading_line("phase-1-foundations"), Some(2));
        assert_eq!(note.heading_line("Phase 1: Foundations"), Some(2));
        assert_eq!(note.heading_line("deep-dive"), Some(5));
        assert_eq!(note.heading_line("nothing here"), None);
        assert_eq!(note.heading_line(""), None);
    }

    #[test]
    fn a_duplicated_heading_resolves_to_the_first() {
        let text = "## Goal\n\na\n## Goal\n\nb\n";
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), text);
        assert_eq!(note.heading_line("goal"), Some(0));
    }

    #[test]
    fn code_fences_are_skipped() {
        let text = "```\n[[NotALink]]\n```\n[[Real]]\n";
        let note = Note::parse(Path::new("/v"), Path::new("/v/n.md"), text);
        assert_eq!(note.links.len(), 1);
        assert_eq!(note.links[0].target, "Real");
    }
}
