//! Collapsing a note's sections in the reading view.
//!
//! The vault this was built against has a heading every six lines of body,
//! twelve per note at the median and sixty-seven at the top end. Nobody reads a
//! note like that from the top; they arrive looking for one section. Folded, a
//! note is a table of contents you can open in place, which is how reference
//! material is actually read.
//!
//! There is one heading scanner in this program and it is here. The outline in
//! the context pane reads it too — a second idea of the document's structure is
//! the same class of bug as a second idea of the layout, and `CLAUDE.md` says
//! why that costs a day.

use std::collections::HashSet;

/// A heading, and where it sits in the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub text: String,
    pub level: usize,
    pub row: usize,
}

/// Every heading in `lines`, skipping anything inside a fenced code block —
/// a `# comment` in a shell sample is not a section.
pub fn headings(lines: &[String]) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut in_code = false;
    for (row, raw) in lines.iter().enumerate() {
        if super::markdown::is_fence(raw) {
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
        out.push(Heading {
            text: trimmed[level..].trim().to_string(),
            level,
            row,
        });
    }
    out
}

/// The line one past the end of the section a heading opens.
///
/// A section runs to the next heading of the same or shallower level, so
/// folding an H2 takes its H3s with it — which is what "fold this section"
/// means to anyone who has used an outliner.
pub fn section_end(heads: &[Heading], at: usize, total: usize) -> usize {
    let Some(here) = heads.iter().position(|h| h.row == at) else {
        return at + 1;
    };
    let level = heads[here].level;
    heads[here + 1..]
        .iter()
        .find(|h| h.level <= level)
        .map(|h| h.row)
        .unwrap_or(total)
}

/// The headings containing `row`, outermost first.
///
/// This is the answer to "which section am I in", which with a heading every
/// six lines of body is a question the reader is otherwise always half-asking.
pub fn chain(heads: &[Heading], row: usize, total: usize) -> Vec<&Heading> {
    let mut out: Vec<&Heading> = Vec::new();
    for h in heads.iter().filter(|h| h.row <= row) {
        if section_end(heads, h.row, total) <= row {
            continue;
        }
        // A heading replaces anything at its level or deeper: `## Two` ends
        // `## One` and everything that was under it.
        out.retain(|k| k.level < h.level);
        out.push(h);
    }
    out
}

/// Which headings are collapsed, per note.
///
/// Keyed by the line the heading sits on. That moves if the note is edited
/// above it, which is a real limitation and the same one Obsidian has; a note
/// is not usually being edited and read at once, and the alternative — keying
/// by heading text — breaks on the two notes here that repeat a heading.
#[derive(Debug, Default, Clone)]
pub struct Folds {
    per_note: std::collections::HashMap<String, HashSet<usize>>,
}

impl Folds {
    pub fn of(&self, note: &str) -> Option<&HashSet<usize>> {
        self.per_note.get(note)
    }

    pub fn is_folded(&self, note: &str, row: usize) -> bool {
        self.per_note.get(note).is_some_and(|s| s.contains(&row))
    }

    pub fn toggle(&mut self, note: &str, row: usize) -> bool {
        let set = self.per_note.entry(note.to_string()).or_default();
        if set.remove(&row) {
            false
        } else {
            set.insert(row);
            true
        }
    }

    /// Fold every heading that has something under it.
    ///
    /// Two exceptions, both found by folding a real note. A heading with an
    /// empty section is left alone: collapsing it would hide nothing and the
    /// marker would lie about there being more. And a heading whose section
    /// contains every other heading is left alone too — a note titled with a
    /// lone `#` is the common case, and folding it collapses the whole note to
    /// one line, which tells you only what the title bar already said.
    pub fn fold_all(&mut self, note: &str, heads: &[Heading], total: usize) {
        let set = self.per_note.entry(note.to_string()).or_default();
        set.clear();
        for h in heads {
            let end = section_end(heads, h.row, total);
            if end <= h.row + 1 {
                continue;
            }
            // "Every *other* heading" needs there to be others: a note with a
            // single section has nothing to hide and should fold normally.
            let others = heads.iter().filter(|o| o.row != h.row).count();
            let swallows_the_note = others > 0
                && heads
                    .iter()
                    .all(|o| o.row == h.row || (o.row > h.row && o.row < end));
            if swallows_the_note {
                continue;
            }
            set.insert(h.row);
        }
    }

    pub fn unfold_all(&mut self, note: &str) {
        self.per_note.remove(note);
    }

    /// Forget a note's folds, for when it is renamed or reloaded from disk and
    /// the line numbers no longer mean what they meant.
    pub fn forget(&mut self, note: &str) {
        self.per_note.remove(note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    const NOTE: &str = "\
# Title
intro
## One
a
b
### One A
c
## Two
d
";

    #[test]
    fn headings_skip_anything_inside_a_fence() {
        let src = lines("# Real\n```sh\n# not a heading\n```\n## Also real");
        let h = headings(&src);
        assert_eq!(
            h.iter().map(|h| h.text.as_str()).collect::<Vec<_>>(),
            ["Real", "Also real"]
        );
    }

    #[test]
    fn a_hash_without_a_space_is_not_a_heading() {
        let src = lines("#tag/reference\n#### deep\n####### too deep");
        let h = headings(&src);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].level, 4);
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_the_same_or_shallower_level() {
        let src = lines(NOTE);
        let h = headings(&src);
        let total = src.len();
        // "## One" (row 2) ends where "## Two" (row 7) begins — its H3 comes
        // with it.
        assert_eq!(section_end(&h, 2, total), 7);
        // "### One A" (row 5) ends at the next H2.
        assert_eq!(section_end(&h, 5, total), 7);
        // "# Title" swallows the lot.
        assert_eq!(section_end(&h, 0, total), total);
        // The last section runs to the end.
        assert_eq!(section_end(&h, 7, total), total);
    }

    #[test]
    fn a_row_that_is_not_a_heading_has_no_section() {
        let src = lines(NOTE);
        let h = headings(&src);
        assert_eq!(section_end(&h, 3, src.len()), 4);
    }

    #[test]
    fn the_chain_names_every_section_a_line_is_inside() {
        let src = lines(NOTE);
        let h = headings(&src);
        let total = src.len();
        let names = |row| {
            chain(&h, row, total)
                .iter()
                .map(|h| h.text.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(1), ["Title"], "the intro is only under the title");
        assert_eq!(names(4), ["Title", "One"]);
        assert_eq!(names(6), ["Title", "One", "One A"], "deepest last");
        assert_eq!(names(8), ["Title", "Two"], "One and its child are done");
    }

    #[test]
    fn a_heading_is_in_its_own_chain() {
        let src = lines(NOTE);
        let h = headings(&src);
        let names: Vec<&str> = chain(&h, 5, src.len())
            .iter()
            .map(|h| h.text.as_str())
            .collect();
        assert_eq!(names, ["Title", "One", "One A"]);
    }

    #[test]
    fn a_line_above_every_heading_has_no_chain() {
        let src = lines("intro\n# Title\nbody");
        let h = headings(&src);
        assert!(chain(&h, 0, src.len()).is_empty());
    }

    #[test]
    fn folds_are_per_note() {
        let mut f = Folds::default();
        assert!(f.toggle("a.md", 2));
        assert!(f.is_folded("a.md", 2));
        assert!(!f.is_folded("b.md", 2), "another note is not folded by it");
        assert!(!f.toggle("a.md", 2), "toggling again opens it");
        assert!(!f.is_folded("a.md", 2));
    }

    #[test]
    fn folding_everything_skips_headings_with_nothing_under_them() {
        let src = lines("# Title\n## Empty\n## Has one\nx");
        let h = headings(&src);
        let mut f = Folds::default();
        f.fold_all("n.md", &h, src.len());
        assert!(
            !f.is_folded("n.md", 1),
            "## Empty hides nothing, so a marker saying it does would lie"
        );
        assert!(f.is_folded("n.md", 2));
    }

    #[test]
    fn folding_everything_leaves_a_lone_title_open() {
        // Every note in the vault this was built for opens with one `#`, and
        // folding that collapses the note to a line saying what the title bar
        // already says.
        let src = lines(NOTE);
        let h = headings(&src);
        let mut f = Folds::default();
        f.fold_all("n.md", &h, src.len());
        assert!(!f.is_folded("n.md", 0), "# Title swallows the whole note");
        assert!(f.is_folded("n.md", 2), "## One is a section worth folding");
        assert!(f.is_folded("n.md", 7), "and so is ## Two");
    }

    #[test]
    fn a_note_with_only_one_section_still_folds_it() {
        // The rule is "contains every *other* heading", so a note with a single
        // heading has nothing to compete with and folds normally.
        let src = lines("## Only\na\nb");
        let h = headings(&src);
        let mut f = Folds::default();
        f.fold_all("n.md", &h, src.len());
        assert!(f.is_folded("n.md", 0));
    }

    #[test]
    fn unfolding_everything_clears_only_that_note() {
        let mut f = Folds::default();
        f.toggle("a.md", 1);
        f.toggle("b.md", 1);
        f.unfold_all("a.md");
        assert!(!f.is_folded("a.md", 1));
        assert!(f.is_folded("b.md", 1), "only that note was cleared");
    }
}
