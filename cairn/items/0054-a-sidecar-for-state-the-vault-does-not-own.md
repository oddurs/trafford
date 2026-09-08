---
id: 54
title: A sidecar for state the vault does not own
type: chore
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
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

## What shipped, and the correction the vault made

`src/sidecar.rs`. Derived state is stored as JSON in an envelope carrying the
format it was written with; a file from another format, a corrupt file and an
absent file all read as `None`, because they call for the same handling and
telling them apart only invites treating one as an error worth showing. Writes
go to a temporary name and are renamed, so a reader never sees half a file.

**It is `.trafford/cache/`, not `.trafford/`.** The item as filed said the
latter, and building it that way would have done real damage: `.trafford/`
already exists in the vault holding `config.toml`, which says of itself "vault
config. Versioned with the notes." Writing a `.gitignore` containing `*` at the
top of that directory would have untracked the reader's configuration — the
opposite of what they set up.

The convention was already there and I had not looked: `cargo run -- init`
scaffolds a `.gitignore` naming `.trafford/cache/`. Found by running the thing
against a real vault, where `ensure()` returned early because the directory
already existed and never wrote its ignore file at all.

The ignore now lives *inside* `cache/`, so the cache ignores itself and nothing
touches either the reader's `.gitignore` or their config.

### Its first consumer

Shipped with recent queries rather than alone, because state nothing reads is
dead code — the same reason 0055 shipped with 0056. A query the reader got an
answer out of is remembered on the way out and offered back in the empty search
box next session. Only on opening a hit, and only when it found something: a
query is half-typed on every keystroke, and remembering those would fill the
list with prefixes of itself.

Verified end to end on a copy of the real vault: `type:reference status:active`
was run, the session quit, and a new one offered it back above the syntax
examples, with `config.toml` still tracked beside an ignored `cache/`.
