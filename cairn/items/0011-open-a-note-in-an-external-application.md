---
id: 11
title: Open a note in an external application
type: feature
status: backlog
milestone: later
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: mouse
effort: s
---

## Problem

A vault is not only read here. Obsidian renders things trafford does not,
`$EDITOR` may be configured the way you want, and a file manager is sometimes
the fastest way to deal with an attachment.

## Proposal

Menu entries that hand the file to something else:

- **Open in Obsidian** — `obsidian://open?vault=…&file=…`, only when a `.obsidian/` is present
- **Open in $EDITOR** — suspend, run it, restore the terminal on return
- **Reveal in Finder**

## Acceptance criteria

- [ ] Entries appear only when the target is plausible, rather than failing when chosen
- [ ] Suspending for `$EDITOR` restores raw mode, the alternate screen and mouse reporting
- [ ] The vault is re-indexed on return, since the file may have changed

## Notes

The `$EDITOR` one is the risky one: the terminal has raw mode, the alternate
screen and three mouse modes enabled, and every one has to be given back and
taken again. Getting it wrong leaves the user's shell unusable after quitting,
which is the worst failure this program could have.
