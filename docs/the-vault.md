---
order: 20
section: Start
description: How notes, links, backlinks and tags resolve — the rules a real vault turned out to need.
---

# The vault

A vault is a folder of markdown files, nested however you like. trafford
indexes it on open and keeps the index current as you write.

## Notes

Any `.md` file is a note. Its title is the frontmatter `title:`, or the first
H1, or the filename — in that order.

```markdown
---
title: How linking works
tags: [reference, links]
---

# How linking works
```

Frontmatter `tags:` is read in both forms — `[a, b]` and a `- item` list — and
so are inline `#tags` anywhere in the body. Nested tags like `type/reference`
are ordinary tags.

> [!warning] A tag needs a non-numeric character
> Without that rule, `#1` in "their #1 barrier" and `#333` in "Lex Fridman
> #333" become tags, and a real vault's tag list turns out to be a third
> prose. Obsidian has the same rule.

## Links

```markdown
[[Note]]
[[folder/Note]]
[[Note#Heading]]
[[Note|shown text]]
```

Resolution goes in one fixed order:

1. exact relative path
2. case-insensitive path
3. filename stem

Changing that order changes which note a link opens, so it does not change. It
is [Vault::resolve_target](api/trafford/vault/index/struct.Vault.html#method.resolve_target)
in the source, and the website you are reading resolves its own links through
the same function.

`enter` follows the link under the cursor. If it points at nothing yet, you
get a prompt to write that note — which is how a vault grows. Renaming a note
rewrites every link that pointed at it.

`K` peeks instead: the first lines of the note the link points at, over the
one you are reading, without moving you. It is the answer to *is this the note
I meant* asked often enough that following and coming back was the cost.

A link into the same note works too: `enter` on `[Phase 1](#phase-1)` jumps to
that heading, matching the slug the way GitHub does, and `[[Note#Heading]]`
opens the other note *at* the heading rather than at the top.

## Attachments

`![[photo.jpg]]` points at a real file. Non-markdown files are indexed by
filename and by relative path, so an embed resolves — otherwise a vault with
images lists its own pictures as notes nobody has written.

## Daily and weekly notes

*Open today's daily note* in the palette writes `journal/2026-09-08.md` if it
is not there yet and opens it if it is; *Open this week's note* does the same
for `journal/2026-W37.md`. The folder and the date format are
[configuration](configuration.md), and if you already have Obsidian's
periodic-notes plugin set up, its settings are read rather than asked for
again.

Either can start from a template — including one written for Templater, whose
`<% tp.date.now(...) %>` expressions are understood well enough that a
journal template with yesterday and tomorrow linked in its header comes out
with real links in it.

## Backlinks

Always in the right-hand pane, with the line each one came from. `ctrl-t`
turns them into a jump list.

Links pointing at notes you have not written yet are collected separately as
*unwritten*. That list is usually the most interesting thing in the pane.

## Search

`ctrl-f` searches the whole vault. Matching is case-insensitive; results show
the line the hit was on and open at it.

## Tags

`t` in the sidebar swaps the file tree for the tag list. Picking a tag filters
the tree to the notes carrying it. Tags are clickable wherever they are drawn,
including in the middle of a paragraph in the reading view.

## Changed underneath you

The vault is watched, so a note written by another program — Obsidian in
another window, an agent, a `git pull` — appears here without anything being
asked of you. The index catches up, the tree catches up, and the note you are
reading redraws.

> [!warning] Unsaved typing is never overwritten
> If the file under you changed and you have edits nobody has seen, trafford
> keeps yours and says so. The reload is what waits, not your paragraph.
