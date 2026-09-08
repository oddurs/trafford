---
order: 80
section: Reference
description: config.toml lives inside the vault, so your settings travel with your notes — and Obsidian's own settings are read when you have not written one.
---

# Configuration

`<vault>/.trafford/config.toml`, so it travels with the notes rather than
living in a dotfile on one machine. Every key is optional; a missing file is
the same as an empty one.

```toml
theme = "gotham"          # a built-in, a theme file, or a Ghostty theme

new_note_dir = ""         # where ctrl-n puts notes; "" is the vault root
new_note_template = ""    # a note to start from; "" is a heading and nothing else
trash = "local"           # "local" moves to .trash/, "none" unlinks

daily_note_dir = "journal"
daily_note_format = "%Y-%m-%d"
daily_note_template = ""
weekly_note_dir = "journal"
weekly_note_format = "%Y-W%V"
weekly_note_template = ""

wrap = true               # fold long lines instead of scrolling sideways
wrap_column = 0           # where to fold; 0 is the pane width
reading_measure = 72      # columns of prose in the reading view; 0 is the pane
reading_focus = true      # reading hides the side panes and the gutter

sidebar = true
sidebar_width = 32        # columns; a deep vault wants more
context_pane = true

model = "claude-sonnet-5"
context_notes = 6         # notes retrieved per question
autocommit_secs = 0       # commit after N seconds idle; 0 disables
```

## Obsidian's settings are read too

If you have not written a key, trafford looks in the vault's own
`.obsidian/` for an answer before falling back to the default. Your
`config.toml` always wins — this only fills in what you left alone.

| Read from | Fills in |
| --- | --- |
| `app.json` → `newFileLocation` / `newFileFolderPath` | `new_note_dir` |
| `app.json` → `trashOption` | `trash` |
| `app.json` → `readableLineLength` | `reading_measure` |
| `plugins/periodic-notes/data.json` | the daily and weekly keys |

Only settings trafford has an answer for are read. Reading a key it then
ignored would suggest a promise it is not keeping.

Two details worth knowing. Obsidian's `"system"` trash — the desktop bin —
is treated as `local`, because a note recoverable inside the vault is closer
to what was asked for than one that is gone. And periodic-notes writes
moment.js formats, which are translated to strftime by the same table the
templates use, so `YYYY-[W]ww` comes out as the week note's real name.

## Templates

`new_note_template`, `daily_note_template` and `weekly_note_template` each
name a note in the vault to start from. Enough of
[Templater](https://github.com/SilentVoid13/Templater)'s expressions are
understood to make the templates a real vault already has work:

```markdown
<% tp.file.title %>                        the new note's title
<% tp.date.now("YYYY-MM-DD") %>            today, in a moment.js format
<% tp.date.now("YYYY-MM-DD", -1) %>        n days either side of today
<% tp.file.cursor(0) %>                    where the cursor lands
```

Anything else is left exactly as written, so a template using more than this
degrades to what it did before rather than losing text.

## Environment

| Variable | What it does |
| --- | --- |
| `TRAFFORD_VAULT` | default vault path, so bare `trafford` works anywhere |
| `ANTHROPIC_API_KEY` | enables the assistant pane |
| `EDITOR` | which editor *Open in $EDITOR* hands the terminal to |

## Themes

`theme` takes four kinds of value. [Themes](themes.md) covers them.
