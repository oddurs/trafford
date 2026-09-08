---
id: 68
title: A landing page of showcases, not a column
type: feature
status: done
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: l
area: site
---

## Problem

The landing page has one recording and then a column of prose. It reads as a
good README with a screenshot on it, not as a page anyone sends to someone.

The comparison worth making is Vercel's: a hero, then a run of showcase
sections that each *demonstrate one thing*, alternating side to side, at a
scale that breaks out of the text column. Every one of their visuals is a
picture of their product doing something. Ours would be the same, except that
ours can be the program itself, recorded — which is the advantage a terminal
application has and almost never uses.

## Proposal

Six more recordings, and a page built to hold them.

**The recordings**, each one thing:

| | shows |
| --- | --- |
| hero | opening a note by name, then reading it |
| links | the cursor on a `[[link]]`, `enter`, and the backlinks pane answering |
| folding | `zM` collapsing a note to its headings, `za` opening one |
| palette | `ctrl-k`, filtered by typing |
| search | `ctrl-f` across the vault, with the line each hit came from |
| git | `ctrl-g` over a dirty tree: stage, diff |
| themes | one screen, three palettes, side by side |

**The page**: hero, then alternating showcase sections — prose one side, the
program the other, flipping each time — then a bento row of the smaller ones,
then the theme strip full width.

**What has to be built for it**

- `![[x.cast.json]]` renders as a player rather than as an image, with its
  poster still underneath. One `[[cast]]` entry should emit both, from the same
  recording, so a poster cannot drift from the cast it belongs to.
- A `theme` on a shot, so the strip is three recordings of one screen rather
  than three screenshots someone took.
- A `dirty` on a shot, so the git panel has something to show. A clean tree
  makes a poor argument for a git panel.
- Sections in the landing layout, so alternation is a CSS rule rather than a
  hand-built page. The markdown stays ordinary markdown.
- Casts load and play when they are scrolled to, and stop when they are not.
  Six players all running behind the fold is a laptop fan.

## Acceptance criteria

- [ ] Seven visuals, every one generated from the real binary
- [ ] Sections alternate without any per-section markup in the note
- [ ] A cast fetches nothing until it is near the viewport, and pauses when it
      leaves
- [ ] `prefers-reduced-motion` gets posters and controls, never movement
- [ ] With JavaScript off every showcase still shows its poster
- [ ] The page stays under 250 KiB on first load, casts excluded — they are
      fetched on approach, not on load
- [ ] Every poster is the cast's own frame, emitted by the same recording

## Notes

The failure mode is a page that is six screenshots and a scroll bar. Each
section has to say one thing the one before it did not, and the recording has
to be of *that* thing — a cast of the tree opening under a heading about links
is worse than no cast.

The other failure mode is weight. Six casts is six fetches; if they are not
lazy the page is a megabyte before a word is read.
