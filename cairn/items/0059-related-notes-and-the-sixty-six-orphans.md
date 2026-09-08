---
id: 59
title: Related notes, and the sixty-six orphans
type: feature
status: planned
milestone: v0.7
depends_on:
- 54
- 58
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: m
area: assistant
---

## Problem

66 of 148 notes contain no `[[link]]` at all. 9 are linked to by nothing, and
13 distinct link targets name notes that were never written — links typed as
intentions and left as dead ends.

This is not a vault that failed. It is what building a graph by hand looks like
after a year: the notes get written, the connecting does not, and the part of
the tool that depends on the connecting quietly stops working.

## Proposal

A related-notes list for the open note, ranked by whatever 0058 settles on, and
one keystroke to accept a suggestion.

**Accepting writes a real `[[wikilink]]` into the note.** This is the whole
point and it is worth being explicit about, because the easy version — keeping
the relationship in the sidecar — produces a vault that is only well-connected
inside trafford. A suggestion has to become something Obsidian, `git diff` and
the next reader can all see, or it is not a link, it is a private opinion.

- The write goes through `App::save`, so it inherits the conflict guard: a note
  changed underneath is not overwritten without asking.
- It lands in the buffer first, so it is undoable like anything typed.
- A **declined** suggestion is remembered in the sidecar (0054), not in the
  note. Declining is trafford's opinion about the vault; it is not something
  the author wrote, and it must not survive into a file Obsidian reads.

## Acceptance criteria

- [ ] Every one of the 66 orphans gets a ranked list, or is reported as having
      no candidate — never an empty pane with no explanation
- [ ] An accepted suggestion writes a link that resolves under the normal
      resolution order
- [ ] Accepting into a note that changed on disk asks, and offers the same
      three answers the save guard already offers
- [ ] A declined suggestion does not reappear, and leaves the markdown untouched
- [ ] `broken` (0056) can list the dead link targets, so the intentions already
      typed are visible alongside the suggested ones
