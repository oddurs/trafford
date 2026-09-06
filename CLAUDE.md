# trafford — notes for agents working on this repo

A terminal knowledge base in Rust: an Obsidian-shaped markdown vault with a
modal editor, git integration, and a note-grounded assistant.

## Commands

```sh
cargo test                  # unit tests live beside the code they cover
cargo clippy --all-targets  # must be clean; CI fails on warnings
cargo fmt                   # must be clean; CI checks --check
cargo run -- init /tmp/v    # scaffold a throwaway vault
cargo run -- /tmp/v         # open it
```

## Layout

| Path | What lives there |
| --- | --- |
| `src/vault/note.rs` | Parsing one note: frontmatter, headings, `#tags`, `[[wikilinks]]` |
| `src/vault/index.rs` | The vault: scanning, link resolution, backlinks, search, rename |
| `src/editor/buffer.rs` | Text buffer — cursor, edits, undo. No key handling. |
| `src/editor/mod.rs` | Modal layer: normal/insert/visual, operators, counts |
| `src/git.rs` | Git by shelling out to `git`. Porcelain parsing, commit, push, pull |
| `src/llm.rs` | Anthropic streaming client; runs on a worker thread |
| `src/app.rs` | Application state, note navigation, assistant plumbing |
| `src/keymap.rs` | Key routing, the command palette, the help table |
| `src/ui/` | Theme, markdown-to-spans renderer, and all drawing |
| `src/main.rs` | CLI, terminal setup, event loop |

The dependency direction is one-way: `vault` and `editor` know nothing about
the UI; `ui` reads `App` but never mutates it except for viewport bookkeeping.

## Invariants worth knowing

- **The markdown renderer never changes characters.** `ui::markdown::Renderer`
  only applies styles, because the editor draws through it and the cursor
  column has to line up with the buffer. There is a test for this; keep it.
- **Columns are characters, never bytes.** `Buffer` converts at the edges
  (`byte_at`). Notes contain non-ASCII.
- **`Note.text` and `Note.haystack` are line-aligned.** `haystack` is the
  lowercased copy used for search; line N of one is line N of the other.
  Match on `haystack`, display from `text`, or you will show mangled case.
- **Link resolution order** is exact relative path, then case-insensitive path,
  then filename stem. Changing it changes which note a `[[link]]` opens.
- **The editor owns no I/O.** `App::save` writes and then re-indexes; the
  editor never touches the filesystem.

## Style

Match what is there: comments explain *why*, not *what*; tests are named as
sentences describing the behaviour they pin; errors go through `anyhow` with
`.context()` at the boundary. Prefer adding a test next to the code over a new
test file.

## Version control

See [docs/version-control-with-agents.md](docs/version-control-with-agents.md).
The short version: branch, commit in reviewable slices, never commit to `main`,
never `push --force`, and leave the working tree clean.
