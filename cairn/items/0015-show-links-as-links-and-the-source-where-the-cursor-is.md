---
id: 15
title: Show links as links, and the source where the cursor is
type: feature
status: backlog
milestone: v0.4
depends_on:
- 13
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: markdown
effort: m
---

## Problem

A note full of links reads as a note full of brackets:

```
- A link to a note that does not exist yet, like [[Someday]], shows in red
| Roadmap | [[ai-ml-learning-roadmap]] | Active |
```

The brackets are syntax, not content. They are there so the file is portable
markdown, and they should not be what you read.

## Proposal

Draw `[[Note|alias]]` as `alias`, `[text](url)` as `text`, `**bold**` as bold
text without the asterisks. The source comes back on the line the cursor is on,
which is the rule Obsidian uses and the only one that stays predictable: you
always know where the raw text is, because it is where you are.

Clicking a rendered link follows it — which already works, and gets easier once
the link is what is drawn rather than a run of punctuation the click has to
land inside.

## Acceptance criteria

- [ ] Wikilinks, markdown links and emphasis render without their syntax
- [ ] The line the cursor is on shows its source, and switching lines swaps them
- [ ] The caret is right on a line where what is drawn is shorter than what is
      written — the case this whole feature is about
- [ ] Editing inside a rendered line behaves as if the source were showing,
      because it is
- [ ] A broken link is still visibly broken when rendered, not just when raw

## Notes

Blocked on #0013.

This is where the invariant actually dies. `markdown::Renderer` has a test named
`rendering_preserves_every_character`, kept deliberately, and this item deletes
it — so it has to replace it with the property that supersedes it: the mapping
round-trips (#0013), which is the weaker guarantee that still keeps the cursor
honest.

Reveal-on-cursor-line is the coarse version. Obsidian reveals the *element* the
cursor is in, so the rest of the line stays rendered. That is nicer and much
fussier, and the line-level rule should ship first: it is one comparison, and if
it turns out to be annoying in use the finer rule is a follow-up rather than a
rewrite.
