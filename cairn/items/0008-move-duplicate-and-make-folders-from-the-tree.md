---
id: 8
title: Move, duplicate, and make folders from the tree
type: feature
status: backlog
milestone: v0.3
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: tree
effort: l
---

## Problem

The tree shows the vault's structure but cannot change it. Reorganising means
leaving for a shell, and moving a note there breaks every `[[link]]` pointing at
it unless you also rewrite them by hand.

Renaming already rewrites incoming links. Moving is the same operation with a
different target and does not exist.

## Proposal

From a right-click in the tree:

- **Move to…** — a folder picker; incoming links are rewritten, as with rename
- **Duplicate** — copy beside the original, with a name that does not collide
- **New folder…** — on a folder, or at the root

## Acceptance criteria

- [ ] Moving a note rewrites `[[links]]` that pointed at it, as rename does
- [ ] Moving refuses a target outside the vault, like `resolve_new_path` does
- [ ] Duplicating never overwrites; it picks the next free name
- [ ] A folder with no notes in it survives a rescan, or the empty folder is refused honestly
- [ ] The tree reveals the note at its new home afterwards

## Notes

`rename_note` already does the hard part — the link rewriting and the path
guard. Moving should call it rather than growing a second implementation.

The empty-folder question needs deciding before the work: the vault is indexed
from notes, so a folder with nothing in it has nowhere to live. Either the
folder is created on disk and tolerated as invisible until it holds something,
or "New folder…" is really "New note in a new folder…".
