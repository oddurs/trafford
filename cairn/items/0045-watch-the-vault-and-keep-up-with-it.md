---
id: 45
title: Watch the vault and keep up with it
type: feature
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
depends_on:
- 43
- 44
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: l
area: vault
---

## Problem

Nothing updates. A note written by another program is invisible until the
`reindex` command is run by hand, and `reindex` has no key binding. A change to
the open note is invisible until the note is reopened. The vault is meant to be
edited by an assistant in another window, and the tool cannot see it happen.

## Proposal

Watch the vault, debounce, and apply.

A thread holds the watcher and sends batches of changed paths down a channel,
the way the assistant already streams. The event loop drains them on each tick;
nothing blocks.

**Debounced**, because editors write in bursts — a temporary file, a rename, a
touch — and a naive watcher fires four times for one save.

**Applied by cost.** Measured on the vault this is built for:

| | |
| --- | ---: |
| full rescan, 127 notes | 9.8 ms |
| one note refreshed | 92 µs |

So: when every changed path is a note already known, refresh those notes.
When anything was created, deleted or renamed, rescan once. The common case —
one note edited — costs less than a tenth of a millisecond.

**Reacting to content, not to events.** trafford's own saves make the watcher
fire, and suppressing "paths we just wrote" is a race waiting to happen: another
program may write the same file a moment later. Instead the open note is
compared with what is on disk. Identical means nothing happened, whoever wrote
it.

## What the reader sees

- A note changed elsewhere, buffer clean: it reloads, keeping the cursor line
  and the reading position.
- A note changed elsewhere, buffer dirty: nothing is touched, and they are told,
  once. #0043 handles it at save time, which is when the choice matters.
- A note created, deleted or renamed: the tree, the switcher, backlinks, tags
  and unresolved links all catch up.

## Acceptance criteria

- [x] A note edited by another program updates on screen without a keystroke
- [x] A note created by another program appears in the tree and the switcher
- [x] A deleted note disappears, and links to it go from resolved to broken
- [x] Backlinks and tags reflect an edit made elsewhere
- [x] An unsaved buffer is never replaced by a watcher event
- [x] trafford's own saves cause no visible churn
- [x] The fold state of a note that changed underneath is dropped, since it is
      keyed by line
- [x] A burst of writes — a git checkout, a bulk rename — costs one rescan
- [x] Watching failing (a platform without it, too many files) is a status
      message and not a crash; `reindex` still works

## Notes

Needs a watcher crate; `notify` is the one. It carries FSEvents on macOS and
inotify on Linux.

`.gitignore` is already respected by the walker, and the watcher has to respect
it too, or Obsidian's `.trash/` and every editor swap file wake it up.

The vault is a git repository. `git pull` and branch switches rewrite many files
at once; that is the burst case, and it is why the debounce exists.
