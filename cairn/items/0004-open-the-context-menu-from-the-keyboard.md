---
id: 4
title: Open the context menu from the keyboard
type: feature
status: doing
milestone: v0.2
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: mouse
effort: s
---

## Problem

The menu is the only part of the interface that needs a mouse. That is backwards
for a TUI, and it means the actions only reachable from the menu are unreachable
over ssh from a terminal without mouse reporting.

## Proposal

Bind the menu to a key that acts on the focused pane's current row — the tree's
selection, the editor's cursor line — and place it at that row rather than at
the pointer.

`m` in the sidebar is free. In the editor every letter is taken, so it wants a
modifier.

## Acceptance criteria

- [ ] A key opens the same menu the mouse would, for the focused pane's selection
- [ ] The menu is placed at the selected row, not at 0,0
- [ ] It works with mouse reporting disabled
- [ ] The binding appears in the help screen and the pane's status-line hint
