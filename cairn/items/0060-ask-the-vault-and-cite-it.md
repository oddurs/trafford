---
id: 60
title: Ask the vault, and cite it
type: feature
status: planned
milestone: v0.7
depends_on:
- 56
- 58
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: l
area: assistant
---

## Problem

`ask-note` and `ask-related` answer about the note that is open. The question a
reader actually arrives with is about the vault: *where did I write about this*,
*what did I conclude*, *do these two projects overlap*. Answering that today
means remembering which note it was, which is the thing the vault was supposed
to do for you.

## Proposal

Retrieval over the query layer (0056), an answer streamed through the existing
client, and **citations that navigate**.

### Retrieval is over sections, not notes

Notes here are large — p50 803 words, p90 2,961, max 8,576. Feeding whole notes
to the model wastes most of the context on prose nobody asked about and buries
the passage that matters. Retrieval is over headings and their sections,
through `ui::fold::headings`, which is already the single heading scanner for
the outline, the folds and the reading view. A second idea of a document's
structure diverges exactly the way a second idea of the layout does.

### The answer cites, and the citations are real

Every claim carries a `note:line`, and every one of them opens through
`App::jump_to`.

**A citation that does not resolve is a bug, not a formatting preference.** An
assistant that names a note which does not exist is worse than one that says it
does not know, because the vault is the one thing in the room that was supposed
to be true. Citations are checked against the index before the answer is drawn,
and one that does not resolve is reported as such.

### It says when it has nothing

A question the vault cannot answer gets "nothing in the vault covers this",
not a fluent paragraph assembled from the nearest three notes.

## Acceptance criteria

- [ ] A question answerable only by combining two notes cites both
- [ ] Every citation opens the right note at the right line, revealing folds
- [ ] A citation naming a note that does not exist is caught before drawing
- [ ] A question with no support in the vault is answered as unsupported
- [ ] Retrieval never sends the whole vault; what was sent is inspectable
