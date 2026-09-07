---
id: 10
title: Submenus, for lists that do not fit a flat menu
type: feature
status: backlog
milestone: later
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: mouse
effort: m
---

## Problem

"Move to…" (#8) wants a folder list, which can be twenty-five entries in a real
vault. A flat menu cannot hold that, and a full-screen picker is a heavier
gesture than the one that opened the menu.

## Proposal

An entry can open a child menu beside it, `l` or hover to enter, `h` or esc to
go back. The child is the same widget, placed relative to its parent and flipped
when it would leave the screen.

## Acceptance criteria

- [ ] A submenu opens beside its parent and flips rather than overflowing
- [ ] Esc closes the child and leaves the parent open
- [ ] Both are keyboard and mouse navigable, with no separate code path

## Notes

Might not be worth it. A picker overlay pre-filtered to folders reuses code that
already exists and is fuzzy-searchable, which twenty-five folders want more than
nesting. Decide before building: this item may close as dropped, and that is a
result.
