---
id: 14
title: Soft-wrap long lines instead of scrolling sideways
type: feature
status: done
milestone: v0.4
depends_on:
- 13
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: editor
effort: m
---

## Problem

Prose does not have short lines. A paragraph written as one long line runs off
the right edge, and the editor answers by scrolling sideways — so reading a note
means the text sliding under a fixed viewport, and the rest of the line is
somewhere off-screen.

Preview mode wraps. The editor does not, and the editor is where the writing
happens.

## Proposal

Wrap at the pane width, breaking on word boundaries, with continuation rows
indented to match any list marker so a wrapped bullet still reads as one item:

```
 12 - A link to a note that does not exist yet, like
      [[Someday]], shows in red — press enter on it
```

`wrap_text` in `src/ui/mod.rs` already breaks on words and measures display
width; the hard part is not the wrapping, it is that the cursor, clicks and
`j`/`k` have to agree about it.

## Acceptance criteria

- [ ] Long lines wrap at the pane width, on word boundaries
- [ ] A wrapped list item or quote indents its continuation to the marker
- [ ] The caret is correct on a wrapped line, including inside a CJK run
- [ ] Clicking a continuation row lands on the character under the pointer
- [ ] Horizontal scrolling goes away, or becomes the opt-out for people who want it
- [ ] `wrap_column` in the config is honoured — it exists and does nothing today

## Notes

Blocked on #0013. `wrap_column = 0` already means "the pane width" in the config
and has never been read; this is the item that gives it meaning.

Worth deciding here: whether wrapping is on by default. A hard-wrapped vault
gets no benefit and a soft-wrapped one is unusable without it, and the config
key exists either way.
