---
id: 6
title: Entries that cannot apply should be shown, not hidden
type: feature
status: done
milestone: v0.3
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: mouse
effort: s
---

## Problem

The menu is assembled from what applies, so an action that does not apply right
now simply is not there. That is tidy, but it means the menu's shape changes
under you, and you cannot learn what is possible from it — "Delete" being absent
looks the same as "Delete" not existing.

## Proposal

Show the entry dimmed and unselectable, with the reason as a suffix where there
is one:

```
│  Delete…            the only note │
```

Only for actions that exist but cannot run *here*. Actions that make no sense
for the thing clicked stay absent — a folder should never list "Delete note",
greyed or otherwise.

## Acceptance criteria

- [ ] `MenuItem` can be disabled, with an optional reason
- [ ] Disabled entries are skipped by the cursor and ignore clicks
- [ ] A menu of only disabled entries still opens rather than silently not opening
