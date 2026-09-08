---
id: 57
title: Somewhere for a query to land
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
claimed: 2026-09-08
depends_on:
- 56
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: chrome
---

## Problem

A query layer with nowhere to put its answers is a library call. The results
need a place that is navigable by keyboard and by mouse, that opens a result
where the reader can see it, and that does not invent a fourth idea of the
layout.

## Proposal

A results pane, drawn under the existing pane rules.

- One row per hit: the note, and the line that matched, from `text` while the
  match was found in `haystack`.
- `enter` and a click both open the note at that line **through
  `App::jump_to`**, which opens the folds hiding it first. A destination the
  reader cannot see is not a destination — the rule search already needed.
- Hit-testing shares the renderer's geometry and reads rects recorded during
  the last draw, via `App::panes`. Never a second, parallel idea of the layout;
  it diverges silently and lands clicks a row off.
- Results group by note where a query returns many lines from few notes, which
  0027 shows is the common case, not the exception.
- Saved queries live in `config.toml` and appear in the palette by name — and
  now findable by that name, since 0052.

## Acceptance criteria

- [ ] Keyboard and mouse both open a result, and land on the same line
- [ ] Opening a result inside a collapsed section reveals it
- [ ] The pane obeys the existing chrome rules: `ctrl-e` reading posture hides
      it, and toggling it by hand clears `chrome_before_preview`
- [ ] A probe run at several terminal widths finds no panic and no row that
      exceeds its pane

## What shipped

The results landed in the search overlay rather than a new pane — see 0056 for
why: text with no `field:` in it still parses to plain terms, so there is one
search surface, not two. What this item added is what makes that surface
readable.

**Results group by note.** The note says its name once, with a count, and its
matching lines sit under it with their line numbers. Ranking purely by score
interleaves notes by where a match happened to sit, which is unreadable when a
query returns many lines from few notes — and 0027 shows that is the common
case, not the exception.

The scroll window is over *drawn rows* rather than hits, because a note's
heading takes a row too. Counting hits made the pane overflow its own rect the
moment grouping was added.

**Saved queries live in `config.toml`** and appear in the palette by name:

```toml
[queries]
stale = "status:active modified:<2026-06-01"
```

`ctrl-k → stale` runs it. They are findable by their own name because 0052 made
that true of everything in the palette.

Probed at 40×12, 60×16, 80×24, 100×30, 140×40 and 200×50: no panic, and no row
wider than its pane.
