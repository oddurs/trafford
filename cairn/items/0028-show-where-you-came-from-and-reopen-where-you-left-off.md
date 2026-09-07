---
id: 28
title: Show where you came from, and reopen where you left off
type: feature
status: backlog
milestone: later
created: 2026-09-07
updated: 2026-09-07
priority: p3
effort: s
area: chrome
---

## Problem

`ctrl-o` goes back, which is most of the value, but nothing shows the trail: you
cannot see how you got somewhere or how far back the way out is. And closing
trafford forgets everything, so every session starts from nowhere.

## Proposal

Two small things that pair:

- A breadcrumb of the notes you came through, in the status line, clickable.
- Reopen the last note and cursor position on start, unless a path was given.

## Acceptance criteria

- [ ] The trail shows the last few notes, most recent nearest
- [ ] Clicking one goes back to it, keeping the rest of the trail
- [ ] Starting with no argument reopens the last note at the last position
- [ ] Starting with a path ignores the saved position, as it should
- [ ] The saved state lives outside the vault's own git history

## Notes

The back-stack already carries `(note id, cursor row)` for `ctrl-o` — the
breadcrumb is a view of a thing that exists.

Session state must not land in the vault as a tracked file. `.trafford/` is
already in the vault and versioned deliberately, so this needs a different home
or a gitignored file inside it.
