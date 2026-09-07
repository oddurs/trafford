//! The sidebar's file tree.
//!
//! A vault is a folder hierarchy, and a flat list of note titles throws that
//! away — the folders are how you know what a note *is*. This turns the note
//! ids into the rows the sidebar draws, honouring which directories are open.
//!
//! The tree is rebuilt on every draw rather than cached. It is a walk over
//! already-loaded ids, and rebuilding sidesteps a whole class of bugs where
//! the tree and the vault disagree after a rename.

use std::collections::{BTreeMap, HashSet};

/// One line in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub depth: usize,
    pub entry: Entry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Dir {
        /// Vault-relative path, which is also its key in the expanded set.
        path: String,
        name: String,
        expanded: bool,
        /// Notes at or below this directory.
        notes: usize,
    },
    Note {
        id: String,
        title: String,
    },
}

impl Row {
    pub fn note_id(&self) -> Option<&str> {
        match &self.entry {
            Entry::Note { id, .. } => Some(id),
            Entry::Dir { .. } => None,
        }
    }

    pub fn dir_path(&self) -> Option<&str> {
        match &self.entry {
            Entry::Dir { path, .. } => Some(path),
            Entry::Note { .. } => None,
        }
    }

    pub fn is_expanded_dir(&self) -> bool {
        matches!(&self.entry, Entry::Dir { expanded: true, .. })
    }
}

/// Flatten `notes` — `(id, title)` pairs — into the visible rows.
///
/// Directories come before notes at each level, both alphabetically, which is
/// the order Obsidian's file explorer uses.
pub fn build(notes: &[(String, String)], expanded: &HashSet<String>) -> Vec<Row> {
    let mut rows = Vec::new();
    level(notes, "", 0, expanded, &mut rows);
    rows
}

fn level(
    notes: &[(String, String)],
    prefix: &str,
    depth: usize,
    expanded: &HashSet<String>,
    out: &mut Vec<Row>,
) {
    // Note counts per immediate child directory, and the notes sitting here.
    let mut dirs: BTreeMap<String, usize> = BTreeMap::new();
    let mut here: Vec<&(String, String)> = Vec::new();

    for note in notes {
        let Some(rest) = strip_dir(&note.0, prefix) else {
            continue;
        };
        match rest.split_once('/') {
            Some((dir, _)) => *dirs.entry(dir.to_string()).or_default() += 1,
            None => here.push(note),
        }
    }

    for (name, count) in dirs {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let is_open = expanded.contains(&path);
        out.push(Row {
            depth,
            entry: Entry::Dir {
                name,
                expanded: is_open,
                notes: count,
                path: path.clone(),
            },
        });
        if is_open {
            level(notes, &path, depth + 1, expanded, out);
        }
    }

    // Titles are what the sidebar shows, so sort by those rather than by path.
    here.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.0.cmp(&b.0))
    });
    for (id, title) in here {
        out.push(Row {
            depth,
            entry: Entry::Note {
                id: id.clone(),
                title: title.clone(),
            },
        });
    }
}

/// The part of `id` below `prefix`, or `None` when it is not under it.
/// Guards against `01-projects` swallowing `01-projects-old/x.md`.
fn strip_dir<'a>(id: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        return Some(id);
    }
    id.strip_prefix(prefix)?.strip_prefix('/')
}

/// Every directory containing `id`, outermost first — the ones that must be
/// open for the note to be visible.
pub fn ancestors(id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    let mut parts: Vec<&str> = id.split('/').collect();
    parts.pop(); // the filename
    for part in parts {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(part);
        out.push(acc.clone());
    }
    out
}

/// The row holding the directory that contains `cursor`: the nearest
/// directory above it one level shallower. `None` at the top level, where
/// there is nothing to go out to.
pub fn parent_row(rows: &[Row], cursor: usize) -> Option<usize> {
    let row = rows.get(cursor)?;
    let target = row.depth.checked_sub(1)?;
    rows[..cursor]
        .iter()
        .rposition(|r| r.depth == target && r.dir_path().is_some())
}

/// Every directory in the vault, for expand-all.
pub fn all_dirs(notes: &[(String, String)]) -> HashSet<String> {
    notes.iter().flat_map(|(id, _)| ancestors(id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(ids: &[&str]) -> Vec<(String, String)> {
        ids.iter()
            .map(|id| {
                let title = id
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .trim_end_matches(".md")
                    .to_string();
                (id.to_string(), title)
            })
            .collect()
    }

    fn expanded(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    /// Render the rows the way the sidebar does, for readable assertions.
    fn render(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                let indent = "  ".repeat(r.depth);
                match &r.entry {
                    Entry::Dir {
                        name,
                        expanded,
                        notes,
                        ..
                    } => format!(
                        "{indent}{} {name} ({notes})",
                        if *expanded { "v" } else { ">" }
                    ),
                    Entry::Note { title, .. } => format!("{indent}{title}"),
                }
            })
            .collect()
    }

    #[test]
    fn a_collapsed_tree_shows_only_the_top_level() {
        let n = notes(&[
            "00-inbox/a.md",
            "01-projects/x/deep.md",
            "01-projects/y.md",
            "root-note.md",
        ]);
        assert_eq!(
            render(&build(&n, &expanded(&[]))),
            vec!["> 00-inbox (1)", "> 01-projects (2)", "root-note"]
        );
    }

    #[test]
    fn expanding_a_directory_reveals_its_children() {
        let n = notes(&["01-projects/x/deep.md", "01-projects/y.md"]);
        assert_eq!(
            render(&build(&n, &expanded(&["01-projects"]))),
            vec!["v 01-projects (2)", "  > x (1)", "  y"]
        );
    }

    #[test]
    fn directories_come_before_notes_at_every_level() {
        let n = notes(&["a/zzz-dir/n.md", "a/aaa-note.md"]);
        let rows = build(&n, &expanded(&["a"]));
        assert_eq!(
            render(&rows),
            vec!["v a (2)", "  > zzz-dir (1)", "  aaa-note"]
        );
    }

    #[test]
    fn counts_include_notes_in_nested_directories() {
        let n = notes(&["p/a.md", "p/q/b.md", "p/q/r/c.md"]);
        let rows = build(&n, &expanded(&[]));
        match &rows[0].entry {
            Entry::Dir { notes, .. } => assert_eq!(*notes, 3),
            other => panic!("expected a directory, got {other:?}"),
        }
    }

    #[test]
    fn a_similarly_named_sibling_is_not_swallowed() {
        // `01-projects` must not claim `01-projects-old`.
        let n = notes(&["01-projects/a.md", "01-projects-old/b.md"]);
        let rows = build(&n, &expanded(&["01-projects"]));
        assert_eq!(
            render(&rows),
            vec!["v 01-projects (1)", "  a", "> 01-projects-old (1)"]
        );
    }

    #[test]
    fn deep_expansion_nests_correctly() {
        let n = notes(&["a/b/c/deep.md"]);
        let rows = build(&n, &expanded(&["a", "a/b", "a/b/c"]));
        assert_eq!(
            render(&rows),
            vec!["v a (1)", "  v b (1)", "    v c (1)", "      deep"]
        );
    }

    #[test]
    fn notes_sort_by_title_not_by_filename() {
        let mut n = notes(&["01-zebra.md", "02-apple.md"]);
        n[0].1 = "Zebra".into();
        n[1].1 = "Apple".into();
        assert_eq!(render(&build(&n, &expanded(&[]))), vec!["Apple", "Zebra"]);
    }

    #[test]
    fn ancestors_lists_containing_directories_outermost_first() {
        assert_eq!(
            ancestors("a/b/c/note.md"),
            vec!["a".to_string(), "a/b".to_string(), "a/b/c".to_string()]
        );
        assert!(ancestors("root.md").is_empty());
    }

    #[test]
    fn all_dirs_finds_every_directory() {
        let n = notes(&["a/b/x.md", "c/y.md", "root.md"]);
        let mut dirs: Vec<String> = all_dirs(&n).into_iter().collect();
        dirs.sort();
        assert_eq!(dirs, vec!["a", "a/b", "c"]);
    }

    #[test]
    fn parent_row_finds_the_containing_directory() {
        let n = notes(&["a/b/deep.md", "a/shallow.md"]);
        let rows = build(&n, &expanded(&["a", "a/b"]));
        // rows: 0 "a", 1 "b", 2 "deep", 3 "shallow"
        assert_eq!(parent_row(&rows, 2), Some(1), "deep -> b");
        assert_eq!(parent_row(&rows, 1), Some(0), "b -> a");
        assert_eq!(parent_row(&rows, 3), Some(0), "shallow -> a");
        assert_eq!(parent_row(&rows, 0), None, "a is top level");
    }

    #[test]
    fn parent_row_skips_over_sibling_subtrees() {
        let n = notes(&["p/one/a.md", "p/two/b.md"]);
        let rows = build(&n, &expanded(&["p", "p/one", "p/two"]));
        // rows: 0 p, 1 one, 2 a, 3 two, 4 b
        assert_eq!(parent_row(&rows, 4), Some(3), "b -> two, not one");
    }

    #[test]
    fn parent_row_handles_an_out_of_range_cursor() {
        assert_eq!(parent_row(&[], 0), None);
    }

    #[test]
    fn an_empty_vault_produces_no_rows() {
        assert!(build(&[], &expanded(&[])).is_empty());
    }
}
