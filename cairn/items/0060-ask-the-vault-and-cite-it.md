---
id: 60
title: Ask the vault, and cite it
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
claimed: 2026-09-08
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

## What shipped

**Retrieval is over sections.** `Vault::relevant_sections` splits each note at
its headings — through `ui::fold::headings`, the one heading scanner the
outline, the folds and the reading view already share — and scores each section
on its own. A heading matching the question counts for three times what a body
term does, since a heading is what the author said the section is about.

`Vault::relevant`, which returned whole notes, is gone. Two ideas of what is
relevant would be two things to keep in step, and on notes with p90 2,961 words
the whole-note version spent most of the context on prose nobody asked about.

**Citations are checked, not trusted.** When an answer finishes streaming,
every `[[link]]` in it is resolved against the index, and any that names a note
the vault does not have is reported. An assistant naming a note that does not
exist is worse than one saying it does not know, because the vault is the one
thing in the room that was supposed to be true — and a reader cannot tell the
two apart by looking.

**A citation opens what was cited.** `[[Note#Heading]]` already jumped to the
heading; a citation without one now lands on the line the retrieved passage
started on, because the top of an 8,000-word note is not where the answer came
from.

**It says when it has nothing.** The system prompt asks for "nothing in the
vault covers this" over a fluent paragraph assembled from the nearest three
notes, and instructs it to cite only passages it was given. Both are pinned by
tests against the prompt text, which is the only part of a model's behaviour
this program can actually assert on.

### What is not claimed

The end-to-end behaviour needs an API key and a live model, so what is tested
here is retrieval, citation resolution and the prompt — not that the model
obeys it. Retrieval returning the right section, a citation opening the right
line, and a fabricated citation being caught are all covered; "the answer is
good" is not something this repo can assert.
