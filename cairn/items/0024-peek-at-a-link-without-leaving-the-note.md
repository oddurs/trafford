---
id: 24
title: Peek at a link without leaving the note
type: feature
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: chrome
---

## Problem

Following a link is commit-then-undo: you open the note, find it was not what
you wanted, and press `ctrl-o`. In a vault this densely linked, deciding
*whether* to follow is most of what browsing is.

Obsidian answers this with a hover preview. trafford deliberately does not track
hover — `MOUSE_ON` in `main.rs` omits 1003, because the loop redraws per event
and nothing reacts to a hover — and a keypress with no latency is a better
answer anyway.

## Proposal

A popover beside the link: what the note is, how connected it is, and enough of
it to decide.

```
  …see Neural Networks: Zero to Hero for the ground-up version.
       ╭────────────────────────────────────────────────╮
       │ Neural Networks: Zero to Hero                  │
       │ #type/reference  ·  9 backlinks                │
       │                                                │
       │ Andrej Karpathy builds neural nets from raw    │
       │ Python and NumPy, then scales to GPT-2.        │
       ╰─ enter opens · esc dismisses ──────────────────╯
```

`K` on a link, which is where vim already puts "tell me about this word", and
right-click → Peek for the mouse.

## Acceptance criteria

- [ ] Peek shows the target's title, its tags, its backlink count, and its first
      paragraph of prose
- [ ] `enter` follows it; `esc` dismisses it and the note underneath has not moved
- [ ] A link to a note that does not exist peeks as such, and offers to create it
- [ ] It is reachable with the mouse as well as the keyboard
- [ ] It draws as an overlay and does not disturb the fold or table layout beneath

## Notes

**No network, ever.** A `[text](https://…)` link peeks as the URL itself and
nothing more. Fetching a page to preview it would turn reading a note into an
outbound request, which is not something this program should start doing
quietly.

`space` was the obvious key and is taken — it toggles the task on the current
line, and the vault has 917 task lines, so that is not a binding to reassign.
`K` is free and idiomatic.

"First paragraph" means the first prose paragraph: skip the frontmatter, the H1,
and a leading callout. `is_placeholder` already knows about template files, so a
template peeks as what it is rather than as `<% tp.file.title %>`.
