---
id: 23
title: Keep the current section's heading on screen
type: feature
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: chrome
---

## Problem

A heading every six lines of body, twelve per note, nested three deep. Scroll
into the middle of a long note and the heading that tells you what you are
reading is off the top of the screen. On the vault's longest note — 1,254 lines,
67 headings — you are always inside a section and can almost never see which.

## Proposal

One dim row at the top of the pane, holding the chain of headings that contain
the first visible line:

```
  Phase 1: Foundations  ›  Month 2: Classical ML
  ────────────────────────────────────────────────
  from scratch before reaching for a library. Start with the
```

Clicking a crumb jumps to that heading. Terminals almost never do this; it costs
one row and removes a question the reader is otherwise always half-asking.

## Acceptance criteria

- [ ] The chain shows the headings containing the top visible line
- [ ] It hides itself when that heading is already on screen, rather than
      printing the same words twice
- [ ] Clicking a crumb goes to that heading
- [ ] The pane's height accounting stays right — one row fewer for text
- [ ] On a narrow pane it truncates from the left, keeping the deepest heading,
      which is the one that says where you are

## Notes

`ui::outline_of` already walks the buffer for headings and skips fenced blocks.
Read it rather than writing a second heading scanner — a parallel idea of the
document's structure is the same class of bug as a parallel idea of the layout.

Whether this shows in the editor as well as preview is worth deciding while
building it. The editor has the same problem and the same headings.
