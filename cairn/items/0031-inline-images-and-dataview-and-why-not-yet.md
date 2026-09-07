---
id: 31
title: Inline images and dataview, and why not yet
type: docs
status: backlog
milestone: later
created: 2026-09-07
updated: 2026-09-07
priority: p3
effort: s
area: docs
---

## Problem

Inline images and dataview are the two Obsidian features most obviously missing
from trafford, and both look like they should be next. Recording here why they
are not, so the question is answered once rather than re-argued.

## What the vault actually contains

Measured against notesnake on 2026-09-07, 129 notes and 176,000 words:

| Feature | Occurrences | Notes affected |
| --- | ---: | ---: |
| `![[embed]]` | 9 | a handful |
| ```` ```dataview ```` | 4 | 1 |
| Callouts | 269 | most |
| Tables | 531 | 95 |
| Task lines | 917 | many |

Images were the tempting one: the terminal in use is Ghostty, which supports the
Kitty graphics protocol, so it is not even hard. Nine embeds in the whole vault.

Dataview was the ambitious one: the Dashboard note has four query blocks that do
nothing here, and the vault index already holds paths, tags and frontmatter, so
a useful subset is reachable. Four blocks in one note.

## What this decides

Neither is worth building yet. Both stay listed so the reasoning is findable,
and both should be revisited if the vault changes shape — a vault that starts
carrying screenshots is a different argument.

## Notes

The general point is worth keeping separately from the specific one: this
project's roadmap is decided by measuring the vault it serves, not by matching a
feature list. The same survey that killed these two also promoted callouts from
"nice" to the best ratio on the board, and caught the table parser being wrong
about escaped pipes.
