---
id: 9
title: Act on a selection from the editor's menu
type: feature
status: done
milestone: v0.3
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: mouse
effort: m
---

## Problem

Dragging selects lines, and then the menu offers the same entries it would with
no selection. The one moment the interface knows exactly what you mean is the
moment it ignores.

## Proposal

With lines selected, the editor's menu leads with what applies to them:

- **Ask the assistant about this** — already a command, but only reachable if you know it exists
- **Copy** — see #7
- **Cut** / **Delete**
- **Indent** / **Outdent**
- **Make a note from this** — the selection becomes a new note, replaced by a link to it

## Acceptance criteria

- [ ] The menu leads with selection actions when there is a selection
- [ ] Every one of them goes through the editor's existing operators, not a second path
- [ ] "Make a note from this" leaves a working `[[link]]` where the text was
- [ ] Undo restores the selection in one step

## Notes

"Make a note from this" is the interesting one — it is the gesture a vault is
for. It needs a name prompt, and it should refuse a name that already exists
rather than merging into it.
