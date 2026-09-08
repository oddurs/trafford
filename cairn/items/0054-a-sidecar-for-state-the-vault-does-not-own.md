---
id: 54
title: A sidecar for state the vault does not own
type: chore
status: planned
milestone: v0.7
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: m
area: config
---

## Problem

Everything in this milestone produces state the vault did not author: a parsed
property index, similarity scores, which suggestions were declined, cached
retrieval. That state needs somewhere to live, and both obvious answers are
wrong.

It cannot go in the notes. The moment derived data is written into a `.md`
file, the note stops being what its author wrote, another program's edit
silently invalidates it, and the thing that makes this vault portable is gone.

It cannot go in `.obsidian/`. That directory belongs to Obsidian, and two
programs writing one settings file is how settings get lost — already an
invariant in this repo.

## Proposal

`.trafford/` at the vault root, holding state that is **derived, disposable and
never authoritative**. The vault's markdown remains the only source of truth;
the sidecar is a cache with opinions.

- Rebuildable from the markdown alone. Deleting it loses nothing but time.
- Stamped with a format version. A sidecar written by an older trafford is
  **discarded, not migrated** — it is derived, so rebuilding is always cheaper
  than the risk of misreading it.
- Added to `.gitignore` if the vault has one, and never added to `.obsidian/`.
- Excluded from the walker, so it never appears in the tree or the index. The
  dot-directory rule in `src/watch.rs` already skips it; this makes that
  deliberate rather than incidental.

## Acceptance criteria

- [ ] Deleting `.trafford/` while trafford is running loses no user data and the
      program keeps working
- [ ] A sidecar with an unknown version is discarded without prompting
- [ ] `.trafford/` never appears in the note tree, the index, or search results
- [ ] Nothing in the vault's markdown ever refers to a path inside it
- [ ] A full rebuild on the 148-note vault is measured and recorded here
