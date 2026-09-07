---
id: 60
title: The roadmap is a page, built from the items
type: feature
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: site
---

## Problem

The thing an open-source site has that a company's cannot fake is proof that
someone is working on it right now. This one shows none — no roadmap, no
history, no sign of life between releases.

`cairn` already holds every item as structured data with milestones, statuses
and dependencies, and already renders `ROADMAP.md`.

## Proposal

A `/roadmap` page built from the items rather than from the rendered markdown.

- Grouped by milestone, with the progress each one already computes
- What is in progress, what is blocked and by what
- Linked to the item on GitHub, so a reader can read the reasoning

`cairn export --json` is the seam; the site should not learn cairn's file
format. If that command does not exist yet, that is the first half of this.

## Acceptance criteria

- [ ] `/roadmap` reflects the items at build time with no hand-editing
- [ ] Milestones, progress and blocked-by all appear
- [ ] The build fails if the export cannot be read, rather than shipping a
      blank page
- [ ] `ROADMAP.md` stays as it is — the page is a second view, not a
      replacement

## Notes

The interesting page is not the roadmap, it is the *spikes*: items that asked a
question and recorded an answer. Those read as thinking rather than as a
backlog, and no company site has anything like them.
