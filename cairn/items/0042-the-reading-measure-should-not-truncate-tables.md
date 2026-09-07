---
id: 42
title: The reading measure should not truncate tables
type: bug
status: done
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: markdown
---

## What happens

Reading a note holds everything to seventy-two columns, including tables. A
table does not wrap at a measure — its cells are already sized — so it gets
cut:

```
 reading      │ Hardware │ Personal laptop for Phases 1-2; cloud GPU (Colab P… │
 editor       │ Hardware │ Personal laptop for Phases 1-2; cloud GPU (Colab Pro, Lambda) for later work │
```

The reading view loses data the editor shows fine, on a pane with room to
spare.

## What should happen

A measure is a rule about prose. A table gets the pane.

## Reproduction

1. A note with a table wider than seventy-two columns
2. `ctrl-e` on a terminal wider than that
3. Cells are truncated with `…` that are not truncated with
   `reading_focus = false`

## Proposal

`Rendered.rigid` marks a line whose width was already decided when it was
drawn. Rigid lines are laid out at the pane; prose folds at the measure.

Prose keeps one left edge — a paragraph centred line by line is not a
paragraph — and a rigid block centres on the same axis, so it grows into both
margins rather than off to one side.

## Acceptance criteria

- [x] A table wider than the measure keeps its cells
- [x] Prose still folds at the measure
- [x] Prose shares one left edge; a wide block centres on it
- [x] Clicks land on the character under the pointer with the offset applied
- [x] Nothing is offset when there is no measure to centre within
