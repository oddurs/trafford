---
id: 61
title: What has drifted
type: feature
status: planned
milestone: v0.7
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
| distinct link targets that resolve to nothing | ~58 |
| notes nothing links to | 28 |
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
