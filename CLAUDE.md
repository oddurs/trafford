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
| `src/ui/` | Theme, markdown-to-spans renderer, tables, and all drawing |
| `src/main.rs` | CLI, terminal setup, event loop |

The dependency direction is one-way: `vault` and `editor` know nothing about
the UI; `ui` reads `App` but never mutates it except for viewport bookkeeping.

## Invariants worth knowing

- **The markdown renderer changes no characters on the editor path.**
  `ui::markdown::Renderer` has a `conceal` flag. With it **off** — which is how
  the editor always builds it — the renderer only applies styles, because the
  editor draws the source and the cursor column has to line up with the buffer.
  `styling_alone_preserves_every_character` pins that; keep it. It replaced
  `rendering_preserves_every_character`, which claimed the same thing of the
  renderer as a whole and stopped being true when preview started concealing.
- **Preview conceals, and pays for it by carrying a map.** With `conceal` on,
  `[[Note|alias]]` is drawn as `alias`, so drawn width and source width part
  company. That is only safe because there is no caret in preview: what protects
  a click instead is `Rendered`, which carries the drawn text and the character
  ranges its links occupy. `PreviewView` folds *that* text, never the source —
  fold the source and a concealed line breaks in the wrong place.
- **Two views, two layouts, and the buffer owns the cursor.** `editor.layout` is
  over the source and is what motions and the caret read, in preview as well.
  `app.preview_view` is over the rendered text and is what preview draws and
  hit-tests. Resolving a motion against rendered columns would move the cursor
  somewhere the file does not agree with.
- **Columns are characters, never bytes.** `Buffer` converts at the edges
  (`byte_at`). Notes contain non-ASCII.
- **`Note.text` and `Note.haystack` are line-aligned.** `haystack` is the
  lowercased copy used for search; line N of one is line N of the other.
  Match on `haystack`, display from `text`, or you will show mangled case.
- **Link resolution order** is exact relative path, then case-insensitive path,
  then filename stem. Changing it changes which note a `[[link]]` opens.
- **The editor owns no I/O.** `App::save` writes and then re-indexes; the
  editor never touches the filesystem.
- **Hit-testing shares the renderer's geometry.** `src/mouse.rs` resolves a
  click with the same `gutter_width`, `editor_hscroll`, `column_at` and
  `scroll_offset` the drawing code uses, against rects recorded during the
  last draw (`App::panes`). Never compute a second, parallel idea of the
  layout — it will diverge silently and clicks will land one row off.
- **Only the mouse modes we use are enabled.** `MOUSE_ON` in `main.rs`
  deliberately omits 1003 (all-motion): the loop redraws per event and nothing
  reacts to a hover, so hover tracking would be pure cost.

## Obsidian compatibility

A vault is someone's real notes, and the rules below are what a real one turned
out to need. Each was found by opening a 127-note vault, not by reading docs.

- **Link resolution order**: exact relative path, then case-insensitive path,
  then filename stem. Changing it changes which note a `[[link]]` opens.
- **Attachments count as link targets.** `![[photo.jpg]]` points at a real
  file. Non-markdown files are indexed by filename and relative path so an
  embed resolves; otherwise a vault with images lists its own pictures as
  notes nobody has written.
- **A tag needs a non-numeric character.** Without that rule `#1` in "their #1
  barrier" and `#333` in "Lex Fridman #333" become tags, and a real vault's
  tag list is a third prose.
- **A template's H1 is not a title.** Templater files carry
  `<% tp.file.title %>` there. Fall back to the filename, which is what
  Obsidian displays.
- **`.gitignore` is respected** by the walker, which is why Obsidian's
  `.trash/` stays out of the index without a special case.
- **Frontmatter** `title:` and `tags:` are read, including the `- item` list
  form. Nested tags like `type/reference` are ordinary tags.
- **There is one heading scanner.** `ui::fold::headings` is it — the outline in
  the context pane, the fold state, and the reading view all read it. A second
  idea of the document's structure diverges exactly the way a second idea of the
  layout does.
- **Fold state lives on `App`, not on `PreviewView`.** The view is rebuilt every
  draw and would forget a fold between one keystroke and the next. It is keyed
  by line number, so `reload_after_external` throws it away — another program
  may have moved every line, and a stale fold collapses whatever now sits where
  a heading used to.
- **Frontmatter is a block too, and preview shows it as properties.** Six lines
  of YAML become at most two rows: the tags, then everything else dimmed.
  Nothing is dropped — a key nobody anticipated still appears. The whole block
  maps back to line 0, because a chip has no YAML line of its own and the top of
  the block is where a click should land. An unterminated `---` is not
  frontmatter and is shown as written.
- **`#tags` are clickable wherever they are drawn.** `markdown::Target` says
  what a run of text points at — a note, a URL, or a tag — and a tag click sets
  the vault filter, which is what the sidebar's tags tab already did.
<<<<<<< HEAD
- **A measure is a rule about prose, and a table is not prose.** Reading holds
  paragraphs to `READING_MEASURE` because prose stretched wide is unreadable.
  Applying that to a table does not wrap it — the cells are already sized — it
  *cuts* them, losing data the editor showed fine. `Rendered.rigid` marks a line
  whose width is already decided; rigid lines get the pane, prose gets the
  measure, via `Layout::with_widths`.
- **Prose shares a left edge; a rigid block centres on the same axis.**
  `Rendered.offset` carries it, so `mouse.rs` subtracts it before asking the
  layout anything. Centring every line individually turns a paragraph into a
  poem; centring none of them puts a wide table off to one side.
- **The position rail draws at the pane's edge, not the measure's.** Reading
  narrows the text to `READING_MEASURE` and centres it; drawing the rail at
  `inner.right()` put it inside that column, over the last character of a line.
  It draws against `pane`, and the measure gives up one column for it.
- **Anything that jumps to a line goes through `App::jump_to`.** It opens the
  folds hiding that line first — a destination the reader cannot see is not a
  destination, and search used to land on the right line inside a collapsed
  section and leave them looking at nothing. Only the folds in the way open;
  ones closed elsewhere stay closed. Ordinary motion does *not* use it, or `za`
  would be undone by the next keystroke.
||||||| 28fc557
=======
- **Anything that jumps to a line goes through `App::jump_to`.** It opens the
  folds hiding that line first — a destination the reader cannot see is not a
  destination, and search used to land on the right line inside a collapsed
  section and leave them looking at nothing. Only the folds in the way open;
  ones closed elsewhere stay closed. Ordinary motion does *not* use it, or `za`
  would be undone by the next keystroke.
>>>>>>> origin/main
- **Preview has its own cursor, and it is authoritative.** `app.preview_row` is
  a row of `PreviewView.layout` — not a buffer line. Preview draws a different
  document from the one the buffer holds (concealment shortens lines,
  frontmatter collapses six rows to two, a fold removes hundreds), so motion
  computed in buffer coordinates moves through lines that are not on screen.
  `buf.row` *follows* it, so leaving preview lands where you were reading and
  `za` / `K` / the crumb act on a line you can see.
- **Nothing re-anchors the reading view per draw.** It used to call
  `sync_scroll_visual` against `buf.row` every frame, which put the view back
  before anyone saw it move — the wheel appeared dead. When a fold changes the
  document, `preview_anchor` asks the *next* draw to remap, because
  `preview_view` is rebuilt during the draw and is stale before it.
- **Reading is a posture, not a rendering.** `ctrl-e` hides the side panes, drops
  the line-number gutter, and holds prose to `READING_MEASURE` (72) centred —
  `wrap_column` defaults to the pane, which is right for an editor and wrong for
  a reading view. `reading_focus = false` keeps the editor's chrome. Toggling a
  pane by hand clears `chrome_before_preview`, so leaving preview does not argue
  with a choice the reader just made.
- **Peek never touches the network.** A `[text](https://…)` link peeks as the URL
  and nothing else. Fetching a page to summarise it would turn reading a note
  into an outbound request, which is not something this program starts doing
  quietly.
- **`K` peeks, not `space`.** `space` toggles the task on the current line and
  the vault has 917 of them. `K` is where vim already puts "tell me about this
  word", and it falls back to the first link on the line — which is what makes
  it work in preview, where a click leaves the cursor at column zero because
  concealment has no honest mapping back to a source column.
- **The section crumb costs a row, and `panes.editor` has to know.** The heading
  chain is drawn above the text, so `draw_editor` shrinks the rect and assigns
  `app.panes.editor` itself — the assignment in `draw` happens *before* the call
  for that reason. `panes.sticky_y` is checked before any pane claims the row,
  since the crumb sits outside the editor rect.
- **A callout is a block that draws one line per source line.** `ui::callout`
  turns `> [!note]` into a barred, labelled block. Unlike a table the mapping
  back to the note is the identity, so nothing has to be tracked. A kind nobody
  anticipated still draws, labelled with what the author wrote — refusing to
  draw it would leave `[!quote]` sitting in the text as characters. Colours come
  from `accent` / `added` / `modified` rather than three new theme roles.
- **`Rendered.rail` is what a wrapped line repeats down its left.** A callout's
  bar is part of the line's text, so without it the bar appears on the first row
  and nowhere else, and the callout stops reading as one halfway through a
  paragraph. `layout::continuation_indent` treats `▎ ` as a marker for the same
  reason it treats `> `.
- **A table is a block, not a line.** `ui::table` parses a header, a separator
  and its rows together, and draws two more lines than the block occupies — so
  preview cannot assume one drawn line per source line. `PreviewView.sources`
  maps each drawn line back, which is what puts a click on the right note line
  and the gutter number beside content rather than beside a rule. Measuring is
  in display columns; a table of CJK is what catches a character count, and it
  catches it silently, by drawing rules that do not line up.
- **`\|` inside a table cell is an escaped pipe.** Obsidian writes
  `[[Note\|alias]]` in tables. Split on it and the row gains a cell, the table
  is ragged, and the whole block falls back for no reason.
- **What preview renders, and what it leaves alone.** Concealed: wikilink and
  markdown-link syntax, `**bold**`, `*italic*`, `==highlight==`, `` `code` ``
  and heading hashes. Kept: `#tags` (the hash is part of the tag, not wrapping
  around it), list markers and checkboxes, `>` quote markers, and everything
  inside a fence, where the syntax *is* the content. A broken link still draws
  in the broken colour — concealing the brackets must not conceal that it goes
  nowhere.

## Testing a TUI

Unit tests cover the parts that are pure. They cannot tell you that a pane got
drawn over, that the cursor is in the wrong column, or that a keystroke went to
the wrong place — none of that is visible from inside the process.

`tools/probe.py` runs the real binary against an emulated terminal and reads
the screen back:

```sh
python3 -m venv .venv && .venv/bin/pip install pyte   # see below
cargo build --release
python3 tools/probe.py screens  /tmp/vault   # what each keystroke draws
python3 tools/probe.py cursor   /tmp/vault   # where the cursor actually lands
python3 tools/probe.py sizes    /tmp/vault   # panic-hunt across terminal sizes
python3 tools/probe.py timings  /tmp/vault   # startup, save, search latency
```

Use `.venv/bin/python` to run it. A plain `pip install pyte` is refused on any
Python installed by Homebrew or a recent distribution — the interpreter is
marked externally managed, and the error names `--break-system-packages`, which
is not the answer. The venv is gitignored.

Every non-trivial bug in this project's first week came from there: a panic on
narrow terminals, CRLF files tearing the layout apart, the cursor drifting on
CJK text, and a 700 ms stall on save in a large vault. All four are invisible
to `cargo test` and to reading a screenshot of ASCII notes.

When it finds something, the fix belongs in a unit test as well — the probe
tells you *what* is wrong, and a test in `src/` keeps it from coming back.
Prefer asserting invariants over examples: `line.width() <= width` across a
matrix of widths catches the off-by-one that one hand-picked case does not.

Four things that each cost an afternoon, so they are worth knowing:

- **One edit per script.** A script that makes several edits and asserts on
  each writes the file only at the end, so a later assertion failing throws
  away the earlier edits that did match — silently, and with the run's other
  commands carrying on around it. Three fixes were lost that way here. Write
  each edit as its own script that saves immediately.

- **Assert on every scripted edit, and let the failure stop the run.** A
  `str.replace` that matches nothing fails silently. Two fixes here were
  written, committed, and believed for days without ever having been applied,
  because `cargo fmt` had rewrapped the target line first. Adding the assert
  is only half of it: if the edit and the build are separate commands in one
  shell invocation, the build still runs on unchanged source and reports
  success. Use `set -e`, or chain with `&&`.

- **`git checkout <file>` reverts *everything* uncommitted in that file.** It
  looks like undoing the last edit and is not. It has thrown away finished work
  here twice, the second time a whole afternoon's wiring — copy the file aside
  first, or revert the specific edit with the same tool that made it.

- **Send `esc` as its own step.** A terminal delivers ESC glued to the next key
  as `Alt+key`, so `b"\x1bа"` is one keypress, not two.

- **Check that a new regression test fails without the fix.** The first version
  of the narrow-terminal test left focus on the editor, so the branch with the
  panic in it never ran, and the test passed against the bug.

- **Look at colour, do not reason about it.** `tools/probe.py` can render the
  screen back out as HTML, and a count of which colours reached which cells
  will tell you a role is off-palette long before your eye does.

## Style

Match what is there: comments explain *why*, not *what*; tests are named as
sentences describing the behaviour they pin; errors go through `anyhow` with
`.context()` at the boundary. Prefer adding a test next to the code over a new
test file.

## Version control

See [docs/version-control-with-agents.md](docs/version-control-with-agents.md).
The short version: branch, commit in reviewable slices, never commit to `main`,
never `push --force`, and leave the working tree clean.

<!-- cairn:begin -->
## Roadmap and issues

<!-- Written by `cairn agent --write CLAUDE.md`; edit cairn.toml, not this. -->


This project tracks its roadmap and issues with `cairn`. Every item is a Markdown file under `cairn/items`, described by the schema in `cairn.toml`.

**Do not create ad-hoc TODO, PLAN or NOTES files.** Create a cairn item instead, so the work appears on the board and in the generated roadmap.

### The loop

1. `cairn next` — what is ready to start. It excludes anything blocked by unfinished dependencies and puts work already in progress first.
2. `cairn claim <ID>` — take it before you start, so no one duplicates the work. `cairn claim --next` picks and claims the top-ranked unclaimed item in one step, and prints its body so you can begin immediately.
3. Do the work, recording what you learn: `cairn set <ID> <field>=<value>`.
4. `cairn close <ID>` when it is done, or `cairn release <ID>` to hand it back.
5. `cairn check` before you report finished. It must pass.

### Commands

```sh
cairn next --json                 # ready work, ranked
cairn claim --next                # take the next ready item
cairn search <TEXT> --json        # titles, bodies and labels
cairn list --json                 # all open items
cairn list --filter 'blocked=false,priority=p0'
cairn show <ID> --json            # one item, including its body
cairn new "<TITLE>" --type <TYPE> --milestone <MILESTONE>
cairn set <ID> status=<STATUS>    # also labels+=x, or any field below
cairn close <ID>
cairn check                       # validate; run before finishing
cairn render                      # regenerate ROADMAP.md
```

### Schema

- **Types**: `feature`, `bug`, `chore`, `docs`
- **Statuses**: `backlog` (open), `planned` (open), `doing` (active), `blocked` (active), `done` (done), `dropped` (dropped)
- **`priority`**: one of p0, p1, p2, p3 — p0 is a release blocker
- **`effort`**: one of s, m, l, xl — Rough size, not an estimate
- **`area`**: one of vault, editor, tree, mouse, chrome, theme, markdown, git, assistant, config, perf, docs, packaging — Subsystem this touches
- **Milestones**: `v0.1` (due 2026-09-06), `v0.2` (due 2026-09-20), `v0.3` (due 2026-10-15), `v1.0` (due 2027-03-01), `later`
- **Saved views** (`cairn list --view NAME`): `now`, `next`, `menu`, `blockers`, `triage`

### Rules

1. Before starting work, find or create the item and set it to an active status.
2. Use the fields above rather than inventing new ones; add new fields to `cairn.toml` first.
3. Never hand-edit the generated roadmap file — change items and run `cairn render`.
4. `cairn check` must pass before the work is considered done. It is not in
   CI — cairn is not published, so a CI job could not install it — which makes
   running it locally the whole of the guarantee.

<!-- cairn:end -->
