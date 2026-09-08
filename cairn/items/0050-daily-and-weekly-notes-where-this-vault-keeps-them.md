---
id: 50
title: Daily and weekly notes where this vault keeps them
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: vault
---

## Problem

trafford has a daily-note command. It puts the note in `journal/` with the name
`%Y-%m-%d` and nothing in it.

This vault keeps daily notes in `00-inbox/daily`, named `YYYY-MM-DD`, from
`_templates/daily-note.md` — which sets the frontmatter, tags it `type/log`,
titles it "Tuesday, March 17, 2026" and links to yesterday and tomorrow. It
keeps weekly notes too, in `00-inbox/weekly`, named `YYYY-[W]ww`, from
`_templates/weekly-review.md`.

So the command exists and would put the note in the wrong place, empty, beside
a folder of correctly-made ones.

## Proposal

Take the folder, the format and the template from wherever the vault says —
`.obsidian/plugins/periodic-notes/data.json` when it is there, `config.toml`
otherwise — and add the weekly note, which trafford has no equivalent of.

## Acceptance criteria

- [ ] Today's note lands where this vault keeps them, from its template
- [ ] A weekly note command, the same way
- [ ] Opening one that already exists opens it rather than overwriting it
- [ ] A vault with no periodic-notes config falls back to `config.toml`
- [ ] The `<< yesterday | tomorrow >>` links the template writes resolve

## Notes

Blocked on #0049: without expansion the template is worse than nothing, since
every new day would start with `<% tp.date.now(...) %>` across four lines.

The prev/next links are the reason the format matters exactly. Get the format
wrong and every day links to a note that does not exist.
