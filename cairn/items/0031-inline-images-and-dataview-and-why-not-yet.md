---
id: 31
title: Inline images and dataview, and why not yet
type: docs
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
claimed: 2026-09-08
depends_on:
- 58
created: 2026-09-07
updated: 2026-09-08
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

## And what `.obsidian/` added

Reading the vault's own Obsidian configuration on 2026-09-07 found four more
features that are *enabled* and have nothing behind them. Enabled is not used,
and the vault is where the difference shows.

| Feature | Enabled | In the vault |
| --- | --- | ---: |
| Bookmarks | yes | 0 saved |
| Tasks plugin queries | yes | 2 notes |
| Graph view | yes | — |
| Canvas, Sync, Publish | **no** | — |

Bookmarks is the clearest: a core plugin, switched on, with an empty
`bookmarks.json`. There is nothing to build for.

Tasks queries are the interesting one, because they point the wrong way. Two
notes contain a ```` ```tasks ```` block, and the vault has **917 task lines**.
The tasks matter enormously and the queries over them do not — which is #0027,
not this.

Graph view stays refused on judgement rather than on counting: it demos well,
it navigates badly, and a terminal is the worst place to try it.

Canvas, Sync and Publish are switched off in the vault. That is an answer too.

## What this decides

None of these is worth building yet. They stay listed so the reasoning is
findable, and each should be revisited if the vault changes shape — a vault that
starts carrying screenshots is a different argument, and so is one that starts
bookmarking things.

## Notes

The general point is worth keeping separately from the specific one: this
project's roadmap is decided by measuring the vault it serves, not by matching a
feature list. The same survey that killed these two also promoted callouts from
"nice" to the best ratio on the board, and caught the table parser being wrong
about escaped pipes.

## Added 2026-09-08, while planning v0.7

**Time as an axis** — refused, and it was already half-designed when the
measurement killed it. The proposal was history as a first-class dimension:
what you thought about something in March, a note's evolution, what moved this
week. The vault's git log is **42 commits, every one dated 2026-05-01**. There
is no history to travel through. Building it would have been building for a
vault nobody has.

What the same measurement *did* justify is 0061: the working tree has drifted
from that single commit by 21 paths and stayed that way for four months. The
valuable thing about git here is not the past, it is the disagreement with the
present.

**Embeddings** — not refused, deferred to 0058, which is a spike rather than a
plan because the honest answer is not known yet. Anthropic has no embeddings
endpoint, so this means a second provider, a second key and outbound traffic in
a program that deliberately keeps peek offline. On 148 notes and 1.3 MB,
lexical scoring may simply be sufficient. That gets measured before it gets
bought.

**Dataview-style query blocks written into notes** — refused, and this one is a
design position rather than a count. The queries in 0056 are real and this
milestone is built around them; what is refused is putting the query *syntax
inside the markdown*. A note containing a query is no longer a note — it is a
program that only one reader can run, and it stops being portable to the
Obsidian and `git diff` this vault is also read with. The sidecar (0054) exists
so that derived things can be rich without the files stopping being files.

## Settled by v0.7, 2026-09-08

**Embeddings — refused, on evidence.** 0058 built the cheap answer and measured
it over the 66 notes with no outgoing link: 0 of 20 had no candidate, 18 of 20
top suggestions were in the same project, and all 20 had a same-project note in
the top three, at 30ms for twenty. Same-project is a conservative proxy — a good
cross-folder suggestion counts against it — and both notes that "missed"
suggested the AI/ML research notes from an AI product project, which a reader
would take.

So the second provider, the second key and the outbound traffic are not bought.
This reopens if grounding (0060) turns out to need paraphrase matching, which is
a different question with a different failure mode: suggestion offers five and
a wrong one costs a glance, while an answer cannot be skimmed the same way.

**Time as an axis — refused, and it was half-designed.** 42 commits, all dated
2026-05-01. What the same look found instead became 0061, which is about the
present disagreeing with the record rather than about the past.

**Dataview-style query blocks in note text — refused on design.** The queries
are real and shipped in 0056; what is refused is writing their *syntax into the
markdown*. A note containing a query is no longer a note — it is a program only
one reader can run — and it stops being portable to the Obsidian and `git diff`
this vault is also read with. `.trafford/cache/` exists so derived things can be
rich without files stopping being files.

**Inline images — still refused, and the count has not moved.** Nine embeds.

## What the milestone taught about this file

Three of v0.7's items were wrong as filed, and every correction came from
running the thing against a real vault rather than from re-reading the plan:

- **The vault's schema is in its tags, not its keys.** `type:reference` would
  have answered "no note has a property named type" — true, and useless.
- **`.trafford/` was already taken.** It holds `config.toml`, versioned with
  the notes on purpose; the sidecar would have gitignored the reader's own
  configuration. Derived state went to `.trafford/cache/`, which `init` had
  been scaffolding a `.gitignore` for all along.
- **28 orphans were 9.** The roadmap figure came from matching filename stems
  against link text, which misses every link written as a relative path.

The general point, which is worth more than any of the three: a plan measured
once is a plan measured at one moment, and the vault is the thing that knows.
