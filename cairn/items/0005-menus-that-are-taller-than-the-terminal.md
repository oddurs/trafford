---
id: 5
title: Menus that are taller than the terminal
type: bug
status: doing
milestone: v0.2
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: mouse
effort: s
---

## What happens

A menu is drawn at its full height. Once it has more entries than the terminal
has rows, the ones past the bottom are simply not drawn, with nothing to say so.

## What should happen

The menu scrolls, keeping the selection visible, and shows that there is more —
the way every other list here already does via `scroll_offset`.

## Reproduction

1. Grow a menu past the terminal height, or shrink the terminal under one.
2. Move the selection down past the last visible row.
3. The selection leaves the screen and the menu appears frozen.

## Notes

`draw_menu` clamps its height to the area but does not scroll within it. The
fix is `scroll_offset`, which the picker and the tree already use — the hazard
is that hit-testing must use the same offset or clicks will land on the wrong
entry.
