---
id: 62
title: A note should open with the note
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
depends_on:
- 55
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: chrome
---

## Problem

In the reading view a note currently opens like this:

```
#type/reference · #topic/computability
created 2026-09-07T00:00:00Z

▾ Computability Preliminaries
```

Three rows before the title, and the most prominent line on the screen is a
machine timestamp in full ISO form. Preview already does real work here — six
lines of YAML are collapsed to two rows — but collapsing the wrong thing
smaller still leaves it first.

Counted across the vault, the frontmatter is remarkably uniform:

| key | notes |
| --- | ---: |
| `created` | 126 |
| `tags` | 126 |
| `source` | 1 |

126 of 148 notes carry a block, p50 six lines, never more than seven. **724
lines of YAML across the vault to carry two facts per note.** One of those
facts is a timestamp nobody reads, and the other is navigation.

Neither is prose. Both are currently sitting where the prose should start.

## Proposal

Frontmatter is chrome, not content. In the reading view the note begins with
the note, and its properties move to where things *about* a note already live.

- **The reading column starts at the first real line.** No property rows, no
  timestamp, no leading blank. A note with frontmatter and a note without it
  open identically, which is the tell that this was always chrome.
- **The context pane gains a properties section**, beside the outline and the
  links out. That pane is already the answer to "what is true about this note"
  and it costs nothing to put two more facts there.
- **In reading posture the side panes are hidden**, so tags go to the status
  line, dimmed, still clickable — `#tags` are clickable wherever they are
  drawn, and that must not become an exception here.
- **Nothing is dropped.** The `source` key on its one note still appears, in
  the same place as everything else. The current properties row was built so
  that a key nobody anticipated survives; moving the row must not quietly
  reintroduce a whitelist.

The editor is untouched. It draws the source, the YAML is part of the source,
and the cursor column has to agree with the buffer — the invariant that keeps
concealment out of the editor covers this exactly.

## Why it belongs in v0.7 rather than beside it

This is the reading-side consequence of 0055. The block is drawn today because
displaying it is the only way the properties are visible at all. Once they are
indexed and first-class, the note body no longer has to carry them: the program
knows what a note's properties are without reading them off the top of the
page, and is free to put them where they belong.

Filed after a reader asked for the top of the note to go away, which it should
have done on its own — 724 lines of ceremony above 1.3 MB of writing.

## Acceptance criteria

- [ ] In reading posture, a note with frontmatter and one without begin on the
      same row with the same content
- [ ] No timestamp appears in the reading column at any width
- [ ] Tags remain reachable and clickable in both postures, and a click still
      sets the vault filter
- [ ] The `source` key, and any key invented tomorrow, is still shown somewhere
- [ ] `PreviewView.sources` still maps every drawn row to a real source line —
      removing rows must not shift the mapping that puts a click on the right
      line
- [ ] The editor still draws the frontmatter exactly as written

## What shipped

The reading column starts at the first real line, and a note with frontmatter
opens identically to one without — pinned by a test, because that identity is
the tell that this was always chrome.

Properties moved to the context pane, above the outline. Tags first as chips,
then everything else dimmed; the one `source` key appears there like any other,
and so would a key invented tomorrow.

In reading posture the side panes are hidden, so tags go to the status line —
**ahead of the editor hint**, which was the thing actually occupying that row.
In that posture the hint reads "tab or click for the file tree" while the file
tree is not drawn, so the tags displace something that was already wrong.

Two consequences worth recording:

- `ContextTarget::Tag` is new, and clicking a chip filters the vault through
  `App::filter_by_tag`. That method is also new and now the only definition of
  what filtering by a tag means — the sidebar's tags tab used to hold its own
  copy, and a tag is clickable in three places now.
- `Rendered::with_link` is gone. The frontmatter chip row was its only caller,
  and the markdown renderer builds its links a different way.
