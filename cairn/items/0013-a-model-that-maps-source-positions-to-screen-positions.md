---
id: 13
title: A model that maps source positions to screen positions
type: feature
status: backlog
milestone: v0.4
depends_on:
- 12
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: chrome
effort: l
---

## Problem

Today the editor addresses the screen directly:

```rust
let y = inner.y + (row - editor.scroll);
let cx = inner.x + gutter + width_of_prefix(&visible, col - hscroll);
```

One buffer line is one screen row, and the column is the display width of the
source prefix. That is true right up until a line wraps, a link renders shorter
than it is written, or a table becomes a box — and then it is false in five
places at once: the caret, the mouse, `j`/`k`, scrolling, and the selection
highlight.

Doing the mapping inline in each of them is how two ideas of the layout come to
disagree. `CLAUDE.md` already says this about hit-testing, learned the hard way:

> Never compute a second, parallel idea of the layout — it will diverge silently
> and clicks will land one row off.

## Proposal

One type that owns the mapping. Given the buffer, the pane width and the cursor,
it produces the rows to draw and answers both directions:

- `screen_of(row, col) -> (x, y)` — where the caret goes
- `source_of(x, y) -> (row, col)` — what a click hit
- `visual_down(row, col)` / `visual_up` — what `j` and `k` mean when one line is
  three rows

Everything that touches the screen goes through it, including the parts that
work fine today, so there is one implementation rather than a new one beside the
old one.

## Acceptance criteria

- [ ] The caret, the mouse, `j`/`k`, scrolling and selection all read the same model
- [ ] Round-trip property: `source_of(screen_of(p)) == p` for every position in a
      buffer of mixed ASCII, CJK, emoji and long lines
- [ ] `j` moves by screen row and `gj` by buffer line, or the reverse — decided
      and documented, not left ambiguous
- [ ] The existing behaviour is unchanged when nothing wraps and nothing renders,
      proved by the current tests still passing untouched

## Revised by #0012

Smaller than this item assumed. The spike put everything that changes the *drawn
characters* into preview, so the editor needs **row mapping only**: one buffer
line becomes several screen rows, and within a row the column is still the
display width of the source before it.

That means `screen_of` and `source_of` are a fold-point list per visible line
plus a search through it, not a general two-way translation. The round-trip
property is unchanged and still the first test to write.

The preview pane needs a separate, weaker thing — "which element is at this
point", enough to follow a link — built while rendering and discarded on the
next draw. It is not this item.

## Notes

Blocked on #0012. If that spike answers **B**, this item shrinks to almost
nothing — the preview pane needs a much weaker mapping, because nothing there
has to map back to a source position for editing. If it answers **A**, this is
the whole job and the three features after it are comparatively small.

The round-trip property is the one to write first. It is cheap to state, it
catches the entire class of bug this item exists to prevent, and it is the test
that would have caught the wide-character cursor drift months earlier.
