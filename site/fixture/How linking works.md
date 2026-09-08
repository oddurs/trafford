---
title: How linking works
tags: [reference, links]
---

# How linking works

Write `[[Note name]]` anywhere and trafford resolves it against the vault:
exact path first, then filename — the same order Obsidian uses.

| Written | Opens |
| --- | --- |
| `[[Welcome]]` | the note named Welcome |
| `[[projects/Roadmap]]` | that exact path |
| `[[Welcome#Tags]]` | Welcome, at the heading |

Renaming a note rewrites every link that pointed at it.

Back to [[Welcome]].
