---
id: 40
title: Never land the cursor on a line that is folded away
type: bug
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: chrome
---

## What happens

Searching for text that lives inside a collapsed section moves the cursor to
that line and leaves it there, folded away. The section crumb names the right
heading, but the match is not on screen and nothing says why.

Following a `[[link#Heading]]` into a folded section does the same.

## What should happen

Landing somewhere reveals it. A destination you cannot see is not a
destination.

## Reproduction

1. Open a long note, `ctrl-e`, `zM`
2. `ctrl-f`, search for a word inside a folded section, enter
3. The cursor reports the right line; the screen does not show it

## Proposal

Anything that moves the cursor to a specific line opens the folds containing it
first — search, backlinks, the outline, a heading link, `NG`. That is one
helper, `reveal(row)`, called from the places that jump.

Folds the reader closed by hand stay closed everywhere else; this only opens
the ones standing between them and where they asked to go.

## Acceptance criteria

- [ ] A search hit inside a fold opens the folds above it and shows the line
- [ ] Clicking a heading in the outline reveals it
- [ ] Following a link to a heading reveals it
- [ ] `42G` reveals line 42
- [ ] Folds not in the way are left alone
