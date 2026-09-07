---
id: 12
title: How much of the source should the editor render?
type: spike
status: done
milestone: v0.4
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: chrome
effort: s
---

## Question

Four requests — wrapping, inline links, table-of-contents links, tables drawn as
tables — all ask the same thing: that the editor stop showing markdown as plain
text. Before any of them, there is one decision that changes what all of them
cost.

**Does the editor render, or does the preview become navigable?**

## Why it has to be answered before the work

The editor is built on an assumption stated in CLAUDE.md as an invariant:

> **The markdown renderer never changes characters.** `ui::markdown::Renderer`
> only applies styles, because the editor draws through it and the cursor
> column has to line up with the buffer.

That is what makes the cursor, the mouse, selection and scrolling correct today:
one buffer line is one screen row, and screen column is the display width of the
source prefix. Every one of these four features breaks it — a wrapped line is
several rows, a rendered link is narrower than its source, a table is a
different shape entirely.

The invariant is not in the way by accident. It is the reason clicks land on the
right character and the caret sits under the right glyph. Replacing it is the
work; doing it twice, once per feature, is how the two ideas of the layout
drift apart.

## Options

**A. Live preview in the editor.** What Obsidian does. The editor renders, and
the line the cursor is on shows its raw source. Everything stays one pane, one
mode. Costs a full position-mapping layer (see the item that follows this one)
and touches the cursor, the mouse, `j`/`k`, scrolling and selection.

**B. Make preview mode the reading mode it should be.** `ctrl-e` already
switches to a rendered, soft-wrapped view — it just has no cursor, no clicking
and no navigation. Give it those, and wrapping, links, anchors and tables all
land there, where nothing has to map back to a source position because nothing
is being edited. The editor stays exactly as it is.

**C. Both, in order.** Do B first because it is cheap and answers most of the
request, then decide whether A is still wanted.

## What would settle it

Which of these is true of how the vault is actually used:

- If most time is spent *reading* notes and *editing* is punctuated, B gives
  almost all of the value for a fraction of the risk.
- If editing is continuous and switching to read is friction, only A helps.

Worth answering by watching, not guessing: `ctrl-e` exists today. If reading in
preview is pleasant once it scrolls and links work, B was right.

The cost asymmetry is large enough to be worth the delay. B is a few days inside
one function. A rewrites how the editor addresses the screen, and every bug it
introduces is a cursor landing in the wrong place — the kind that erodes trust
in an editor rather than annoying you once.

## Answer

**Split by what a feature is for, not by which mode it lives in.** Neither A nor
B as written: the question was framed wrongly, and looking at what preview
actually does today shows why.

### What preview is now

It is not a reading view. It is source mode with `Wrap` switched on — same
lines, same gutter, same raw `**asterisks**`. The only differences are that long
lines fold and the caret is not drawn:

```
  5 **Total distance:** ~3,025
miles
```

Note the fold starting at column zero rather than under the text: even the
wrapping is ratatui's, not ours, and it does not know about the gutter. So "make
preview navigable" understates it — preview would have to be *built*, not
finished.

### The distinction that matters

Not editor versus preview. **Row mapping versus column mapping.**

| what it needs | which features | does it break the invariant? |
| --- | --- | --- |
| one buffer line becomes several rows | wrapping | **no** |
| drawn characters differ from source characters | inline links, tables | yes |

Wrapping does not change what is drawn, only where the drawing breaks. The
column of a character is still the display width of the source before it — the
existing rule holds, applied per fragment instead of per line. Concealing `[[`
and drawing a table are the things that make source width and screen width
differ, and those are what force a full two-way map.

### So

- **Wrapping goes in the editor**, with a live caret. It is about *writing*
  prose, not reading it, and an editor that scrolls sideways through a paragraph
  is the actual complaint. It needs row mapping only.
- **Rendering goes in preview**, which becomes a real reading view: no gutter, no
  caret, syntax hidden, tables drawn, links clickable. It needs a much weaker
  map — "which element is at this point", enough to follow a link, and nothing
  that has to survive an edit, because nothing there is edited.

### What this changes

- **#0013 shrinks.** The editor needs row mapping, which is a fold-point list
  per line and a round trip through it. Preview needs an element map, which is
  built while rendering and thrown away on the next draw.
- **#0015 moves to preview** and stops being frightening. Concealment with no
  caret to keep honest is just rendering.
- **#0019 becomes a clarification, not a retirement.** The invariant survives on
  the editor path, which is the path it was written to protect. It should say
  so instead of being deleted.
- **#0017 is answered by this**, and separately.

The cost asymmetry that made the original question worth asking still holds; it
just resolves differently. Nothing here rewrites how the editor addresses the
screen. The only new two-way mapping is inside a pane that does not edit.
