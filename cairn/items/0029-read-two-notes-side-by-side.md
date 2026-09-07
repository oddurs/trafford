---
id: 29
title: Read two notes side by side
type: feature
status: backlog
milestone: later
created: 2026-09-07
updated: 2026-09-07
priority: p3
effort: l
area: chrome
---

## Problem

Reading a knowledge base is often comparative — a roadmap against the note that
tracks progress on it, a research note against the draft it feeds. Today that
means `ctrl-o` back and forth, holding one of them in your head.

## Proposal

Split the editor pane vertically, two notes, one focused. The sidebar and
context pane serve whichever has focus.

## Acceptance criteria

- [ ] Two notes open side by side, one focused, `tab` or a click moves focus
- [ ] Each half scrolls and folds independently
- [ ] The context pane follows the focused half
- [ ] Clicks land correctly in both halves
- [ ] A narrow terminal refuses the split rather than drawing two unusable columns

## Notes

Genuinely useful and genuinely invasive: `App` currently has one `Editor`, one
`layout`, one `preview_view`, and `Panes` records one editor rect. Everything
that reads `app.editor` would have to learn which one it means.

Worth doing eventually. Not worth doing before the single-pane reading view is
good, because it would double the surface every v0.5 item has to work against.
