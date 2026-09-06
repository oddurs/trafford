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

## Keys

`ctrl-p` open · `ctrl-k` palette · `ctrl-f` search · `ctrl-n` new · `ctrl-l` insert link ·
`ctrl-t` backlinks · `ctrl-e` preview · `ctrl-g` git · `ctrl-j` assistant · `f1` all of them.

## Config

`<vault>/.trafford/config.toml`, so it travels with the notes:

```toml
theme = "night"          # night | paper | mono
new_note_dir = ""        # where ctrl-n puts notes
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
cargo test          # 77 unit tests, no fixtures required
cargo clippy --all-targets
cargo fmt
```

Working on this with an agent? Read [CLAUDE.md](CLAUDE.md) for the architecture
and [docs/version-control-with-agents.md](docs/version-control-with-agents.md)
for how changes get committed and reviewed.

## License

MIT.
