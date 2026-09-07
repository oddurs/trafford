use super::note::{relative_id, Note};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::PathBuf;

/// A reference from one note to another, with the line it appeared on.
#[derive(Debug, Clone)]
pub struct Backlink {
    pub from: String,
    pub line: usize,
    pub context: String,
}

/// A search hit inside a note.
#[derive(Debug, Clone)]
pub struct Hit {
    pub id: String,
    pub title: String,
    pub line: usize,
    pub context: String,
    pub score: i64,
}

#[derive(Debug, Default)]
pub struct Vault {
    pub root: PathBuf,
    pub notes: Vec<Note>,
    by_id: HashMap<String, usize>,
    by_stem: HashMap<String, Vec<usize>>,
    backlinks: HashMap<String, Vec<Backlink>>,
    /// Link targets that resolve to nothing — the vault's growing edge.
    pub unresolved: HashMap<String, Vec<Backlink>>,
}

impl Vault {
    pub fn open(root: impl Into<PathBuf>) -> Result<Vault> {
        let root = root.into();
        let root = root
            .canonicalize()
            .with_context(|| format!("vault path does not exist: {}", root.display()))?;
        let mut vault = Vault {
            root,
            ..Default::default()
        };
        vault.rescan()?;
        Ok(vault)
    }

    /// Walk the vault and rebuild every index. Respects `.gitignore`.
    pub fn rescan(&mut self) -> Result<()> {
        let mut notes = Vec::new();
        let walker = WalkBuilder::new(&self.root)
            .hidden(true)
            .git_ignore(true)
            .git_global(false)
            .build();

        for entry in walker.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let mut note = Note::parse(&self.root, path, &text);
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    note.modified = modified;
                }
            }
            notes.push(note);
        }

        notes.sort_by(|a, b| a.id.cmp(&b.id));
        self.notes = notes;
        self.reindex();
        Ok(())
    }

    fn reindex(&mut self) {
        self.by_id.clear();
        self.by_stem.clear();
        self.backlinks.clear();
        self.unresolved.clear();

        for (i, note) in self.notes.iter().enumerate() {
            self.by_id.insert(note.id.clone(), i);
            self.by_stem
                .entry(note.stem().to_lowercase())
                .or_default()
                .push(i);
        }

        // Second pass: resolve every link now that all ids are known.
        let mut resolved: Vec<(String, Backlink)> = Vec::new();
        let mut missing: Vec<(String, Backlink)> = Vec::new();
        for note in &self.notes {
            for link in &note.links {
                let backlink = Backlink {
                    from: note.id.clone(),
                    line: link.line,
                    context: link.context.clone(),
                };
                match self.resolve_target(&link.target) {
                    Some(idx) => resolved.push((self.notes[idx].id.clone(), backlink)),
                    None => missing.push((link.target.clone(), backlink)),
                }
            }
        }
        for (id, bl) in resolved {
            self.backlinks.entry(id).or_default().push(bl);
        }
        for (target, bl) in missing {
            self.unresolved.entry(target).or_default().push(bl);
        }
    }

    /// Resolve a wikilink target to a note index, matching Obsidian's rules:
    /// exact relative path first, then a unique filename stem, case-insensitively.
    pub fn resolve_target(&self, target: &str) -> Option<usize> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }
        let with_ext = if target.ends_with(".md") {
            target.to_string()
        } else {
            format!("{target}.md")
        };
        if let Some(&i) = self.by_id.get(&with_ext) {
            return Some(i);
        }
        let lower = with_ext.to_lowercase();
        if let Some((_, &i)) = self.by_id.iter().find(|(id, _)| id.to_lowercase() == lower) {
            return Some(i);
        }
        let stem = target
            .rsplit('/')
            .next()
            .unwrap_or(target)
            .trim_end_matches(".md")
            .to_lowercase();
        self.by_stem.get(&stem).and_then(|v| v.first().copied())
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    pub fn get(&self, id: &str) -> Option<&Note> {
        self.index_of(id).map(|i| &self.notes[i])
    }

    pub fn backlinks_for(&self, id: &str) -> &[Backlink] {
        self.backlinks.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Notes this note links out to, deduplicated and resolved.
    pub fn outgoing(&self, id: &str) -> Vec<String> {
        let Some(note) = self.get(id) else {
            return Vec::new();
        };
        let mut out: Vec<String> = note
            .links
            .iter()
            .filter_map(|l| self.resolve_target(&l.target))
            .map(|i| self.notes[i].id.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    pub fn all_tags(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for note in &self.notes {
            for tag in &note.tags {
                *counts.entry(tag.as_str()).or_default() += 1;
            }
        }
        let mut out: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(t, c)| (t.to_string(), c))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }

    pub fn notes_with_tag(&self, tag: &str) -> Vec<&Note> {
        self.notes
            .iter()
            .filter(|n| n.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Full-text search over note bodies. Case-insensitive substring match,
    /// ranked so that title matches float above body matches.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for note in &self.notes {
            let title_match = note.title.to_lowercase().contains(&q);
            let mut found_in_body = false;
            // Match against the folded copy, but display the line as written.
            for (i, (folded, raw)) in note.haystack.lines().zip(note.text.lines()).enumerate() {
                if folded.contains(&q) {
                    found_in_body = true;
                    hits.push(Hit {
                        id: note.id.clone(),
                        title: note.title.clone(),
                        line: i,
                        context: raw.trim().to_string(),
                        score: if title_match { 1000 } else { 100 } - i as i64,
                    });
                    if hits.len() > limit * 4 {
                        break;
                    }
                }
            }
            if title_match && !found_in_body {
                hits.push(Hit {
                    id: note.id.clone(),
                    title: note.title.clone(),
                    line: 0,
                    context: String::new(),
                    score: 900,
                });
            }
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.score));
        hits.truncate(limit);
        hits
    }

    /// Rank whole notes against a natural-language question. Used to build
    /// context for the assistant: term overlap, weighted toward titles.
    pub fn relevant(&self, question: &str, limit: usize) -> Vec<&Note> {
        let terms: Vec<String> = question
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() > 2)
            .map(|t| t.to_string())
            .collect();
        if terms.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(i64, &Note)> = self
            .notes
            .iter()
            .map(|note| {
                let title = note.title.to_lowercase();
                let mut score = 0i64;
                for term in &terms {
                    if title.contains(term) {
                        score += 25;
                    }
                    if note.tags.iter().any(|t| t.to_lowercase().contains(term)) {
                        score += 10;
                    }
                    let occurrences = note.haystack.matches(term.as_str()).count() as i64;
                    score += occurrences.min(8);
                }
                (score, note)
            })
            .filter(|(s, _)| *s > 0)
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored.into_iter().take(limit).map(|(_, n)| n).collect()
    }

    pub fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Create a new note, making parent directories as needed. Returns its id.
    pub fn create_note(&mut self, rel: &str, contents: &str) -> Result<String> {
        let rel = if rel.ends_with(".md") {
            rel.to_string()
        } else {
            format!("{rel}.md")
        };
        let path = self.root.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        if !path.exists() {
            std::fs::write(&path, contents)
                .with_context(|| format!("writing {}", path.display()))?;
        }
        self.rescan()?;
        Ok(relative_id(&self.root, &path))
    }

    /// Rename a note on disk and rewrite every `[[link]]` that pointed at it.
    /// Returns the new id and the number of files whose links were updated.
    pub fn rename_note(&mut self, id: &str, new_rel: &str) -> Result<(String, usize)> {
        let Some(note) = self.get(id) else {
            anyhow::bail!("no such note: {id}");
        };
        let old_stem = note.stem().to_string();
        let old_path = note.path.clone();
        let new_rel = if new_rel.ends_with(".md") {
            new_rel.to_string()
        } else {
            format!("{new_rel}.md")
        };
        let new_path = self.root.join(&new_rel);
        if new_path.exists() {
            anyhow::bail!("{new_rel} already exists");
        }
        if let Some(parent) = new_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&old_path, &new_path)
            .with_context(|| format!("renaming {}", old_path.display()))?;

        let new_stem = new_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&new_rel)
            .to_string();

        let mut rewritten = 0usize;
        let referrers: Vec<PathBuf> = self
            .backlinks_for(id)
            .iter()
            .filter_map(|bl| self.get(&bl.from).map(|n| n.path.clone()))
            .collect();
        for path in dedup_paths(referrers) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let updated = rewrite_links(&text, &old_stem, &new_stem);
            if updated != text {
                std::fs::write(&path, updated)?;
                rewritten += 1;
            }
        }
        self.rescan()?;
        Ok((relative_id(&self.root, &new_path), rewritten))
    }
}

fn dedup_paths(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths.dedup();
    paths
}

/// Replace `[[old]]`, `[[old|alias]]` and `[[old#head]]` with the new stem,
/// leaving aliases and headings untouched.
pub fn rewrite_links(text: &str, old_stem: &str, new_stem: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == '[' {
            if let Some(close) = (i + 2..chars.len().saturating_sub(1))
                .find(|&j| chars[j] == ']' && chars[j + 1] == ']')
            {
                let inner: String = chars[i + 2..close].iter().collect();
                let (target, rest) = match inner.find(['|', '#']) {
                    Some(p) => (&inner[..p], &inner[p..]),
                    None => (inner.as_str(), ""),
                };
                let matches = target.trim().eq_ignore_ascii_case(old_stem)
                    || target
                        .trim()
                        .rsplit('/')
                        .next()
                        .map(|s| s.eq_ignore_ascii_case(old_stem))
                        .unwrap_or(false);
                if matches {
                    out.push_str("[[");
                    out.push_str(new_stem);
                    out.push_str(rest);
                    out.push_str("]]");
                } else {
                    out.push_str("[[");
                    out.push_str(&inner);
                    out.push_str("]]");
                }
                i = close + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(files: &[(&str, &str)]) -> (crate::testing::TempDir, Vault) {
        let dir = crate::testing::TempDir::with_files(files);
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    #[test]
    fn indexes_notes_and_backlinks() {
        let (_d, vault) = scratch(&[
            ("a.md", "# A\nlinks to [[b]]\n"),
            ("b.md", "# B\nnothing\n"),
        ]);
        assert_eq!(vault.notes.len(), 2);
        let back = vault.backlinks_for("b.md");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].from, "a.md");
        assert_eq!(vault.outgoing("a.md"), vec!["b.md".to_string()]);
    }

    #[test]
    fn backlink_context_keeps_its_original_case() {
        let (_d, vault) = scratch(&[("a.md", "See The Big [[B]] Note\n"), ("b.md", "# B\n")]);
        assert_eq!(
            vault.backlinks_for("b.md")[0].context,
            "See The Big [[B]] Note"
        );
    }

    #[test]
    fn resolves_links_by_stem_across_folders() {
        let (_d, vault) = scratch(&[
            ("top.md", "[[Deep Note]]\n"),
            ("sub/Deep Note.md", "# Deep\n"),
        ]);
        assert_eq!(vault.backlinks_for("sub/Deep Note.md").len(), 1);
    }

    #[test]
    fn unresolved_links_are_tracked() {
        let (_d, vault) = scratch(&[("a.md", "[[Ghost]]\n")]);
        assert!(vault.unresolved.contains_key("Ghost"));
    }

    #[test]
    fn rename_rewrites_incoming_links() {
        let (_d, mut vault) = scratch(&[
            ("a.md", "see [[b]] and [[b|alias]] and [[b#head]]\n"),
            ("b.md", "# B\n"),
        ]);
        let (new_id, touched) = vault.rename_note("b.md", "c").unwrap();
        assert_eq!(new_id, "c.md");
        assert_eq!(touched, 1);
        let a = std::fs::read_to_string(vault.path_for("a.md")).unwrap();
        assert!(a.contains("[[c]]"), "{a}");
        assert!(a.contains("[[c|alias]]"), "{a}");
        assert!(a.contains("[[c#head]]"), "{a}");
    }

    #[test]
    fn rewrite_leaves_unrelated_links_alone() {
        let text = "[[keep]] [[old]]";
        assert_eq!(rewrite_links(text, "old", "new"), "[[keep]] [[new]]");
    }

    #[test]
    fn search_ranks_title_matches_first() {
        let (_d, vault) = scratch(&[
            ("rust.md", "# rust\nnotes\n"),
            ("other.md", "mentions rust once\n"),
        ]);
        let hits = vault.search("rust", 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id, "rust.md");
    }

    #[test]
    fn search_context_keeps_its_original_case() {
        let (_d, vault) = scratch(&[("a.md", "The Quick Brown Fox\n")]);
        let hits = vault.search("quick", 10);
        assert_eq!(hits[0].context, "The Quick Brown Fox");
    }

    #[test]
    fn relevance_prefers_title_and_tag_overlap() {
        let (_d, vault) = scratch(&[
            ("gardening.md", "---\ntags: [outdoors]\n---\n# gardening\n"),
            ("misc.md", "a passing mention of gardening\n"),
        ]);
        let top = vault.relevant("how is gardening going", 2);
        assert_eq!(top[0].id, "gardening.md");
    }
}
