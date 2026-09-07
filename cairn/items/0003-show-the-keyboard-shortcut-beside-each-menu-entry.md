---
id: 3
title: Show the keyboard shortcut beside each menu entry
type: feature
status: done
milestone: v0.2
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: mouse
effort: s
---

## Problem

The menu is where someone discovers an action exists. It is also the best place
to teach the key for it, and it currently says nothing — so the menu trains
people to keep using the menu.

## Proposal

A menu entry carries an optional shortcut, right-aligned and dimmed, the way
every desktop menu has done for thirty years:

```
╭ karpathy-llm-wiki ──────────────╮
│▌ Open                    enter  │
│  Insert a link to this  ctrl-l  │
│  Rename…                        │
│  Delete…                        │
╰─────────────────────────────────╯
```

Entries with no binding show nothing rather than an em dash.

## Acceptance criteria

- [ ] `MenuItem` carries an optional shortcut
- [ ] Shortcuts are right-aligned and dimmed, and never truncate the label
- [ ] The strings come from the same table as the help screen, so they cannot drift
- [ ] The menu still fits its widest entry on a narrow terminal
