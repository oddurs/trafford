---
id: 58
title: Is lexical similarity enough, or does this need embeddings?
type: spike
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
depends_on:
- 56
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: s
area: assistant
---

## Question

Suggesting connections between notes (0059) and retrieving passages to ground
an answer (0060) both need a similarity signal. Is term overlap plus shared
frontmatter properties good enough on a vault this size, or does this need real
embeddings?

## Why it has to be answered before the work

Because the two answers cost wildly different amounts, and the expensive one is
irreversible in the ways that matter.

Anthropic has no embeddings endpoint. Embeddings therefore mean a **second
provider** — a second API key for the reader to obtain, a second dependency, a
second failure mode, and outbound network traffic in a program whose peek
deliberately "never touches the network" so that reading a note does not become
an outbound request. That is a real cost. It should be paid on evidence, not on
the assumption that similarity means vectors.

The vault is also small: 148 notes, 1.3 MB, p50 803 words. It is entirely
possible that lexical scoring is not a degraded approximation here but simply
sufficient.

## Options

**(a) Offline and lexical.** BM25-style term overlap, weighted by shared
frontmatter properties (0055) and co-citation — two notes linked from the same
third note. Zero dependencies, zero cost, works on a plane.

**(b) Embeddings from a second provider.** Better recall on paraphrase, at the
cost above. 1.3 MB is cheap to embed once and cheap to keep current in the
sidecar (0054).

**(c) The model itself, batched.** Ask for the connections directly over the
whole vault. Most expensive per run, best at *explaining* a connection, worst
as something that runs on every keystroke.

These are not exclusive. The likely shape is (a) for ranking and (c) for saying
*why* two notes belong together — but that is a hypothesis, and this spike
exists to stop it being adopted as a conclusion.

## What would settle it

Build (a). Run it over the 66 notes that have no outgoing link. For 20 of them,
read the top five suggestions and count how many a reader would actually
accept. Record the count here.

If (a) produces a suggestion worth accepting for most of those 20, it ships and
(b) is not built. If it does not, this item reopens with a measurement behind
it instead of an intuition.

Precedent: inline images and dataview were both dropped this way — nine embeds
and one note respectively — and both were things instinct said to build.

## Answer

<!-- Filled in when the spike closes. A spike that closes with no answer
     recorded was a waste of the time it took. -->

## Answer

**Lexical is enough. Embeddings are not bought.**

Built option (a): BM25 over the prose, plus two signals a vault has that a
corpus of documents does not — notes sharing a tag, and notes cited together by
a third. Frontmatter and fenced code are excluded from the prose, or every note
recording a `created` date would look alike and any two Rust snippets would
share `fn`, `let` and `impl`.

Run over the 66 notes with no outgoing link, taking the first 20:

| | |
| --- | ---: |
| orphans with no candidate at all | **0** of 20 |
| top suggestion in the same project | **18** of 20 |
| a same-project note in the top three | **20** of 20 |
| 20 notes scored | **30 ms** |

Same-project is a deliberately conservative proxy for "a reader would accept
it": a good cross-folder suggestion counts against the score. Both notes that
missed by that measure — `04-valerie/03-regression-detection` and
`07-regulated-industries`, each suggesting `ai-ml/ai-ml-learning-roadmap` — are
suggestions a reader would probably take, since the project is an AI product
and the target is AI research. So 18/20 is a floor, not a ceiling.

The judge here is me, not the reader, which is the honest limit of this
measurement.

### The first run was wrong in a way worth recording

A flat weight per shared tag put `#status/active` and `#type/reference` in the
reason for nearly every pair. They are on 62 and 92 of 148 notes: they say
"this is a vault", not "these two belong together". Tags are now scored by the
same IDF as words, and a tag worn by most of the vault is not named as a reason
at all. Rarity matters as much for a hand-applied tag as for a word.

### What this does not settle

Retrieval for 0060 is a different question with a different failure mode. This
measured *suggestion*, where the reader picks from a list of five and a wrong
entry costs a glance. An answer assembled from retrieved passages cannot be
skimmed the same way, and if grounding turns out to need paraphrase matching,
this reopens with that evidence rather than with an intuition.

The harness stays in `src/vault/similar.rs` as an `#[ignore]`d test, so the
question can be asked again when the vault changes shape.
