---
id: 49
title: Expand a template when a note is made from one
type: feature
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: vault
---

## Problem

The vault has `_templates/` with five templates — daily note, weekly review,
inbox note, new project, resource note — and they are wired into how the reader
actually works: the periodic-notes config names `_templates/daily-note.md` for
every new day.

trafford knows enough about templates to keep them out of its way. `is_placeholder`
spots `<% %>` so a template is listed by filename rather than by
`<% tp.file.title %>`. It cannot do the one thing templates are for.

## Proposal

When a note is created from a template, expand it. Templater's whole language is
not the target; the handful of expressions these templates actually use is:

```
<% tp.date.now("YYYY-MM-DD") %>            today, formatted
<% tp.date.now("YYYY-MM-DD", -1) %>        offset in days
<% tp.file.title %>                        the new note's name
```

That covers all five templates in the vault. Anything not understood is left
exactly as written rather than blanked, so a template using more than this
degrades to what it is now instead of losing text.

## Acceptance criteria

- [x] Creating a note from a template expands the expressions above
- [x] An expression that is not understood is left as written, not emptied
- [x] The template itself is never modified
- [x] `_templates/` is still listed by filename rather than by its placeholders
- [x] A vault with no templates is unchanged

## Notes

Templater's date tokens are moment.js, not strftime. `YYYY-MM-DD` and `dddd,
MMMM D, YYYY` both appear in this vault, so the mapping has to cover at least
those. `chrono` is already a dependency.
