---
id: 55
title: Frontmatter properties are first-class
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: m
area: vault
---

## Problem

`Note` reads `title:` and `tags:` from frontmatter and discards every other
key. Measured on the vault this is built for, that is throwing away the
organising system the reader actually maintains:

| value | notes |
| --- | --- |
| `type/reference` | 92 |
| `status/active` | 62 |
| `priority/high` | 25 |
| `type/checklist` | 9 |
| `status/done` | 8 |
| `topic/computability` | 6 |

Around 220 typed values across 148 notes, in four families — `type`, `status`,
`priority`, `topic`. The vault has a schema. The program that draws it cannot
see one.

This was nearly missed. An earlier count of `#tags` in note bodies found 45
uses across five families and read as "tagging was never adopted"; the real
figure is five times that, because this vault writes its tags in frontmatter
list form, which that grep did not match. The conclusion drawn from the first
number was wrong in exactly the direction that would have shrunk this
milestone.

## Proposal

Index every frontmatter key and its value, not a chosen few.

- Values are scalars or lists; both are kept, and list order is preserved.
- Values are stored **as written**. `type/Reference` is what the author typed,
  and normalising its case would make the vault disagree with Obsidian about
  its own contents. Matching is case-insensitive at query time instead — the
  same split the `text` / `haystack` pair already makes.
- Unknown keys are ordinary. A vault with a key nobody anticipated must not
  lose it, which is already the rule preview follows when it draws properties.
- Preview's properties row reads the index rather than re-parsing the block, so
  there is one idea of what a note's properties are.

## Acceptance criteria

- [ ] Every frontmatter key present in the real vault is indexed
- [ ] List-form and scalar-form values both parse, and round-trip unchanged
- [ ] A malformed or unterminated block yields no properties and no panic —
      it is not frontmatter, and is shown as written
- [ ] Preview's properties row and the index never disagree

## What shipped

`split_frontmatter` now returns `Properties` — a key and every value written
under it, in order — instead of comma-joining lists into one string. That
join was lossy in a way nobody had hit yet: `title: Hello, world` is one
value, and splitting it later would have invented a second. Inline `[a, b]`
arrays are split at parse time, where the brackets say to.

Values keep the case they were written in; `Note::property` and
`property_is` match keys and values case-insensitively, so the decision is
the asker's rather than the parser's.

Shipped with 0056 rather than alone, because an index nobody queries is
provably dead code — clippy said so, under `-D warnings`, which is the
correct answer.
