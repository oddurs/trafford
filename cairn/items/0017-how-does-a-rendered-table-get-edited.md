---
id: 17
title: How does a rendered table get edited?
type: spike
status: done
milestone: v0.4
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: chrome
effort: s
---

## Question

A markdown table should look like a table. But a table you cannot edit is worse
than pipes you can, so: **what does interacting with a rendered table do?**

The request said "unless clicked or something (the unless clicked UX should be
figured out)", which is the right instinct — the rendering is easy and the
interaction is the whole design.

## Why it has to be answered before the work

A table is the first *block* element. Everything before it is per-line: a
wrapped line is still one line, a rendered link is still on its row. Six source
lines becoming a nine-row box is a different thing, and how you get back to the
source decides how much of the mapping in #0013 has to exist.

Pick the interaction first and the implementation follows. Build the rendering
first and the interaction gets whatever the implementation left possible.

## Options

**A. The cursor reveals it.** Move into the block, see the pipes; move out, see
the table. Consistent with the line rule in #0015, so there is one thing to
learn. But a table is tall, so the whole block flips as the cursor crosses its
edge, and the text below jumps by the difference in height. Obsidian does this
and the jump is the part people complain about.

**B. A key toggles the block.** `enter` or `space` on a table switches that one
between drawn and raw, and it stays until toggled back. Nothing moves unless you
ask. Costs a piece of per-block state that is not in the file, so it is lost on
reopen — probably fine, possibly annoying.

**C. Click a cell to edit that cell.** The table stays drawn; a click puts a
caret inside one cell and typing edits the source behind it. The nicest to use
and by far the most to build: cell-level position mapping, plus what tab does,
plus what happens when a cell grows past its column.

**D. Drawn in preview, pipes in the editor.** No new interaction at all — the
editor shows what the file says, and `ctrl-e` shows the table. Free, if #0012
answers B.

## What would settle it

How often a table is *edited* versus *read*. A vault with twenty tables that are
written once and consulted often wants D or B. A vault where tables are working
documents wants C, and should not pretend otherwise.

The 127-note vault has tables in the roadmap, the reading lists and the
dashboard. Reading them is the common case by a wide margin. That points at D
or B, and B only if D turns out to be too coarse.

## Answer

**D — drawn in preview, pipes in the editor.** Settled by #0012, which put every
feature that changes the drawn characters into the preview pane.

The question "what does interacting with a rendered table do?" dissolves once
tables are only rendered where nothing is edited. There is no reveal, no toggle,
no per-block state, and nothing to learn: the editor shows the file, and `ctrl-e`
shows the table. You already know which one you are looking at, because you
chose it.

### Why the others lose

**A, cursor reveals the block.** The failure is specific: a nine-row table
collapsing to six source lines as the caret crosses its edge shifts everything
below by three rows, mid-keystroke. Obsidian does this and it is the part people
complain about. Buying that with a position-mapping layer, to solve a problem
created by the same feature, is a poor trade.

**B, a key toggles it.** Better than A, and the state is per-block and not in the
file, so it is lost on reopen and cannot be explained by anything the user can
see. A mode you can enter and not notice.

**C, click a cell to edit it.** The nicest to use. It needs cell-level position
mapping, a tab order, and an answer for a cell that grows past its column —
against a vault where tables are read far more than edited. Worth revisiting if
that ever stops being true; it is not a first version.

### The evidence

In this vault tables appear in the roadmap, the reading lists and the dashboard.
All three are consulted, not filled in. The editing that does happen is adding a
row, which is easier against pipes than against any of A, B or C.

**Reopen this** if tables start being written here rather than read, which would
show up as time spent in a table with the pipes visible. Nothing about D
forecloses C later — a rendered table in preview is the same parse either way.
