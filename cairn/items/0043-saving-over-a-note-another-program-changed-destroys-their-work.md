---
id: 43
title: Saving over a note another program changed destroys their work
type: bug
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: m
area: vault
---

## What happens

trafford holds the open note in a buffer. Another program — Claude Code, an
editor, a script — writes to the same file. Pressing `ctrl-s` writes the buffer
over it. The other program's work is gone, silently, with a "saved" in the
status line.

```
   file on disk    # Note / Original line / A paragraph written by Claude Code.
   after ctrl-s    # Note / Original line / !
```

## What should happen

Notice, and ask. Never write over a change nobody has seen.

## Reproduction

1. Open a note in trafford
2. Append a line to the same file from another program
3. Type anything in trafford and press `ctrl-s`
4. The appended line is gone

## Why

`App::save` is a bare `fs::write`. Nothing compares what is on disk with what
was read.

The information is already there: `Note.modified` is a `SystemTime` and the
index holds it. What is missing is the comparison.

## Proposal

Record the file's mtime when a note is loaded. On save, stat it again. If it
moved, do not write — say so, and offer the choices worth having: overwrite,
reload and lose the local edit, or write the buffer beside it as a conflict
copy so nothing is lost while the reader decides.

The last option is what makes this safe rather than merely careful. A prompt
that offers only "overwrite" and "cancel" pushes people towards overwrite.

## Acceptance criteria

- [x] Saving a note that changed on disk since it was loaded does not write
- [x] The reader is told what happened and offered overwrite / reload / keep both
- [x] "Keep both" writes the buffer to a new file and leaves the other alone
- [x] Saving a note nobody else touched is unchanged, with no extra prompt
- [x] A note created by trafford and saved for the first time is unchanged

## Notes

This is the whole reason the question came up: the vault is meant to be edited
by an assistant in another window. The tool has to survive its own workflow.

`autocommit_secs` makes the loss recoverable from git *if* it fired between the
two writes. That is luck, not a guarantee.
