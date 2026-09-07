---
id: 41
title: Say where you are in the note
type: feature
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: chrome
---

## Problem

The status line in preview shows `592:1` — a source line and a column. The
column is always 1, because preview has no meaningful column. The line number
is a number in a file, not a position in what is being read.

In a 592-line note there is nothing that says how far through it you are.

## Proposal

In preview, spend that space on where you are in the document instead: how far
through, and a rail down the edge showing it.

```
 READING   AI/ML Learning Roadmap        44%   ▐
```

The rail is one column, drawn from what the layout already knows: the view's
row range over the total. It costs nothing to compute and answers the question
a long note constantly raises.

## Acceptance criteria

- [x] Preview shows how far through the note the view is
- [x] A rail down the pane's edge shows the same thing at a glance
- [x] It reflects the drawn rows, so a folded note is honest about being short
- [x] The editor's line:column is unchanged
