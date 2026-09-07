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

A link into the same note works too: `enter` on `[Phase 1](#phase-1)` jumps to
that heading, matching the slug the way GitHub does, and `[[Note#Heading]]`
opens the other note *at* the heading rather than at the top.

## Attachments

`![[photo.jpg]]` points at a real file. Non-markdown files are indexed by
filename and by relative path, so an embed resolves — otherwise a vault with
images lists its own pictures as notes nobody has written.

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
