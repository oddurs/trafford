---
id: 25
title: Let the chrome recede while reading
type: feature
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
depends_on:
- 20
- 23
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: m
area: chrome
---

## Problem

On an eighty-column terminal with the sidebar (32) and the context pane (~30)
open, the text column is about eighteen characters wide. That is not a width
anyone reads at. Preview also keeps the line-number gutter, which is an editor
affordance — nobody reads a book with numbers down the side.

On a wide terminal the opposite happens: prose stretches to 150 columns, well
past the measure at which it stays comfortable.

## Proposal

`ctrl-e` already means "show me this rendered". Let it mean the whole posture:
the panes step aside, the gutter goes, and the prose holds at `wrap_column`,
centred, with the recovered width left as margin.

Leaving preview restores whatever was open before. One key, not a second thing
to remember.

## Acceptance criteria

- [ ] Entering preview hides the sidebar and context pane; leaving restores
      exactly what was open before
- [ ] The line-number gutter is gone in preview
- [ ] Prose holds at `wrap_column` and is centred on a wide terminal
- [ ] Toggling a pane while in preview is respected and sticks
- [ ] Clicks still land on the character under the pointer with the new geometry
- [ ] It can be turned off, for someone who wants the panes while reading

## Notes

Blocked on #0020 and #0023, both of which change the reading pane's geometry.
Doing this first means changing it twice.

Removing the gutter moves `text_x`, which `mouse.rs` uses for hit-testing. The
gutter width has to keep coming from one place — a second idea of the layout is
the bug `CLAUDE.md` warns about, and it has already cost a day here.

`wrap_column` defaults to 0, meaning the pane width. A reading view probably
wants a real measure instead. Decide it here: it is a one-line change with an
opinion attached.
