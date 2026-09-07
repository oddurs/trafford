---
id: 39
title: Give preview a cursor of its own
type: bug
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: m
area: chrome
---

## What happens

In preview the wheel does nothing. Thirty wheel-downs on a 592-line note leave
the view exactly where it was. `j` moves the cursor without moving the view.
`ctrl-d` pages by a distance that does not match what is on screen. `G` in a
folded note puts the cursor on line 592, which is inside a fold and not drawn.

## What should happen

Scrolling scrolls. Every motion moves something visible.

## Reproduction

1. Open a long note, `ctrl-e`
2. Wheel down thirty times — nothing moves
3. `zM`, then `G` — the cursor reports line 592 and the view shows the top

## Why

Preview draws from `PreviewView.layout` and computes motion and scrolling from
`editor.layout`. Those are different documents: concealment shortens lines,
frontmatter collapses six rows to two, and a fold removes hundreds. They agree
only when nothing is rendered, which is now never.

Worse, `draw_editor` calls `sync_scroll_visual` anchored to the cursor's source
line on every draw, so any scroll is undone before it is seen. That is the
pinning.

## Proposal

Preview gets a cursor of its own: a row index into its own layout, which every
motion in preview moves and which the view follows the way the sidebar and the
pickers already work in this program.

`buf.row` becomes derived rather than authoritative — the source line under the
preview cursor — so leaving preview lands where you were reading, and `za`, `K`
and the section crumb keep working on a line that is actually on screen.

Nothing re-anchors per draw. The scroll is remapped once, explicitly, when the
thing under it changes: folding, switching notes, resizing.

## Acceptance criteria

- [x] The wheel scrolls preview, and the view stays where it was put
- [x] `j`/`k` move a visible row; the view follows when they reach the edge
- [x] `ctrl-d`/`ctrl-u` page by what is on screen
- [x] `gg`/`G` go to the first and last drawn rows
- [x] Folding above the cursor does not throw the view somewhere unrelated
- [x] Leaving preview puts the buffer cursor on the line that was under the
      preview cursor
- [x] `za` and `K` act on the row the reader can see
