---
id: 48
title: Take the vault's settings from Obsidian's own config
type: feature
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: config
---

## Problem

trafford has its own config, and the vault already has one. Every setting below
is recorded in `.obsidian/app.json` and every one of them trafford either
guesses at or ignores:

| Obsidian says | trafford does |
| --- | --- |
| `newFileFolderPath: "00-inbox"` | `new_note_dir = ""` — the vault root |
| `attachmentFolderPath: "_assets"` | nothing |
| `trashOption: "local"` | unlinks the file |
| `readableLineLength: true` | `wrap_column = 0`, guessed at 72 |
| `showFrontmatter: false` | guessed right, by measuring the notes |
| `foldHeading: true` | guessed right, the same way |

A reader who has already told Obsidian where new notes go should not have to
tell trafford again, in a different file, in a different format.

## Proposal

Read `.obsidian/app.json` on open and let it fill in what `config.toml` has not
been asked about explicitly. The precedence is the reader's own words first:
anything set in `.trafford/config.toml` wins, and Obsidian's answer is the
default rather than an override.

This is the move trafford already makes for Ghostty themes — read the
configuration the reader already keeps, rather than asking for it again.

## Acceptance criteria

- [x] `newFileFolderPath` becomes the default for new notes
- [x] `readableLineLength: false` turns the reading measure off
- [x] Anything set in `config.toml` beats anything in `app.json`
- [x] A vault with no `.obsidian/` is unchanged
- [x] Malformed or partial JSON is ignored rather than fatal — a vault whose
      Obsidian config is half-written still opens

## What was dropped from this, and why

`attachmentFolderPath` was one of the four settings this was filed for. It is
not honoured, and the criterion has been taken out rather than ticked or left
to fail.

trafford has nothing that writes an attachment — no paste, no drop, no capture.
There is nothing to point at the folder. Reading the setting and storing it
would look like support and be a promise it is not keeping, which is worse than
not reading it. Whatever first writes an attachment should read it then, and
this is the note saying so.

## Notes

`.obsidian/` is a dot-directory, so the walker already skips it for indexing and
the watcher already ignores it. Reading it is a deliberate exception, not a
change to either rule.

Do not write to it. It belongs to Obsidian, and two programs writing one
settings file is how settings get lost.
