---
id: 26
title: A rail showing the shape of the note beside it
type: feature
status: backlog
milestone: later
depends_on:
- 20
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: l
area: chrome
---

## Problem

Prose reads best somewhere near seventy columns. On a wide terminal the reading
pane has width left over and currently just stretches into it, which makes the
text worse rather than better.

Meanwhile the note's structure — twelve headings at the median, sixty-seven at
the top end — is only visible in a context pane that truncates at "… 43 more".

## Proposal

Spend the leftover width on a thin structural rail: the shape of the document,
with your position marked.

```
 ─  Assumptions & Constraints     │  Month 2: Classical ML
 ─  Weekly Schedule               │
 ━  Phase 1: Foundations          │  Supervised learning end to end.
 ─    Month 1: Math & Python      │  You will implement each of these
 ━    Month 2: Classical ML  ◀    │  from scratch before reaching for
 ─    Month 3: Unsupervised       │  a library.
 ─  Phase 2: Deep Learning        │
```

A minimap that means something, rather than one made of shrunken pixels.

## Acceptance criteria

- [ ] The rail shows the note's headings, indented by level
- [ ] The section containing the top visible line is marked
- [ ] Clicking a heading scrolls to it
- [ ] The rail is the same fold state as #0020 — collapsing here collapses there
- [ ] It appears only when there is width to spare, and never squeezes the prose

## Notes

Blocked on #0020. The rail and the fold are two views of one tree; building the
rail first means building that tree twice.

This is the most distinctive idea in the v0.5 design note and also the least
certain. It should follow the things that are merely correct.
