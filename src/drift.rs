//! What the vault says about itself that is no longer true.
//!
//! This is not the time machine an earlier plan wanted. The vault this was
//! built for has 42 commits, every one dated the same day, so there is no
//! history to travel through — and building one would have been building for a
//! vault nobody has.
//!
//! What there is instead is disagreement with the present. That same vault's
//! working tree had drifted from its single commit by 21 paths — nine
//! deletions, five renames, five files never added — and had sat that way for
//! four months with nothing saying so. Add the links that go nowhere, the notes
//! nothing points at, and the notes still claiming to be active months after
//! anyone touched them, and you have four kinds of true-but-invisible.
//!
//! Deliberately not a nag. It is a view you open and a count you can ignore. A
//! knowledge base that scolds you on startup is one you stop opening, and a
//! vault is allowed to be untidy — the failure being fixed is that the
//! untidiness is unknowable, not that it exists.

use crate::git;
use crate::vault::Vault;
use std::time::{Duration, SystemTime};

/// How long a note may claim to be active without anyone touching it before it
/// is worth mentioning. Two months: long enough that a slow project is not
/// nagged about, short enough that a forgotten one surfaces.
pub const STALE_AFTER: Duration = Duration::from_secs(60 * 60 * 24 * 60);

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    /// What opening this row should do, if anything.
    pub go: Option<Go>,
}

/// Where a row leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Go {
    /// Open the git panel, where a change can actually be committed.
    Git,
    /// Run a query, so the answer arrives in the surface that already knows how
    /// to show and open results.
    Query(String),
    /// Open a note.
    Note(String),
}

/// A section of the report: a heading, a count, and what is in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub count: usize,
    pub rows: Vec<Row>,
}

/// The whole report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub sections: Vec<Section>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|s| s.count == 0)
    }

    /// Everything worth mentioning, added up. This is the number a reader
    /// glances at; the sections are what they open when it is not zero.
    pub fn total(&self) -> usize {
        self.sections.iter().map(|s| s.count).sum()
    }
}

/// How many rows of any one section are listed before the rest are counted
/// only. A vault with 400 orphans should say 400, not print 400 lines.
const SHOWN: usize = 8;

/// Build the report.
///
/// `repo` is `None` when the vault is not a git repository, in which case the
/// git section is left out entirely rather than shown as an error — not
/// tracking a vault in git is a choice, not a drift.
pub fn report(vault: &Vault, repo: Option<&git::Snapshot>, now: SystemTime) -> Report {
    let mut sections = Vec::new();

    if let Some(snap) = repo {
        sections.push(uncommitted(snap));
    }
    sections.push(broken(vault));
    sections.push(orphans(vault));
    sections.push(stale(vault, now));
    Report { sections }
}

/// Uncommitted paths, grouped by what happened to them.
///
/// A rename is read as a rename rather than as a delete beside an add, because
/// that is what it is and the pair is how a reader mistakes a rename for lost
/// work.
fn uncommitted(snap: &git::Snapshot) -> Section {
    use git::Status::*;
    let mut rows = Vec::new();
    for (status, name) in [
        (Deleted, "deleted"),
        (Renamed, "renamed"),
        (Untracked, "never added"),
        (Modified, "modified"),
        (Added, "added"),
        (Conflicted, "conflicted"),
    ] {
        let n = snap.changes.iter().filter(|c| c.status == status).count();
        if n > 0 {
            rows.push(Row {
                label: format!("{n} {name}"),
                go: Some(Go::Git),
            });
        }
    }
    Section {
        title: format!("uncommitted on {}", snap.branch),
        count: snap.changes.len(),
        rows,
    }
}

/// Links that resolve to nothing, by target, most-referenced first.
///
/// By target rather than by note: `[[Some Idea]]` written in four places is one
/// note somebody meant to write, not four problems.
fn broken(vault: &Vault) -> Section {
    let mut targets: Vec<(&String, usize)> = vault
        .unresolved
        .iter()
        .map(|(target, refs)| (target, refs.len()))
        .collect();
    targets.sort_by_key(|(target, n)| (std::cmp::Reverse(*n), (*target).clone()));
    let count = targets.len();
    let mut rows: Vec<Row> = targets
        .iter()
        .take(SHOWN)
        .map(|(target, n)| Row {
            label: match n {
                1 => (*target).to_string(),
                n => format!("{target}  ({n} notes)"),
            },
            go: Some(Go::Note((*target).to_string())),
        })
        .collect();
    if count > rows.len() {
        rows.push(Row {
            label: format!("… and {} more", count - rows.len()),
            go: Some(Go::Query("broken".into())),
        });
    }
    Section {
        title: "links that go nowhere".into(),
        count,
        rows,
    }
}

fn orphans(vault: &Vault) -> Section {
    let ids: Vec<&str> = vault
        .notes
        .iter()
        .filter(|n| vault.backlinks_for(&n.id).is_empty())
        .map(|n| n.id.as_str())
        .collect();
    let mut rows: Vec<Row> = ids
        .iter()
        .take(SHOWN)
        .map(|id| Row {
            label: (*id).to_string(),
            go: Some(Go::Note((*id).to_string())),
        })
        .collect();
    if ids.len() > rows.len() {
        rows.push(Row {
            label: format!("… and {} more", ids.len() - rows.len()),
            go: Some(Go::Query("orphan".into())),
        });
    }
    Section {
        title: "notes nothing links to".into(),
        count: ids.len(),
        rows,
    }
}

/// Notes that say they are active and have not been touched in a while.
///
/// The claim is the note's own — `status/active` as a tag, or `status: active`
/// as a key — so this reports the vault disagreeing with itself rather than
/// with anyone's idea of how often notes should be edited.
fn stale(vault: &Vault, now: SystemTime) -> Section {
    let mut ids: Vec<(&str, Duration)> = vault
        .notes
        .iter()
        .filter(|n| {
            n.tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case("status/active"))
                || n.property_is("status", "active")
        })
        .filter_map(|n| {
            let age = now.duration_since(n.modified).ok()?;
            (age > STALE_AFTER).then_some((n.id.as_str(), age))
        })
        .collect();
    ids.sort_by_key(|(id, age)| (std::cmp::Reverse(*age), *id));
    Section {
        title: "marked active, untouched for months".into(),
        count: ids.len(),
        rows: ids
            .iter()
            .take(SHOWN)
            .map(|(id, age)| Row {
                label: format!("{id}  ({} months)", age.as_secs() / (60 * 60 * 24 * 30)),
                go: Some(Go::Note((*id).to_string())),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn vault_of(files: &[(&str, &str)]) -> (TempDir, Vault) {
        let dir = TempDir::with_files(files);
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    fn snapshot(changes: &[(git::Status, &str)]) -> git::Snapshot {
        git::Snapshot {
            branch: "main".into(),
            changes: changes
                .iter()
                .map(|(status, path)| git::Change {
                    path: (*path).to_string(),
                    status: *status,
                    staged: false,
                })
                .collect(),
            ..Default::default()
        }
    }

    /// A rename is a rename. Shown as a delete beside an add it reads as lost
    /// work, which is the specific alarm this must not raise.
    #[test]
    fn a_rename_is_reported_as_a_rename() {
        let snap = snapshot(&[
            (git::Status::Renamed, "a.md"),
            (git::Status::Deleted, "b.md"),
            (git::Status::Deleted, "c.md"),
        ]);
        let section = uncommitted(&snap);
        assert_eq!(section.count, 3);
        let labels: Vec<&str> = section.rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["2 deleted", "1 renamed"]);
    }

    /// One missing note referenced four times is one note somebody meant to
    /// write, not four problems.
    #[test]
    fn broken_links_are_counted_by_target_not_by_mention() {
        let (_d, vault) = vault_of(&[
            ("a.md", "# A\n[[Ghost]] and [[Ghost]] again\n"),
            ("b.md", "# B\n[[Ghost]]\n"),
        ]);
        let section = broken(&vault);
        assert_eq!(section.count, 1, "one target");
        assert!(section.rows[0].label.starts_with("Ghost"));
    }

    #[test]
    fn orphans_are_the_notes_nothing_points_at() {
        let (_d, vault) = vault_of(&[("a.md", "# A\nsee [[b]]\n"), ("b.md", "# B\n")]);
        let section = orphans(&vault);
        assert_eq!(section.count, 1);
        assert_eq!(section.rows[0].label, "a.md");
    }

    /// The claim is the note's own, so a note that never said it was active is
    /// not stale however old it is.
    #[test]
    fn only_a_note_that_calls_itself_active_can_go_stale() {
        let (_d, vault) = vault_of(&[
            ("live.md", "---\ntags:\n  - status/active\n---\n# Live\n"),
            ("quiet.md", "# Quiet\n"),
        ]);
        let long_after = SystemTime::now() + STALE_AFTER + Duration::from_secs(60);
        let section = stale(&vault, long_after);
        assert_eq!(section.count, 1);
        assert!(section.rows[0].label.starts_with("live.md"));

        // And nothing is stale the moment it is written.
        assert_eq!(stale(&vault, SystemTime::now()).count, 0);
    }

    /// Not being a git repository is a choice, not a drift, so the section is
    /// absent rather than an error.
    #[test]
    fn a_vault_outside_git_reports_everything_else() {
        let (_d, vault) = vault_of(&[("a.md", "# A\n[[Ghost]]\n")]);
        let r = report(&vault, None, SystemTime::now());
        assert!(!r
            .sections
            .iter()
            .any(|s| s.title.starts_with("uncommitted")));
        assert!(r
            .sections
            .iter()
            .any(|s| s.title == "links that go nowhere"));
    }

    #[test]
    fn a_tidy_vault_reports_nothing() {
        let (_d, vault) = vault_of(&[("a.md", "# A\nsee [[b]]\n"), ("b.md", "# B\nsee [[a]]\n")]);
        let r = report(&vault, Some(&snapshot(&[])), SystemTime::now());
        assert!(r.is_empty(), "{r:?}");
        assert_eq!(r.total(), 0);
    }
}
