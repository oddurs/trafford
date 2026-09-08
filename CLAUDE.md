# trafford — notes for agents working on this repo

A terminal knowledge base in Rust: an Obsidian-shaped markdown vault with a
modal editor, git integration, and a note-grounded assistant.

## Commands

```sh
cargo test                  # unit tests live beside the code they cover
cargo clippy --all-targets -- -D warnings   # CI fails on any warning
cargo fmt --all --check     # CI checks this exact form
cargo run -p trafford -- init /tmp/v        # scaffold a throwaway vault
cargo run -p trafford -- /tmp/v             # open it
```

The repository is a Cargo workspace. `trafford/` is the application; `site/`
builds the documentation and landing page and is never a dependency of the
binary — `cargo tree -p trafford` is the check, and it is in CI.

```sh
cargo run -p trafford-site -- serve   # build the site, watch, and serve it
cargo run -p trafford-site -- build   # write it to target/site
cargo run -p trafford-site -- sync    # regenerate docs/keys.md from the keymap
.venv/bin/python tools/shots.py       # regenerate the screenshots
```

CI runs the **latest stable** toolchain, which knows lints yours may not. A
clean local clippy is not proof the build is green — `rustup update stable`
first, or run `cargo +<version> clippy` with the version CI reports. Two
`unnecessary_sort_by` errors reached `main` this way already.

## Layout

| Path | What lives there |
| --- | --- |
| `trafford/src/vault/note.rs` | Parsing one note: frontmatter, headings, `#tags`, `[[wikilinks]]` |
| `trafford/src/vault/index.rs` | The vault: scanning, link resolution, backlinks, search, rename |
| `trafford/src/editor/buffer.rs` | Text buffer — cursor, edits, undo. No key handling. |
| `trafford/src/editor/mod.rs` | Modal layer: normal/insert/visual, operators, counts |
| `trafford/src/git.rs` | Git by shelling out to `git`. Porcelain parsing, commit, push, pull |
| `trafford/src/llm.rs` | Anthropic streaming client; runs on a worker thread |
| `trafford/src/app.rs` | Application state, note navigation, assistant plumbing |
| `trafford/src/keymap.rs` | Key routing, the command palette, the help table |
| `trafford/src/ui/` | Theme, markdown-to-spans renderer, tables, and all drawing |
| `trafford/src/watch.rs` | Watching the vault: debounced filesystem events |
| `trafford/src/main.rs` | CLI, terminal setup, event loop |
| `trafford/src/lib.rs` | What the site can reach: `vault`, `ui::fold`, `ui::table`, `ui::callout`, `ui::markdown::scan`, `ui::theme` |
| `site/src/html.rs` | Notes to HTML, through the app's scanners. No markdown parser |
| `site/src/build.rs` | The build: pages, assets, link checking, the atomic swap |
| `site/src/serve.rs` | The development server. Binds `127.0.0.1:0` |
| `site/src/palette.rs` | `ui::theme` to CSS custom properties |
| `docs/` | The documentation, which is a vault the app can open |
| `tools/shots.py` | The website's screenshots, driven out of the real binary |

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
- **Navigating away holds an unsaved buffer; it does not discard it.**
  `App.unsaved` keeps dirty buffers per note, so switching away and back returns
  what was typed. Following a link is the common case and a prompt on every link
  would be intolerable, so the answer is to keep rather than to ask. Clean
  buffers are deliberately *not* held: they are what is on disk, and re-reading
  picks up anything written meanwhile. The stamp from #0043 travels with a held
  buffer, or holding one would quietly disarm the conflict guard.
- **The vault is watched, and the watcher reacts to content, not to events.**
  `src/watch.rs` debounces filesystem events for 120ms and sends batches;
  `App::absorb_disk_changes` drains them each tick. trafford's own saves make the
  watcher fire too, and suppressing "paths we just wrote" is a race — another
  program may write the same file a moment later — so the open note is compared
  with what is on disk instead. Identical bytes mean nothing happened, whoever
  wrote them.
- **A watcher event never replaces unsaved typing.** It says so and leaves the
  buffer alone; the save guard asks at save time, which is when the reader can
  choose. A reload drops that note's folds, because they are keyed by line and
  the lines just moved.
- **Structural changes rescan; edits patch.** Measured on the vault this is built
  for: a full rescan is 9.8ms, one note is 92µs. Creating, deleting or renaming
  cannot be expressed as a patch, so it rebuilds; an edit to a known note
  refreshes just that note. Dot-directories and editor scratch files are ignored,
  or `.git` during a commit would rescan continuously.
- **A save never writes over a change nobody has seen.** `App::save` compares
  the file's mtime and length against what they were when the note was loaded.
  If they moved, it compares the *content* — identical bytes are not a
  disagreement, whoever wrote them — and only then asks. The three answers are
  keep both, load theirs, overwrite; "keep both" exists because a prompt offering
  only overwrite and cancel pushes people towards overwrite, which is the thing
  being prevented. This matters because the vault is meant to be edited by an
  assistant in another window.
- **The editor owns no I/O.** `App::save` writes and then re-indexes; the
  editor never touches the filesystem.
- **Hit-testing shares the renderer's geometry.** `trafford/src/mouse.rs` resolves a
  click with the same `gutter_width`, `editor_hscroll`, `column_at` and
  `scroll_offset` the drawing code uses, against rects recorded during the
  last draw (`App::panes`). Never compute a second, parallel idea of the
  layout — it will diverge silently and clicks will land one row off.
- **Only the mouse modes we use are enabled.** `MOUSE_ON` in `main.rs`
  deliberately omits 1003 (all-motion): the loop redraws per event and nothing
  reacts to a hover, so hover tracking would be pure cost.

## The docs site

`site/` renders `docs/` — which is a vault, opened by `vault::Index` exactly as
a user's is. Five things about it are load-bearing.

- **There is no markdown parser in `site/`.** Headings come from
  `ui::fold::headings`, tables from `ui::table::parse`, callouts from
  `ui::callout::parse`, frontmatter from `note::frontmatter_block`, inline
  markup from `ui::markdown::scan`. The HTML renderer is a second *backend*,
  not a second parser — which is why `[[wikilinks]]`, `> [!note]` and
  `[[Note\|alias]]` inside a table work on the website. A generator with its own
  parser is the "second heading scanner" failure, shipped deliberately.
- **The development server binds port zero.** Work here happens in several
  worktrees at once. A fixed port does not usually fail loudly — the second
  server does not start, the browser keeps showing the first worktree's build,
  and you review a change that is not there. The port is read *after* binding;
  probing for a free one and then binding it is a race. `target/.serve.json`
  carries the URL for anything that would rather not scrape stdout, and is
  never trusted on its own — a killed process leaves one behind, so ask the
  port with `serve::alive`.
- **Every href is relative to the page that holds it.** That is what makes one
  build correct at a domain root, under the GitHub Pages project subpath, and
  over `file://` with no server. There is no `--base-url`; `--site-url` exists
  only for the canonical tags and the sitemap, which have to be absolute.
- **Anything generated is checked by regenerating it.** `docs/keys.md` comes
  from `keymap::HELP` via `site sync`; the screenshots come from the real
  binary via `tools/shots.py`; the palette comes from `ui::theme`. CI
  regenerates and diffs. A generated file edited by hand is reverted by the
  next sync, and CI says so first.
- **The landing page follows obsidian.md's rhythm, in this project's clothes.**
  A claim at four times the body size, the program full width beneath it, then
  section header → cards, and a claim again at the end. Taken from the
  reference: the layout. Not taken: the sans, the purple, and the gem — a
  geometric sans is right for a GUI app and wearing it here would be borrowing
  an identity rather than learning from a layout.
- **A card puts its copy at the top and lets the recording bleed off the
  bottom.** The eye finishes on the words rather than on a cut-off pixel, which
  is why this reads better than cropping to the right — which is what the first
  version did.
- **The last section of a landing note is its closing**, marked in
  `Writer::finish` rather than by anything in the markdown. It is always the
  last one, so a rule beats a marker.
- **The landing page is a run of showcases, and every one is the program.**
  Each H2 opens a `<section>` (`html::render_sectioned`), so the note stays
  ordinary markdown and the layout is a selector. A recording has to be of the
  thing its heading claims: the first backlinks cast ran `ctrl-t` on whichever
  note the app opens by default, which has none, and was a cast of an empty
  pane.
- **A player keeps its poster until it actually starts.** Loading used to
  reveal the recording's *first* frame, which is the least interesting one —
  the poster was chosen for being worth looking at.
- **The recording styles live in their own section of `site.css`.** They were
  written inside the landing block once and were deleted with it when that
  block was rewritten, which left every recording unstyled and drawn at
  whatever size the box happened to imply.
- **A cell crops a recording rather than scaling it.** A third of the page is
  380 pixels and a terminal is 120 columns; scaled to fit, the text is four
  pixels tall. The bento cells keep the picture readable and clip it, which
  reads as a piece of a real interface rather than an illegible whole.
- **No recording shows a relative time.** The git panel draws how old a commit
  is, which is a function of today's date rather than of anything this project
  does — a shot of it goes stale when the calendar turns. The commit date is
  pinned (which pins the hash, which the panel also draws) and the git shot
  opens a *diff* rather than the commit list.
- **The hero is a recording, and the still is the content.** `tools/shots.py`
  keeps the frames as well as the last one; `site.js` paints them over the
  still, which stays in the flow with `visibility: hidden` so nothing moves
  when the frames land. With no JavaScript, with reduced motion, or before the
  fetch returns, the still is what a reader sees — which is why it is still
  generated. Every control on the page ships `hidden` and is revealed by
  script, and a test asserts that of every button on every page. Casts are
  named in a `data-` attribute the browser ignores and fetched on approach —
  six of them eagerly would be a megabyte before a word is read — and a test
  asserts that too.
- **A recording carries roles, not colours.** `shots.py` records through a
  *sentinel* theme — one unique colour per role, written into the recording
  vault's own `.trafford/themes/` — so a captured cell maps back to exactly one
  role. Reversing a real palette cannot work: gotham draws `faint` and
  `selection` in one hex and `link` and `muted` in another, and in paper those
  pairs are nowhere near each other. The cast JSON then names roles, `site.js`
  paints `var(--term-<role>)`, and every recording wears whatever theme the
  reader picked — which is what the theme section claims and what every
  recording used to contradict, being gotham on a cream page. The sentinel goes
  in the *vault* rather than the user config directory, which is
  `~/Library/Application Support` on macOS and `~/.config` on Linux: written to
  one it is invisible on the other, and the symptom is every recording coming
  out in two colours with nothing failing.
- **A still is one file that wears two schemes.** The SVG carries role classes
  and a `<style>` of its own, `fill: var(--term-r, #hex)`. Inlined — which the
  hero is — the variable resolves and it follows the chosen theme; loaded as an
  `<img>`, nothing of the page reaches in and the fallback draws, so the media
  query swaps the *fallback* rather than the variable. The rules are scoped
  under `.shot-palette` because an inlined `<style>` is a document-wide rule and
  the page has other inline SVG for a bare `.accent { fill: … }` to repaint.
- **A shot that names a theme means it.** The theme strip is three palettes side
  by side and must not follow the reader's, so naming a theme in `shots.toml`
  opts out of roles and records literal colour.
- **`page_roles` decides a colour once.** `vars()` formats it and `swatches()`
  reads it. The specimen used to work the colours out a second time, and the day
  `accent-text` began lifting against the panel as well as the page, the
  specimen went on printing the old ratio — a page whose whole purpose is that
  it cannot disagree with the site, disagreeing with the site.
- **A callout title is words on a panel, so it is lifted against both grounds.**
  `legible()` clears AA against `bg` *and* `surface`. Lifting against the page
  alone is not enough: in paper `surface` is the darker of the two, and all
  three callout titles missed there — the worst at 3.88:1 — in the one theme
  nobody was looking at.
- **The reload client exists only in `serve`.** `build` never writes it, and a
  test asserts that of every page. A deployed page carrying a livereload script
  is the standard way this leaks.

Four smaller things that each cost time to find.

The recordings run against a *copy* of `site/fixture` in a temp directory, with
`HOME` pointed there too, and the copy is made into a git repository with the
branch, the identity and both dates pinned. In place, trafford walks up and
finds *this* repository, so its dirty-file count went into the status bar and
every shot changed whenever anything else did; without a repository at all the
status bar reads `no git`, which is an odd thing to show under a claim about
git being in the status bar; and `Theme::resolve` reads
`~/.config/trafford/themes/` *before* the built-ins, so a developer's own
gotham.toml would take a different screenshot from CI.

The recorder waits for the screen to stop changing rather than for a fixed
pause. A guess broke silently the moment startup grew a git poll: the first
keystroke arrived before the program was listening and the shot recorded the
wrong note without failing.

`mono` is not offered as a site theme, because it sets its background and
foreground to `reset`, meaning "whatever the terminal already is", which a
browser cannot answer.

The typeface is subset to the characters the built site draws, and the subset
is derived from the built HTML rather than listed by hand — a character the
subset lacks falls back mid-word and reads as one broken glyph. `--check` also
pins the advance width at 0.6em, which every fallback in the stack shares, so
the swap when the font lands moves nothing.

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
- **The vault has already been configured, in Obsidian's words.** `Config::load`
  reads `.obsidian/app.json` and fills in what `config.toml` did not mention —
  where new notes go, what deleting means, whether prose is held to a measure.
  The reader's own file always wins, which is why `Config::read` returns the set
  of keys the TOML actually named: `#[serde(default)]` cannot tell
  `new_note_dir = ""` from a key nobody wrote, and that difference *is* the
  precedence rule. Only settings trafford has an answer for are read — reading
  one and ignoring it would suggest a promise it is not keeping. Never write to
  `.obsidian/`: it belongs to Obsidian, and two programs writing one settings
  file is how settings get lost.
- **Periodic notes come from the plugin that makes them.** Folder, format and
  template for the day and the week are read from
  `.obsidian/plugins/periodic-notes/data.json`, with `config.toml` still
  winning. The formats are moment.js and trafford's are strftime, so they go
  through the same table the templates use — `YYYY-[W]ww` must come out as the
  week note's real name, or every week links to one that does not exist. Asking
  for a note that already exists opens it; overwriting today's work with a blank
  template is the worst thing that command could do.
- **Templates expand, but only the expressions this vault uses.**
  `vault::template` covers `tp.date.now` with an optional day offset,
  `tp.file.title` and `tp.file.cursor` — the whole inventory of the five
  templates in a real vault, counted rather than guessed. Anything else is left
  *exactly as written*: losing text is worse than not expanding it. Templater's
  formats are moment.js and chrono's are strftime, so the token table is matched
  longest-first, or `YYYY` reads as two `YY`s; `[W]` stays a literal W, which is
  how a week note is named.
- **Deleting moves a note to `.trash/`, it does not unlink it.** Obsidian puts
  deletions there and a real vault already has a `.trash/` full of them, so
  unlinking made trafford the one program that could take a note away for good.
  The name is flattened (`a/b/Note.md` → `a-b-Note.md`) and a name already taken
  gets a suffix — the trash holds last copies, and overwriting one deleted note
  with another is the single thing it must not do. `trash = "none"` unlinks, for
  anyone who wants that.
- **`.gitignore` is respected** by the walker, which is why Obsidian's
  `.trash/` stays out of the index without a special case.
- **Frontmatter** `title:` and `tags:` are read, including the `- item` list
  form. Nested tags like `type/reference` are ordinary tags.
- **`CLAUDE.md` and `AGENTS.md` are instructions, not notes.** They live in the
  vault and index like any markdown, but they are addressed to a machine. The
  tree names them by their *file* and dims them — the same rule a template
  already gets, and for the same reason: their H1 does not name them. The
  `CLAUDE.md` in the vault this was built for opens
  `# Notesnake - Obsidian Vault`, so by heading it is indistinguishable from a
  note, and from an `AGENTS.md` beside it. Matched on the filename at any depth,
  since a nested `CLAUDE.md` is just as much instructions.
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

## The design system

Every measurement the site makes comes from `site/src/design.rs` — a type
scale, a spacing scale, radii, motion and layout, as data. `palette.rs` already
worked this way for colour; everything else did not, and the stylesheet had
about sixty locally-chosen sizes that were each defensible on their own line
and were not a system.

They live in Rust rather than in a `:root` block for three reasons a block
would not give:

- **`/design` is rendered from them**, so a specimen cannot show one thing
  while the site uses another. The colour swatches print the contrast each role
  actually achieves, which is the same argument this file makes about the
  terminal: look at colour, do not reason about it.
- **`no_raw_values_for_what_the_system_owns` reads `site.css`** and fails when
  a font size, radius, duration or space is written as a literal. That is the
  rule that keeps the scale real.
- **Reduced motion is a property of the tokens.** The durations become `0ms` in
  one block, so every transition stops — including ones written afterwards by
  someone who never read this.

Four rules the layout is made of, which took a bug to find.

- **A band is full width; its content sits in the page column.** One
  declaration does it — `padding-inline: max(gutter, (100% - page) / 2)` — and
  the header, the hero, every section and the footer share it. What it replaced
  was `max-width` / `margin: auto` / `padding` written out at each band, and
  that trio is how the container rule came to be on `<html>`: `.landing` is set
  on the root element *and* on `main`, so the rule meant to centre one column
  was boxing the whole document. The sticky bar's rule stopped short of the
  viewport on both sides, content scrolled through the gap, and `--page` could
  never be reached because the gutter was applied twice.
- **`padding-block`, never the `padding` shorthand, on a band.** The shorthand
  resets the inline inset the band rule just set, which puts the content hard
  against the window.
- **Sections alternate ground, using the elevation the palette already has**:
  `--bg` is the page, `--surface` is a band on it, `--overlay` is a card on a
  band. Three roles that exist in every theme rather than a fourth invented for
  the landing page. The closing section returns to `--bg` because the footer is
  a `--surface` band, and two tinted blocks in a row read as one.
- **The section number lives in the band's own inset**, which is the margin the
  content column is centred by, and it is hidden below the width where that
  margin is too narrow to hold it. It used to be positioned outside a box, and
  worked only because `<html>` was providing a margin by accident.

Two things about the type scale.

- **Tracking is a token, picked by size.** The stylesheet had nine values
  between -0.045em and 0.09em, each defensible on its own line and not a
  system — `.callout-title` at 0.07em and `th` at 0.06em were the same idea
  written twice. Five steps replace them: large type pulls in, small type opens
  up.
- **`no_raw_values_for_what_the_system_owns` used to be fooled by a zero.** It
  allowed a value *containing* `0`, so `-0.045em` and `20px` both passed, and
  the rule that keeps the scale real was not keeping it. It now checks each
  part of a value, exactly, and a value doing arithmetic is checked for naming
  a token at all. Tightening it found `.notfound h1` at a hand-picked `4rem`.
  `em` is still allowed for `font-size` and `padding`, because it means
  "relative to the text I am inside" — what a `<kbd>` needs and what a rem
  scale cannot say. Tracking gets no such exemption: an em is the only unit it
  is written in.

What is deliberately not a token: the width of a recording, the height of a
cropped card. Those measure a particular picture rather than deciding anything,
and folding them in would make the scale meaningless.

Two rules that bit while building it. `:not(:has(…))` outspecifies a plain
class selector, so a rule written to widen a page with no table of contents
also won inside the narrow-screen media query and left the content 76 pixels
wide. And a grid track sized `1fr` still refuses to shrink below its content —
`minmax(0, 1fr)` is what actually means "take what is left".

## Looking at the site

`tools/look.py` is `probe.py` pointed at the website: it drives a real browser,
screenshots each section on its own, and measures what a full-page screenshot
cannot tell you.

```sh
.venv/bin/pip install playwright
.venv/bin/python -m playwright install chromium
.venv/bin/python tools/look.py                    # every section, 1440 wide
.venv/bin/python tools/look.py --width 380        # a phone
.venv/bin/python tools/look.py --scheme light     # Paper, which is what a
                                                  # reader with a light system
                                                  # actually gets
```

It reports anything wider than the viewport, any text under 11px or 4.5:1, and
how much of each section is air. Run it after touching `site.css` — the pass
that added it found a page that scrolled sideways at 420px, a footer heading at
3.06:1, and a card whose last line sat on its own border, none of which a
5000-pixel-tall screenshot showed.

A note on what it does *not* check: anything inside a recording. Those are
pictures of a terminal, drawn in the terminal's colours on the terminal's own
ground, and checking them reported forty false failures and hid the one real
one.

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
tells you *what* is wrong, and a test in `trafford/src/` keeps it from coming back.
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
