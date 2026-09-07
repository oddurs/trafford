---
id: 20
title: Fold a note's sections in the reading view
type: feature
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: m
area: markdown
---

## Problem

The vault has a heading every six lines of body, twelve per note at the median
and sixty-seven at the top end, nested three deep. Nobody reads a note like
that from the top; they arrive looking for one section.

Preview shows all 1,254 lines of the longest note in order, so finding a
section means scrolling past every section before it. The context pane's
outline is the only structural view, and it truncates at "… 43 more" — on a
sixty-seven-heading note that is most of the document.

## Proposal

Collapse a section to its heading, with a count of what is inside:

```
  ▾ Assumptions & Constraints
      ┌────────────────┬──────────────────────────────┐
      │ Weekly hours   │ 10-15 (evenings + weekends)  │
      └────────────────┴──────────────────────────────┘

  ▸ Weekly Schedule Template                        7 lines
  ▸ Phase 1: Foundations (Months 1–3)             184 lines
  ▸ Phase 2: Deep Learning Core                   212 lines
```

A folded note is a table of contents you can open in place, which is how
reference material is read.

Keys follow vim, since the editor already does: `za` toggles, `zR` opens
everything, `zM` closes everything. The arrow is clickable, because everything
here is.

A section runs from its heading to the next heading of the same or shallower
level. Folding an H2 takes its H3s with it.

## Acceptance criteria

- [ ] A section folds to its heading, showing how many lines are hidden
- [ ] Folding an outer heading hides the inner ones with it
- [ ] `za`, `zR`, `zM` work, and clicking the marker does the same thing
- [ ] Fold state survives scrolling and switching to the editor and back
- [ ] A click below a fold lands on the note line it looks like it lands on
- [ ] The outline in the context pane agrees with what is folded

## Notes

Preview already draws a different number of lines than the note has, because
tables do, and `PreviewView.sources` already maps each drawn line back. Folding
is the same move: emit fewer lines, keep the map honest. That is the whole
reason this is `m` and not `l`.

Fold state belongs to the note, not to `PreviewView`, which is rebuilt every
draw. Keep it on `App`, keyed by note id and heading line, or every redraw
forgets it.

Whether the editor gets folding too is a separate question. Do not answer it
here — the editor has a caret to keep honest and preview does not, which is the
distinction #0012 drew.
