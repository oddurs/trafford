---
id: 44
title: Switching notes throws away unsaved changes without asking
type: bug
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: s
area: editor
---

## What happens

Type into a note, do not save, open another note. The edit is gone — from the
buffer and from disk — with no prompt and no message.

Quitting with unsaved changes *does* prompt: "This note has unsaved changes.
Quit anyway?". Switching notes is the same loss with none of the protection.

## What should happen

The same guard, or better: keep the buffer.

## Reproduction

1. Open a note, type something, do not save
2. `ctrl-p`, open a different note
3. Go back — the edit is gone

## Proposal

The confirm machinery exists — `ConfirmKind::QuitDirty` — and the same shape
fits: ask before leaving a dirty buffer.

Worth considering instead: hold unsaved buffers per note, so switching away and
back returns to what was typed. That is what an editor with tabs does, and it
removes the prompt rather than adding one. Bigger, and better if it is cheap.

Ask first, since the loss is the urgent part.

## Acceptance criteria

- [ ] Leaving a note with unsaved changes asks before discarding
- [ ] The answer includes saving, not only discarding
- [ ] Every path that changes the open note is covered — the switcher, links,
      backlinks, the tree, search, the outline
- [ ] A clean buffer switches with no prompt at all

## Notes

Every jump goes through `open_note`, which is the one place to guard — the same
argument as `jump_to` for reveals.

Following a link is the common case, and a prompt on every link would be
intolerable. That is the argument for holding buffers rather than asking.
