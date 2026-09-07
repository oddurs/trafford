---
id: 19
title: Retire the character-preserving invariant
type: docs
status: done
milestone: v0.4
depends_on:
- 15
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: docs
effort: s
---

## Problem

`CLAUDE.md` states:

> **The markdown renderer never changes characters.** `ui::markdown::Renderer`
> only applies styles, because the editor draws through it and the cursor column
> has to line up with the buffer. There is a test for this; keep it.

"Keep it" was right when it was written and stops being right the moment #0015
lands. An invariant that the code no longer holds is worse than none: it is
documentation that will be trusted and is false.

## Proposal

Replace it with the guarantee that takes over — the round-trip property from
#0013 — and say what happened, so the next person understands why the renderer
is allowed to change characters now and what protects the cursor instead.

Delete `rendering_preserves_every_character` and name its replacement in the
same commit, so the history shows the trade rather than a test quietly going
missing.

## Acceptance criteria

- [ ] The invariant is replaced, not deleted, and points at the property that
      supersedes it
- [ ] The removed test is named in the commit that removes it
- [ ] The "Obsidian compatibility" section says what is rendered and what is not

## Revised by #0012

Not a retirement — a clarification. The invariant holds on the **editor** path,
which is the path it was written to protect: the editor still draws the source,
so the cursor column is still the display width of what precedes it.

What it needs is a scope. It currently reads as though it applies to
`markdown::Renderer` everywhere, and preview will not honour it. The test stays,
narrowed to the editor's use of the renderer.

## Notes

Small, and easy to forget, which is why it is an item. Blocked on #0015 because
until that lands the invariant is still true and should stay.
