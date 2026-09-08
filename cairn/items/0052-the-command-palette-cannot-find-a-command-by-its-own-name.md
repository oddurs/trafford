---
id: 52
title: The command palette cannot find a command by its own name
type: bug
status: done
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: chrome
effort: s
---

## What happens

## What should happen

## Reproduction

1.

Typing `weekly` in the command palette found nothing, though the weekly
note command was registered: the matcher saw only a command's label and
detail, and "weekly" is not a subsequence of "Open this week's note".

A test that drives a real `Picker` for every word of every command key
showed this was not one command but ten — `help`, `backlinks`, `ask`,
`git-panel` and the selection operators were all unreachable by name.

Fixed by scoring the key alongside the shown text. Scoring them
*separately* rather than concatenated matters: joined into one haystack a
fuzzy match can begin in the label and end in the key, and "close"
answers with `outdent-selection`.
