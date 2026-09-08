---
id: 51
title: Delete to the vault's trash, not to nowhere
type: bug
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: vault
---

## What happens

Deleting a note unlinks the file. It is gone, and only git can bring it back —
if it was committed.

The vault is set to `trashOption: "local"`, which is Obsidian's "move it to
`.trash/` inside the vault". There is a `.trash/` in this vault already, from
Obsidian doing exactly that.

## What should happen

Deleting moves the file to `.trash/`, as everything else that touches this
vault already does.

## Reproduction

1. Delete a note from the tree's context menu
2. Look in `.trash/` — it is not there
3. Look for the file — it is not anywhere

## Proposal

Move rather than unlink, honouring `trashOption`: `local` puts it in the vault's
`.trash/`, `system` hands it to the desktop trash, `none` unlinks as now.

A name already taken in `.trash/` gets a suffix rather than overwriting what is
there — the trash is the last copy, and quietly replacing one deleted note with
another is the one thing it must not do.

## Acceptance criteria

- [ ] A deleted note appears in `.trash/`
- [ ] Deleting two notes with the same name keeps both
- [ ] `.trash/` stays out of the index, as it already is
- [ ] `trashOption: "none"` still unlinks, for anyone who wants that
- [ ] Deleting still rewrites nothing else — links to it go broken, as now

## Notes

`.gitignore` in this vault already covers `.trash/`, which is why Obsidian's
deletions have never shown up in the git panel.
