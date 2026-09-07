---
id: 16
title: Follow a table-of-contents link to its heading
type: feature
status: backlog
milestone: v0.4
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: markdown
effort: s
---

## Problem

Long notes carry their own contents list:

```markdown
- [Phase 1: Foundations](#phase-1-foundations)
- [Phase 2: Deep Learning](#phase-2-deep-learning)
```

Those are links, and nothing happens when you press enter on one. The outline
pane can jump to a heading and a link in the text cannot, which is an odd place
to draw the line.

## Proposal

An anchor link — `[text](#slug)` — resolves against the headings of the note it
is in and jumps there. Same for the anchor half of `[[Note#Heading]]`, which is
already parsed and only used when following the link to another note.

GitHub's slug rule is the one people's notes are written against: lowercase,
spaces to hyphens, punctuation dropped. Match it rather than inventing one.

## Acceptance criteria

- [ ] `[text](#slug)` jumps to the heading whose slug matches, in this note
- [ ] `[[Note#Heading]]` jumps to that heading in the note it opens — it opens
      the note today and ignores the anchor
- [ ] An anchor that matches nothing says so rather than doing nothing quietly
- [ ] Duplicate headings resolve to the first, as GitHub does
- [ ] Slugging is tested against the headings in a real note, not invented ones

## Notes

Independent of the rendering work: this is link resolution, and the machinery
for following a link already exists. It could ship before #0012 is even
answered.

`Note.headings` already carries the text and line of every heading, so this is a
slug function and a lookup.
