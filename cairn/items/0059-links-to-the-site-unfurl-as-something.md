---
id: 59
title: Links to the site unfurl as something
type: chore
status: backlog
milestone: v1.0
depends_on:
- 56
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: site
---

## Problem

A link to this site currently unfurls as nothing — no image, no card. That is
the first impression in every place a link is actually shared, and it is
currently blank.

## Proposal

Generate the card from the pipeline that already draws the screenshots.

`tools/shots.py` renders a terminal grid to SVG. An Open Graph card is the same
thing at 1200×630 with the title over it. Rasterise once at build time, because
`og:image` has to be a PNG or a JPEG — every scraper is entitled to ignore SVG,
and most do.

- One card per page, with the page's own title, over a dimmed screenshot
- `og:image`, `twitter:card`, and the dimensions, so nothing is guessed
- Committed and checked, like the screenshots

## Acceptance criteria

- [ ] Every page has an `og:image` that resolves and is under 200 KiB
- [ ] The card carries the page's title, not the site's
- [ ] Regenerating is one command; CI fails if a card is stale
- [ ] No new runtime dependency

## Notes

Rasterising is the only awkward part — nothing in the workspace draws a PNG.
The cheapest honest answer is a headless browser at generation time, which the
site already relies on for nothing and would then rely on for this; the
alternative is a hand-rolled PNG writer over a glyph atlas, which is a fun
afternoon and a bad idea.
