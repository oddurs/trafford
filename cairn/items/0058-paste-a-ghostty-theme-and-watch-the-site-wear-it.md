---
id: 58
title: Paste a Ghostty theme and watch the site wear it
type: feature
status: backlog
milestone: v1.0
depends_on:
- 55
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: m
area: site
---

## Problem

The best claim this project makes is in the README and nobody can check it:
*whatever your terminal is already wearing, trafford can wear too*. It reads
Ghostty theme files directly. On the website that is a sentence.

## Proposal

A box you paste a Ghostty theme into. The whole site recolours, and a mock of
the interface beside it recolours with it.

`Theme::from_ghostty` and `palette::stylesheet` already exist and neither
touches the filesystem. Compiling those two — and nothing else — to
`wasm32-unknown-unknown` is a small, contained build: parse the pasted text,
emit custom properties, set them on `:root`.

- Drag a file in, or paste the text
- A link that puts the theme in the URL, so one can be shared
- A refusal that says *why* when the text is not a theme, since that is the
  interesting case
- With no JavaScript the section is not drawn at all — no dead box

## Acceptance criteria

- [ ] Pasting a real Ghostty theme recolours the page immediately
- [ ] The colours match what the app produces for the same file, asserted by a
      test that runs both paths over the same input
- [ ] The wasm bundle is under 100 KiB, loaded only when the section is reached
- [ ] Nothing is fetched from another host
- [ ] A file that is not a theme produces a readable explanation

## Notes

The size discipline is the whole risk. Compiling the theme parser means
compiling `toml` and `anyhow` behind it; if that lands at half a megabyte the
honest answer is to port the parser to a hundred lines of JavaScript and pin it
with a test that feeds both the same corpus.
