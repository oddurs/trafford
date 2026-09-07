---
id: 2
title: Right-click every surface that has actions
type: feature
status: planned
milestone: v0.2
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: mouse
effort: m
---

## Problem

Right-click works in the tree, the editor and the context pane. Everywhere else
it does nothing, so the rule "right-click tells you what you can do here" is
only true in three places — which makes it a feature you have to remember rather
than one you can rely on.

Surfaces with actions and no menu:

| surface | actions that exist but are keyboard-only |
| --- | --- |
| git panel | stage, unstage, diff, discard, open the note |
| tags pane | filter by this tag |
| search results | open the hit, open the note |
| quick switcher | open, insert a link to it |
| assistant | insert the last answer, save it as a note, clear |
| status line | the branch: push, pull, open the git panel |

## Proposal

Extend the hit-testing already in `src/mouse.rs`. Each surface contributes a
`(title, items)` the same way the tree and editor do; nothing new is needed in
the menu itself.

The git panel is the most valuable: its actions are single letters nobody
remembers (`X` to discard), which is exactly what a menu is for.

## Acceptance criteria

- [ ] Every surface in the table above answers a right-click
- [ ] Each menu is built from the row under the pointer, not the pane
- [ ] A right-click on a surface with nothing to offer does nothing, quietly
- [ ] Menus are driven with real SGR events in a probe run, not assumed

## Notes

`Overlay::Menu` and `MenuAction` already carry everything needed; the work is
in `right_click` and in giving the overlays row-level hit-testing they lack.
