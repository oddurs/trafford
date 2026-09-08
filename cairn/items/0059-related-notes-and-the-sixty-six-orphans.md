---
id: 59
title: Related notes, and the sixty-six orphans
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
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

## What shipped

A `RELATED` section at the foot of the context pane, four suggestions with the
words or tags that earned them. A suggestion a reader cannot evaluate is one
they have to take on faith, so every row says why in the vault's own terms.

A click opens the note. Linking and dismissing are on the right-click menu,
which is where an action that writes into a file belongs.

**Accepting writes a real `[[wikilink]]`** at the end of the note, on its own
line — anywhere cleverer would be guessing at a structure the note may not
have. It lands in the buffer behind a single checkpoint, so one undo takes the
whole insertion back including the blank line, and it goes out through `save`
and its conflict guard like anything typed.

**Declining is remembered in the sidecar, never in the note.** Declining is
trafford's opinion about the vault, not something the author wrote, and it must
not survive into a file Obsidian reads. It is directional: turning down A→B
says nothing about B→A, because those are different questions asked while
reading different notes.

The corpus costs 15ms to build over 148 notes and 1.6ms per lookup, both
measured, so it is built once and dropped when the vault rescans — never per
draw, where 15ms is a whole frame.

One thing the first draw got wrong: the outline's height budget did not count
the new section, so it was computed and then pushed off the bottom of the pane,
where nobody would know it existed. Everything drawn below the outline has to
be in that sum.
