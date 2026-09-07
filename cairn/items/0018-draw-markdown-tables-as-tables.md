---
id: 18
title: Draw markdown tables as tables
type: feature
status: backlog
milestone: v0.4
depends_on:
- 13
- 17
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: markdown
effort: l
---

## Problem

A markdown table is drawn as its source, which is a row of pipes and a row of
dashes and no alignment at all:

```
| When | What |
| ---- | ---- |
| **April 3, 2026** | Karpathy posts on X under the title "LLM wiki" |
```

The columns line up in the file only if they were typed that way, and in a real
note they are not.

## Proposal

Parse the block and draw it with its columns measured and its rules drawn:

```
┌────────────────┬──────────────────────────────────────────┐
│ When           │ What                                     │
├────────────────┼──────────────────────────────────────────┤
│ April 3, 2026  │ Karpathy posts on X under the title …    │
└────────────────┴──────────────────────────────────────────┘
```

Column widths from the content, capped so one long cell does not squeeze the
rest to nothing. Alignment from the separator row's colons, which is what they
are for.

## Acceptance criteria

- [ ] A table is measured and drawn with aligned columns and rules
- [ ] Alignment markers in the separator row are honoured
- [ ] A cell too wide for its column is truncated visibly, not silently
- [ ] Inline markup inside a cell is rendered, since the cell is already rendered
- [ ] A malformed table — ragged rows, a missing separator — falls back to its
      source rather than guessing
- [ ] The interaction #0017 chose is what ships

## Notes

Blocked on #0013 and #0017. Do not start this before the spike closes: the
rendering is the easy half, and building it first means the interaction inherits
whatever the rendering happened to allow.

Measuring uses display width, not character count — a table of CJK is exactly
the case that catches a naive implementation, and there is already a helper for
it.
