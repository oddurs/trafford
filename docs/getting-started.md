---
order: 10
section: Start
description: Install trafford, create a vault, and open it.
---

# Getting started

## Install

trafford is one binary with no runtime dependencies.

```sh
cargo install --git https://github.com/oddurs/trafford
```

From a clone:

```sh
git clone https://github.com/oddurs/trafford
cd trafford
cargo install --path trafford
```

The repository is a Cargo workspace: `trafford/` is the application and
`site/` builds this website. Installing from the workspace root will not work
— name the member, as above.

## Make a vault

```sh
trafford init ~/vault
```

That writes a couple of starter notes, a config file under
`<vault>/.trafford/`, and initialises a git repository. Nothing is hidden
anywhere else on your machine: the vault is the whole state.

## Open it

```sh
trafford ~/vault
```

`TRAFFORD_VAULT` sets the default, so a bare `trafford` opens your vault from
anywhere.

```sh
export TRAFFORD_VAULT=~/vault
```

## An existing Obsidian vault

Point trafford at it. There is nothing to import and nothing to convert.

```sh
trafford ~/Documents/MyVault
```

Obsidian's `.obsidian/` folder is left alone, `.trash/` stays out of the index
because `.gitignore` is respected, and attachments are indexed as link targets
so `![[photo.jpg]]` resolves rather than looking like a note nobody wrote.

> [!tip] Both at once is fine
> trafford re-reads a note that changed underneath it, and never writes a file
> you have not edited. Keeping Obsidian open on the same vault works.

## The first minute

| Key | What it does |
| --- | --- |
| `ctrl-p` | open a note by name |
| `ctrl-f` | search the vault |
| `ctrl-n` | new note |
| `ctrl-e` | switch between writing and reading |
| `ctrl-g` | the git panel |
| `f1` | every key there is |

Everything is also clickable — see [The mouse](the-mouse.md).

## Next

- [The vault](the-vault.md) — links, backlinks, tags, and how they resolve
- [Editing](editing.md) — the modal layer
- [Configuration](configuration.md) — the config file, and what is in it
