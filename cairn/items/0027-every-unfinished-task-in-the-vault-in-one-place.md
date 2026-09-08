---
id: 27
title: Every unfinished task in the vault, in one place
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
claimed: 2026-09-08
depends_on:
- 56
- 57
created: 2026-09-07
updated: 2026-09-08
priority: p1
effort: m
area: vault
---

## Problem

917 task lines across the vault, spread over notes that were each written for a
different reason, and no way to see them together. A checkbox in a project note
from March is invisible unless you happen to open that note.

## Proposal

A view that collects every `- [ ]` in the vault, grouped by note, with the
heading it sits under for context. Toggling one there writes through to the file
it came from.

This is browsing rather than reading, which is why it is not in v0.5 — but it is
the largest single body of structured data in the vault and nothing looks at it.

## Acceptance criteria

- [ ] Every unfinished task in the vault, grouped by note
- [ ] The section heading a task sits under is shown with it
- [ ] Toggling a task writes through to its file and re-indexes
- [ ] Completed tasks are reachable but not in the way
- [ ] Filterable by tag, so `#status/active` narrows it

## What the vault's Obsidian config adds

Filed from counting task lines alone. Reading `.obsidian/` on 2026-09-07 found
`obsidian-tasks-plugin` installed and enabled — so this is not a use the reader
might have, it is one they already have, in another program.

It also found the shape of it. Two notes contain a ```` ```tasks ```` query
block against **917 task lines**. Almost nobody writes the queries; everybody
writes the tasks. So the thing to build is the collection and the view, not a
query language over it.

## Notes

`space` already toggles a task on the current line, so the write-through path
exists; this needs the collection and the view.

Consider whether this is a sidebar tab (alongside Notes and Tags) or an overlay
like the git panel. The sidebar already has the tab machinery.

## Recounted on 2026-09-08, and it changes the design

The 917 figure was a count of task lines. Counting checkbox state instead:

| | |
| --- | --- |
| checkboxes in the vault | 1,078 |
| **ticked** | **11** |
| open | 1,067 |
| notes containing any | 59 |

**One percent.** And the open ones are not spread evenly — 470 of the 1,067
live in six notes:

| note | open |
| --- | ---: |
| `01-projects/08-chameleon-trip/09-timeline.md` | 102 |
| `03-resources/ai-ml/ai-ml-learning-roadmap.md` | 89 |
| `01-projects/06-ai-ml-mastery/01-roadmap.md` | 84 |
| `01-projects/06-ai-ml-mastery/04-library.md` | 71 |
| `01-projects/08-chameleon-trip/10-brooklyn-prep.md` | 65 |
| `01-projects/08-chameleon-trip/08-packing.md` | 59 |

These are not a to-do list. They are **checklists inside project notes** — a
trip timeline, two learning roadmaps, a reading library, a packing list. They
were written to be a plan, and the boxes were mostly never meant to be ticked
one at a time.

So the original framing — collect every unfinished task in one place — would
produce a flat list of 1,067 rows, 44% of it from six documents, and the
reader would close it immediately. **The grouping is not a presentation
detail; it is the feature.** The view leads with the notes that hold tasks and
how many, and a checklist is opened rather than flattened into the list.

That also makes this the first consumer of the query layer rather than a
bespoke pane: it is `task:open` (0056), grouped by note, rendered in the
results pane (0057). If it cannot be expressed that way, the query layer is
missing something and this item is the thing that will find it.

The 1% completion rate is worth not designing around. It is not a problem to
solve with nudges; it is evidence that these boxes are structure, not
commitments.

## What shipped

`task:open` in the search box, grouped by note. Not a bespoke pane: it is a
query (0056) drawn by the results surface (0057), which is what the recount
above argued for — the grouping is the feature, and grouping is not specific to
tasks.

On the real vault it reads as intended: the note's name once with its count,
then its task lines with the line numbers that open them.

```
DIY Hardware Calculator  5
     9 - [ ] Architecture: logic gates vs microcontroller vs FPGA
    10 - [ ] Display type: 7-segment LED, LCD, OLED
    …
Book Idea: The Greatest Stories of Revenge in History  7
   174 - [ ] Decide on book structure
```

The header says `200 of 1067 hits` rather than `200 hits`, so a checklist of
102 cannot pass its first few rows off as the whole answer. Hits are capped per
note, so one long checklist does not crowd out every other note — a defect a
review caught in 0056 before this item was built on top of it.

The 1% completion rate is not designed around. `space` already toggles a task
on the current line, and opening a hit lands on that line, so ticking one from
here is two keystrokes — but nothing nudges, and the count is not shown as a
number to bring down. Those boxes are structure, not commitments.
