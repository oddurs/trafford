# trafford — notes for agents working on this repo

A terminal knowledge base in Rust: an Obsidian-shaped markdown vault with a
modal editor, git integration, and a note-grounded assistant.

## Commands

```sh
cargo test                  # unit tests live beside the code they cover
cargo clippy --all-targets -- -D warnings   # CI fails on any warning
cargo fmt --all --check     # CI checks this exact form
cargo run -- init /tmp/v    # scaffold a throwaway vault
cargo run -- /tmp/v         # open it
```

CI runs the **latest stable** toolchain, which knows lints yours may not. A
clean local clippy is not proof the build is green — `rustup update stable`
first, or run `cargo +<version> clippy` with the version CI reports. Two
`unnecessary_sort_by` errors reached `main` this way already.

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

## Testing a TUI

Unit tests cover the parts that are pure. They cannot tell you that a pane got
drawn over, that the cursor is in the wrong column, or that a keystroke went to
the wrong place — none of that is visible from inside the process.

`tools/probe.py` runs the real binary against an emulated terminal and reads
the screen back:

```sh
pip install pyte
cargo build --release
python3 tools/probe.py screens  /tmp/vault   # what each keystroke draws
python3 tools/probe.py cursor   /tmp/vault   # where the cursor actually lands
python3 tools/probe.py sizes    /tmp/vault   # panic-hunt across terminal sizes
python3 tools/probe.py timings  /tmp/vault   # startup, save, search latency
```

Every non-trivial bug in this project's first week came from there: a panic on
narrow terminals, CRLF files tearing the layout apart, the cursor drifting on
CJK text, and a 700 ms stall on save in a large vault. All four are invisible
to `cargo test` and to reading a screenshot of ASCII notes.

When it finds something, the fix belongs in a unit test as well — the probe
tells you *what* is wrong, and a test in `src/` keeps it from coming back.
Prefer asserting invariants over examples: `line.width() <= width` across a
matrix of widths catches the off-by-one that one hand-picked case does not.

Two things that cost an afternoon each, so they are worth knowing:

- **Send `esc` as its own step.** A terminal delivers ESC glued to the next key
  as `Alt+key`, so `b"\x1bа"` is one keypress, not two.
- **Check that a new regression test fails without the fix.** The first version
  of the narrow-terminal test left focus on the editor, so the branch with the
  panic in it never ran, and the test passed against the bug.

## Style

Match what is there: comments explain *why*, not *what*; tests are named as
sentences describing the behaviour they pin; errors go through `anyhow` with
`.context()` at the boundary. Prefer adding a test next to the code over a new
test file.

## Version control

See [docs/version-control-with-agents.md](docs/version-control-with-agents.md).
The short version: branch, commit in reviewable slices, never commit to `main`,
never `push --force`, and leave the working tree clean.
