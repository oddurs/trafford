---
id: 21
title: Show frontmatter as properties, not as seven lines of YAML
type: feature
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: markdown
---

## Problem

123 of the vault's 129 notes open with a frontmatter block, six lines at the
median. On a thirty-row terminal that is a fifth of the first screen spent on
machine bookkeeping before the reader reaches a sentence — and preview draws it
as literal YAML, exactly as the editor does.

```
│  1 ---
│  2 created: 2026-03-22T00:00:00Z
│  3 tags:
│  4   - status/active
│  5   - priority/high
│  6   - type/reference
│  7 ---
│  8
│  9 AI/ML Learning Roadmap
```

Eleven rows to reach the first sentence.

## Proposal

One row of the things a reader uses, one dim row of the things they occasionally
want, and then the note starts:

```
  AI/ML LEARNING ROADMAP
  status/active · priority/high · type/reference
  created 2026-03-22
```

Tags are the same clickable tags they are in the body — clicking one filters the
vault, which already works. Other keys are shown dim, in source order.

## Acceptance criteria

- [ ] A frontmatter block draws as at most two rows, and the title reads first
- [ ] Tags are clickable and filter, exactly as a `#tag` in the body does
- [ ] Other keys still appear; nothing in the block is silently dropped
- [ ] A note with no frontmatter is unchanged
- [ ] A malformed or unterminated block falls back to showing its source
- [ ] The editor is untouched — it shows the file, YAML included

## Notes

Only a block starting on line 1 is frontmatter. A `---` later in a note is a
horizontal rule and already renders as one.

`vault::note` already parses `title:` and `tags:`, including the `- item` list
form. Read from there rather than parsing YAML a second time in the renderer.

Six lines collapsing to two means `PreviewView.sources` has to map the whole
block back to something sensible, or a click on the properties row lands
nowhere. Point them at the first line of the block.
