---
id: 61
title: Why trafford exists, as its own page
type: docs
status: backlog
milestone: v1.0
depends_on:
- 56
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: docs
---

## Problem

The best writing in this project is the README's "Why" — *a vault is just a
folder of markdown files; everything good about Obsidian is a way of reading
that folder, and none of it needs a browser engine*. On the site it is a
paragraph under a fold on the landing page, set at the same size as everything
else.

An argument is the one thing a site can have that a feature list cannot.

## Proposal

A `/why` page, given the typographic treatment an essay gets rather than the
one a reference page gets: a wider measure for the opening, real hierarchy,
and room.

What it has to say, and nothing more:

- A vault is a folder. Everything else is reading.
- Why the terminal, honestly — not "it is fast", but that notes and git and an
  editor live in the same place, and moving between them is the friction.
- What was given up, said out loud. That is what makes the rest believable.

## Acceptance criteria

- [ ] Reachable from the landing page and the docs navigation
- [ ] Reads as an argument, not as a feature list
- [ ] Says at least one true thing about what trafford is bad at
- [ ] No new layout machinery — the docs shell with a different measure

## Notes

The failure mode is a manifesto. Six hundred words, one idea, and a link to
getting started at the end.
