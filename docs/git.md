---
order: 60
section: Using it
description: Branch, dirty count and ahead/behind in the status bar; stage, diff, commit, push and pull from a panel.
---

# Git

A vault is a git repository, and `trafford init` makes one. The status bar
carries the branch, the dirty count, and how far ahead or behind the remote
you are.

## The panel

`ctrl-g` opens it. From there: stage, unstage, diff, commit, push, pull.

Pull is `--rebase --autostash`, which is what you want for a vault — a merge
commit between two machines editing the same notes is noise, and the autostash
covers the note you were halfway through.

## Autocommit

```toml
autocommit_secs = 300
```

Commits the vault after that many seconds of idle, if anything changed. `0`
disables it. Useful if you would rather not think about it; unhelpful if you
review your own history.

## The history of one note

*Git: history of this note* in the palette lists the commits that touched the
open file — subject, author and when — rather than the whole vault's log. A
note usually has a handful of them, which is short enough to read and long
enough to answer what changed here and when.

## How it works

By shelling out to `git`, and parsing porcelain output. No linked library, no
second implementation of the object store — which means your hooks, your
config, your credential helper and your signing key all work, because it is
your git.

## Agents and history

If something other than you is writing into the vault, the review problem
changes shape. [Version control with agents](version-control-with-agents.md)
covers what to do about it.

