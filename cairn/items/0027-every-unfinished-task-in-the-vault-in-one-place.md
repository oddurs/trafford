---
id: 27
title: Every unfinished task in the vault, in one place
type: feature
status: backlog
milestone: later
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: m
area: vault
---

## Problem

917 task lines across the vault, spread over notes that were each written for a
different reason, and no way to see them together. A checkbox in a project note
from March is invisible unless you happen to open that note.

## Proposal

A view that collects every `- [ ]` in the vault, grouped by note, with the
heading it sits under for context. Toggling one there writes through to the file
it came from.

This is browsing rather than reading, which is why it is not in v0.5 — but it is
the largest single body of structured data in the vault and nothing looks at it.

## Acceptance criteria

- [ ] Every unfinished task in the vault, grouped by note
- [ ] The section heading a task sits under is shown with it
- [ ] Toggling a task writes through to its file and re-indexes
- [ ] Completed tasks are reachable but not in the way
- [ ] Filterable by tag, so `#status/active` narrows it

## Notes

`space` already toggles a task on the current line, so the write-through path
exists; this needs the collection and the view.

Consider whether this is a sidebar tab (alongside Notes and Tags) or an overlay
like the git panel. The sidebar already has the tab machinery.
