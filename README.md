# trafford

A terminal knowledge base. An Obsidian-shaped vault — plain markdown, `[[wikilinks]]`,
backlinks, tags — with a modal editor, git built into the status bar, and an
assistant that reads your notes before it answers.

```
╭ vault ─────────────────────╮╭ How linking works.md ● ────────────────────────╮╭ context ─────────────────────╮
│2 notes · 152 words         ││  4 # How linking works                         ││OUTLINE                       │
│▌How linking works          ││  5                                             ││How linking works             │
│ Welcome                    ││  6 Write `[[Note name]]` anywhere and trafford  ││                              │
│                            ││  7 resolves it against the vault.              ││LINKS OUT · 1                 │
│                            ││  8                                             ││  → Welcome                   │
│                            ││  9 Back to [[Welcome]].                        ││BACKLINKS · 1                 │
╰────────────────────────────╯╰────────────────────────────────────────────────╯╰──────────────────────────────╯
 NORMAL   ⎇ main  ●3  ↑1   2 notes                                                          66 words  9:24
```

## Why

A vault is just a folder of markdown files. Everything good about Obsidian —
links that resolve by name, backlinks that appear without being asked for,
tags, daily notes — is a way of reading that folder. None of it needs a
browser engine, and all of it belongs next to git.

## Install

```sh
cargo install --path .
```

## Use

```sh
trafford init ~/vault    # starter notes, config, and a git repo
trafford ~/vault         # open it
```

`TRAFFORD_VAULT` sets the default path, so plain `trafford` opens your vault
from anywhere.

## What's in it

**The sidebar is a tree.** Your folders are how you know what a note *is*, so
the sidebar shows them: collapsible, counted, and opened to reveal whatever
note you have in front of you.

```
▸ 00-inbox                   5
▸ 01-projects               69
▾ 03-resources              22
  ▾ ai-ml                    9
  │   AI/ML Learning Roadmap
  │ ▌ Karpathy's LLM Wiki
  ▸ calculators              6
  Dashboard
```

`l` opens a folder, then steps into it, then opens a note — hold it and you
walk down to the first note. `h` closes a folder, or jumps to the one holding
it; hold it and you walk back out. `enter` and `space` toggle a folder without
moving. `E` and `C` expand and collapse everything, and `.` jumps back to the
note you have open. `t` swaps the tree for the tag list; picking a tag filters
the tree down to the notes carrying it.

**Notes.** Markdown files in a folder, nested however you like. Frontmatter
`title:` and `tags:` are read; so are inline `#tags`.

**Links.** `[[Note]]`, `[[folder/Note]]`, `[[Note#Heading]]`, `[[Note|shown text]]`.
They resolve by exact path first, then by filename — the same rules Obsidian
uses. `enter` follows the link under the cursor; if it points at nothing yet,
you get a prompt to write it. Renaming a note rewrites every link that pointed
at it.

**Backlinks.** Always in the right-hand pane, with the line each one came from.
`ctrl-t` turns them into a jump list. Links that point at notes you have not
written yet are listed as "unwritten" — the vault's growing edge.

**Editing.** Vim motions and operators: `hjkl w b 0 ^ $ gg G`, `x dd dw D cc cw C`,
`yy p P`, `v V`, `>> <<`, `u` / `ctrl-r`, counts like `2dd` and `10G`. Enter
continues list markers and increments ordered lists; `space` toggles the task
on the current line. `f1` lists every key.

**Git.** The branch, dirty count, and ahead/behind are in the status bar.
`ctrl-g` opens a panel to stage, unstage, diff, commit, push, and pull —
`--rebase --autostash`, which is what you want for a vault. `autocommit_secs`
in the config commits the vault on idle if you would rather not think about it.

**The assistant.** `ctrl-j`. The open note is always in context; other notes
are retrieved by keyword overlap and named in the prompt, so answers cite
`[[notes]]` you can jump straight to. `ctrl-y` inserts the last answer at the
cursor. Needs `ANTHROPIC_API_KEY`.

## Mouse

All of it is clickable. Click a pane to focus it, a folder to fold it, a note
to open it. Click an outline entry to jump to that heading, or a backlink to
open that note at the line mentioning this one. The wheel scrolls whatever is
under the pointer, dragging in the editor selects lines for `y`, `d` and
`c`, and clicking outside an overlay dismisses it.

**Right-click** gives you a menu of what can be done to whatever is under the
pointer, rather than one fixed list. Every surface that has actions answers it:
the tree, the editor, the context pane, the git panel, the tag list, search
results, the quick switcher and the assistant. The git panel is the one to
know — its actions are single letters otherwise.

An action that exists but cannot run right now is shown greyed with the reason
(`Delete… — the only note`), so the menu's shape does not change under you. Keys
are shown beside the entries that have them.

Links follow Obsidian: in source mode a click puts the cursor in the link and
`ctrl-click` follows it, so a link is still editable; in preview a plain click
follows.

Mouse capture takes your terminal's own text selection away — hold `shift` to
get it back, which every terminal worth using supports.

## Keys

`ctrl-p` open · `ctrl-k` palette · `ctrl-f` search · `ctrl-n` new · `ctrl-l` insert link ·
`ctrl-t` backlinks · `ctrl-e` preview · `ctrl-g` git · `ctrl-j` assistant · `f1` all of them.

## Themes

Ships with **Gotham** (the default), Night, Paper, and Mono. A theme is a set
of roles rather than a list of colours, so it can be swapped without touching
anything else:

```toml
theme = "gotham"
```

Three other things work in that field, in this order:

```toml
theme = "my-theme"                            # <vault>/.trafford/themes/my-theme.toml
theme = "~/.config/ghostty/themes/gotham"     # a file, anywhere
theme = "Catppuccin Mocha"                    # a Ghostty theme, by name
```

That last one is the useful one: **trafford reads Ghostty theme files
directly**, so whatever your terminal is already wearing, trafford can wear
too — no transcribing, and every theme Ghostty ships works. Names match
loosely, so `catppuccin-mocha` finds `Catppuccin Mocha`.

`ctrl-k` → *Change theme* lists everything it can find and previews each one
as you move through the list; `esc` puts back the one you had.

A theme file states as little as it likes. Anything left out is derived by
mixing the background and the text, so five lines is a coherent theme:

```toml
name = "Squid"
background = "#0d1117"
text       = "#c9d1d9"
accent     = "#d29922"
link       = "#58a6ff"
```

The full set of roles is in `themes/gotham.toml`, which is an ordinary theme
file — the built-ins are compiled in and parsed by the same code you would use,
so they cannot drift from the format.

## Config

`<vault>/.trafford/config.toml`, so it travels with the notes:

```toml
theme = "gotham"         # a built-in, a theme file, or a Ghostty theme
new_note_dir = ""        # where ctrl-n puts notes
sidebar_width = 32       # columns; a deep vault wants more
daily_note_dir = "journal"
daily_note_format = "%Y-%m-%d"
model = "claude-sonnet-5"
context_notes = 6        # notes retrieved per question
autocommit_secs = 0      # commit after N seconds idle; 0 disables
sidebar = true
context_pane = true
```

## Development

```sh
cargo test          # unit tests live beside the code they cover
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check

python3 tools/probe.py sizes /tmp/vault   # drive the real TUI in a pty
```

Where it is going: [ROADMAP.md](ROADMAP.md), generated from the items in
`cairn/items`. Working on this with an agent? Read [CLAUDE.md](CLAUDE.md) for
the architecture and
[docs/version-control-with-agents.md](docs/version-control-with-agents.md) for
how changes get committed and reviewed.

## License

MIT.
