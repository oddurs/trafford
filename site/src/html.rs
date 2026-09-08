//! Notes to HTML, through the same scanners the terminal draws with.
//!
//! There is no markdown parser in this file. Headings come from
//! `ui::fold::headings`, tables from `ui::table::parse`, callouts from
//! `ui::callout::parse`, frontmatter from `vault::note::frontmatter_block`,
//! and inline markup from `ui::markdown::scan`. What is here is a second
//! *backend* for those — tags where the terminal writes styles — which is the
//! only way to render a vault's markdown twice without two ideas of what it
//! says.
//!
//! The consequence worth stating: `[[wikilinks]]`, `> [!note]` callouts,
//! `#tags` and `[[Note\|alias]]` inside a table cell all work here because
//! they work in the app, not because they were implemented again.

use std::collections::BTreeMap;

/// Where files copied out of the vault land, relative to the site root.
pub const ASSET_DIR: &str = "assets";
use std::fmt::Write as _;

use trafford::ui::markdown::{scan, split_list_marker, Inline};
use trafford::ui::{callout, fold, table};
use trafford::vault::note::{self, frontmatter_block};
use trafford::vault::{Note, Vault};

/// Something wrong with the source, reported with enough to go and fix it.
///
/// Collected rather than returned on the first one: a rename breaks every link
/// to the old name at once, and being told about them one build at a time is
/// how a broken tree takes ten builds to unbreak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub file: String,
    /// One-based, so it matches what an editor shows.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

/// Where a note ended up on the site.
#[derive(Debug, Clone)]
pub struct PageLink {
    /// Path from the site root, with a trailing slash: `docs/getting-started/`.
    pub url: String,
    pub title: String,
}

/// What the renderer needs to turn a link into an href.
pub struct Ctx<'a> {
    pub vault: &'a Vault,
    /// Note id to page, for every note that became a page.
    pub pages: &'a BTreeMap<String, PageLink>,
    /// How many path segments deep the page being rendered is.
    ///
    /// Every href on the site is relative, computed from this. That is not a
    /// stylistic choice: it is what makes the same output correct at a domain
    /// root, under a GitHub Pages project subpath, and opened from `file://`
    /// with no server at all — one build, not three.
    pub depth: usize,
}

impl Ctx<'_> {
    /// A root-relative path, rewritten to be relative to the current page.
    pub fn href(&self, from_root: &str) -> String {
        let mut out = "../".repeat(self.depth);
        out.push_str(from_root);
        if out.is_empty() {
            "./".to_string()
        } else {
            out
        }
    }
}

/// One heading, for the table of contents beside the page.
#[derive(Debug, Clone)]
pub struct TocEntry {
    pub level: usize,
    pub text: String,
    pub anchor: String,
}

/// A rendered note.
pub struct Rendered {
    pub html: String,
    pub toc: Vec<TocEntry>,
    /// The first paragraph, as plain text — the page description.
    pub summary: String,
}

/// Escape text for an element body.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape text for a quoted attribute value.
pub fn escape_attr(s: &str) -> String {
    escape(s).replace('"', "&quot;").replace('\'', "&#39;")
}

/// The table of contents for a note: its H2s and H3s.
///
/// Reads `fold::headings`, which is the one heading scanner — so the anchors
/// here and the sections the app folds are the same list, and a `[[Note#X]]`
/// link cannot land somewhere the outline does not know about.
pub fn toc(lines: &[String]) -> Vec<TocEntry> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    fold::headings(lines)
        .into_iter()
        .filter(|h| h.level >= 2 && h.level <= 3)
        .map(|h| {
            let anchor = unique_anchor(&mut seen, &h.text);
            TocEntry {
                level: h.level,
                text: h.text,
                anchor,
            }
        })
        .collect()
}

/// An id for a heading, suffixed if the note repeats one.
///
/// Two headings with the same text would otherwise share an id, and a browser
/// jumps to the first — silently, so the link looks like it worked.
fn unique_anchor(seen: &mut BTreeMap<String, usize>, text: &str) -> String {
    let base = note::slug(text);
    let base = if base.is_empty() {
        "section".to_string()
    } else {
        base
    };
    let n = seen.entry(base.clone()).or_insert(0);
    *n += 1;
    if *n == 1 {
        base
    } else {
        format!("{base}-{n}")
    }
}

/// Render a note's body. Frontmatter is not part of it — the shell draws that.
pub fn render(note: &Note, ctx: &Ctx<'_>, problems: &mut Vec<Problem>) -> Rendered {
    render_with(note, ctx, problems, false)
}

/// Render, grouping each H2 and what follows it into a `<section>`.
pub fn render_sectioned(note: &Note, ctx: &Ctx<'_>, problems: &mut Vec<Problem>) -> Rendered {
    render_with(note, ctx, problems, true)
}

fn render_with(
    note: &Note,
    ctx: &Ctx<'_>,
    problems: &mut Vec<Problem>,
    sections: bool,
) -> Rendered {
    let lines: Vec<String> = note.text.lines().map(str::to_string).collect();
    let start = frontmatter_block(&lines).map(|(_, body)| body).unwrap_or(0);
    let mut w = Writer::new(note, ctx, problems);
    w.sections = sections;
    w.blocks(&lines, start, 0);
    w.finish(&lines)
}

/// Frontmatter as the properties row preview draws, minus the terminal.
///
/// Nothing is dropped: a key nobody anticipated still appears, because a vault
/// is someone's real notes and the schema is whatever they typed.
pub fn properties(note: &Note) -> Vec<(String, String)> {
    let lines: Vec<String> = note.text.lines().map(str::to_string).collect();
    frontmatter_block(&lines)
        .map(|(pairs, _)| pairs)
        .unwrap_or_default()
        .into_iter()
        // `", "` is what the terminal's property row joins a list with, and
        // this is meant to be that row minus the terminal.
        .map(|(k, v)| (k, v.join(", ")))
        .collect()
}

struct Writer<'a, 'b> {
    out: String,
    note: &'a Note,
    ctx: &'a Ctx<'b>,
    problems: &'a mut Vec<Problem>,
    seen_anchors: BTreeMap<String, usize>,
    summary: String,
    /// Group each H2 and what follows it into a `<section>`. The landing page
    /// wants it; a documentation page does not.
    sections: bool,
    section_open: bool,
}

/// One open list, so nesting closes in the right order.
struct Level {
    indent: usize,
    ordered: bool,
    item_open: bool,
}

impl<'a, 'b> Writer<'a, 'b> {
    fn new(note: &'a Note, ctx: &'a Ctx<'b>, problems: &'a mut Vec<Problem>) -> Self {
        Writer {
            out: String::new(),
            note,
            ctx,
            problems,
            seen_anchors: BTreeMap::new(),
            summary: String::new(),
            sections: false,
            section_open: false,
        }
    }

    fn finish(mut self, lines: &[String]) -> Rendered {
        if self.section_open {
            self.out.push_str("</section>\n");
        }
        // The last section of a landing note is its closing: a claim, centred,
        // with the command under it. A rule rather than a marker in the
        // markdown, because "the last one" is what it always is.
        if let Some(at) = self.out.rfind(SECTION) {
            self.out.replace_range(at..at + SECTION.len(), CLOSING);
        }
        Rendered {
            html: self.out,
            toc: toc(lines),
            summary: self.summary,
        }
    }

    fn problem(&mut self, line: usize, message: String) {
        self.problems.push(Problem {
            file: self.note.id.clone(),
            line: line + 1,
            message,
        });
    }

    /// Walk `lines[from..]` as blocks. `depth` guards the one recursive case.
    fn blocks(&mut self, lines: &[String], from: usize, depth: usize) {
        let mut lists: Vec<Level> = Vec::new();
        let mut para: Vec<(usize, String)> = Vec::new();
        let mut i = from;

        macro_rules! flush_para {
            () => {
                if !para.is_empty() {
                    let taken = std::mem::take(&mut para);
                    self.paragraph(&taken);
                }
            };
        }
        macro_rules! flush_lists {
            () => {
                while let Some(level) = lists.pop() {
                    if level.item_open {
                        self.out.push_str("</li>");
                    }
                    self.out
                        .push_str(if level.ordered { "</ol>\n" } else { "</ul>\n" });
                }
            };
        }

        while i < lines.len() {
            let raw = &lines[i];
            let trimmed = raw.trim();

            if trimmed.is_empty() {
                flush_para!();
                // A blank line only ends a list if what follows is not more of
                // it. Obsidian's notes are full of loose lists, and closing on
                // the blank turns one list into three.
                if !lists.is_empty() && !continues_list(lines, i + 1) {
                    flush_lists!();
                }
                i += 1;
                continue;
            }

            // A fence is opaque: the syntax inside it *is* the content.
            if trafford::ui::markdown::is_fence(raw) {
                flush_para!();
                flush_lists!();
                i = self.fence(lines, i);
                continue;
            }

            // Before quotes: a callout is a quote with a kind, and checking
            // the other order draws every callout as a blockquote.
            if let Some(c) = callout::parse(lines, i) {
                flush_para!();
                flush_lists!();
                self.callout(lines, i, &c, depth);
                i += c.height;
                continue;
            }

            if let Some(t) = table::parse(lines, i) {
                flush_para!();
                flush_lists!();
                self.table(&t, i);
                i += t.height;
                continue;
            }

            // An embed alone on a line is a block, not a word in a
            // paragraph. That is what lets a section be "prose and a figure"
            // rather than "prose containing a picture".
            if let Some(embed) = lone_embed(trimmed) {
                flush_para!();
                flush_lists!();
                let html = self.inline(embed, i);
                // The alias is the caption. `![[theme-paper.svg|Paper]]` in a
                // row of three is otherwise three colour schemes a reader
                // cannot name.
                let caption = match embed_label(embed) {
                    Some(label) if !label.ends_with(".svg") && !label.ends_with(".json") => {
                        format!("\n<figcaption>{}</figcaption>", escape(&label))
                    }
                    _ => String::new(),
                };
                let _ = writeln!(self.out, "<figure class=\"shot\">{html}{caption}</figure>");
                i += 1;
                continue;
            }

            if let Some((level, text)) = heading(raw) {
                flush_para!();
                flush_lists!();
                self.heading(level, text, i);
                i += 1;
                continue;
            }

            if matches!(trimmed, "---" | "***" | "___") {
                flush_para!();
                flush_lists!();
                self.out.push_str("<hr>\n");
                i += 1;
                continue;
            }

            if trimmed.starts_with('>') {
                flush_para!();
                flush_lists!();
                i = self.quote(lines, i, depth);
                continue;
            }

            if let Some((marker, body)) = split_list_marker(raw) {
                flush_para!();
                self.item(&mut lists, marker, body, i);
                i += 1;
                continue;
            }

            // A line indented under an open list item continues it rather than
            // starting a paragraph beside it.
            if !lists.is_empty() && raw.starts_with(' ') && para.is_empty() {
                let text = self.inline(trimmed, i);
                let _ = write!(self.out, " {text}");
                i += 1;
                continue;
            }

            flush_lists!();
            para.push((i, trimmed.to_string()));
            i += 1;
        }
        flush_para!();
        flush_lists!();
    }

    /// A run of plain lines is one paragraph. Notes here are hard-wrapped at
    /// 78 columns, so joining is not an optimisation — without it every line
    /// break in the source becomes a paragraph break on the page.
    fn paragraph(&mut self, lines: &[(usize, String)]) {
        let at = lines[0].0;
        let text = lines
            .iter()
            .map(|(_, s)| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if self.summary.is_empty() {
            self.summary = plain(&text);
        }
        let body = self.inline(&text, at);
        let _ = writeln!(self.out, "<p>{body}</p>");
    }

    fn heading(&mut self, level: usize, text: &str, at: usize) {
        // The landing page is a run of showcases, and a showcase is an H2 and
        // everything under it. Grouping them here means the note stays
        // ordinary markdown and the alternation is `:nth-of-type(even)`.
        if self.sections && level == 2 {
            if self.section_open {
                self.out.push_str("</section>\n");
            }
            self.out.push_str(SECTION);
            self.out.push('\n');
            self.section_open = true;
        }
        let anchor = unique_anchor(&mut self.seen_anchors, text);
        let body = self.inline(text, at);
        // The link is on the heading itself: a reader who wants to send someone
        // a section should not have to find a hover target first.
        let _ = writeln!(
            self.out,
            "<h{level} id=\"{id}\"><a class=\"anchor\" href=\"#{id}\">{body}</a></h{level}>",
            id = escape_attr(&anchor),
        );
    }

    /// A fenced block, from its opening fence to its closing one or the end of
    /// the note — an unclosed fence is a typo, not a reason to lose the rest.
    fn fence(&mut self, lines: &[String], at: usize) -> usize {
        let lang = lines[at]
            .trim_start()
            .trim_start_matches(['`', '~'])
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        let mut body = String::new();
        let mut i = at + 1;
        while i < lines.len() && !trafford::ui::markdown::is_fence(&lines[i]) {
            body.push_str(&lines[i]);
            body.push('\n');
            i += 1;
        }
        let class = if lang.is_empty() {
            String::new()
        } else {
            format!(" class=\"language-{}\"", escape_attr(&lang))
        };
        let _ = writeln!(
            self.out,
            "<pre><code{class}>{}</code></pre>",
            escape(&body).trim_end()
        );
        // Past the closing fence, or past the end if there was not one.
        (i + 1).min(lines.len())
    }

    fn quote(&mut self, lines: &[String], at: usize, depth: usize) -> usize {
        let mut inner = Vec::new();
        let mut i = at;
        while i < lines.len() && lines[i].trim_start().starts_with('>') {
            inner.push(strip_quote(&lines[i]));
            i += 1;
        }
        self.out.push_str("<blockquote>\n");
        self.nested(&inner, at, depth);
        self.out.push_str("</blockquote>\n");
        i
    }

    fn callout(&mut self, lines: &[String], at: usize, c: &callout::Callout, depth: usize) {
        let body: Vec<String> = lines[at..at + c.height]
            .iter()
            .skip(1)
            .map(|l| strip_quote(l))
            .collect();
        let title = c
            .title
            .clone()
            .unwrap_or_else(|| capitalise(&c.kind).to_string());
        // The kind is whatever the author wrote, lowercased. A kind nobody
        // anticipated still draws, labelled with what they typed — the same
        // rule the terminal follows, for the same reason.
        let _ = writeln!(
            self.out,
            "<aside class=\"callout callout-{kind}\" data-callout=\"{kind}\">\n<p class=\"callout-title\">{title}</p>",
            kind = escape_attr(&c.kind),
            title = escape(&title),
        );
        self.nested(&body, at + 1, depth);
        self.out.push_str("</aside>\n");
    }

    /// Render lines that came out of a container, guarding the recursion.
    ///
    /// A quote inside a quote inside a quote is real markdown; a note that
    /// nests forty deep is a stack overflow, and the build should say so
    /// rather than abort the process.
    fn nested(&mut self, lines: &[String], at: usize, depth: usize) {
        if depth >= 8 {
            self.problem(at, "block nested deeper than eight levels".into());
            return;
        }
        // The recursive call re-derives blocks from the stripped lines, which
        // is why a table or a list inside a callout works without a case here.
        let mut sub = Writer {
            out: std::mem::take(&mut self.out),
            note: self.note,
            ctx: self.ctx,
            problems: self.problems,
            seen_anchors: std::mem::take(&mut self.seen_anchors),
            summary: std::mem::take(&mut self.summary),
            // A quote or a callout is not where a showcase begins.
            sections: false,
            section_open: false,
        };
        sub.blocks(lines, 0, depth + 1);
        self.out = sub.out;
        self.seen_anchors = sub.seen_anchors;
        self.summary = sub.summary;
    }

    fn table(&mut self, t: &table::Table, at: usize) {
        self.out
            .push_str("<div class=\"table-scroll\">\n<table>\n<thead>\n<tr>");
        for (i, cell) in t.head.iter().enumerate() {
            let align = align_attr(t.aligns.get(i));
            let body = self.inline(cell, at);
            let _ = write!(self.out, "<th{align}>{body}</th>");
        }
        self.out.push_str("</tr>\n</thead>\n<tbody>\n");
        for row in &t.rows {
            self.out.push_str("<tr>");
            for (i, cell) in row.iter().enumerate() {
                let align = align_attr(t.aligns.get(i));
                let body = self.inline(cell, at);
                let _ = write!(self.out, "<td{align}>{body}</td>");
            }
            self.out.push_str("</tr>\n");
        }
        self.out.push_str("</tbody>\n</table>\n</div>\n");
    }

    fn item(&mut self, lists: &mut Vec<Level>, marker: &str, body: &str, at: usize) {
        let indent = marker.len() - marker.trim_start().len();
        let m = marker.trim_start();
        let ordered = m.chars().next().is_some_and(|c| c.is_ascii_digit());
        let checked = match m {
            _ if m.starts_with("- [x]") => Some(true),
            _ if m.starts_with("- [ ]") => Some(false),
            _ => None,
        };

        while lists.last().is_some_and(|l| l.indent > indent) {
            let level = lists.pop().expect("checked above");
            if level.item_open {
                self.out.push_str("</li>");
            }
            self.out
                .push_str(if level.ordered { "</ol>\n" } else { "</ul>\n" });
        }
        match lists.last_mut() {
            // Deeper than anything open: the new list belongs inside the item
            // that is still open, which is why nothing is closed first.
            None => {
                self.out.push_str(if ordered { "<ol>\n" } else { "<ul>\n" });
                lists.push(Level {
                    indent,
                    ordered,
                    item_open: false,
                });
            }
            Some(level) if level.indent < indent => {
                self.out.push_str(if ordered { "<ol>\n" } else { "<ul>\n" });
                lists.push(Level {
                    indent,
                    ordered,
                    item_open: false,
                });
            }
            Some(level) => {
                if level.item_open {
                    self.out.push_str("</li>\n");
                    level.item_open = false;
                }
            }
        }
        let class = match checked {
            Some(true) => " class=\"task done\"",
            Some(false) => " class=\"task\"",
            None => "",
        };
        let box_html = match checked {
            Some(done) => format!(
                "<input type=\"checkbox\" disabled{}> ",
                if done { " checked" } else { "" }
            ),
            None => String::new(),
        };
        let body = self.inline(body, at);
        let _ = write!(self.out, "<li{class}>{box_html}{body}");
        if let Some(level) = lists.last_mut() {
            level.item_open = true;
        }
    }

    /// Inline markup, from `markdown::scan` and nothing else.
    fn inline(&mut self, text: &str, at: usize) -> String {
        let mut out = String::with_capacity(text.len());
        for piece in scan(text) {
            match &piece.kind {
                Inline::Text => out.push_str(&escape(&piece.raw)),
                Inline::Code { inner } => {
                    // A key is not a snippet, and a page about a keyboard-driven
                    // program is mostly keys. `<kbd>` is what they are.
                    let tag = if is_key(inner) { "kbd" } else { "code" };
                    let _ = write!(out, "<{tag}>{}</{tag}>", escape(inner));
                }
                Inline::Strong { inner } => {
                    let _ = write!(out, "<strong>{}</strong>", escape(inner));
                }
                Inline::Emphasis { inner } => {
                    let _ = write!(out, "<em>{}</em>", escape(inner));
                }
                Inline::Highlight { inner } => {
                    let _ = write!(out, "<mark>{}</mark>", escape(inner));
                }
                Inline::Tag { name } => {
                    let _ = write!(out, "<span class=\"tag\">#{}</span>", escape(name));
                }
                Inline::Wiki {
                    target,
                    heading,
                    label,
                } => {
                    let embed = take_embed_marker(&mut out);
                    let html =
                        self.wikilink(target, heading.as_deref(), label, embed, at, &piece.raw);
                    out.push_str(&html);
                }
                Inline::Link { url, label } => {
                    let embed = take_embed_marker(&mut out);
                    let html = self.url_link(url, label, embed, at);
                    out.push_str(&html);
                }
            }
        }
        out
    }

    /// `[[Note#Heading|alias]]`, resolved the way the app resolves it.
    fn wikilink(
        &mut self,
        target: &str,
        heading: Option<&str>,
        label: &str,
        embed: bool,
        at: usize,
        // The link as the author wrote it, so an error names what they typed
        // rather than a normalised form they would then have to go and find.
        written: &str,
    ) -> String {
        // Attachments first: `![[photo.jpg]]` points at a real file, and a
        // vault that treats images as missing notes lists its own pictures as
        // things nobody has written.
        // `![[x.cast.json]]` is a recording. The poster is `x.svg`, emitted by
        // the same run, and it is what a reader sees with no JavaScript, with
        // reduced motion, or before the frames arrive.
        if embed {
            if let Some(cast) = target.strip_suffix(".cast.json") {
                return self.cast(target, cast, label, at);
            }
        }
        if let Some(rel) = self.ctx.vault.attachment(target) {
            let src = self.ctx.href(&format!("{ASSET_DIR}/{rel}"));
            return if embed {
                format!(
                    "<img src=\"{}\" alt=\"{}\" loading=\"lazy\">",
                    escape_attr(&src),
                    escape_attr(label)
                )
            } else {
                format!("<a href=\"{}\">{}</a>", escape_attr(&src), escape(label))
            };
        }

        let Some(index) = self.ctx.vault.resolve_target(target) else {
            self.problem(at, format!("{written} resolves to nothing"));
            return format!("<span class=\"broken\">{}</span>", escape(label));
        };
        let id = self.ctx.vault.notes[index].id.clone();
        let Some(page) = self.ctx.pages.get(&id) else {
            self.problem(
                at,
                format!("{written} resolves to {id}, which is not a page"),
            );
            return format!("<span class=\"broken\">{}</span>", escape(label));
        };
        let mut href = self.ctx.href(&page.url);
        if let Some(anchor) = heading {
            match self.ctx.vault.notes[index].heading_line(anchor) {
                Some(_) => {
                    href.push('#');
                    href.push_str(&note::slug(anchor));
                }
                None => self.problem(at, format!("{written} names a heading {id} does not have")),
            }
        }
        format!("<a href=\"{}\">{}</a>", escape_attr(&href), escape(label))
    }

    /// A recording, drawn over the poster the same run produced.
    fn cast(&mut self, target: &str, stem: &str, label: &str, at: usize) -> String {
        let (Some(json), Some(poster)) = (
            self.ctx.vault.attachment(target),
            self.ctx.vault.attachment(&format!("{stem}.svg")),
        ) else {
            self.problem(
                at,
                format!("![[{target}]] needs both {target} and {stem}.svg in the vault"),
            );
            return String::new();
        };
        format!(
            "<span class=\"cast\" data-cast=\"{}\"><img src=\"{}\" alt=\"{}\" loading=\"lazy\"></span>",
            escape_attr(&self.ctx.href(&format!("{ASSET_DIR}/{json}"))),
            escape_attr(&self.ctx.href(&format!("{ASSET_DIR}/{poster}"))),
            escape_attr(label)
        )
    }

    fn url_link(&mut self, url: &str, label: &str, embed: bool, at: usize) -> String {
        if embed {
            return format!(
                "<img src=\"{}\" alt=\"{}\" loading=\"lazy\">",
                escape_attr(url),
                escape_attr(label)
            );
        }
        // `[text](other.md)` is how markdown that has to survive GitHub writes
        // a link to another note, and it should be checked exactly as hard as
        // a `[[wikilink]]` — a docs tree with two link syntaxes and one link
        // checker is a docs tree with unchecked links.
        if let Some(anchor) = url.strip_prefix('#') {
            if self.note.heading_line(anchor).is_none() {
                self.problem(at, format!("#{anchor} names no heading in this note"));
            }
            return format!(
                "<a href=\"#{}\">{}</a>",
                escape_attr(&note::slug(anchor)),
                escape(label)
            );
        }
        let external =
            url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:");
        // `api/...` is the rustdoc tree, which is a build product rather than a
        // note. Root-relative, then made relative to this page like everything
        // else. `build::check_api_links` is what makes a renamed type a build
        // failure rather than a 404.
        if let Some(rest) = url.strip_prefix("api/") {
            return format!(
                "<a href=\"{}\"><code>{}</code></a>",
                escape_attr(&self.ctx.href(&format!("api/{rest}"))),
                escape(label)
            );
        }
        if !external && !url.starts_with('/') {
            let (path, anchor) = match url.split_once('#') {
                Some((p, a)) => (p, Some(a)),
                None => (url, None),
            };
            let target = path.strip_suffix(".md").unwrap_or(path);
            return self.wikilink(
                target,
                anchor,
                label,
                false,
                at,
                &format!("[{label}]({url})"),
            );
        }
        // rel on external links only: noopener is meaningless without a target,
        // and putting it everywhere hides which links leave the site.
        let extra = if external {
            " rel=\"noreferrer noopener\" target=\"_blank\""
        } else {
            ""
        };
        format!(
            "<a href=\"{}\"{extra}>{}</a>",
            escape_attr(url),
            escape(label)
        )
    }
}

/// `![[x]]` and `![](x)` are embeds, and the scanner reports the `!` as text
/// because it is text everywhere else. Take it back off the tail if it is
/// there — the alternative is teaching the scanner about a rule that only the
/// renderer cares about.
fn take_embed_marker(out: &mut String) -> bool {
    if out.ends_with('!') {
        out.pop();
        true
    } else {
        false
    }
}

fn align_attr(align: Option<&table::Align>) -> &'static str {
    match align {
        Some(table::Align::Right) => " class=\"right\"",
        Some(table::Align::Center) => " class=\"center\"",
        _ => "",
    }
}

const SECTION: &str = "<section class=\"showcase\">";
const CLOSING: &str = "<section class=\"showcase closing\">";

/// Whether a code span is a keystroke rather than a snippet.
///
/// The rule has to keep `git`, `main` and `.gitignore` out while letting `zM`,
/// `t` and `ctrl-e` in, which is why length is part of it: in this project's
/// writing a one- or two-character code span is always a key, and three
/// characters is `git` about as often as it is anything else.
fn is_key(text: &str) -> bool {
    const NAMED: [&str; 11] = [
        "esc",
        "enter",
        "tab",
        "space",
        "backspace",
        "delete",
        "up",
        "down",
        "left",
        "right",
        "menu key",
    ];
    let modified = ["ctrl-", "alt-", "shift-", "cmd-"]
        .iter()
        .any(|m| text.starts_with(m) && text.len() > m.len());
    let function = text.strip_prefix('f').is_some_and(|n| {
        !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && n.parse() != Ok(0u8)
    });
    let short = text.chars().count() <= 2 && text.chars().all(char::is_alphanumeric);
    modified || function || short || NAMED.contains(&text)
}

/// What an embed reads as: the alias if it has one, otherwise the target.
fn embed_label(embed: &str) -> Option<String> {
    scan(embed).into_iter().find_map(|p| match p.kind {
        Inline::Wiki { label, .. } | Inline::Link { label, .. } => Some(label),
        _ => None,
    })
}

/// The embed on a line that holds nothing else.
fn lone_embed(line: &str) -> Option<&str> {
    let t = line.trim();
    let looks_like = t.starts_with("![[") && t.ends_with("]]")
        || t.starts_with("![") && t.ends_with(')') && t.contains("](");
    // One embed, not two: `![[a]] ![[b]]` is a row of images and belongs in a
    // paragraph, where a reader can put a caption between them.
    (looks_like && t.matches("![").count() == 1).then_some(t)
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let level = t.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 || t.chars().nth(level) != Some(' ') {
        return None;
    }
    Some((level, t[level..].trim()))
}

fn strip_quote(line: &str) -> String {
    let t = line.trim_start();
    let rest = t.strip_prefix('>').unwrap_or(t);
    rest.strip_prefix(' ').unwrap_or(rest).to_string()
}

fn continues_list(lines: &[String], from: usize) -> bool {
    lines[from..]
        .iter()
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| split_list_marker(l).is_some() && l.starts_with(' '))
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Lowercase, slash-normalised, for matching an attachment by name or path.
pub fn normalise(s: &str) -> String {
    s.trim().trim_start_matches("./").to_lowercase()
}

/// Markup stripped, for a `<meta name="description">`.
fn plain(text: &str) -> String {
    let mut out = String::new();
    for piece in scan(text) {
        match &piece.kind {
            Inline::Text => out.push_str(&piece.raw),
            Inline::Code { inner }
            | Inline::Strong { inner }
            | Inline::Emphasis { inner }
            | Inline::Highlight { inner } => out.push_str(inner),
            Inline::Wiki { label, .. } | Inline::Link { label, .. } => out.push_str(label),
            Inline::Tag { name } => {
                out.push('#');
                out.push_str(name);
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use trafford::testing::TempDir;

    /// Render one note, in a vault that holds only it.
    fn render_lines(source: &str) -> String {
        with_vault(&[("note.md", source)], "note.md")
    }

    fn with_vault(files: &[(&str, &str)], id: &str) -> String {
        let dir = TempDir::with_files(files);
        let vault = Vault::open(dir.path()).expect("open the vault");
        let mut pages = BTreeMap::new();
        for note in &vault.notes {
            pages.insert(
                note.id.clone(),
                PageLink {
                    url: format!("docs/{}/", note::slug(note.stem())),
                    title: note.title.clone(),
                },
            );
        }
        let ctx = Ctx {
            vault: &vault,
            pages: &pages,
            depth: 0,
        };
        let note = vault.get(id).expect("the note under test");
        let mut problems = Vec::new();
        let html = render(note, &ctx, &mut problems).html;
        assert!(problems.is_empty(), "unexpected problems: {problems:?}");
        html
    }

    #[test]
    fn a_hard_wrapped_paragraph_is_one_paragraph() {
        let html = render_lines("these notes are wrapped\nat seventy-eight columns\n");
        assert_eq!(
            html.trim(),
            "<p>these notes are wrapped at seventy-eight columns</p>"
        );
    }

    #[test]
    fn a_fence_keeps_its_syntax_as_content() {
        let html = render_lines("```rust\nlet x = a < b && c > d;\n```\n");
        assert!(
            html.contains("<pre><code class=\"language-rust\">"),
            "{html}"
        );
        assert!(html.contains("a &lt; b &amp;&amp; c &gt; d"), "{html}");
        // What is inside a fence is not markup, however much it looks like it.
        assert!(!html.contains("<em>"), "{html}");
    }

    #[test]
    fn a_callout_is_an_aside_labelled_with_what_the_author_wrote() {
        let html = render_lines("> [!warning] Mind this\n> body text\n");
        assert!(html.contains("callout-warning"), "{html}");
        assert!(html.contains("Mind this"), "{html}");
        assert!(html.contains("<p>body text</p>"), "{html}");
    }

    /// A kind nobody anticipated still draws. Refusing would leave `[!quote]`
    /// sitting in the page as characters, which is the bug this replaced.
    #[test]
    fn an_unknown_callout_kind_still_draws() {
        let html = render_lines("> [!hypothesis]\n> maybe\n");
        assert!(html.contains("callout-hypothesis"), "{html}");
        assert!(html.contains("Hypothesis"), "{html}");
    }

    #[test]
    fn a_table_survives_an_escaped_pipe() {
        let html = render_lines("| a | b |\n| --- | --- |\n| `x \\| y` | z |\n");
        assert!(html.contains("<table>"), "{html}");
        assert_eq!(html.matches("<td").count(), 2, "{html}");
        assert!(html.contains("x | y"), "{html}");
    }

    #[test]
    fn nested_lists_close_in_the_right_order() {
        let html = render_lines("- one\n  - inner\n- two\n");
        assert_eq!(html.matches("<ul>").count(), 2, "{html}");
        assert_eq!(html.matches("</ul>").count(), 2, "{html}");
        assert_eq!(html.matches("<li").count(), 3, "{html}");
        assert!(html.find("inner").unwrap() < html.find("two").unwrap());
    }

    #[test]
    fn a_task_list_renders_its_boxes() {
        let html = render_lines("- [x] done\n- [ ] not\n");
        assert!(html.contains("class=\"task done\""), "{html}");
        assert!(html.contains("checkbox\" disabled checked"), "{html}");
    }

    #[test]
    fn repeated_headings_get_distinct_anchors() {
        let html = render_lines("## Notes\n\ntext\n\n## Notes\n\nmore\n");
        assert!(html.contains("id=\"notes\""), "{html}");
        assert!(html.contains("id=\"notes-2\""), "{html}");
    }

    #[test]
    fn a_heading_inside_a_fence_is_not_a_heading() {
        let html = render_lines("```sh\n# not a heading\n```\n");
        assert!(!html.contains("<h1"), "{html}");
    }

    #[test]
    fn text_is_escaped_everywhere_it_is_drawn() {
        let html = render_lines("a <script>alert(1)</script> & more\n");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn a_tag_is_drawn_with_its_hash() {
        let html = render_lines("about #focus/deep today\n");
        assert!(
            html.contains("<span class=\"tag\">#focus/deep</span>"),
            "{html}"
        );
    }

    /// The whole reason the site is generated from the vault rather than by a
    /// markdown tool: a wikilink is a link, and an alias is what it reads as.
    #[test]
    fn a_wikilink_resolves_to_the_page_it_names() {
        let html = with_vault(
            &[
                ("a.md", "see [[b|the other one]]\n"),
                ("b.md", "# B\n\n## Rules\n\ntext\n"),
            ],
            "a.md",
        );
        assert!(html.contains("href=\"docs/b/\""), "{html}");
        assert!(html.contains(">the other one</a>"), "{html}");
    }

    #[test]
    fn a_wikilink_with_a_heading_lands_on_the_section() {
        let html = with_vault(
            &[
                ("a.md", "see [[b#Rules]]\n"),
                ("b.md", "# B\n\n## Rules\n\ntext\n"),
            ],
            "a.md",
        );
        assert!(html.contains("href=\"docs/b/#rules\""), "{html}");
    }

    /// A link that goes nowhere is the failure the build exists to catch. It
    /// has to be reported with a file and a line, or fixing it is a search.
    #[test]
    fn a_link_to_nothing_is_a_problem_with_a_line_number() {
        let dir = TempDir::with_files(&[("a.md", "intro\n\nsee [[Gone]]\n")]);
        let vault = Vault::open(dir.path()).unwrap();
        let mut pages = BTreeMap::new();
        pages.insert(
            "a.md".to_string(),
            PageLink {
                url: "docs/a/".into(),
                title: "A".into(),
            },
        );
        let ctx = Ctx {
            vault: &vault,
            pages: &pages,
            depth: 0,
        };
        let mut problems = Vec::new();
        render(vault.get("a.md").unwrap(), &ctx, &mut problems);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].line, 3);
        assert!(problems[0].message.contains("Gone"), "{problems:?}");
    }

    /// An anchor that names a heading the target does not have is a link that
    /// silently lands at the top of the page — which looks like it worked.
    #[test]
    fn an_anchor_that_names_no_heading_is_a_problem() {
        let dir = TempDir::with_files(&[("a.md", "see [[b#Missing]]\n"), ("b.md", "# B\n")]);
        let vault = Vault::open(dir.path()).unwrap();
        let mut pages = BTreeMap::new();
        for note in &vault.notes {
            pages.insert(
                note.id.clone(),
                PageLink {
                    url: format!("docs/{}/", note.stem()),
                    title: note.title.clone(),
                },
            );
        }
        let ctx = Ctx {
            vault: &vault,
            pages: &pages,
            depth: 0,
        };
        let mut problems = Vec::new();
        render(vault.get("a.md").unwrap(), &ctx, &mut problems);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].message.contains("Missing"), "{problems:?}");
    }

    #[test]
    fn hrefs_are_relative_to_the_page_that_holds_them() {
        let dir = TempDir::with_files(&[("a.md", "x\n")]);
        let vault = Vault::open(dir.path()).unwrap();
        let pages = BTreeMap::new();
        let at = |depth| {
            Ctx {
                vault: &vault,
                pages: &pages,
                depth,
            }
            .href("assets/site.css")
        };
        assert_eq!(at(0), "assets/site.css");
        assert_eq!(at(2), "../../assets/site.css");
    }
}
