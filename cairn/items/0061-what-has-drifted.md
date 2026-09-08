---
id: 61
title: What has drifted
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: git
---

## Problem

The vault's git history is 42 commits, every one of them dated 2026-05-01.
Since that day the working tree has drifted by 21 paths — **nine deletions,
five renames, five files never added, two modifications** — and has stayed that
way for four months without anything saying so.

This is the measurement that removed "time as an axis" from this milestone. An
earlier draft proposed history as a first-class dimension: what you thought in
March, a note's evolution, what changed this week. There is no history here to
build that on. What there is instead is a vault that has quietly stopped
agreeing with its own record, and no surface that mentions it.

Three other kinds of drift were counted at the same time and behave the same
way — true, knowable, and invisible:

| | |
| --- | --- |
| paths differing from the last commit | 21 |
| distinct link targets that resolve to nothing | 13 |
| notes nothing links to | 9 |
| notes marked `status/active` | 62 |

That last row is the interesting one: 62 notes claim to be active in a vault
where 32 files have been touched in the last month.

## Proposal

A view of what has drifted, and one number in the status line.

- Uncommitted paths, grouped by what happened to them — a rename read as a
  rename, not as a delete beside an add.
- Links that resolve to nothing, from `broken` (0056).
- Notes nothing links to, from `orphan` (0056).
- Notes marked active that nothing has touched in months.

**Not a nag.** It is a view you open and a count you can ignore. A knowledge
base that scolds you on startup is one you stop opening, and the vault is
allowed to be untidy — the failure being fixed is that the untidiness is
currently unknowable, not that it exists.

Committing from the view goes through the git integration that already exists.

## Acceptance criteria

- [ ] Opening it on the real vault names the nine deletions and five renames,
      and reads the renames as renames
- [ ] The counts agree with `git status` and with `cairn`-independent hand counts
- [ ] Committing from the view works through `src/git.rs` and leaves the vault's
      working tree in the state the reader chose
- [ ] Nothing about it interrupts startup or steals focus
- [ ] A vault that is not a git repository shows the link and staleness rows and
      no git row, rather than an error

## What shipped

`src/drift.rs` builds the report; `ctrl-k → drift` opens it. Four sections,
each with its full count in the heading and the first eight rows listed —
a vault with four hundred orphans should say four hundred, not print four
hundred lines. A truncated section ends in a row that runs the matching query
(`broken`, `orphan`), so the reader is handed to the surface that already knows
how to show and open results rather than being told the list is incomplete.

Enter on a git row opens the git panel, where a change can actually be
committed. Enter on a dead link offers to write that note — the same answer
following the link in the text already gives.

Measured on the real vault: **9 deleted, 5 renamed, 31 never added, 2
modified**. The deletions and renames match the hand count exactly. The
untracked figure differs from the 21 recorded above because `git status`
collapses an untracked *directory* to one line and trafford names the files;
five directories, thirty-one files. Both are true, and naming them is the point
of a drift report.

### It found three bugs in link resolution, not in itself

The report is only as honest as what it reads, and running it on a real vault
showed the reading was wrong three ways:

- **`[[#Section]]` was a broken link.** A heading-only link names a heading in
  the note it is written in; there is no target. Nine of them collapsed into
  one dead link with an empty name, referenced nine times, drawn as a blank row.
- **`[[Note\|alias]]` kept its backslash.** Obsidian writes the escaped pipe
  inside a table, where a bare one would end the cell. Left on the target the
  link resolved to nothing — the vault had one pointing at
  `../07-chameleon-research/00-overview\`. `CLAUDE.md` already warned about this
  for the *table* parser; the link parser had the same hole.
- **A template's unexpanded expression was a broken link.**
  `[[<% tp.date.now("YYYY-MM-DD", -1) %>]]` is tomorrow's note name waiting to
  be written, not one somebody forgot.

Dead targets went from 12 to 10, and all three fixes are in link resolution
where every other feature benefits from them.

### The status-line count was dropped

The item asked for "one number in the status line". The status line already
carries `●47` for uncommitted changes, which is the same number by a different
name, and a second count beside it would be two things to keep in step. The
report is reached from the palette instead.
