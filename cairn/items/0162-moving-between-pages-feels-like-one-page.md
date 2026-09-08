---
id: 162
title: Moving between pages feels like one page
type: chore
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p3
effort: s
area: site
---

## Problem

Every navigation is a full page load with a white flash between. On a site
whose pages share a header, a sidebar and a stylesheet, that is a step
backwards from what the platform now does for free.

## Proposal

Two things, both a few lines, both progressive enhancements.

- **`@view-transition { navigation: auto }`** — cross-document view
  transitions, so the header and sidebar stay put and the content crossfades.
  Browsers without it navigate exactly as they do now.
- **Prefetch on hover**, for same-origin documentation links, once each, and
  only when the connection is not metered or saving data.

## Acceptance criteria

- [ ] Navigation between docs pages does not flash
- [ ] `prefers-reduced-motion` gets no transition
- [ ] `Save-Data` and a metered connection get no prefetch
- [ ] Nothing regresses with JavaScript disabled
- [ ] No page is fetched more than once by the prefetcher

## Notes

The failure mode is prefetching the whole site from the sidebar the moment the
pointer crosses it. Hover with a delay, and once per link.
