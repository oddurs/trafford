---
id: 22
title: Draw callouts as callouts
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

The vault has 269 callouts and exactly three kinds — `note` (111), `tip` (105)
and `warning` (57). Preview draws every one as an italic blockquote with the
marker sitting in the text as literal characters:

```
> [!note] Research Scope
> Lesser-known, unusual, and extraordinary stories of revenge.
```

The `[!note]` is syntax. Showing it is the same failure as showing the `[[`
around a link, and it is on most notes in the vault.

## Proposal

A coloured bar, the kind as a label, and the title when one is given:

```
  ▎ NOTE · Research Scope
  ▎ Lesser-known, unusual, and extraordinary stories of
  ▎ revenge throughout history.
```

Three kinds is a small enough set to give each its own colour role: note reads
as information, tip as affirmative, warning as caution.

## Acceptance criteria

- [ ] The three kinds each draw with their own colour and label
- [ ] A kind nobody anticipated still draws as a callout, labelled with whatever
      was written, rather than falling back to a plain quote
- [ ] `> [!note] Some Title` uses the title; without one, just the label
- [ ] Markup inside a callout still renders — links, emphasis, code
- [ ] A `>` quote that is not a callout is unchanged
- [ ] Colours come from `Theme` roles, so a Ghostty import still governs them

## Notes

A callout continues across every following `>` line, so it is a block like a
table rather than a line like a heading — which means it draws through the path
tables already use and inherits the `sources` mapping for free.

Do not hardcode colour. `Theme` has accent and added roles already; add roles
rather than hex if the three kinds do not map cleanly onto what exists.

Obsidian also has `> [!note]-` for a callout collapsed by default. Out of scope
until #0020 lands, since it is the same fold state.
