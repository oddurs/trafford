---
order: 80
section: Reference
description: config.toml lives inside the vault, so your settings travel with your notes.
---

# Configuration

`<vault>/.trafford/config.toml`, so it travels with the notes rather than
living in a dotfile on one machine.

```toml
theme = "gotham"         # a built-in, a theme file, or a Ghostty theme
new_note_dir = ""        # where ctrl-n puts notes
sidebar_width = 32       # columns; a deep vault wants more
daily_note_dir = "journal"
daily_note_format = "%Y-%m-%d"
model = "claude-sonnet-5"
context_notes = 6        # notes retrieved per question
autocommit_secs = 0      # commit after N seconds idle; 0 disables
wrap = true              # fold long lines instead of scrolling sideways
wrap_column = 0          # where to fold; 0 is the pane width
sidebar = true
context_pane = true
```

Every key is optional. A missing file is the same as an empty one.

## Environment

| Variable | What it does |
| --- | --- |
| `TRAFFORD_VAULT` | default vault path, so bare `trafford` works anywhere |
| `ANTHROPIC_API_KEY` | enables the assistant pane |
| `EDITOR` | which editor *Open in $EDITOR* hands the terminal to |

## Themes

`theme` takes four kinds of value. [Themes](themes.md) covers them.
