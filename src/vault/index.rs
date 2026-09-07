use super::note::{relative_id, Note};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

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
    /// Lowercased ids, so a case-insensitive path match is a lookup rather
    /// than a scan of every note.
    by_id_lower: HashMap<String, usize>,
    by_stem: HashMap<String, Vec<usize>>,
    backlinks: HashMap<String, Vec<Backlink>>,
    /// Non-markdown files in the vault — images, PDFs, anything a note embeds
    /// with `![[file.jpg]]`. Keyed by lowercased filename and by lowercased
    /// relative path, since both forms appear in links.
    attachments: HashMap<String, String>,
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
        let mut attachments = HashMap::new();
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
                // Not a note, but a note may embed it.
                let rel = relative_id(&self.root, path);
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    attachments.insert(name.to_lowercase(), rel.clone());
                }
                attachments.insert(rel.to_lowercase(), rel);
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
        self.attachments = attachments;
        self.reindex();
        Ok(())
    }

    /// The vault-relative path of an embedded file, if the vault holds one.
    ///
    /// `![[diagram.png]]` is a link to something real; without this it looked
    /// like a link to a note nobody had written yet, and a vault with images
    /// showed a list of "unwritten notes" that were all pictures.
    pub fn attachment(&self, target: &str) -> Option<&str> {
        let target = target.trim().trim_start_matches("./");
        if target.is_empty() {
            return None;
        }
        self.attachments
            .get(&target.to_lowercase())
            .map(|s| s.as_str())
    }

    /// Whether a `[[target]]` points at anything in the vault at all.
    pub fn resolves(&self, target: &str) -> bool {
        self.resolve_target(target).is_some() || self.attachment(target).is_some()
    }

    /// Re-read a single note and rebuild the link indexes, without touching
    /// the rest of the vault. Saving a note is the hot path here: a full
    /// rescan re-reads every file on disk, which is a visible stall once a
    /// vault has a few thousand notes.
    ///
    /// Falls back to a full rescan when the note is not one we already know
    /// about, since that means the tree changed underneath us.
    pub fn refresh_note(&mut self, id: &str) -> Result<()> {
        let Some(index) = self.index_of(id) else {
            return self.rescan();
        };
        let path = self.notes[index].path.clone();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            // The file is gone: the vault no longer matches our picture of it.
            Err(_) => return self.rescan(),
        };
        let mut note = Note::parse(&self.root, &path, &text);
        if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
            note.modified = modified;
        }
        self.notes[index] = note;
        self.reindex();
        Ok(())
    }

    fn reindex(&mut self) {
        self.by_id.clear();
        self.by_id_lower.clear();
        self.by_stem.clear();
        self.backlinks.clear();
        self.unresolved.clear();

        for (i, note) in self.notes.iter().enumerate() {
            self.by_id.insert(note.id.clone(), i);
            self.by_id_lower.insert(note.id.to_lowercase(), i);
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
                    // An embedded image is not a note waiting to be written.
                    None if self.attachment(&link.target).is_some() => {}
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
        if let Some(&i) = self.by_id_lower.get(&with_ext.to_lowercase()) {
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

    /// The id of the note at a path, if the vault holds one there.
    ///
    /// An id is a relative path with `/` separators, so this is the inverse of
    /// `path_for` — but it answers `None` for a file the index has never seen,
    /// which is how the watcher tells "somebody edited a note" from "somebody
    /// added one".
    pub fn id_for_path(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let id = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        self.get(&id).map(|n| n.id.clone())
    }

    /// Create a new note, making parent directories as needed. Returns its id.
    pub fn create_note(&mut self, rel: &str, contents: &str) -> Result<String> {
        let path = self.resolve_new_path(rel)?;
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

    /// Turn a user-supplied name into a path inside the vault, or refuse.
    ///
    /// Names reach this from typed prompts and from `[[links]]` — including
    /// links an assistant wrote — so a name that climbs out of the vault has
    /// to be rejected rather than followed.
    fn resolve_new_path(&self, rel: &str) -> Result<PathBuf> {
        let rel = rel.trim();
        if rel.is_empty() {
            anyhow::bail!("note name is empty");
        }
        let candidate = Path::new(rel);
        if candidate.is_absolute() {
            anyhow::bail!("note names are relative to the vault: {rel}");
        }
        for part in candidate.components() {
            match part {
                Component::Normal(_) | Component::CurDir => {}
                _ => anyhow::bail!("note names cannot leave the vault: {rel}"),
            }
        }
        let rel = if rel.ends_with(".md") {
            rel.to_string()
        } else {
            format!("{rel}.md")
        };
        Ok(self.root.join(rel))
    }

    /// Every directory that holds a note, plus the root, for a "move where?"
    /// picker. Sorted, and the root first since it is the shortest answer.
    pub fn folders(&self) -> Vec<String> {
        let mut dirs: Vec<String> = self
            .notes
            .iter()
            .flat_map(|n| crate::tree::ancestors(&n.id))
            .collect();
        dirs.sort();
        dirs.dedup();
        let mut out = vec![String::new()];
        out.extend(dirs);
        out
    }

    /// Copy a note beside itself under a name nothing else has taken.
    ///
    /// The copy is a new note rather than a second home for the old one, so
    /// its incoming links are deliberately *not* redirected: the original
    /// keeps them.
    pub fn duplicate_note(&mut self, id: &str) -> Result<String> {
        let Some(note) = self.get(id) else {
            anyhow::bail!("no such note: {id}");
        };
        let text = note.text.clone();
        let stem = note.stem().to_string();
        let parent = id.rsplit_once('/').map(|(d, _)| d.to_string());
        for n in 1..1000 {
            let candidate = match n {
                1 => format!("{stem} copy"),
                _ => format!("{stem} copy {n}"),
            };
            let rel = match &parent {
                Some(dir) => format!("{dir}/{candidate}"),
                None => candidate,
            };
            if self.resolve_new_path(&rel)?.exists() {
                continue;
            }
            return self.create_note(&rel, &text);
        }
        anyhow::bail!("could not find a free name for a copy of {stem}")
    }

    /// Rename a note on disk and rewrite every `[[link]]` that pointed at it.
    /// Returns the new id and the number of files whose links were updated.
    pub fn rename_note(&mut self, id: &str, new_rel: &str) -> Result<(String, usize)> {
        let Some(note) = self.get(id) else {
            anyhow::bail!("no such note: {id}");
        };
        let old_stem = note.stem().to_string();
        let old_path = note.path.clone();
        let new_path = self.resolve_new_path(new_rel)?;
        if new_path.exists() {
            anyhow::bail!("{} already exists", relative_id(&self.root, &new_path));
        }
        if let Some(parent) = new_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&old_path, &new_path)
            .with_context(|| format!("renaming {}", old_path.display()))?;

        let new_stem = new_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(new_rel)
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

    /// A vault with images showed a list of "unwritten notes" that were all
    /// pictures, because only markdown was indexed.
    #[test]
    fn embedded_files_are_not_unwritten_notes() {
        let dir = crate::testing::TempDir::with_files(&[(
            "trip.md",
            "# Trip\n\n![[photo.jpg]] and ![[_assets/map.png]]\n",
        )]);
        std::fs::write(dir.path().join("photo.jpg"), b"x").unwrap();
        std::fs::create_dir_all(dir.path().join("_assets")).unwrap();
        std::fs::write(dir.path().join("_assets/map.png"), b"x").unwrap();
        let vault = Vault::open(dir.path()).unwrap();

        assert_eq!(vault.notes.len(), 1, "images must not be indexed as notes");
        assert!(vault.unresolved.is_empty(), "{:?}", vault.unresolved.keys());
        assert_eq!(vault.attachment("photo.jpg"), Some("photo.jpg"));
        assert_eq!(vault.attachment("_assets/map.png"), Some("_assets/map.png"));
        assert!(vault.resolves("photo.jpg"));
    }

    #[test]
    fn an_embed_of_a_missing_file_is_still_unresolved() {
        let (_d, vault) = scratch(&[("n.md", "![[nothere.jpg]]\n")]);
        assert!(vault.unresolved.contains_key("nothere.jpg"));
        assert!(!vault.resolves("nothere.jpg"));
    }

    #[test]
    fn attachments_match_by_bare_filename_case_insensitively() {
        let dir = crate::testing::TempDir::with_files(&[("n.md", "![[Photo.JPG]]\n")]);
        std::fs::create_dir_all(dir.path().join("deep/nested")).unwrap();
        std::fs::write(dir.path().join("deep/nested/Photo.JPG"), b"x").unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        assert_eq!(vault.attachment("photo.jpg"), Some("deep/nested/Photo.JPG"));
        assert!(vault.unresolved.is_empty());
    }

    #[test]
    fn notes_still_win_over_attachments_of_the_same_name() {
        let dir = crate::testing::TempDir::with_files(&[
            ("a.md", "[[thing]]\n"),
            ("thing.md", "# Thing\n"),
        ]);
        std::fs::write(dir.path().join("thing.txt"), b"x").unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        assert_eq!(vault.backlinks_for("thing.md").len(), 1);
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

    /// Names reach `create_note` from typed prompts and from `[[links]]`,
    /// including links an assistant wrote, so one that climbs out of the vault
    /// must be refused rather than followed.
    #[test]
    fn note_names_cannot_escape_the_vault() {
        let (dir, mut vault) = scratch(&[("n.md", "# n\n")]);
        let outside = dir.path().parent().unwrap().join("escaped.md");
        let _ = std::fs::remove_file(&outside);

        for name in ["../escaped", "a/../../escaped", "../../escaped.md"] {
            let err = vault.create_note(name, "# nope\n").unwrap_err();
            assert!(
                err.to_string().contains("cannot leave the vault"),
                "{name} gave {err}"
            );
        }
        assert!(!outside.exists(), "a note was written outside the vault");
        assert_eq!(vault.notes.len(), 1);
    }

    #[test]
    fn absolute_note_names_are_refused() {
        let (_d, mut vault) = scratch(&[("n.md", "# n\n")]);
        let err = vault.create_note("/tmp/absolute", "x").unwrap_err();
        assert!(err.to_string().contains("relative to the vault"), "{err}");
    }

    #[test]
    fn renaming_cannot_escape_the_vault_either() {
        let (dir, mut vault) = scratch(&[("n.md", "# n\n")]);
        let err = vault.rename_note("n.md", "../escaped").unwrap_err();
        assert!(err.to_string().contains("cannot leave the vault"), "{err}");
        // The original must survive a refused rename.
        assert!(dir.path().join("n.md").exists());
    }

    #[test]
    fn nested_names_still_work() {
        let (_d, mut vault) = scratch(&[]);
        let id = vault.create_note("a/b/deep note", "# deep\n").unwrap();
        assert_eq!(id, "a/b/deep note.md");
        assert_eq!(vault.notes.len(), 1);
    }

    #[test]
    fn a_leading_dot_slash_is_harmless() {
        let (_d, mut vault) = scratch(&[]);
        assert_eq!(vault.create_note("./here", "x").unwrap(), "here.md");
    }

    /// Moving a note is renaming it into another directory, so the tree's
    /// "Move to…" needs no vault code of its own.
    ///
    /// Links survive without being touched: they name the *stem*, and a move
    /// does not change it. Nothing is rewritten because nothing needs to be —
    /// which is worth pinning, since a passing rewrite count would otherwise
    /// look like the interesting result.
    #[test]
    fn moving_a_note_between_folders_leaves_its_incoming_links_working() {
        let (dir, mut vault) = scratch(&[
            ("inbox/thought.md", "# Thought\n"),
            ("a.md", "see [[thought]] and [[thought|it]]\n"),
        ]);
        let (new_id, touched) = vault
            .rename_note("inbox/thought.md", "archive/2026/thought")
            .unwrap();

        assert_eq!(new_id, "archive/2026/thought.md");
        assert_eq!(
            touched, 0,
            "the stem did not change, so no text needed editing"
        );
        // Directories that did not exist are created on the way.
        assert!(dir.path().join("archive/2026/thought.md").exists());
        assert!(!dir.path().join("inbox/thought.md").exists());
        let a = std::fs::read_to_string(vault.path_for("a.md")).unwrap();
        assert!(a.contains("[[thought]]"), "{a}");
        // What matters is that they still resolve, at the new location.
        assert_eq!(vault.backlinks_for("archive/2026/thought.md").len(), 2);
    }

    /// A link written as a path does have to be rewritten, since the path is
    /// exactly what a move invalidates.
    #[test]
    fn moving_a_note_rewrites_links_that_named_its_old_path() {
        let (_d, mut vault) = scratch(&[
            ("inbox/thought.md", "# Thought\n"),
            ("a.md", "see [[inbox/thought]]\n"),
        ]);
        vault
            .rename_note("inbox/thought.md", "archive/thought")
            .unwrap();
        let a = std::fs::read_to_string(vault.path_for("a.md")).unwrap();
        assert!(!a.contains("inbox/thought"), "the stale path survived: {a}");
        assert_eq!(vault.backlinks_for("archive/thought.md").len(), 1);
    }

    #[test]
    fn folders_lists_every_directory_with_the_root_first() {
        let (_d, vault) = scratch(&[("a/b/deep.md", "x"), ("c/mid.md", "x"), ("top.md", "x")]);
        assert_eq!(
            vault.folders(),
            vec!["".to_string(), "a".into(), "a/b".into(), "c".into()]
        );
    }

    #[test]
    fn duplicating_picks_a_free_name_beside_the_original() {
        let (_d, mut vault) = scratch(&[("notes/idea.md", "# Idea\n\nbody\n")]);
        let first = vault.duplicate_note("notes/idea.md").unwrap();
        assert_eq!(first, "notes/idea copy.md");
        // Same folder, same content.
        assert_eq!(vault.get(&first).unwrap().text, "# Idea\n\nbody\n");
        // A second copy must not overwrite the first.
        let second = vault.duplicate_note("notes/idea.md").unwrap();
        assert_eq!(second, "notes/idea copy 2.md");
        assert_eq!(vault.notes.len(), 3);
    }

    /// A copy is a new note, not a second home for the old one, so the links
    /// that pointed at the original keep pointing there.
    #[test]
    fn duplicating_does_not_steal_the_original_backlinks() {
        let (_d, mut vault) = scratch(&[("idea.md", "# Idea\n"), ("a.md", "[[idea]]\n")]);
        vault.duplicate_note("idea.md").unwrap();
        assert_eq!(vault.backlinks_for("idea.md").len(), 1);
        assert_eq!(vault.backlinks_for("idea copy.md").len(), 0);
    }

    #[test]
    fn duplicating_a_note_at_the_root_stays_at_the_root() {
        let (_d, mut vault) = scratch(&[("top.md", "x")]);
        assert_eq!(vault.duplicate_note("top.md").unwrap(), "top copy.md");
    }

    #[test]
    fn moving_onto_an_existing_note_is_refused() {
        let (_d, mut vault) = scratch(&[("a/n.md", "# n\n"), ("b/n.md", "# other\n")]);
        let err = vault.rename_note("a/n.md", "b/n").unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        // Both survive a refused move.
        assert!(vault.get("a/n.md").is_some());
        assert!(vault.get("b/n.md").is_some());
    }

    #[test]
    fn rewrite_leaves_unrelated_links_alone() {
        let text = "[[keep]] [[old]]";
        assert_eq!(rewrite_links(text, "old", "new"), "[[keep]] [[new]]");
    }

    #[test]
    fn refreshing_one_note_picks_up_its_new_links() {
        let (dir, mut vault) = scratch(&[("a.md", "# A\n"), ("b.md", "# B\n")]);
        assert!(vault.backlinks_for("b.md").is_empty());

        std::fs::write(dir.path().join("a.md"), "# A\n\nnow links to [[b]]\n").unwrap();
        vault.refresh_note("a.md").unwrap();

        assert_eq!(vault.backlinks_for("b.md").len(), 1);
        assert_eq!(vault.outgoing("a.md"), vec!["b.md".to_string()]);
        assert_eq!(vault.notes.len(), 2, "no note should have been added");
    }

    #[test]
    fn refreshing_one_note_drops_links_it_no_longer_has() {
        let (dir, mut vault) = scratch(&[("a.md", "# A\n\n[[b]]\n"), ("b.md", "# B\n")]);
        assert_eq!(vault.backlinks_for("b.md").len(), 1);

        std::fs::write(dir.path().join("a.md"), "# A\n\nno links now\n").unwrap();
        vault.refresh_note("a.md").unwrap();

        assert!(vault.backlinks_for("b.md").is_empty());
    }

    #[test]
    fn refreshing_updates_the_title_and_tags() {
        let (dir, mut vault) = scratch(&[("a.md", "# Old\n")]);
        assert_eq!(vault.get("a.md").unwrap().title, "Old");

        std::fs::write(dir.path().join("a.md"), "# New\n\ntagged #fresh\n").unwrap();
        vault.refresh_note("a.md").unwrap();

        let note = vault.get("a.md").unwrap();
        assert_eq!(note.title, "New");
        assert_eq!(note.tags, vec!["fresh".to_string()]);
    }

    /// The point of the fast path is that it agrees with the slow one.
    #[test]
    fn refreshing_agrees_with_a_full_rescan() {
        let files: &[(&str, &str)] = &[
            ("a.md", "# A\n\n[[b]] [[missing]]\n"),
            ("b.md", "# B\n\n[[a]]\n"),
            ("sub/c.md", "# C\n\n[[a]] #tagged\n"),
        ];
        let (dir, mut incremental) = scratch(files);
        std::fs::write(dir.path().join("a.md"), "# A2\n\n[[sub/c]] #other\n").unwrap();

        let mut full = Vault::open(dir.path()).unwrap();
        full.rescan().unwrap();
        incremental.refresh_note("a.md").unwrap();

        assert_eq!(incremental.all_tags(), full.all_tags());
        for id in ["a.md", "b.md", "sub/c.md"] {
            assert_eq!(incremental.outgoing(id), full.outgoing(id), "outgoing {id}");
            let (a, b) = (incremental.backlinks_for(id), full.backlinks_for(id));
            assert_eq!(a.len(), b.len(), "backlinks {id}");
            assert_eq!(
                incremental.get(id).unwrap().title,
                full.get(id).unwrap().title
            );
        }
        let mut mine: Vec<&String> = incremental.unresolved.keys().collect();
        let mut theirs: Vec<&String> = full.unresolved.keys().collect();
        mine.sort();
        theirs.sort();
        assert_eq!(mine, theirs);
    }

    #[test]
    fn refreshing_an_unknown_note_falls_back_to_a_full_rescan() {
        let (dir, mut vault) = scratch(&[("a.md", "# A\n")]);
        std::fs::write(dir.path().join("new.md"), "# New\n").unwrap();
        vault.refresh_note("new.md").unwrap();
        assert_eq!(vault.notes.len(), 2, "the new note should have been found");
    }

    #[test]
    fn refreshing_a_deleted_note_falls_back_to_a_full_rescan() {
        let (dir, mut vault) = scratch(&[("a.md", "# A\n"), ("b.md", "# B\n")]);
        std::fs::remove_file(dir.path().join("b.md")).unwrap();
        vault.refresh_note("b.md").unwrap();
        assert_eq!(vault.notes.len(), 1);
        assert!(vault.get("b.md").is_none());
    }

    #[test]
    fn case_insensitive_path_links_still_resolve() {
        let (_d, vault) = scratch(&[("Sub/Note.md", "# n\n"), ("a.md", "[[sub/note]]\n")]);
        assert_eq!(vault.backlinks_for("Sub/Note.md").len(), 1);
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
