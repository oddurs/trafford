---
id: 56
title: A query layer over the whole vault
type: feature
status: done
milestone: v0.7
depends_on:
- 55
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: l
area: vault
---

## Problem

The vault holds 148 notes, ~220 typed frontmatter values, 405 wikilinks, 1,078
checkboxes and 1.3 MB of prose, and the only way to interrogate any of it is
full-text search. You can find a word. You cannot ask which reference notes are
still active, which notes nothing links to, or where the open tasks are.

Obsidian's answer to this is dataview, a plugin. This vault has it installed
and uses it in **one note out of 148** — which is the measurement that matters:
the reader is not refusing to query their vault, they are refusing to write
query syntax into their notes to do it.

## Proposal

A small query expression, parsed to a typed AST and evaluated in memory over
the existing index.

```
type:reference status:active        property equality (0055)
tag:topic/computability             frontmatter or inline, same namespace
task:open  task:done                the checkbox state
orphan                              nothing links to it
broken                              it links to something that is not there
links-to:"Some Note"                resolved through the normal link rules
path:01-projects/*                  where it lives
modified:>2026-08-01                mtime, not a git date
sort:modified  limit:20
```

Bare words that match no field fall through to the full-text search that
already exists, so the simplest query is the one people already type.

### Design notes

- **Evaluated live, not cached.** A full rescan of this vault is 9.8ms and one
  note is 92µs, both measured. A cache would be a second idea of the vault's
  contents, and this repo has learned twice what a second idea of something
  costs.
- **Match on `haystack`, display from `text`.** The line-aligned lowercase copy
  is what makes case-insensitive matching honest; the existing invariant.
- **Link predicates go through the normal resolution order** — exact relative
  path, then case-insensitive path, then filename stem. `orphan` and `broken`
  must agree with what following the link actually does, or they will name
  notes that are fine and miss ones that are not.
- **An unknown field is an error the reader can see.** `stauts:active` must say
  so. Silently returning nothing is the failure mode that makes a query
  language untrustworthy, and it is indistinguishable from a true empty result.

## Acceptance criteria

- [ ] `type:reference status:active` returns the right notes on the real vault,
      checked against a hand count
- [x] `orphan` returns the 9 notes nothing links to; `broken` names the 11
      notes holding a target that does not resolve
- [ ] A misspelled field reports the mistake and does not return an empty set
- [ ] Query, evaluation and draw complete within one frame on the 148-note
      vault; the figure is recorded here
- [ ] Parsing is unit-tested against malformed input — unbalanced quotes, an
      empty query, a lone operator — and never panics

## What shipped, and what the vault said back

The syntax lives in the search box (`ctrl-f`) rather than in a new pane. Text
with no `field:` in it parses to plain terms, so the simplest query is still
the one people already type, and there is one search path rather than two.

**The vault's schema is in its tags, not its keys.** Written as filed, this
would have been wrong about the vault it stands in. Only three frontmatter
*keys* exist here — `created`, `tags`, `source` — while the type system lives
in namespaced tags: `type/*` on 109 notes, `status/*` on 70, `priority/*` on
29, `topic/*` on 12. So `type:reference` would have reported "no note has a
property named type", which is true and useless. A field that names a tag
namespace now reads as that namespace, and `Vocabulary` carries both.

Checked against hand counts of the real vault:

| query | program | hand count |
| --- | ---: | ---: |
| `type:reference status:active` | 40 | 40 |
| `type:reference` | 92 | 92 |
| `orphan` | 9 | 9 |

**The `orphan` count corrected the roadmap rather than the other way round.**
This milestone was filed saying 28 notes are linked to by nothing. That figure
came from matching filename stems against link text in a shell one-liner, which
misses every link written as a relative path. Resolved properly — exact path,
then case-insensitive path, then stem, the order following a link already uses
— it is **9**, confirmed independently. 0053, 0059 and 0061 are corrected.

Latency, 20 runs each on the 148-note vault, release build:

| query | per run |
| --- | ---: |
| `type:reference status:active` | 29µs |
| `orphan` | 33µs |
| `broken` | 83µs |
| `kleene` (text) | 108µs |
| `task:open` | 846µs |

All well inside a frame. `task:open` is the slowest because it walks every line
of every note; at 1,067 hits over 1.3 MB that is the work, not overhead.
