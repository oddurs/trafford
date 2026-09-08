---
id: 169
title: Measure the site in CI, not only by eye
type: chore
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: site
---

## Problem

`tools/look.py` measures the built site in a real browser — anything wider than
the viewport, text under 11px or 4.5:1, and how much of a section is air. On
the pass that introduced it, it found four things in ten minutes that a
full-page screenshot had not:

- the whole page scrolled sideways at 420px, because a brand, four links and a
  control do not fit across a phone
- the footer's column headings were 3.06:1
- the closing section's command was centred *in its column* rather than on the
  page, because the section still had two of them
- cards with no recording had no bottom padding, so their last line sat on the
  border

None of that fails a build today. All of it is exactly the class of thing that
ships and stays shipped, because nobody opens the site at 420px on purpose.

## Proposal

Run it in CI, on the sizes that matter, and fail on the checks that are
unambiguous:

- the page must not scroll sideways at 380, 700 and 1440
- no text under 11px, none under 4.5:1
- both colour schemes

The "mostly air" number is a hint for a person and should stay one — a section
that is deliberately spacious is not a bug.

## What it costs

`playwright install chromium` is about 100 MB and a minute or two on a cold
runner, against a `site` job that currently finishes in two. Cache the browser
by version and it is seconds after the first run.

That is the whole question, and it is why this is a separate item rather than
part of the pass that wrote the tool: it is a real bill, and the site is not
yet load-bearing enough to have obviously earned it.

## Acceptance criteria

- [ ] A pull request that makes the page scroll sideways at any of the three
      widths fails
- [ ] A pull request that drops any text under 11px or 4.5:1 fails
- [ ] The browser is cached; the job grows by seconds, not minutes, when warm
- [ ] The failure names the element and the measurement, not just "failed"
- [ ] Running it locally needs one command and no arguments

## Notes

Keep the thresholds in the tool rather than in the workflow. A number in a YAML
file is a number nobody reads.
