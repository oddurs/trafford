use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

impl Status {
    pub fn glyph(&self) -> &'static str {
        match self {
            Status::Added => "+",
            Status::Modified => "~",
            Status::Deleted => "-",
            Status::Renamed => "»",
            Status::Untracked => "?",
            Status::Conflicted => "!",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Change {
    pub path: String,
    pub status: Status,
    pub staged: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub branch: String,
    pub changes: Vec<Change>,
    pub ahead: usize,
    pub behind: usize,
    pub has_remote: bool,
}

impl Snapshot {
    pub fn is_clean(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn staged_count(&self) -> usize {
        self.changes.iter().filter(|c| c.staged).count()
    }
}

#[derive(Debug, Clone)]
pub struct Commit {
    pub short: String,
    pub subject: String,
    pub author: String,
    pub when: String,
}

/// A git working copy rooted at the vault.
#[derive(Debug, Clone)]
pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    /// Returns `None` when the vault is not inside a git repository.
    pub fn discover(vault_root: &Path) -> Option<Repo> {
        let out = Command::new("git")
            .arg("-C")
            .arg(vault_root)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if root.is_empty() {
            return None;
        }
        Some(Repo {
            root: PathBuf::from(root),
        })
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .with_context(|| format!("running git {}", args.join(" ")))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            bail!(if stderr.is_empty() {
                format!("git {} failed", args.join(" "))
            } else {
                stderr
            });
        }
        Ok(stdout)
    }

    /// Initialise a repository in `path` with a vault-appropriate .gitignore.
    pub fn init(path: &Path) -> Result<Repo> {
        let out = Command::new("git").arg("init").arg(path).output()?;
        if !out.status.success() {
            bail!(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(Repo {
            root: path.to_path_buf(),
        })
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        // `branch --show-current` reports the branch even before the first
        // commit, where `rev-parse HEAD` would fail outright.
        let branch = self
            .git(&["branch", "--show-current"])
            .map(|s| s.trim().to_string())
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "HEAD".into());

        let porcelain = self.git(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
        let changes = parse_porcelain(&porcelain);

        let has_remote = !self.git(&["remote"]).unwrap_or_default().trim().is_empty();

        let (ahead, behind) =
            match self.git(&["rev-list", "--left-right", "--count", "@{upstream}...HEAD"]) {
                Ok(counts) => parse_ahead_behind(&counts),
                Err(_) => (0, 0),
            };

        Ok(Snapshot {
            branch,
            changes,
            ahead,
            behind,
            has_remote,
        })
    }

    pub fn stage(&self, path: &str) -> Result<()> {
        self.git(&["add", "--", path]).map(|_| ())
    }

    pub fn stage_all(&self) -> Result<()> {
        self.git(&["add", "--all"]).map(|_| ())
    }

    pub fn unstage(&self, path: &str) -> Result<()> {
        self.git(&["restore", "--staged", "--", path]).map(|_| ())
    }

    /// Discard working-tree changes to a file. Destructive, so the caller
    /// confirms first.
    pub fn discard(&self, path: &str) -> Result<()> {
        self.git(&["checkout", "--", path]).map(|_| ())
    }

    pub fn commit(&self, message: &str) -> Result<String> {
        if message.trim().is_empty() {
            bail!("commit message is empty");
        }
        let out = self.git(&["commit", "-m", message])?;
        let subject = out.lines().next().unwrap_or("committed").trim().to_string();
        Ok(subject)
    }

    pub fn push(&self) -> Result<String> {
        let branch = self.git(&["branch", "--show-current"])?;
        let branch = branch.trim();
        if branch.is_empty() {
            bail!("cannot push a detached HEAD");
        }
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["push", "--set-upstream", "origin", branch])
            .output()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !out.status.success() {
            bail!(combined.trim().to_string());
        }
        Ok(summarise(&combined, "pushed"))
    }

    pub fn pull(&self) -> Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["pull", "--rebase", "--autostash"])
            .output()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !out.status.success() {
            bail!(combined.trim().to_string());
        }
        Ok(summarise(&combined, "pulled"))
    }

    pub fn log(&self, limit: usize) -> Result<Vec<Commit>> {
        let fmt = "--pretty=format:%h%x1f%s%x1f%an%x1f%cr";
        let out = self.git(&["log", &format!("-{limit}"), fmt])?;
        Ok(out.lines().filter_map(parse_commit_line).collect())
    }

    /// History for a single note, so a note's own revisions are one keypress away.
    pub fn log_for(&self, rel_path: &str, limit: usize) -> Result<Vec<Commit>> {
        let fmt = "--pretty=format:%h%x1f%s%x1f%an%x1f%cr";
        let out = self.git(&["log", &format!("-{limit}"), fmt, "--", rel_path])?;
        Ok(out.lines().filter_map(parse_commit_line).collect())
    }

    /// Unified diff for a path, or the whole worktree when `path` is `None`.
    pub fn diff(&self, path: Option<&str>) -> Result<String> {
        let mut args = vec!["diff", "--no-color", "HEAD"];
        if let Some(p) = path {
            args.push("--");
            args.push(p);
        }
        self.git(&args)
    }

    /// Path of `file` relative to the repository root, as git expects it.
    pub fn rel(&self, file: &Path) -> String {
        file.strip_prefix(&self.root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

fn summarise(output: &str, fallback: &str) -> String {
    output
        .lines()
        .map(|l| l.trim())
        .rfind(|l| !l.is_empty())
        .map(|l| l.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn parse_commit_line(line: &str) -> Option<Commit> {
    let mut parts = line.split('\u{1f}');
    Some(Commit {
        short: parts.next()?.to_string(),
        subject: parts.next()?.to_string(),
        author: parts.next()?.to_string(),
        when: parts.next().unwrap_or("").to_string(),
    })
}

fn parse_ahead_behind(counts: &str) -> (usize, usize) {
    let mut parts = counts.split_whitespace();
    let behind = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let ahead = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

/// Parse `git status --porcelain=v1 -z`. Entries are NUL separated; a rename
/// entry is followed by a second NUL-terminated field holding the old path.
pub fn parse_porcelain(input: &str) -> Vec<Change> {
    let mut out = Vec::new();
    let mut fields = input.split('\0').filter(|s| !s.is_empty()).peekable();
    while let Some(entry) = fields.next() {
        if entry.len() < 3 {
            continue;
        }
        let bytes: Vec<char> = entry.chars().collect();
        let (x, y) = (bytes[0], bytes[1]);
        let path = entry[3..].to_string();
        // A rename consumes the following field (the original path).
        if x == 'R' || y == 'R' {
            fields.next();
        }
        if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
            out.push(Change {
                path,
                status: Status::Conflicted,
                staged: false,
            });
            continue;
        }
        if x == '?' && y == '?' {
            out.push(Change {
                path,
                status: Status::Untracked,
                staged: false,
            });
            continue;
        }
        if x != ' ' && x != '?' {
            out.push(Change {
                path: path.clone(),
                status: code_to_status(x),
                staged: true,
            });
        }
        if y != ' ' && y != '?' {
            out.push(Change {
                path,
                status: code_to_status(y),
                staged: false,
            });
        }
    }
    out
}

fn code_to_status(c: char) -> Status {
    match c {
        'A' => Status::Added,
        'D' => Status::Deleted,
        'R' => Status::Renamed,
        _ => Status::Modified,
    }
}

/// A commit message for an unattended vault snapshot, summarising what moved.
pub fn autocommit_message(changes: &[Change]) -> String {
    let mut paths: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
    paths.sort();
    paths.dedup();
    match paths.len() {
        0 => "vault: no changes".to_string(),
        1 => format!("vault: update {}", trim_ext(paths[0])),
        2..=3 => format!(
            "vault: update {}",
            paths
                .iter()
                .map(|p| trim_ext(p))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        n => format!("vault: update {} notes", n),
    }
}

fn trim_ext(path: &str) -> String {
    path.trim_end_matches(".md").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_splits_staged_and_unstaged_halves() {
        let input = "MM notes/a.md\0 M notes/b.md\0A  notes/c.md\0";
        let changes = parse_porcelain(input);
        assert_eq!(changes.len(), 4);
        assert!(changes[0].staged && changes[0].path == "notes/a.md");
        assert!(!changes[1].staged && changes[1].path == "notes/a.md");
        assert!(!changes[2].staged && changes[2].path == "notes/b.md");
        assert!(changes[3].staged && changes[3].status == Status::Added);
    }

    #[test]
    fn untracked_files_are_reported_once() {
        let changes = parse_porcelain("?? new.md\0");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].status, Status::Untracked);
        assert!(!changes[0].staged);
    }

    #[test]
    fn renames_consume_their_trailing_old_path_field() {
        let input = "R  new.md\0old.md\0 M other.md\0";
        let changes = parse_porcelain(input);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].path, "new.md");
        assert_eq!(changes[0].status, Status::Renamed);
        assert_eq!(changes[1].path, "other.md");
    }

    #[test]
    fn conflicts_are_flagged() {
        let changes = parse_porcelain("UU merged.md\0");
        assert_eq!(changes[0].status, Status::Conflicted);
    }

    #[test]
    fn ahead_behind_reads_left_right_counts() {
        assert_eq!(parse_ahead_behind("2\t5"), (5, 2));
        assert_eq!(parse_ahead_behind(""), (0, 0));
    }

    #[test]
    fn autocommit_messages_scale_with_change_count() {
        let mk = |p: &str| Change {
            path: p.into(),
            status: Status::Modified,
            staged: false,
        };
        assert_eq!(autocommit_message(&[mk("a.md")]), "vault: update a");
        assert_eq!(
            autocommit_message(&[mk("a.md"), mk("b.md")]),
            "vault: update a, b"
        );
        let many: Vec<Change> = (0..7).map(|i| mk(&format!("n{i}.md"))).collect();
        assert_eq!(autocommit_message(&many), "vault: update 7 notes");
    }

    #[test]
    fn duplicate_paths_from_both_halves_count_once() {
        let changes = parse_porcelain("MM a.md\0");
        assert_eq!(autocommit_message(&changes), "vault: update a");
    }

    #[test]
    fn commit_lines_parse_into_records() {
        let line = "abc\u{1f}subject here\u{1f}Ada\u{1f}2 hours ago";
        let c = parse_commit_line(line).unwrap();
        assert_eq!(c.short, "abc");
        assert_eq!(c.subject, "subject here");
        assert_eq!(c.author, "Ada");
    }
}
