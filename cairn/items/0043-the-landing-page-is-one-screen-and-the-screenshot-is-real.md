---
id: 43
title: The landing page is one screen, and the screenshot is real
type: feature
status: done
milestone: v1.0
depends_on:
- 34
- 38
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: site
---

## Problem

Someone arriving at the site has one question — what is this, and is it for me —
and roughly eight seconds in which to answer it. The README answers it with an
ASCII frame of the actual interface, which is the right instinct and the wrong
medium: it is a code block, it wraps on a phone, and it is unmaintained by
anything.

## Proposal

One screen, in this order, and no more above the fold than fits on a laptop:

1. **What it is**, in a sentence: a terminal knowledge base — an Obsidian-shaped
   vault with a modal editor, git in the status bar, and an assistant that reads
   your notes.
2. **The interface**, as a generated screenshot from #0038 — the real thing,
   at a real size, current with the commit.
3. **How to get it**, as a command you can copy, with the copy button working
   without a framework.
4. **Into the docs**, as three or four links, not a wall.

Constraints the page has to hold to, which are the parts worth writing down:

- **No layout shift.** Images carry width and height; fonts are self-hosted with
  `font-display: swap` and a metric-compatible fallback.
- **No network dependencies.** Nothing loads from a CDN — no analytics, no font
  service, no third party that can be down or watching.
- **Readable with JavaScript off.** The page is HTML; the copy button and the
  theme switcher are enhancements that fail invisibly.
- **A phone renders it correctly**, including the screenshot, which is the one
  hard case.
- **Fast.** A cold load is one HTML file, one stylesheet, one SVG; anything more
  needs a reason.

## Acceptance criteria

- [ ] The first screen answers what it is, what it looks like, and how to
      install it, on a 1280×800 display and on a phone
- [ ] Lighthouse: 100 for accessibility and best practices, and no layout shift
- [ ] The page works with JavaScript disabled, including navigation to the docs
- [ ] No request leaves the origin
- [ ] Every colour comes from the generated palette in #0037
- [ ] The screenshot is generated, never pasted

## Notes

The temptation is a terminal animation on the hero. It is expensive, it is a
dependency, and a still of a real screen with a caption tends to convert
better than a loop nobody waits out. If it earns its place later it is the cast
half of #0038, not a bespoke script.

Content is not the hard part of this item and should not be argued in it. The
page is a shell with four slots; what goes in them can change weekly without
touching anything here.
