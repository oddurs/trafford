---
id: 57
title: The hero plays, and it is the real program
type: feature
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: site
---

## Problem

The hero is a still photograph of a program whose whole appeal is what happens
when you press a key. `l` walking down the tree into a note, `ctrl-e` flipping
into the reading view, `enter` following a link — none of that survives a
screenshot, and those three seconds are the pitch.

Every terminal project reaches for asciinema here, and asciinema means a
`<script>` from another host, which the site does not do — `no_page_asks_the_
network_for_anything` fails the build over it, deliberately.

## Proposal

Record the cast with the machinery that already takes the screenshots.

`tools/shots.py` drives the real binary against an emulated terminal and reads
the screen back. A cast is the same run with the frames *kept* rather than only
the last one, so the recorder is a `[[cast]]` section in `shots.toml` beside
the `[[shot]]` entries and shares everything else — the fixture vault, the
isolated `$HOME`, the key names.

**A frame format, not a video.** One JSON per cast: a style table, then a row
of runs per line, with each frame carrying only the rows that changed from the
one before. The text stays real text. A twenty-frame cast of a 120×24 terminal
should be tens of kilobytes, not the megabytes twenty full SVGs would be.

**A player of about eighty lines.** Paints runs into a `<pre>`, honours
`prefers-reduced-motion` by showing the first frame with a play control, and
loops with a pause between passes rather than instantly.

**It degrades.** With no JavaScript the `<noscript>` shows the still SVG that
is already there, which is why that SVG stays generated.

## Acceptance criteria

- [ ] A cast is one entry in `shots.toml` and one command to regenerate
- [ ] Frames are diff-encoded; the hero cast is under 40 KiB
- [ ] CI checks the committed cast the way it checks the screenshots
- [ ] `prefers-reduced-motion` gets a still and a control, never motion
- [ ] With JavaScript off the page shows the screenshot and no dead controls
- [ ] Nothing is fetched from another host

## Notes

The temptation is to record the *whole* interface tour. Three seconds and one
idea. A loop nobody waits out is worse than a still.
