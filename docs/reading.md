---
order: 40
section: Using it
description: The reading view — folding, properties, callouts, tables, and what stays as written.
---

# Reading

`ctrl-e` switches between the editor and the reading view. They are not the
same view with a flag: the editor draws the file, and the reading view draws
what the file *means*.

## What is concealed

Wikilink and markdown-link syntax, `**bold**`, `*italic*`, `==highlight==`,
`` `code` `` and heading hashes.

## What is kept

`#tags` — the hash is part of the tag, not wrapping around it — list markers
and checkboxes, `>` quote markers, and everything inside a fence, where the
syntax *is* the content.

A broken link still draws in the broken colour. Concealing the brackets must
not conceal that the link goes nowhere.

## Folding

The vault this was built against has a heading every six lines of body. Nobody
reads a note like that from the top; they arrive looking for one section.

- `za` folds or unfolds the section under the cursor
- `zR` opens everything
- `zM` closes everything

A folded section shows how many lines are hidden. Folding an H2 takes its H3s
with it. The marker is clickable.

## Properties

Frontmatter is not seven lines of YAML in the reading view. It becomes at most
two rows: the tags, then everything else dimmed. Nothing is dropped — a key
nobody anticipated still appears — because a vault is someone's real notes and
the schema is whatever they typed.

## Callouts

```markdown
> [!warning] Mind this
> Body text.
```

> [!warning] Mind this
> Body text.

A kind nobody anticipated still draws, labelled with what you wrote. Refusing
would leave `[!quote]` sitting in the text as characters.

## Tables

| Column | What it holds |
| --- | --- |
| Measured | in display columns, not characters |
| Escaped | `\|` inside a cell is a pipe, not a new cell |

Measuring in display columns is what a table of CJK catches — and it catches
it silently, by drawing rules that do not line up.

## The section you are in

The heading chain of wherever you are stays on screen above the text, so with
a heading every six lines you always know which section you are reading.
