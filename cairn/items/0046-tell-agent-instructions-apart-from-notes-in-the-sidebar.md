---
id: 46
title: Tell agent instructions apart from notes in the sidebar
type: feature
status: done
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: tree
---

## Problem

`CLAUDE.md` and `AGENTS.md` live in the vault and index like any other
markdown, but they are not the reader's writing — they are instructions
addressed to a machine. The sidebar shows only titles, and a title cannot say
that.

Worse, it actively misleads. The `CLAUDE.md` in the vault this was built for
opens `# Notesnake - Obsidian Vault`, which is the most note-looking title in
the whole vault. Put an `AGENTS.md` beside it and the tree shows two rows with
identical names:

```
  ▌ Notesnake - Obsidian Vault
    Notesnake - Obsidian Vault
```

## Proposal

Name them by their file, and let them recede.

```
  ▌ Dashboard
  · AGENTS.md
  · CLAUDE.md
```

Naming by filename is the rule a template already gets — an H1 that does not
name the file is not a title. The colour is `muted`, which the theme defines as
"secondary text that still has to be read": 6.2:1 against the ground where an
ordinary note is 11.3:1, so it recedes without becoming hard to read. The marker
takes the column that is otherwise blank and shares the title's colour, so the
two read as one signal.

Being open still wins. A file you are looking at is the file you are looking at.

## Acceptance criteria

- [x] Agent instruction files are named by their file rather than their heading
- [x] They are drawn dimmer than an ordinary note, and still legible
- [x] A marker distinguishes them in the column that is otherwise blank
- [x] An ordinary note is unchanged
- [x] A note merely *called* something similar is not caught
- [x] The open file still shows as the open one

## Notes

Matched on the filename at any depth: a nested `CLAUDE.md` applies to its own
directory and is just as much instructions.

The switcher still shows their heading. It also shows the path beside it, so the
confusion is smaller there — worth doing for consistency, not urgent.
