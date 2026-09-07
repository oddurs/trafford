---
id: 56
title: The landing page has a grid, not a column
type: feature
status: done
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: site
---

## Problem

The landing page is one 62rem column with the hero held to 46ch, so the right
third of a laptop screen is dead air. Nothing on the page is doing structural
work — there is no shape to it, only a stack.

Vercel's whole visual identity is hairline cells; Stripe's is a persistent
asymmetric split. Neither is decoration. A grid is what tells a reader where
they are before they read a word.

## Proposal

Hairline cells, in the colour the app already uses for pane borders.

- **The hero is an asymmetric split**, not a centred column: the sentence and
  the install command on the left, the running program on the right, sharing a
  rule.
- **Features become cells**, bordered rather than stacked, so the page reads
  as a specimen sheet rather than a scroll.
- **The rules extend past the content** the way a terminal's pane borders do —
  the one visual idea the site should borrow from the program it documents.
- **One accent, used sparingly.** `--accent` on the brand mark, the current nav
  item, and nothing else on the landing page.

Narrow screens collapse to one column and the rules become separators. The
screenshot keeps scrolling inside its own box, which already works.

## Acceptance criteria

- [ ] No dead column on a 1280×800 display
- [ ] The page still fits and reads at 380px with nothing overflowing the body
- [ ] Every rule is `--border`; no new colour role is invented for the grid
- [ ] Lighthouse stays at 100 for accessibility, with no layout shift
- [ ] The first screen still answers what it is, what it looks like, and how to
      install it

## Notes

The failure mode is a grid that is decoration: cells that hold one word each,
borders that do not line up with anything. Every rule should be separating two
things a reader would otherwise have to separate themselves.
