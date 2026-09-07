---
id: 1
title: Mouse support, and a right-click menu
type: feature
status: done
milestone: v0.2
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: mouse
effort: l
---

## Problem

Everything was keyboard-only, and the keys were documented only on a help
screen you had to already suspect existed. "I don't understand how to use the
left navbar" and "I want the whole thing to be mouse usable" were the same
complaint twice.

## Proposal

Record where every pane is drawn, then resolve a click against the frame
actually on screen. Left-click focuses, opens and folds; right-click offers a
menu built from whatever is under the pointer.

## Acceptance criteria

- [x] Left-click focuses a pane, folds a folder, opens a note wherever it appears
- [x] Clicking an outline entry or a backlink jumps to it
- [x] The wheel scrolls whatever is under the pointer
- [x] Dragging in the editor selects lines
- [x] Right-click menus for the tree, the editor and the context pane
- [x] Hit-testing shares the renderer's geometry rather than duplicating it

## Notes

Shipped in #15 and #16. The menu is contextual from the start: it is built from
what was clicked, so it never offers to rename empty space.
