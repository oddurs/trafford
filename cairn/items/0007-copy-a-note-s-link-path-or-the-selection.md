---
id: 7
title: Copy a note's link, path, or the selection
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

There is no way to get anything out of trafford and into anything else. Writing
`[[Some Note]]` in another editor means retyping the title exactly, which is
what a link is supposed to spare you.

## Proposal

Menu entries that put text on the system clipboard:

- **Copy a link to this** — `[[Note title]]`, the form you would paste into another note
- **Copy the path** — vault-relative, for a terminal or a script
- **Copy the selection** — in the editor, when lines are selected

## Acceptance criteria

- [ ] Copying works on macOS, and degrades to a clear message where it cannot
- [ ] OSC 52 is used so it also works over ssh, where there is no local pasteboard
- [ ] The status line confirms what was copied
- [ ] Failure to reach a clipboard is reported, not swallowed

## Notes

OSC 52 is the interesting half: it makes copy work through a terminal that is
not on this machine, which is where a TUI beats a GUI. Terminals that refuse it
must be reported rather than silently doing nothing.
