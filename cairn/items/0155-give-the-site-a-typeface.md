---
id: 155
title: Give the site a typeface
type: chore
status: done
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: site
---

## Problem

`system-ui` is the loudest "this came from a template" signal a website has.
It was the right first call — a web font service is a request leaving the
origin, and self-hosting means committing a binary — but it is the single
cheapest thing standing between this site and looking like someone made it.

## Proposal

**One variable monospace, self-hosted, subset.** Not mono for code and a sans
for prose: mono for *everything*. For a terminal knowledge base that is a
statement rather than a default, and it costs one file instead of two.

- JetBrains Mono, variable weight axis, SIL OFL 1.1 — with `OFL.txt` committed
  beside it, because shipping a font without its licence is not shipping it.
- Subset to latin plus the box-drawing and arrow characters the prose uses,
  `woff2`, one weight axis. The target is under 45 KiB.
- `font-display: swap`, with a metric-matched fallback stack so the swap does
  not move a single line.
- Long-form prose gets a larger size, a looser line height and the measure it
  already has. Monospace at a sans's metrics is what makes people say
  monospace is unreadable.

The page currently costs 57 KiB over three requests. This roughly doubles it
and it is still an order of magnitude under any site it is being compared to.
If it cannot be kept under 100 KiB total, it is not worth having.

## Acceptance criteria

- [ ] One font file, self-hosted, under 45 KiB, with its licence beside it
- [ ] `no_page_asks_the_network_for_anything` still passes
- [ ] No layout shift when the font arrives — asserted, not eyeballed
- [ ] The subset covers every character the built site actually draws, checked
      by a script rather than by looking
- [ ] A cold landing page stays under 100 KiB

## Notes

Berkeley Mono is the one that would actually make this sing and it is paid.
Worth revisiting if this ever has a budget.

The subset check is the part that rots: prose gains a character the subset does
not have and it silently falls back mid-word. Generate the character set from
the built HTML.
