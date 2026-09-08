use crate::config::Config;
use crate::editor::{Buffer, Editor};
use crate::git::{self, Repo};
use crate::llm;
use crate::tree;
use crate::ui::theme::Theme;
use crate::vault::Vault;
use anyhow::Result;
use ratatui::layout::{Position, Rect};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// How many past queries to keep. Enough for a working session, short
/// enough that the list stays readable in an empty search box.
const RECENT_QUERIES: usize = 8;

// ---------------------------------------------------------------------------
// Fuzzy matching
// ---------------------------------------------------------------------------

/// Subsequence fuzzy match. Returns a score and the matched character indices,
/// or `None` when `query` is not a subsequence of `text`. Consecutive matches,
/// word-boundary matches, and early matches all score higher.
pub fn fuzzy_match(query: &str, text: &str) -> Option<(i64, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = query.chars().collect();
    let mut indices = Vec::with_capacity(needle.len());
    let mut score = 0i64;
    let mut hi = 0usize;
    let mut last_match: Option<usize> = None;

    for &nc in &needle {
        let nc_lower = nc.to_ascii_lowercase();
        let mut found = None;
        while hi < hay.len() {
            if hay[hi].to_ascii_lowercase() == nc_lower {
                found = Some(hi);
                break;
            }
            hi += 1;
        }
        let idx = found?;
        score += 10;
        if idx == 0 {
            score += 15;
        } else if !hay[idx - 1].is_alphanumeric() {
            score += 12; // start of a word or path segment
        }
        if hay[idx] == nc {
            score += 2; // exact case
        }
        if last_match == Some(idx.saturating_sub(1)) {
            // Contiguity is the strongest signal a fuzzy finder has: an
            // uninterrupted run must outrank the same characters scattered
            // across word boundaries.
            score += 20;
        }
        indices.push(idx);
        last_match = Some(idx);
        hi = idx + 1;
    }
    // Prefer earlier matches, but charge for the position once rather than per
    // matched character. Per-character it compounded, so a long path whose
    // filename matched the query exactly lost to a shorter path that merely
    // contained the letters scattered about.
    score -= (indices[0] as i64) / 4;
    // Shorter haystacks with the same match are tighter matches.
    score -= (hay.len() as i64) / 20;
    Some((score, indices))
}

// ---------------------------------------------------------------------------
// Pickers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PickItem {
    pub label: String,
    pub detail: String,
    /// Opaque payload: a note id for the switcher, a command key for the palette.
    pub key: String,
}

#[derive(Debug, Clone, Default)]
pub struct Picker {
    pub title: String,
    pub query: String,
    pub items: Vec<PickItem>,
    /// Indices into `items`, plus the matched character positions in the label.
    pub matches: Vec<(usize, Vec<usize>)>,
    pub cursor: usize,
}

impl Picker {
    pub fn new(title: &str, items: Vec<PickItem>) -> Picker {
        let mut p = Picker {
            title: title.to_string(),
            items,
            ..Default::default()
        };
        p.refilter();
        p
    }

    pub fn refilter(&mut self) {
        let mut scored: Vec<(i64, usize, Vec<usize>)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let shown = if item.detail.is_empty() {
                    item.label.clone()
                } else {
                    format!("{} {}", item.label, item.detail)
                };
                // The key is never displayed, but it is the name of the thing,
                // and typing the name of the thing is the first thing a reader
                // tries. `weekly-note`, labelled "Open this week's note", was
                // unreachable by typing "weekly"; so were nine others — "help"
                // did not find "Keyboard reference".
                //
                // Score the two separately and keep the better, rather than
                // concatenating them: one fuzzy match running off the end of
                // the label and into the key finds nonsense, and "close" would
                // answer with `outdent-selection`.
                let on_shown = fuzzy_match(&self.query, &shown);
                let on_key = fuzzy_match(&self.query, item.key.as_str());
                let best = match (on_shown, on_key) {
                    (Some(a), Some(b)) if b.0 > a.0 => (b.0, Vec::new()),
                    (Some(a), _) => a,
                    (None, Some(b)) => (b.0, Vec::new()),
                    (None, None) => return None,
                };
                let (score, idx) = best;
                // Only keep highlight indices that fall inside the label.
                let label_len = item.label.chars().count();
                let idx = idx.into_iter().filter(|i| *i < label_len).collect();
                Some((score, i, idx))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.matches = scored.into_iter().map(|(_, i, idx)| (i, idx)).collect();
        self.cursor = self.cursor.min(self.matches.len().saturating_sub(1));
    }

    pub fn selected(&self) -> Option<&PickItem> {
        self.matches.get(self.cursor).map(|(i, _)| &self.items[*i])
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as isize;
        self.cursor = (((self.cursor as isize + delta) % len + len) % len) as usize;
    }

    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.cursor = 0;
        self.refilter();
    }

    pub fn pop(&mut self) {
        self.query.pop();
        self.cursor = 0;
        self.refilter();
    }
}

// ---------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    NewNote,
    /// Creating a note because a `[[link]]` pointed at nothing.
    NewNoteFromLink,
    Rename,
    Commit,
    SaveAnswerAs,
    /// Naming the note that selected lines are being moved into. Carries the
    /// text, since the selection is gone by the time the name is confirmed.
    ExtractNote(String),
}

#[derive(Debug, Clone)]
pub struct Prompt {
    pub kind: PromptKind,
    pub title: String,
    pub input: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteNote(String),
    DiscardChanges(String),
    QuitDirty,
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub kind: ConfirmKind,
    pub message: String,
}

/// What a context-menu entry does. Most defer to an existing palette command;
/// the rest need the thing that was clicked, which a command name cannot carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    Command(&'static str),
    OpenNote(String),
    OpenNoteAt(String, usize),
    RenameNote(String),
    DeleteNote(String),
    LinkToNote(String),
    HistoryOf(String),
    NewNoteIn(String),
    ExpandUnder(String),
    CollapseDir(String),
    GoToLine(usize),
    CreateNote(String),
    /// Git actions carry the path they act on, since the pane's own cursor may
    /// not be where the pointer was.
    GitStage(String),
    GitUnstage(String),
    GitDiff(String),
    GitDiscard(String),
    FilterByTag(String),
    /// Put text on the clipboard. The label says what it is; this carries it.
    Copy {
        what: &'static str,
        text: String,
    },
    MoveNote(String),
    DuplicateNote(String),
    /// Answers to "this note changed on disk since you opened it".
    SaveAsConflictCopy(String),
    ReloadFromDisk(String),
    OverwriteOnDisk(String),
    /// Turn the selected lines into their own note, leaving a link behind.
    ExtractSelection,
    /// Hand a file to another application and carry on. Used for things that
    /// return immediately, like `open`.
    Spawn {
        program: String,
        args: Vec<String>,
    },
    /// Give the terminal to another program until it exits. Used for `$EDITOR`,
    /// which needs the screen.
    Suspend {
        program: String,
        args: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub action: MenuAction,
    /// Why this cannot run right now. `None` means it can.
    ///
    /// An action that exists but does not apply *here* is shown greyed with
    /// the reason, rather than left out: a menu whose shape changes under you
    /// cannot teach what is possible, and an absent entry looks the same as an
    /// action that was never built. Actions that make no sense for the thing
    /// clicked stay absent — a folder never lists "Delete note", greyed or
    /// otherwise.
    pub disabled: Option<String>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, action: MenuAction) -> MenuItem {
        MenuItem {
            label: label.into(),
            action,
            disabled: None,
        }
    }

    /// The same entry, greyed out, with the reason shown beside it.
    pub fn unless(mut self, blocked: bool, reason: &str) -> MenuItem {
        if blocked {
            self.disabled = Some(reason.to_string());
        }
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.disabled.is_none()
    }
}

impl MenuItem {
    /// The key that does the same thing, for the hint beside the entry.
    ///
    /// Read from the command table the help screen uses, so the two cannot
    /// disagree — a menu that teaches the wrong key is worse than one that
    /// teaches none.
    pub fn shortcut(&self) -> Option<&'static str> {
        use MenuAction as A;
        let key = match &self.action {
            A::Command(name) => {
                return crate::keymap::COMMANDS
                    .iter()
                    .find(|(k, _, _)| k == name)
                    .map(|(_, _, shortcut)| *shortcut)
                    .filter(|s| !s.is_empty())
            }
            // These have no command behind them; the key is the one that does
            // the same thing to the row the menu was opened on.
            A::OpenNote(_) | A::OpenNoteAt(..) | A::GoToLine(_) | A::CreateNote(_) => "enter",
            A::ExpandUnder(_) => "E",
            A::CollapseDir(_) => "h",
            _ => return None,
        };
        Some(key)
    }
}

/// A menu anchored to the point that was right-clicked.
#[derive(Debug, Clone)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
    pub cursor: usize,
    pub at: (u16, u16),
}

impl Menu {
    /// Move by `delta`, skipping entries that cannot run. Stays put when
    /// nothing in the menu is selectable, rather than spinning forever.
    pub fn move_cursor(&mut self, delta: isize) {
        if self.items.is_empty() || !self.items.iter().any(|i| i.is_enabled()) {
            return;
        }
        let len = self.items.len() as isize;
        let step = if delta >= 0 { 1 } else { -1 };
        let mut at = self.cursor as isize;
        for _ in 0..len {
            at = (at + step).rem_euclid(len);
            if self.items[at as usize].is_enabled() {
                break;
            }
        }
        self.cursor = at as usize;
    }

    /// Put the cursor on the first entry that can actually run.
    pub fn select_first_enabled(&mut self) {
        if let Some(i) = self.items.iter().position(|i| i.is_enabled()) {
            self.cursor = i;
        }
    }

    pub fn selected(&self) -> Option<&MenuItem> {
        self.items.get(self.cursor).filter(|i| i.is_enabled())
    }
}

#[derive(Debug, Default)]
pub struct SearchPane {
    pub query: String,
    pub hits: Vec<crate::vault::Hit>,
    /// How many there were before the list was cut to what fits. Shown when it
    /// differs, so a truncated answer cannot pass itself off as the whole one.
    pub total: usize,
    /// Queries this reader ran before, offered while the box is empty.
    pub recent: Vec<String>,
    pub cursor: usize,
    /// What is wrong with the query, if anything. A misspelled field reports
    /// itself here rather than returning nothing — an empty result and a
    /// mistake look identical, and only one of them is an answer.
    pub problem: Option<String>,
}

/// The drift report, and where the cursor is in it. The report is computed
/// when the pane opens rather than per draw: it walks every note, and a reader
/// scrolling a list should not be recomputing what the list says.
#[derive(Debug, Default)]
pub struct DriftPane {
    pub report: crate::drift::Report,
    pub cursor: usize,
}

impl DriftPane {
    /// The rows a reader can actually land on, paired with the section they
    /// belong to. Headings are drawn but not selectable.
    pub fn selectable(&self) -> Vec<&crate::drift::Row> {
        self.report
            .sections
            .iter()
            .flat_map(|s| s.rows.iter())
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct GitPane {
    pub snapshot: git::Snapshot,
    pub cursor: usize,
    pub log: Vec<git::Commit>,
}

pub enum Overlay {
    Palette(Picker),
    Switcher(Picker),
    /// Same as the switcher, but inserts `[[Target]]` instead of navigating.
    LinkPicker(Picker),
    /// Notes linking to the open note; selecting one jumps to the exact line.
    Backlinks(Picker),
    /// Every theme trafford can find, previewed by applying it live.
    Themes(Picker),
    /// A right-click menu, drawn where the click landed.
    Menu(Menu),
    /// Where to move a note to. Carries the note, since the picker itself only
    /// knows about folders.
    MoveTo {
        picker: Picker,
        note: String,
    },
    Search(SearchPane),
    /// What the vault says about itself that is no longer true.
    Drift(DriftPane),
    Prompt(Prompt),
    Git(GitPane),
    History(Vec<git::Commit>),
    /// A unified diff, shown from the git pane.
    Diff {
        title: String,
        body: String,
        scroll: u16,
    },
    Confirm(Confirm),
    /// A link's destination, without going there.
    Peek(Peek),
    Help,
}

/// What a link points at, shown beside it.
///
/// Deciding whether to follow a link is most of what browsing a vault is, and
/// the alternative is commit-then-undo: open it, find it was not what you
/// wanted, press `ctrl-o`.
#[derive(Debug, Clone)]
pub struct Peek {
    pub title: String,
    /// Tags and backlink count, or why there is nothing to show.
    pub detail: String,
    /// The first paragraph of prose, as written.
    pub body: String,
    /// The note to open on `enter`, when there is one.
    pub open: Option<String>,
    /// The name to write on `enter`, when the link goes nowhere.
    pub create: Option<String>,
}

/// The first paragraph of prose in a note.
///
/// Skips the frontmatter, the title, and a leading callout — all three are
/// things the reader can already see or does not need in a two-line summary.
/// What is wanted is the sentence that says what the note is about.
fn first_paragraph(text: &str) -> String {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let start = crate::vault::note::frontmatter_block(&lines)
        .map(|(_, body)| body)
        .unwrap_or(0);
    let mut out = String::new();
    let mut in_code = false;
    for line in lines.iter().skip(start) {
        let t = line.trim();
        if crate::ui::markdown::is_fence(t) {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let skip = t.is_empty()
            || t.starts_with('#')
            || t.starts_with('>')
            || t.starts_with("---")
            || t.starts_with('|');
        if skip {
            if out.is_empty() {
                continue;
            }
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
    }
    out
}

#[cfg(test)]
mod peek_tests {
    use super::first_paragraph;

    #[test]
    fn the_first_paragraph_skips_what_the_reader_can_already_see() {
        let note = "---\ntags:\n  - one\n---\n\n# Title\n\n> [!note]\n> An aside.\n\nThe sentence that says what this is about.\nStill the same paragraph.\n\nA second paragraph.\n";
        assert_eq!(
            first_paragraph(note),
            "The sentence that says what this is about. Still the same paragraph."
        );
    }

    #[test]
    fn a_note_that_is_only_a_heading_has_no_paragraph() {
        assert_eq!(first_paragraph("# Title\n\n## Section\n"), "");
    }

    #[test]
    fn a_fenced_block_is_not_the_summary() {
        let note = "# T\n\n```sh\nls -la\n```\n\nActual prose.\n";
        assert_eq!(first_paragraph(note), "Actual prose.");
    }

    #[test]
    fn a_note_that_opens_with_prose_still_works() {
        assert_eq!(first_paragraph("Straight in.\n\nMore.\n"), "Straight in.");
    }

    #[test]
    fn a_table_is_not_prose() {
        assert_eq!(
            first_paragraph("| a | b |\n| - | - |\n\nProse.\n"),
            "Prose."
        );
    }
}

/// A file's modification time and length, as far as the filesystem will say.
fn stamp_of(path: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

// ---------------------------------------------------------------------------
// Assistant
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Chat {
    pub messages: Vec<llm::Message>,
    pub input: String,
    pub streaming: bool,
    pub rx: Option<Receiver<llm::Event>>,
    pub scroll: u16,
    /// Notes fed to the model for the current turn, shown as provenance.
    pub context_ids: Vec<String>,
}

/// What draining the stream produced this tick.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// The chat changed and the UI should redraw.
    pub changed: bool,
    /// The turn failed; the message is for the status line.
    pub error: Option<String>,
}

impl Chat {
    /// The most recent assistant reply, for inserting into a note.
    pub fn last_answer(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == llm::Role::Assistant)
            .map(|m| m.text.as_str())
            .filter(|t| !t.trim().is_empty())
    }

    /// Consume whatever the worker thread has produced so far, appending
    /// deltas to the open assistant message. Never blocks.
    pub fn drain(&mut self) -> Drained {
        let Some(rx) = &self.rx else {
            return Drained::default();
        };
        let mut out = Drained::default();
        loop {
            match rx.try_recv() {
                Ok(llm::Event::Delta(text)) => {
                    if let Some(last) = self.messages.last_mut() {
                        last.text.push_str(&text);
                    }
                    out.changed = true;
                }
                Ok(llm::Event::Done) => {
                    self.finish();
                    out.changed = true;
                    break;
                }
                Ok(llm::Event::Error(err)) => {
                    // Drop the placeholder if nothing was streamed into it, so
                    // a failed turn does not leave an empty bubble behind.
                    if self.messages.last().is_some_and(|m| m.text.is_empty()) {
                        self.messages.pop();
                    }
                    self.finish();
                    out.changed = true;
                    out.error = Some(err);
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.finish();
                    out.changed = true;
                    break;
                }
            }
        }
        out
    }

    fn finish(&mut self) {
        self.streaming = false;
        self.rx = None;
    }
}

/// A note name guessed from the first line of a passage.
///
/// The first line usually says what the passage is about, but it says it in
/// markdown: emphasis, list markers, headings. Those have to come off, and so
/// does anything that would change where the note lands — a `/` in a guessed
/// name would silently create a folder.
pub fn suggest_note_name(text: &str) -> String {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let stripped = first
        .trim()
        .trim_start_matches(['#', '-', '*', '+', '>', ' ']);
    let cleaned: String = stripped
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '[' | ']' | '#' | '|'))
        // Path separators and the characters filesystems dislike become
        // spaces rather than being dropped, so words do not run together.
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '?' | '"' | '<' | '>') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    words
        .chars()
        .take(60)
        .collect::<String>()
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// Where each pane was drawn, so a click can be resolved to the thing under
/// it. Recorded on every draw; the mouse handler reads it and nothing else.
/// These are the *inner* areas, inside the borders.
#[derive(Debug, Clone, Copy, Default)]
pub struct Panes {
    pub sidebar: Rect,
    pub editor: Rect,
    pub context: Rect,
    pub assistant: Rect,
    pub assistant_input: Rect,
    /// The list area of whatever overlay is open.
    pub overlay: Rect,
    /// The screen row the section crumb was drawn on, when there was one.
    pub sticky_y: Option<u16>,
}

fn hit(area: Rect, column: u16, row: u16) -> bool {
    area.width > 0 && area.height > 0 && area.contains(Position::new(column, row))
}

impl Panes {
    pub fn sidebar_hit(&self, c: u16, r: u16) -> bool {
        hit(self.sidebar, c, r)
    }
    pub fn editor_hit(&self, c: u16, r: u16) -> bool {
        hit(self.editor, c, r)
    }
    pub fn context_hit(&self, c: u16, r: u16) -> bool {
        hit(self.context, c, r)
    }
    pub fn assistant_hit(&self, c: u16, r: u16) -> bool {
        hit(self.assistant, c, r) || hit(self.assistant_input, c, r)
    }
    pub fn overlay_hit(&self, c: u16, r: u16) -> bool {
        hit(self.overlay, c, r)
    }
}

/// Something in the context pane a click can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextTarget {
    /// A heading in the open note, by buffer row.
    Heading(usize),
    /// Another note, opened whole.
    Note(String),
    /// Another note, opened at the line that mentions this one.
    Backlink(String, usize),
    /// An unwritten note, offered for creation.
    Unwritten(String),
    /// A tag, which filters the vault the way the sidebar's tags tab does.
    /// Tags are clickable wherever they are drawn, and moving them out of the
    /// note must not make the context pane the one place they are not.
    Tag(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Editor,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Notes,
    Tags,
}

pub struct App {
    pub vault: Vault,
    pub repo: Option<Repo>,
    pub config: Config,
    pub theme: Theme,
    pub editor: Editor,
    pub current: Option<String>,
    /// Back-stack of (note id, cursor row) for `Ctrl-O`.
    pub history: Vec<(String, usize)>,
    pub focus: Focus,
    pub sidebar_tab: SidebarTab,
    /// Index into the rows [`App::tree_rows`] returns, not into the note list.
    pub sidebar_cursor: usize,
    /// Directories the sidebar is showing the contents of.
    pub expanded: std::collections::HashSet<String>,
    pub sidebar_visible: bool,
    pub context_visible: bool,
    pub assistant_visible: bool,
    pub preview: bool,
    pub overlay: Option<Overlay>,
    pub chat: Chat,
    pub status: Option<(String, Instant)>,
    pub git_status: git::Snapshot,
    pub last_git_poll: Instant,
    pub last_edit: Instant,
    pub should_quit: bool,
    /// Height of the editor viewport, updated each draw so paging can use it.
    pub editor_height: usize,
    /// Where the panes were drawn, for hit-testing the mouse.
    pub panes: Panes,
    /// Where each crumb of the section header sits — start column, end column,
    /// and the note line it names — recorded at draw time so a click can find
    /// it without a second idea of the geometry.
    pub sticky: Vec<(u16, u16, usize)>,
    /// Watches the vault for changes made anywhere else. `None` when watching
    /// could not start, which is a status message rather than a failure —
    /// `reindex` still works.
    pub watcher: Option<crate::watch::Watcher>,
    /// Notes with unsaved edits, kept while you are elsewhere.
    ///
    /// Navigating away used to throw the buffer out — silently, and without the
    /// prompt that quitting has had all along. Following a link is the common
    /// case and a prompt on every link would be intolerable, so the answer is
    /// not to ask but to keep: switch away, switch back, your typing is there.
    ///
    /// Only dirty buffers are held. A clean one is exactly what is on disk and
    /// re-reading it is both cheaper and more correct, since something else may
    /// have written to it.
    pub unsaved: std::collections::HashMap<String, (Buffer, Option<(std::time::SystemTime, u64)>)>,
    /// What the open note looked like on disk when it was loaded — its
    /// modification time and its length.
    ///
    /// Saving compares against this. Without it a save is a blind `fs::write`,
    /// and the work of anything else writing to the vault — an assistant in
    /// another window, most obviously — is gone with a "saved" in the status
    /// line.
    pub loaded_from_disk: Option<(std::time::SystemTime, u64)>,
    /// A note line the next draw should scroll the reading view onto, set when
    /// a fold changes the document out from under it.
    pub preview_anchor: Option<usize>,
    /// Where the reader is in the reading view, as a row of the *preview*
    /// layout — not a buffer line.
    ///
    /// Preview draws a different document from the one the buffer holds:
    /// concealment shortens lines, frontmatter collapses six rows to two, and a
    /// fold removes hundreds. Driving the view from `buf.row` meant motion and
    /// scrolling were computed in coordinates the screen did not use, so the
    /// wheel did nothing and `G` landed inside a fold. This is authoritative
    /// while preview is on; `buf.row` follows it.
    pub preview_row: usize,
    /// Which panes were open before preview hid them, so leaving preview puts
    /// the frame back. Cleared when a pane is toggled by hand: at that point
    /// the reader has said what they want and it is not this program's to
    /// undo.
    pub chrome_before_preview: Option<(bool, bool)>,
    /// True after the first `g` in the reading view, waiting for the second.
    pub pending_gg: bool,
    /// True after `z`, waiting for the key that says what to fold.
    pub pending_fold: bool,
    /// Which sections are collapsed, per note.
    ///
    /// Lives here rather than on `PreviewView`, which is rebuilt on every draw
    /// and would forget a fold between one keystroke and the next.
    pub folded: crate::ui::fold::Folds,
    /// The note as preview draws it: concealed text, and where its links are.
    /// Rebuilt each draw while preview is on, `None` otherwise.
    ///
    /// It is separate from `editor.layout` on purpose. Concealed text folds
    /// differently from the source, so preview needs its own rows — but the
    /// cursor still lives in the buffer, and letting a motion resolve against
    /// rendered columns would move it somewhere the file does not agree with.
    pub preview_view: Option<crate::ui::PreviewView>,
    /// A program to hand the terminal to, performed by the event loop —
    /// `App` does not own the terminal and must not try to.
    pub pending_suspend: Option<(String, Vec<String>)>,
    /// What each row of the context pane points at, parallel to its lines.
    pub context_targets: Vec<Option<ContextTarget>>,
    /// Tag currently filtering the note list, if any.
    pub tag_filter: Option<String>,
    /// Derived state the vault did not author, in `.trafford/`.
    pub sidecar: crate::sidecar::Sidecar,
    /// Queries worth offering back, most recent first. Derived and disposable:
    /// losing them costs a reader some retyping and nothing else.
    pub recent_queries: Vec<String>,
    /// Where the current theme came from, for the status line and the picker.
    pub theme_source: String,
    /// The theme in use before the picker started previewing, so esc restores.
    pub theme_before_preview: Option<String>,
}

impl App {
    pub fn new(vault: Vault, config: Config) -> App {
        let repo = Repo::discover(&vault.root);
        let git_status = repo
            .as_ref()
            .and_then(|r| r.snapshot().ok())
            .unwrap_or_default();
        let (theme, theme_source) = Theme::resolve(&config.theme, &vault.root);
        let root = vault.root.clone();
        let recent_queries = crate::sidecar::Sidecar::beside(&root)
            .load("queries")
            .unwrap_or_default();
        let mut app = App {
            vault,
            repo,
            theme,
            editor: Editor::default(),
            current: None,
            history: Vec::new(),
            focus: Focus::Editor,
            sidebar_tab: SidebarTab::Notes,
            sidebar_cursor: 0,
            expanded: std::collections::HashSet::new(),
            sidebar_visible: config.sidebar,
            context_visible: config.context_pane,
            assistant_visible: false,
            preview: false,
            overlay: None,
            chat: Chat::default(),
            status: None,
            git_status,
            last_git_poll: Instant::now(),
            last_edit: Instant::now(),
            should_quit: false,
            editor_height: 20,
            panes: Panes::default(),
            folded: crate::ui::fold::Folds::default(),
            watcher: None,
            unsaved: std::collections::HashMap::new(),
            loaded_from_disk: None,
            preview_row: 0,
            preview_anchor: None,
            chrome_before_preview: None,
            sticky: Vec::new(),
            pending_gg: false,
            pending_fold: false,
            preview_view: None,
            pending_suspend: None,
            context_targets: Vec::new(),
            tag_filter: None,
            sidecar: crate::sidecar::Sidecar::beside(&root),
            recent_queries,
            theme_source,
            theme_before_preview: None,
            config,
        };
        // A theme that could not be found has to say so; silently using a
        // different one looks like the config was ignored.
        if app.theme_source.contains("using") {
            app.set_status(format!("theme {}", app.theme_source.clone()));
        }

        // Open the most recently modified note so the app never starts blank.
        if let Some(id) = app
            .vault
            .notes
            .iter()
            .max_by_key(|n| n.modified)
            .map(|n| n.id.clone())
        {
            app.open_note(&id, false);
        }
        app.sidebar_cursor = 0;
        if let Some(id) = app.current.clone() {
            app.reveal_in_tree(&id);
        }
        app
    }

    // ---- status -------------------------------------------------------

    /// Switch themes, remembering the choice so a later save keeps it.
    pub fn apply_theme(&mut self, spec: &str) {
        let (theme, source) = Theme::resolve(spec, &self.vault.root);
        self.theme = theme;
        self.theme_source = source;
        self.config.theme = spec.to_string();
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    pub fn status_text(&self) -> Option<&str> {
        self.status
            .as_ref()
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(5))
            .map(|(s, _)| s.as_str())
    }

    fn report(&mut self, result: Result<String>) {
        match result {
            Ok(msg) => self.set_status(msg),
            Err(err) => self.set_status(format!("error: {err}")),
        }
    }

    // ---- notes --------------------------------------------------------

    /// The notes the sidebar may show, honouring any tag filter.
    pub fn listed_notes(&self) -> Vec<&crate::vault::Note> {
        match &self.tag_filter {
            Some(tag) => self.vault.notes_with_tag(tag),
            None => self.vault.notes.iter().collect(),
        }
    }

    fn tree_input(&self) -> Vec<(String, String)> {
        self.listed_notes()
            .iter()
            .map(|n| (n.id.clone(), n.title.clone()))
            .collect()
    }

    /// The sidebar's visible rows: directories and notes, in tree order.
    pub fn tree_rows(&self) -> Vec<tree::Row> {
        tree::build(&self.tree_input(), &self.expanded)
    }

    /// Open every directory containing `id`, so the note is visible, and put
    /// the cursor on it.
    pub fn reveal_in_tree(&mut self, id: &str) {
        for dir in tree::ancestors(id) {
            self.expanded.insert(dir);
        }
        if let Some(pos) = self
            .tree_rows()
            .iter()
            .position(|r| r.note_id() == Some(id))
        {
            self.sidebar_cursor = pos;
        }
    }

    /// Remember a query the reader got an answer out of.
    ///
    /// Only on the way out, and only when it found something: a query is
    /// half-typed on every keystroke, and remembering those would fill the list
    /// with prefixes of itself.
    pub fn remember_query(&mut self, query: &str, found: usize) {
        let query = query.trim();
        if query.is_empty() || found == 0 {
            return;
        }
        self.recent_queries.retain(|q| q != query);
        self.recent_queries.insert(0, query.to_string());
        self.recent_queries.truncate(RECENT_QUERIES);
        // Best effort. A vault on a read-only disk should still search.
        let _ = self.sidecar.store("queries", &self.recent_queries);
    }

    /// Show only the notes carrying a tag. One definition, because a tag is
    /// clickable in the sidebar, in the note, and now in the context pane, and
    /// three ideas of what a click does is three things to get out of step.
    pub fn filter_by_tag(&mut self, tag: &str) {
        self.tag_filter = Some(tag.to_string());
        self.sidebar_tab = SidebarTab::Notes;
        self.sidebar_cursor = 0;
        self.expand_all();
        self.set_status(format!("filtering by #{tag}"));
    }

    pub fn expand_all(&mut self) {
        self.expanded = tree::all_dirs(&self.tree_input());
    }

    /// Collapse every directory, leaving the cursor somewhere useful.
    pub fn collapse_all(&mut self) {
        self.expanded.clear();
        self.sidebar_cursor = 0;
        let Some(id) = self.current.clone() else {
            return;
        };
        let rows = self.tree_rows();
        // The open note is no longer visible, so aim at the outermost folder
        // holding it: you can see where you were, and `l` walks back in.
        let found = match tree::ancestors(&id).into_iter().next() {
            Some(dir) => rows.iter().position(|r| r.dir_path() == Some(dir.as_str())),
            // A note at the vault root stays visible when all is collapsed.
            None => rows.iter().position(|r| r.note_id() == Some(id.as_str())),
        };
        if let Some(pos) = found {
            self.sidebar_cursor = pos;
        }
    }

    pub fn open_note(&mut self, id: &str, record_history: bool) {
        if record_history {
            if let Some(cur) = &self.current {
                self.history.push((cur.clone(), self.editor.buf.row));
                if self.history.len() > 100 {
                    self.history.remove(0);
                }
            }
        }
        // Put the note being left somewhere safe if it has unsaved work in it.
        self.park_current();

        // Typing that was parked earlier comes back rather than the file, which
        // would be the older of the two.
        if let Some((buf, stamp)) = self.unsaved.remove(id) {
            self.loaded_from_disk = stamp;
            self.editor.load(buf);
            self.current = Some(id.to_string());
            self.focus = Focus::Editor;
            self.reveal_in_tree(id);
            self.preview_row = 0;
            return;
        }

        let path = self.vault.path_for(id);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.loaded_from_disk = stamp_of(&path);
                self.editor.load(Buffer::from_str(&text));
                self.current = Some(id.to_string());
                self.focus = Focus::Editor;
                // Keep the sidebar pointing at whatever is open, opening the
                // directories needed to show it.
                self.reveal_in_tree(id);
            }
            Err(err) => self.set_status(format!("cannot open {id}: {err}")),
        }
    }

    pub fn go_back(&mut self) {
        match self.history.pop() {
            Some((id, row)) => {
                self.open_note(&id, false);
                self.editor.buf.goto_line(row);
            }
            None => self.set_status("no earlier note"),
        }
    }

    pub fn save(&mut self) {
        let Some(id) = self.current.clone() else {
            self.set_status("no note open");
            return;
        };
        let path = self.vault.path_for(&id);

        // Somebody else wrote to this file since it was read. Do not put the
        // buffer over the top of work nobody has seen.
        if self.changed_underneath(&path) {
            let text = self.editor.buf.text();
            if std::fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
                // The same bytes, whoever wrote them. Nothing to resolve.
                self.loaded_from_disk = stamp_of(&path);
            } else {
                self.ask_about_conflict(&id);
                return;
            }
        }

        self.write_note(&id);
    }

    /// Hold on to the open note's buffer if it has unsaved work in it.
    ///
    /// Clean buffers are dropped on purpose: they are what is on disk, and
    /// re-reading is both cheaper and more correct when something else may have
    /// written to the file in the meantime.
    fn park_current(&mut self) {
        let Some(id) = self.current.clone() else {
            return;
        };
        if !self.editor.buf.dirty {
            self.unsaved.remove(&id);
            return;
        }
        let buf = std::mem::replace(&mut self.editor.buf, Buffer::from_str(""));
        self.unsaved.insert(id, (buf, self.loaded_from_disk));
    }

    /// Every note with typing in it that has not reached the disk.
    pub fn unsaved_notes(&self) -> Vec<String> {
        let mut out: Vec<String> = self.unsaved.keys().cloned().collect();
        if self.editor.buf.dirty {
            if let Some(id) = &self.current {
                out.push(id.clone());
            }
        }
        out.sort();
        out
    }

    /// Whether the file has moved on since it was read into the buffer.
    ///
    /// mtime *and* length: a filesystem with one-second mtime granularity will
    /// happily report the same instant for two writes a moment apart, and a
    /// length that changed catches most of what that misses. Neither is a
    /// content hash, and neither needs to be — the content is compared before
    /// anyone is asked anything.
    fn changed_underneath(&self, path: &std::path::Path) -> bool {
        match (self.loaded_from_disk, stamp_of(path)) {
            (Some(was), Some(now)) => was != now,
            // Never read, or gone: not a conflict this can reason about.
            _ => false,
        }
    }

    /// Offer the three answers worth having when two writers disagree.
    ///
    /// "Keep both" is what makes this safe rather than merely careful. A prompt
    /// offering only overwrite and cancel pushes people towards overwrite,
    /// which is the thing being prevented.
    fn ask_about_conflict(&mut self, id: &str) {
        let short = crate::mouse::short_name(id);
        self.overlay = Some(Overlay::Menu(Menu {
            title: format!("{short} changed on disk"),
            items: vec![
                MenuItem::new(
                    "Keep both — save mine beside it",
                    MenuAction::SaveAsConflictCopy(id.to_string()),
                ),
                MenuItem::new(
                    "Load theirs — lose my changes",
                    MenuAction::ReloadFromDisk(id.to_string()),
                ),
                MenuItem::new(
                    "Overwrite theirs with mine",
                    MenuAction::OverwriteOnDisk(id.to_string()),
                ),
            ],
            cursor: 0,
            at: (2, 2),
        }));
    }

    /// Write the buffer beside the note rather than over it, so neither writer
    /// loses anything and the reader can compare them at leisure.
    pub fn save_as_conflict_copy(&mut self, id: &str) {
        let stem = id.trim_end_matches(".md");
        let mut n = 1;
        let mut rel = format!("{stem} (conflict).md");
        while self.vault.path_for(&rel).exists() {
            n += 1;
            rel = format!("{stem} (conflict {n}).md");
        }
        let path = self.vault.path_for(&rel);
        match std::fs::write(&path, self.editor.buf.text()) {
            Ok(()) => {
                let _ = self.vault.rescan();
                self.open_note(&rel, true);
                self.set_status(format!("kept both — yours is now {rel}"));
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("could not write the copy: {err}")),
        }
    }

    /// Throw the buffer away and take what is on disk.
    pub fn reload_from_disk(&mut self, id: &str) {
        let id = id.to_string();
        self.editor.buf.mark_saved();
        self.folded.forget(&id);
        self.open_note(&id, false);
        self.set_status(format!("reloaded {id} from disk"));
    }

    /// Write the buffer to the open note, unconditionally.
    fn write_note(&mut self, id: &str) {
        let path = self.vault.path_for(id);
        let text = self.editor.buf.text();
        match std::fs::write(&path, &text) {
            Ok(()) => {
                self.loaded_from_disk = stamp_of(&path);
                self.editor.buf.mark_saved();
                // Re-index so links, backlinks and tags reflect what was
                // written. Only this note changed, so the whole vault does not
                // need re-reading.
                if let Err(err) = self.vault.refresh_note(id) {
                    self.set_status(format!("saved, but reindex failed: {err}"));
                } else {
                    self.set_status(format!("saved {id}"));
                }
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("save failed: {err}")),
        }
    }

    /// The line an anchor names in the note that is open.
    ///
    /// Reads the live buffer rather than the index: the heading may have been
    /// typed a moment ago and not saved, and a link that works only after a
    /// save would be a puzzle.
    pub fn heading_line_here(&self, anchor: &str) -> Option<usize> {
        let wanted = crate::vault::note::slug(anchor);
        let mut in_code = false;
        for (row, raw) in self.editor.buf.lines.iter().enumerate() {
            if crate::ui::markdown::is_fence(raw) {
                in_code = !in_code;
                continue;
            }
            if in_code {
                continue;
            }
            let trimmed = raw.trim_start();
            if !trimmed.starts_with('#') {
                continue;
            }
            let level = trimmed.chars().take_while(|c| *c == '#').count();
            if level > 6 || trimmed.chars().nth(level) != Some(' ') {
                continue;
            }
            let text = trimmed[level..].trim();
            if text.eq_ignore_ascii_case(anchor.trim()) || crate::vault::note::slug(text) == wanted
            {
                return Some(row);
            }
        }
        None
    }

    /// Offer to create a note for a link that resolves to nothing.
    pub fn prompt_new_note_from_link(&mut self, target: &str) {
        self.overlay = Some(Overlay::Prompt(Prompt {
            kind: PromptKind::NewNoteFromLink,
            title: "Create note".into(),
            input: target.to_string(),
            hint: "this link points at nothing yet — enter to create".into(),
        }));
    }

    /// Follow the `[[link]]` under the cursor, offering to create it if missing.
    pub fn follow_link(&mut self) {
        // An anchor into this note is checked first: it is cheaper, and a
        // contents list is exactly where enter gets pressed most.
        if let Some(anchor) = self.editor.anchor_under_cursor() {
            match self.heading_line_here(&anchor) {
                Some(row) => {
                    self.jump_to(row);
                    self.focus = Focus::Editor;
                }
                None => self.set_status(format!("no heading called \"{anchor}\" in this note")),
            }
            return;
        }
        let Some(link) = self.editor.link_under_cursor() else {
            self.set_status("no link under cursor");
            return;
        };
        let heading = link.heading.clone();
        self.open_target(&link.target, heading);
    }

    /// Collapse or open the section a note line belongs to.
    ///
    /// The line need not be the heading: folding is a thing you do to the
    /// section you are in, so a cursor anywhere inside one folds it. That is
    /// what `za` means in an outliner, and pressing it on body text and having
    /// nothing happen would be the wrong answer.
    pub fn toggle_fold_at(&mut self, row: usize) {
        let Some(id) = self.current.clone() else {
            return;
        };
        let was = self.preview_source();
        let heads = crate::ui::fold::headings(&self.editor.buf.lines);
        let total = self.editor.buf.len();
        // The innermost heading at or above this line whose section still
        // contains it — the one you would point at if asked "which section?".
        let target = heads
            .iter()
            .rfind(|h| h.row <= row && crate::ui::fold::section_end(&heads, h.row, total) > row)
            .map(|h| h.row);
        let Some(row) = target else {
            self.set_status("no section here");
            return;
        };
        if crate::ui::fold::section_end(&heads, row, total) <= row + 1 {
            self.set_status("nothing under this heading");
            return;
        }
        let shut = self.folded.toggle(&id, row);
        // Keep the cursor on the heading when its section closes underneath it,
        // or it is left pointing at text that is no longer drawn.
        if shut {
            self.editor.buf.goto_line(row);
        }
        // The drawn document just changed length, so the reader's row now means
        // something else. Put them back over the line they were looking at.
        self.keep_preview_place(if shut { row } else { was });
    }

    pub fn fold_all(&mut self) {
        let Some(id) = self.current.clone() else {
            return;
        };
        let heads = crate::ui::fold::headings(&self.editor.buf.lines);
        let total = self.editor.buf.len();
        let was = self.preview_source();
        self.folded.fold_all(&id, &heads, total);
        self.keep_preview_place(was);
        self.set_status("folded everything");
    }

    pub fn unfold_all(&mut self) {
        let Some(id) = self.current.clone() else {
            return;
        };
        let was = self.preview_source();
        self.folded.unfold_all(&id);
        self.keep_preview_place(was);
        self.set_status("opened everything");
    }

    /// The link `K` would act on: the one under the cursor, or failing that the
    /// first on this line.
    ///
    /// The fallback is what makes this work in preview, where a click leaves
    /// the cursor at column zero because concealment has no honest mapping back
    /// to a source column. "Tell me about the link on this line" is a rule a
    /// reader can hold, and it is right whenever there is only one.
    fn link_to_peek(&self) -> Option<crate::vault::WikiLink> {
        self.editor.link_under_cursor().or_else(|| {
            let line = self.editor.buf.line(self.editor.buf.row);
            crate::vault::note::parse_wikilinks(line, self.editor.buf.row)
                .into_iter()
                .next()
        })
    }

    /// How many rows the reading view has, and how tall the pane is.
    ///
    /// Both come from the last draw. Before the first one there is nothing to
    /// move through, which the callers treat as "do not move".
    fn preview_extent(&self) -> Option<(usize, usize)> {
        let view = self.preview_view.as_ref()?;
        (view.layout.len() > 0).then(|| (view.layout.len(), self.editor_height.max(1)))
    }

    /// Move the reading view by `delta` rows, or to an end when `to` says so.
    ///
    /// Everything in preview goes through here: `j`, `k`, `ctrl-d`, `ctrl-u`,
    /// `gg`, `G` and the wheel. One place that knows what a row is means the
    /// keyboard and the mouse cannot disagree about it, which is the bug this
    /// replaces.
    pub fn scroll_preview(&mut self, delta: isize) -> bool {
        let Some((rows, _)) = self.preview_extent() else {
            return false;
        };
        let last = rows.saturating_sub(1) as isize;
        self.preview_row = (self.preview_row as isize + delta).clamp(0, last) as usize;
        true
    }

    pub fn preview_page(&mut self, down: bool) -> bool {
        let Some((_, height)) = self.preview_extent() else {
            return false;
        };
        let step = (height / 2).max(1) as isize;
        self.scroll_preview(if down { step } else { -step })
    }

    pub fn preview_to_end(&mut self, end: bool) -> bool {
        let Some((rows, _)) = self.preview_extent() else {
            return false;
        };
        self.preview_row = if end { rows - 1 } else { 0 };
        true
    }

    /// Take in everything that changed on disk since the last look.
    ///
    /// Called once per tick. Cheap when nothing happened, which is almost
    /// always: an empty channel and an early return.
    pub fn absorb_disk_changes(&mut self) {
        let Some(watcher) = &self.watcher else {
            return;
        };
        let batches = watcher.drain();
        if batches.is_empty() {
            return;
        }
        let structural = batches.iter().any(|b| b.structural);
        let paths: Vec<std::path::PathBuf> = batches.into_iter().flat_map(|b| b.paths).collect();

        // Measured on the vault this is built for: a full rescan is 9.8ms and
        // one note is 92µs. So patch when the batch is all known notes — the
        // ordinary case of somebody editing one — and rebuild when anything
        // appeared, vanished or moved, because a patch cannot express that.
        if structural {
            let _ = self.vault.rescan();
        } else {
            for path in &paths {
                if let Some(id) = self.vault.id_for_path(path) {
                    let _ = self.vault.refresh_note(&id);
                }
            }
        }

        self.reload_open_note_if_it_changed();
        self.refresh_git();
    }

    /// Bring the open note up to date with the file, if the two have parted.
    ///
    /// Compares content rather than trusting the event. trafford's own saves
    /// make the watcher fire too, and suppressing "paths we just wrote" is a
    /// race — another program may write the same file a moment later. Identical
    /// bytes mean nothing happened, whoever wrote them.
    fn reload_open_note_if_it_changed(&mut self) {
        let Some(id) = self.current.clone() else {
            return;
        };
        let path = self.vault.path_for(&id);
        let Ok(text) = std::fs::read_to_string(&path) else {
            // Deleted or renamed out from under us. The buffer is all that is
            // left of it, so it is kept rather than blanked.
            return;
        };
        if text == self.editor.buf.text() {
            self.loaded_from_disk = stamp_of(&path);
            return;
        }
        if self.editor.buf.dirty {
            // Never replace typing nobody has saved. #0043 asks at save time,
            // which is when the reader can actually choose.
            self.set_status(format!(
                "{} changed on disk; your unsaved version is still here",
                crate::mouse::short_name(&id)
            ));
            return;
        }

        // Clean: take the new text and stay where the reader was.
        let row = self.editor.buf.row;
        let scroll = self.editor.scroll;
        let reading = self.preview_row;
        // Folds are keyed by line and the lines just moved.
        self.folded.forget(&id);
        self.loaded_from_disk = stamp_of(&path);
        self.editor.load(Buffer::from_str(&text));
        self.editor
            .buf
            .goto_line(row.min(self.editor.buf.len().saturating_sub(1)));
        self.editor.scroll = scroll.min(self.editor.buf.len().saturating_sub(1));
        self.preview_row = reading;
        self.set_status(format!(
            "{} changed on disk — reloaded",
            crate::mouse::short_name(&id)
        ));
    }

    /// Ask the next draw to put the reading view back over this note line.
    ///
    /// Deferred rather than done here: `preview_view` is rebuilt during the
    /// draw, so at the moment a fold is toggled it still describes the document
    /// as it was, and resolving against it would answer the wrong question.
    pub fn keep_preview_place(&mut self, was: usize) {
        self.preview_anchor = Some(was);
    }

    /// The note line the reading view is sitting on.
    pub fn preview_source(&self) -> usize {
        match &self.preview_view {
            Some(v) => v.source(v.layout.row(self.preview_row).map(|r| r.line).unwrap_or(0)),
            None => self.editor.buf.row,
        }
    }

    /// Show what a link points at without going there.
    pub fn peek(&mut self) {
        let Some(link) = self.link_to_peek() else {
            self.set_status("no link on this line");
            return;
        };
        let peek = match self.vault.resolve_target(&link.target) {
            Some(idx) => {
                let note = &self.vault.notes[idx];
                let id = note.id.clone();
                let mut detail = note
                    .tags
                    .iter()
                    .map(|t| format!("#{t}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let backlinks = self.vault.backlinks_for(&id).len();
                if !detail.is_empty() {
                    detail.push_str("  ·  ");
                }
                detail.push_str(&match backlinks {
                    1 => "1 backlink".to_string(),
                    n => format!("{n} backlinks"),
                });
                Peek {
                    title: note.title.clone(),
                    detail,
                    body: first_paragraph(&note.text),
                    open: Some(id),
                    create: None,
                }
            }
            None => Peek {
                title: link.target.clone(),
                detail: "no note by that name".into(),
                body: String::new(),
                open: None,
                create: Some(link.target.clone()),
            },
        };
        self.overlay = Some(Overlay::Peek(peek));
    }

    /// Go to a line, opening whatever folds are hiding it.
    ///
    /// Everything that jumps somewhere specific goes through here: search,
    /// backlinks, the outline, a heading link, a crumb. A destination the
    /// reader cannot see is not a destination — the search used to land on the
    /// right line inside a collapsed section and leave them looking at nothing.
    ///
    /// Only the folds standing in the way are opened. Ones the reader closed
    /// elsewhere stay closed.
    pub fn jump_to(&mut self, row: usize) {
        if let Some(id) = self.current.clone() {
            let heads = crate::ui::fold::headings(&self.editor.buf.lines);
            let total = self.editor.buf.len();
            let containing: Vec<usize> = crate::ui::fold::chain(&heads, row, total)
                .iter()
                .map(|h| h.row)
                .collect();
            for head in containing {
                if self.folded.is_folded(&id, head) {
                    self.folded.toggle(&id, head);
                }
            }
        }
        self.editor.buf.goto_line(row);
        self.keep_preview_place(row);
    }

    /// Open what a wikilink names, jumping to its heading if it named one.
    ///
    /// Shared with preview, where the syntax has been concealed and there is no
    /// cursor to parse a link out from under — only a target the renderer kept.
    pub fn open_target(&mut self, target: &str, heading: Option<String>) {
        match self.vault.resolve_target(target) {
            Some(idx) => {
                let id = self.vault.notes[idx].id.clone();
                self.open_note(&id, true);
                if let Some(h) = heading {
                    // By slug or by text: a link may be written either way.
                    match self.vault.get(&id).and_then(|n| n.heading_line(&h)) {
                        Some(row) => self.jump_to(row),
                        None => self.set_status(format!(
                            "opened {id}, but it has no heading called \"{h}\""
                        )),
                    }
                }
            }
            None => self.prompt_new_note_from_link(target),
        }
    }

    fn new_note_path(&self, name: &str) -> String {
        let dir = self.config.new_note_dir.trim_matches('/');
        if dir.is_empty() || name.contains('/') {
            name.to_string()
        } else {
            format!("{dir}/{name}")
        }
    }

    /// What a new note starts as, and which line to put the cursor on.
    ///
    /// A template when one is configured and readable, a heading otherwise.
    /// Anything the expander does not understand is left as written, so a
    /// template using more of Templater than this degrades to showing its own
    /// source rather than losing it.
    fn body_for_new_note(&self, title: &str) -> (String, Option<usize>) {
        let plain = (format!("# {title}\n\n"), None);
        let path = self.config.new_note_template.trim();
        if path.is_empty() {
            return plain;
        }
        let Ok(template) = std::fs::read_to_string(self.vault.path_for(path)) else {
            return plain;
        };
        let out = crate::vault::template::expand(&template, title, chrono::Local::now());
        (out.text, out.cursor)
    }

    pub fn create_note(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.set_status("note name is empty");
            return;
        }
        let rel = self.new_note_path(name);
        let title = rel
            .rsplit('/')
            .next()
            .unwrap_or(name)
            .trim_end_matches(".md");
        let (body, cursor) = self.body_for_new_note(title);
        match self.vault.create_note(&rel, &body) {
            Ok(id) => {
                self.open_note(&id, true);
                self.editor.buf.goto_line(cursor.unwrap_or(1));
                self.set_status(format!("created {id}"));
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("create failed: {err}")),
        }
    }

    pub fn daily_note(&mut self) {
        let (fmt, dir, template) = (
            self.config.daily_note_format.clone(),
            self.config.daily_note_dir.clone(),
            self.config.daily_note_template.clone(),
        );
        self.periodic_note(&fmt, &dir, &template);
    }

    pub fn weekly_note(&mut self) {
        let (fmt, dir, template) = (
            self.config.weekly_note_format.clone(),
            self.config.weekly_note_dir.clone(),
            self.config.weekly_note_template.clone(),
        );
        self.periodic_note(&fmt, &dir, &template);
    }

    /// Open the note for a period, writing it from its template if it is not
    /// there yet.
    ///
    /// One function for both cadences: they differ only in the format, the
    /// folder and the template, and a second copy would be a second place for
    /// the "open it if it already exists" rule to go wrong.
    fn periodic_note(&mut self, format: &str, dir: &str, template: &str) {
        let name = chrono::Local::now().format(format).to_string();
        let dir = dir.trim_matches('/');
        let rel = if dir.is_empty() {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        // Already written: open it. Overwriting today's note with a blank
        // template would be the worst thing this command could do.
        if let Some(idx) = self.vault.resolve_target(&rel) {
            let id = self.vault.notes[idx].id.clone();
            self.open_note(&id, true);
            self.set_status(format!("opened {id}"));
            return;
        }
        let (body, cursor) = match template.trim() {
            "" => (format!("# {name}\n\n"), None),
            path => match std::fs::read_to_string(self.vault.path_for(path)) {
                Ok(text) => {
                    let out = crate::vault::template::expand(&text, &name, chrono::Local::now());
                    (out.text, out.cursor)
                }
                Err(_) => (format!("# {name}\n\n"), None),
            },
        };
        match self.vault.create_note(&rel, &body) {
            Ok(id) => {
                self.open_note(&id, true);
                self.editor.buf.goto_line(cursor.unwrap_or(1));
                self.set_status(format!("created {id}"));
            }
            Err(err) => self.set_status(format!("create failed: {err}")),
        }
    }

    pub fn rename_current(&mut self, new_name: &str) {
        let Some(id) = self.current.clone() else {
            return;
        };
        match self.vault.rename_note(&id, new_name.trim()) {
            Ok((new_id, touched)) => {
                self.current = Some(new_id.clone());
                self.set_status(format!(
                    "renamed to {new_id}{}",
                    if touched > 0 {
                        format!(" — rewrote links in {touched} note(s)")
                    } else {
                        String::new()
                    }
                ));
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("rename failed: {err}")),
        }
    }

    pub fn delete_note(&mut self, id: &str) {
        let path = self.vault.path_for(id);
        // `none` is for anyone who genuinely wants the file gone; everything
        // else goes to the vault's own trash, which is where the rest of this
        // vault's deletions already are.
        let outcome = if self.config.trash == "none" {
            std::fs::remove_file(&path).map(|()| None)
        } else {
            self.vault
                .trash_note(id)
                .map(Some)
                .map_err(|e| std::io::Error::other(format!("{e:#}")))
        };
        match outcome {
            Ok(where_it_went) => {
                let _ = self.vault.rescan();
                if self.current.as_deref() == Some(id) {
                    self.current = None;
                    self.editor.load(Buffer::from_str(""));
                    if let Some(next) = self.vault.notes.first().map(|n| n.id.clone()) {
                        self.open_note(&next, false);
                    }
                }
                self.folded.forget(id);
                self.unsaved.remove(id);
                self.set_status(match where_it_went {
                    Some(to) => format!("moved {id} to {to}"),
                    None => format!("deleted {id}"),
                });
                self.refresh_git();
            }
            Err(err) => self.set_status(format!("delete failed: {err}")),
        }
    }

    // ---- git ----------------------------------------------------------

    pub fn refresh_git(&mut self) {
        if let Some(repo) = &self.repo {
            if let Ok(snap) = repo.snapshot() {
                self.git_status = snap;
            }
        }
        self.last_git_poll = Instant::now();
    }

    /// Commit every change in the vault. Used by both the git pane and autocommit.
    pub fn commit_all(&mut self, message: &str) {
        let Some(repo) = self.repo.clone() else {
            self.set_status("not a git repository");
            return;
        };
        let result = (|| -> Result<String> {
            repo.stage_all()?;
            let subject = repo.commit(message)?;
            Ok(subject)
        })();
        self.report(result);
        self.refresh_git();
    }

    // ---- assistant ----------------------------------------------------

    /// Assemble grounding context and start a streaming turn.
    pub fn ask(&mut self, question: String) {
        if question.trim().is_empty() {
            return;
        }
        if self.chat.streaming {
            self.set_status("assistant is still answering");
            return;
        }
        if llm::api_key().is_none() {
            self.set_status("set ANTHROPIC_API_KEY to use the assistant");
            return;
        }

        let mut context: Vec<llm::ContextNote> = Vec::new();
        let mut ids: Vec<String> = Vec::new();

        // The open note is always the first piece of context.
        if let Some(id) = &self.current {
            if let Some(note) = self.vault.get(id) {
                context.push(llm::ContextNote {
                    id: note.id.clone(),
                    title: note.title.clone(),
                    body: llm::excerpt(&self.editor.buf.text(), 6000),
                });
                ids.push(note.id.clone());
            }
        }
        for note in self.vault.relevant(&question, self.config.context_notes) {
            if ids.contains(&note.id) {
                continue;
            }
            context.push(llm::ContextNote {
                id: note.id.clone(),
                title: note.title.clone(),
                body: llm::excerpt(&note.text, 2500),
            });
            ids.push(note.id.clone());
        }

        let vault_name = self
            .vault
            .root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "vault".into());
        let system = llm::system_prompt(&vault_name, &context);

        self.chat.messages.push(llm::Message {
            role: llm::Role::User,
            text: question,
        });
        self.chat.messages.push(llm::Message {
            role: llm::Role::Assistant,
            text: String::new(),
        });
        self.chat.context_ids = ids;

        let history: Vec<llm::Message> = self
            .chat
            .messages
            .iter()
            .filter(|m| !(m.role == llm::Role::Assistant && m.text.is_empty()))
            .cloned()
            .collect();

        let (tx, rx) = std::sync::mpsc::channel();
        llm::spawn(self.config.model.clone(), system, history, tx);
        self.chat.rx = Some(rx);
        self.chat.streaming = true;
        self.assistant_visible = true;
        self.focus = Focus::Assistant;
    }

    /// Re-read the vault after another program has had it.
    ///
    /// The guest may have edited the open note, or any other. Unsaved local
    /// changes win: overwriting them with what is on disk would lose work the
    /// user never agreed to give up.
    pub fn reload_after_external(&mut self) -> anyhow::Result<()> {
        self.vault.rescan()?;
        // Folds are keyed by line number, and another program may have moved
        // every line. Keeping them would collapse whatever now happens to sit
        // where a heading used to.
        if let Some(id) = self.current.clone() {
            self.folded.forget(&id);
        }
        self.refresh_git();
        if self.editor.buf.dirty {
            self.set_status("reloaded the vault; this note has unsaved changes and was kept");
            return Ok(());
        }
        if let Some(id) = self.current.clone() {
            if self.vault.get(&id).is_some() {
                let row = self.editor.buf.row;
                self.open_note(&id, false);
                self.editor.buf.goto_line(row);
            }
        }
        Ok(())
    }

    /// Drain any streamed tokens. Called once per event-loop tick.
    pub fn poll_assistant(&mut self) -> bool {
        let drained = self.chat.drain();
        if let Some(err) = drained.error {
            self.set_status(format!("assistant: {err}"));
        }
        drained.changed
    }

    /// Replace the selected lines with a link to a new note holding them.
    ///
    /// The gesture a vault is for: a passage that has outgrown its note
    /// becomes a note, and the place it came from still points at it.
    pub fn extract_selection(&mut self, name: &str, text: &str) {
        let Some((start, end)) = self.editor.selection_rows() else {
            self.set_status("nothing selected");
            return;
        };
        let stem = name
            .trim()
            .rsplit('/')
            .next()
            .unwrap_or(name)
            .trim_end_matches(".md")
            .to_string();
        if stem.is_empty() {
            self.set_status("the note needs a name");
            return;
        }
        let rel = self.new_note_path(name.trim());
        // Refuse before touching the buffer: half of this operation is worse
        // than none of it.
        if self.vault.resolve_target(&rel).is_some() {
            self.set_status(format!("{stem} already exists"));
            return;
        }
        let body = if text.trim_start().starts_with('#') {
            text.to_string()
        } else {
            format!("# {stem}\n\n{text}")
        };
        match self.vault.create_note(&rel, &body) {
            Ok(id) => {
                self.editor.buf.checkpoint();
                self.editor.buf.delete_lines(start, end - start + 1);
                self.editor.buf.lines.insert(start, format!("[[{stem}]]"));
                self.editor.buf.goto_line(start);
                self.editor.mode = crate::editor::Mode::Normal;
                self.editor.buf.dirty = true;
                self.set_status(format!("moved {} line(s) into {id}", end - start + 1));
            }
            Err(err) => self.set_status(format!("could not create the note: {err}")),
        }
    }

    pub fn insert_last_answer(&mut self) {
        let Some(answer) = self.chat.last_answer().map(|s| s.to_string()) else {
            self.set_status("no answer to insert");
            return;
        };
        self.editor.buf.checkpoint();
        self.editor.buf.line_end(true);
        self.editor.buf.insert_newline();
        self.editor.buf.insert_str(&answer);
        self.focus = Focus::Editor;
        self.set_status("inserted the assistant's answer");
    }

    // ---- periodic work -------------------------------------------------

    /// Refresh git state occasionally, and autocommit if configured and idle.
    pub fn tick(&mut self) {
        if self.repo.is_some() && self.last_git_poll.elapsed() > Duration::from_secs(5) {
            self.refresh_git();
        }
        let idle = self.config.autocommit_secs;
        if idle > 0
            && self.repo.is_some()
            && !self.git_status.is_clean()
            && !self.editor.buf.dirty
            && self.last_edit.elapsed() > Duration::from_secs(idle)
        {
            let msg = git::autocommit_message(&self.git_status.changes);
            self.commit_all(&msg);
            self.last_edit = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A query the reader got an answer out of comes back next session. The
    /// sidecar is the only thing that makes that true, and losing it costs
    /// them some retyping and nothing else.
    #[test]
    fn a_useful_query_is_offered_back_next_time() {
        let dir = crate::testing::TempDir::with_files(&[("a.md", "# A\n")]);
        {
            let vault = Vault::open(dir.path()).unwrap();
            let mut app = App::new(vault, Config::default());
            app.remember_query("type:reference status:active", 40);
            app.remember_query("orphan", 9);
        }
        let vault = Vault::open(dir.path()).unwrap();
        let app = App::new(vault, Config::default());
        assert_eq!(
            app.recent_queries,
            ["orphan", "type:reference status:active"]
        );
    }

    /// A query nobody got an answer from is not worth offering back, and a
    /// half-typed one never gets that far.
    #[test]
    fn a_query_that_found_nothing_is_not_remembered() {
        let dir = crate::testing::TempDir::with_files(&[("a.md", "# A\n")]);
        let vault = Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, Config::default());
        app.remember_query("typ", 0);
        app.remember_query("   ", 3);
        assert!(app.recent_queries.is_empty());
    }

    /// Running the same query again moves it up rather than listing it twice.
    #[test]
    fn repeating_a_query_does_not_duplicate_it() {
        let dir = crate::testing::TempDir::with_files(&[("a.md", "# A\n")]);
        let vault = Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, Config::default());
        app.remember_query("orphan", 9);
        app.remember_query("broken", 2);
        app.remember_query("orphan", 9);
        assert_eq!(app.recent_queries, ["orphan", "broken"]);
    }

    /// An app on a one-note vault, that note open.
    fn app_on_a_note(body: &str) -> (crate::testing::TempDir, App) {
        let dir = crate::testing::TempDir::with_files(&[("Note.md", body)]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.open_note("Note.md", false);
        (dir, app)
    }

    /// Write to the note behind trafford's back, far enough after the load that
    /// a one-second mtime cannot report the same instant.
    fn write_behind_its_back(dir: &crate::testing::TempDir, body: &str) {
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(dir.path().join("Note.md"), body).unwrap();
    }

    fn two_note_app() -> (crate::testing::TempDir, App) {
        let dir = crate::testing::TempDir::with_files(&[
            ("One.md", "# One\n\nfirst\n"),
            ("Two.md", "# Two\n\nsecond\n"),
        ]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.open_note("One.md", false);
        (dir, app)
    }

    /// Drive `absorb_disk_changes` without a real watcher, by handing the app
    /// the batch a watcher would have produced.
    fn absorb(app: &mut App, paths: &[&std::path::Path], structural: bool) {
        if structural {
            let _ = app.vault.rescan();
        } else {
            for path in paths {
                if let Some(id) = app.vault.id_for_path(path) {
                    let _ = app.vault.refresh_note(&id);
                }
            }
        }
        app.reload_open_note_if_it_changed();
    }

    #[test]
    fn an_edit_made_elsewhere_reaches_the_open_note() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        std::fs::write(dir.path().join("Note.md"), "# Note\n\nRewritten.\n").unwrap();
        absorb(&mut app, &[&dir.path().join("Note.md")], false);
        assert!(app.editor.buf.text().contains("Rewritten."));
        assert!(
            !app.editor.buf.dirty,
            "it is what is on disk, so it is clean"
        );
    }

    #[test]
    fn an_edit_made_elsewhere_never_replaces_unsaved_typing() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        app.editor.buf.lines.push("mine".into());
        app.editor.buf.dirty = true;
        std::fs::write(dir.path().join("Note.md"), "# Note\n\ntheirs\n").unwrap();

        absorb(&mut app, &[&dir.path().join("Note.md")], false);
        assert!(app.editor.buf.text().contains("mine"), "typing survives");
        assert!(!app.editor.buf.text().contains("theirs"));
        assert!(
            app.status_text()
                .is_some_and(|m| m.contains("changed on disk")),
            "and the reader is told"
        );
    }

    #[test]
    fn a_write_that_changes_nothing_is_not_a_reload() {
        // trafford's own save makes the watcher fire. Reacting to content
        // rather than to the event is what keeps that from churning.
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        app.set_status("something else");
        let before = app.status_text().map(str::to_string);
        std::fs::write(dir.path().join("Note.md"), "# Note\n\nOriginal.\n").unwrap();
        absorb(&mut app, &[&dir.path().join("Note.md")], false);
        assert_eq!(
            app.status_text().map(str::to_string),
            before,
            "nothing was announced"
        );
    }

    #[test]
    fn a_reload_keeps_the_reader_where_they_were() {
        let mut body = String::from("# Note\n");
        for i in 0..40 {
            body.push_str(&format!("\nline {i}\n"));
        }
        let (dir, mut app) = app_on_a_note(&body);
        app.editor.buf.goto_line(30);
        std::fs::write(
            dir.path().join("Note.md"),
            format!("{body}\nand one more\n"),
        )
        .unwrap();
        absorb(&mut app, &[&dir.path().join("Note.md")], false);
        assert_eq!(app.editor.buf.row, 30, "still on the same line");
        assert!(app.editor.buf.text().contains("and one more"));
    }

    #[test]
    fn a_reload_drops_folds_because_they_are_keyed_by_line() {
        let (dir, mut app) = app_on_a_note("# Note\n\n## One\na\n\n## Two\nb\n");
        app.folded.toggle("Note.md", 2);
        assert!(app.folded.is_folded("Note.md", 2));
        std::fs::write(dir.path().join("Note.md"), "# Note\n\nall different now\n").unwrap();
        absorb(&mut app, &[&dir.path().join("Note.md")], false);
        assert!(
            !app.folded.is_folded("Note.md", 2),
            "line 2 is not the heading it was"
        );
    }

    #[test]
    fn a_note_created_elsewhere_joins_the_index() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        assert_eq!(app.vault.notes.len(), 1);
        std::fs::write(dir.path().join("Fresh.md"), "# Fresh\n\nbody\n").unwrap();
        absorb(&mut app, &[&dir.path().join("Fresh.md")], true);
        assert_eq!(app.vault.notes.len(), 2);
        assert!(app.vault.get("Fresh.md").is_some());
    }

    #[test]
    fn a_note_deleted_elsewhere_leaves_the_index() {
        let dir = crate::testing::TempDir::with_files(&[
            ("Note.md", "# Note\n\nSee [[Gone]].\n"),
            ("Gone.md", "# Gone\n"),
        ]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.open_note("Note.md", false);
        assert!(app.vault.resolves("Gone"), "resolves while it exists");

        std::fs::remove_file(dir.path().join("Gone.md")).unwrap();
        absorb(&mut app, &[&dir.path().join("Gone.md")], true);
        assert!(app.vault.get("Gone.md").is_none());
        assert!(
            !app.vault.resolves("Gone"),
            "and the link to it is now broken"
        );
    }

    #[test]
    fn the_open_note_vanishing_leaves_the_buffer_alone() {
        // All that is left of it is in memory. Blanking the screen would be
        // the one unrecoverable thing to do.
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        std::fs::remove_file(dir.path().join("Note.md")).unwrap();
        absorb(&mut app, &[&dir.path().join("Note.md")], true);
        assert!(app.editor.buf.text().contains("Original."));
    }

    #[test]
    fn deleting_moves_the_note_to_the_trash_and_says_where() {
        let (dir, mut app) = two_note_app();
        app.delete_note("Two.md");
        assert!(
            dir.path().join(".trash/Two.md").exists(),
            "it is in the trash"
        );
        assert!(
            app.status_text().is_some_and(|m| m.contains(".trash")),
            "and the reader is told where it went: {:?}",
            app.status_text()
        );
        assert!(app.vault.get("Two.md").is_none(), "gone from the index");
    }

    #[test]
    fn trash_none_still_unlinks_for_anyone_who_wants_that() {
        let (dir, mut app) = two_note_app();
        app.config.trash = "none".into();
        app.delete_note("Two.md");
        assert!(!dir.path().join(".trash/Two.md").exists());
        assert!(!dir.path().join("Two.md").exists(), "genuinely gone");
    }

    #[test]
    fn deleting_a_note_forgets_what_was_being_held_for_it() {
        // A held buffer or a fold set for a note that no longer exists is a
        // stale key waiting to be applied to whatever takes its place.
        let (_dir, mut app) = two_note_app();
        app.open_note("Two.md", true);
        app.editor.buf.lines.push("typed".into());
        app.editor.buf.dirty = true;
        app.open_note("One.md", true);
        assert!(app.unsaved_notes().contains(&"Two.md".to_string()));

        app.delete_note("Two.md");
        assert!(
            !app.unsaved_notes().contains(&"Two.md".to_string()),
            "nothing is held for a note that is gone"
        );
    }

    #[test]
    fn a_daily_note_is_written_from_its_template() {
        let dir = crate::testing::TempDir::with_files(&[(
            "_templates/daily.md",
            "---\ntags:\n  - type/log\n---\n\n# <% tp.date.now(\"dddd, MMMM D, YYYY\") %>\n\n<< [[<% tp.date.now(\"%Y-%m-%d\", -1) %>]] >>\n",
        )]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.config.daily_note_dir = "00-inbox/daily".into();
        app.config.daily_note_template = "_templates/daily.md".into();

        app.daily_note();
        let text = app.editor.buf.text();
        assert!(
            text.contains("type/log"),
            "the template came with it: {text:?}"
        );
        assert!(!text.contains("<%"), "and expanded");
        let id = app.current.clone().unwrap();
        assert!(
            id.starts_with("00-inbox/daily/"),
            "in the vault's own folder: {id}"
        );
    }

    #[test]
    fn asking_twice_opens_the_note_rather_than_rewriting_it() {
        // The worst thing this command could do is replace today's note with a
        // blank template.
        let dir = crate::testing::TempDir::with_files(&[("Seed.md", "# Seed\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.config.daily_note_dir = "journal".into();

        app.daily_note();
        let id = app.current.clone().unwrap();
        app.editor.buf.lines.push("something I wrote today".into());
        app.save();

        app.open_note("Seed.md", true);
        app.daily_note();
        assert_eq!(app.current.as_deref(), Some(id.as_str()));
        assert!(
            app.editor.buf.text().contains("something I wrote today"),
            "the day's work survived asking again"
        );
    }

    #[test]
    fn a_weekly_note_is_its_own_note_in_its_own_place() {
        let dir = crate::testing::TempDir::with_files(&[("Seed.md", "# Seed\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.config.daily_note_dir = "00-inbox/daily".into();
        app.config.weekly_note_dir = "00-inbox/weekly".into();
        app.config.weekly_note_format = "%Y-W%V".into();

        app.daily_note();
        let day = app.current.clone().unwrap();
        app.weekly_note();
        let week = app.current.clone().unwrap();
        assert_ne!(day, week);
        assert!(week.starts_with("00-inbox/weekly/"), "{week}");
        assert!(week.contains("-W"), "named as a week: {week}");
    }

    #[test]
    fn a_missing_template_still_gives_you_todays_note() {
        let dir = crate::testing::TempDir::with_files(&[("Seed.md", "# Seed\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.config.daily_note_template = "_templates/gone.md".into();
        app.daily_note();
        assert!(app.current.is_some(), "the note was still made");
        assert!(!app.editor.buf.text().is_empty());
    }

    #[test]
    fn a_new_note_uses_the_template_when_one_is_configured() {
        let dir = crate::testing::TempDir::with_files(&[(
            "_templates/inbox.md",
            "---\ntags:\n  - type/log\n---\n\n# <% tp.file.title %>\n\n<% tp.file.cursor(0) %>\n",
        )]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.config.new_note_template = "_templates/inbox.md".into();

        app.create_note("Reading list");
        let text = app.editor.buf.text();
        assert!(
            text.contains("# Reading list"),
            "the title went in: {text:?}"
        );
        assert!(
            text.contains("type/log"),
            "and the frontmatter came with it"
        );
        assert!(!text.contains("<%"), "nothing left unexpanded");
        assert_eq!(
            app.editor.buf.row, 7,
            "the cursor landed where the template asked"
        );
    }

    #[test]
    fn a_new_note_without_a_template_is_what_it_always_was() {
        let dir = crate::testing::TempDir::with_files(&[("Seed.md", "# Seed\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.create_note("Plain");
        assert_eq!(app.editor.buf.text(), "# Plain\n\n");
    }

    #[test]
    fn a_template_that_cannot_be_read_falls_back_rather_than_failing() {
        let dir = crate::testing::TempDir::with_files(&[("Seed.md", "# Seed\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.config.new_note_template = "_templates/missing.md".into();
        app.create_note("Still Fine");
        assert_eq!(app.editor.buf.text(), "# Still Fine\n\n");
    }

    #[test]
    fn making_a_note_never_changes_the_template() {
        let dir =
            crate::testing::TempDir::with_files(&[("_templates/t.md", "# <% tp.file.title %>\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.config.new_note_template = "_templates/t.md".into();
        app.create_note("A Note");
        let template = std::fs::read_to_string(dir.path().join("_templates/t.md")).unwrap();
        assert_eq!(
            template, "# <% tp.file.title %>\n",
            "the template is untouched"
        );
    }

    #[test]
    fn unsaved_typing_survives_going_somewhere_else() {
        let (_dir, mut app) = two_note_app();
        app.editor.buf.lines.push("typed but not saved".into());
        app.editor.buf.dirty = true;

        app.open_note("Two.md", true);
        assert!(!app.editor.buf.text().contains("typed but not saved"));
        app.open_note("One.md", true);
        assert!(
            app.editor.buf.text().contains("typed but not saved"),
            "the typing came back"
        );
        assert!(app.editor.buf.dirty, "and is still unsaved");
    }

    #[test]
    fn a_clean_note_is_re_read_rather_than_remembered() {
        // Nothing is held for a clean buffer, so a change made elsewhere while
        // you were away is picked up instead of a stale copy being restored.
        let (dir, mut app) = two_note_app();
        app.open_note("Two.md", true);
        std::fs::write(dir.path().join("One.md"), "# One\n\nrewritten elsewhere\n").unwrap();
        app.open_note("One.md", true);
        assert!(app.editor.buf.text().contains("rewritten elsewhere"));
    }

    #[test]
    fn saving_a_note_stops_holding_it() {
        let (_dir, mut app) = two_note_app();
        app.editor.buf.lines.push("typed".into());
        app.editor.buf.dirty = true;
        app.save();
        assert!(app.unsaved_notes().is_empty(), "saved is not unsaved");
        app.open_note("Two.md", true);
        app.open_note("One.md", true);
        assert!(app.editor.buf.text().contains("typed"));
        assert!(!app.editor.buf.dirty);
    }

    #[test]
    fn every_held_note_is_reported_not_only_the_open_one() {
        let (_dir, mut app) = two_note_app();
        app.editor.buf.lines.push("one".into());
        app.editor.buf.dirty = true;
        app.open_note("Two.md", true);
        app.editor.buf.lines.push("two".into());
        app.editor.buf.dirty = true;
        assert_eq!(app.unsaved_notes(), vec!["One.md", "Two.md"]);
    }

    #[test]
    fn the_conflict_stamp_travels_with_a_held_buffer() {
        // A note parked before someone else wrote to it must still notice on
        // the way back, or holding buffers would quietly disarm #0043.
        let (dir, mut app) = two_note_app();
        app.editor.buf.lines.push("mine".into());
        app.editor.buf.dirty = true;
        app.open_note("Two.md", true);

        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(dir.path().join("One.md"), "# One\n\ntheirs\n").unwrap();

        app.open_note("One.md", true);
        app.save();
        assert!(
            matches!(app.overlay, Some(Overlay::Menu(_))),
            "a parked buffer still knows what the file looked like"
        );
    }

    #[test]
    fn saving_over_someone_elses_write_asks_first() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        write_behind_its_back(&dir, "# Note\n\nOriginal.\nTheirs.\n");
        app.editor.buf.lines.push("Mine.".into());
        app.editor.buf.dirty = true;

        app.save();
        assert!(
            matches!(app.overlay, Some(Overlay::Menu(_))),
            "a save into a changed file must stop and ask"
        );
        let on_disk = std::fs::read_to_string(dir.path().join("Note.md")).unwrap();
        assert!(on_disk.contains("Theirs."), "and must not have written yet");
        assert!(!on_disk.contains("Mine."));
    }

    #[test]
    fn an_ordinary_save_does_not_ask_anything() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        app.editor.buf.lines.push("Mine.".into());
        app.save();
        assert!(app.overlay.is_none(), "nobody else touched it");
        let on_disk = std::fs::read_to_string(dir.path().join("Note.md")).unwrap();
        assert!(on_disk.contains("Mine."));
        assert!(!app.editor.buf.dirty);
    }

    #[test]
    fn saving_twice_in_a_row_does_not_conflict_with_itself() {
        // trafford's own write moves the mtime. If that counted, the second
        // save would accuse the reader of being someone else.
        let (_dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        app.editor.buf.lines.push("One.".into());
        app.save();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        app.editor.buf.lines.push("Two.".into());
        app.save();
        assert!(app.overlay.is_none(), "its own write is not a conflict");
    }

    #[test]
    fn an_identical_write_by_another_program_is_not_a_conflict() {
        // Two writers, same bytes — a formatter, a sync, a git checkout that
        // restored what was already there. There is nothing to resolve.
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        write_behind_its_back(&dir, "# Note\n\nOriginal.\n");
        app.save();
        assert!(
            app.overlay.is_none(),
            "the same content is not a disagreement"
        );
    }

    #[test]
    fn keeping_both_leaves_each_writer_their_work() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        write_behind_its_back(&dir, "# Note\n\nTheirs.\n");
        app.editor.buf.lines = vec!["# Note".into(), String::new(), "Mine.".into()];
        app.editor.buf.dirty = true;

        app.save_as_conflict_copy("Note.md");
        let theirs = std::fs::read_to_string(dir.path().join("Note.md")).unwrap();
        let mine = std::fs::read_to_string(dir.path().join("Note (conflict).md")).unwrap();
        assert!(theirs.contains("Theirs."), "their file is untouched");
        assert!(mine.contains("Mine."), "and mine is beside it");
        assert_eq!(app.current.as_deref(), Some("Note (conflict).md"));
    }

    #[test]
    fn keeping_both_twice_does_not_overwrite_the_first_copy() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        std::fs::write(dir.path().join("Note (conflict).md"), "an earlier rescue").unwrap();
        app.vault.rescan().unwrap();
        app.open_note("Note.md", false);
        app.editor.buf.lines = vec!["Mine.".into()];
        app.save_as_conflict_copy("Note.md");
        let first = std::fs::read_to_string(dir.path().join("Note (conflict).md")).unwrap();
        assert_eq!(first, "an earlier rescue", "the earlier copy survived");
        assert_eq!(app.current.as_deref(), Some("Note (conflict 2).md"));
    }

    #[test]
    fn loading_theirs_throws_the_buffer_away_and_says_so() {
        let (dir, mut app) = app_on_a_note("# Note\n\nOriginal.\n");
        write_behind_its_back(&dir, "# Note\n\nTheirs.\n");
        app.editor.buf.lines.push("Mine.".into());
        app.editor.buf.dirty = true;

        app.reload_from_disk("Note.md");
        assert!(app.editor.buf.text().contains("Theirs."));
        assert!(!app.editor.buf.text().contains("Mine."));
        assert!(!app.editor.buf.dirty);
    }

    fn assistant_turn() -> (Chat, std::sync::mpsc::Sender<llm::Event>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let chat = Chat {
            messages: vec![
                llm::Message {
                    role: llm::Role::User,
                    text: "what is here?".into(),
                },
                // The empty assistant message deltas stream into.
                llm::Message {
                    role: llm::Role::Assistant,
                    text: String::new(),
                },
            ],
            streaming: true,
            rx: Some(rx),
            ..Default::default()
        };
        (chat, tx)
    }

    #[test]
    fn deltas_accumulate_into_the_open_assistant_message() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Delta("Your ".into())).unwrap();
        tx.send(llm::Event::Delta("vault.".into())).unwrap();
        let out = chat.drain();
        assert!(out.changed);
        assert_eq!(chat.messages[1].text, "Your vault.");
        // Still streaming: no Done arrived.
        assert!(chat.streaming);
    }

    #[test]
    fn draining_an_empty_channel_changes_nothing() {
        let (mut chat, _tx) = assistant_turn();
        assert_eq!(chat.drain(), Drained::default());
    }

    #[test]
    fn done_ends_the_turn_and_keeps_the_text() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Delta("answer".into())).unwrap();
        tx.send(llm::Event::Done).unwrap();
        chat.drain();
        assert!(!chat.streaming);
        assert!(chat.rx.is_none());
        assert_eq!(chat.last_answer(), Some("answer"));
    }

    #[test]
    fn an_error_before_any_text_removes_the_empty_bubble() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Error("no credit".into())).unwrap();
        let out = chat.drain();
        assert_eq!(out.error.as_deref(), Some("no credit"));
        assert_eq!(chat.messages.len(), 1, "the placeholder should be gone");
        assert_eq!(chat.messages[0].role, llm::Role::User);
        assert!(!chat.streaming);
    }

    #[test]
    fn an_error_after_partial_text_keeps_what_arrived() {
        let (mut chat, tx) = assistant_turn();
        tx.send(llm::Event::Delta("half an ans".into())).unwrap();
        tx.send(llm::Event::Error("connection reset".into()))
            .unwrap();
        let out = chat.drain();
        assert_eq!(out.error.as_deref(), Some("connection reset"));
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.last_answer(), Some("half an ans"));
    }

    #[test]
    fn a_worker_that_dies_without_speaking_ends_the_turn() {
        let (mut chat, tx) = assistant_turn();
        drop(tx);
        let out = chat.drain();
        assert!(out.changed);
        assert!(out.error.is_none());
        assert!(!chat.streaming, "a dead worker must not leave a spinner up");
    }

    #[test]
    fn last_answer_ignores_a_blank_placeholder() {
        let (chat, _tx) = assistant_turn();
        assert_eq!(chat.last_answer(), None);
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    fn item(action: MenuAction) -> MenuItem {
        MenuItem::new("whatever", action)
    }

    fn menu(items: Vec<MenuItem>) -> Menu {
        Menu {
            title: "t".into(),
            items,
            cursor: 0,
            at: (0, 0),
        }
    }

    /// The point of reading the command table rather than repeating it: the
    /// menu cannot teach a key the help screen disagrees with.
    #[test]
    fn a_command_entry_takes_its_shortcut_from_the_command_table() {
        let save = crate::keymap::COMMANDS
            .iter()
            .find(|(k, _, _)| *k == "save")
            .map(|(_, _, s)| *s)
            .unwrap();
        assert_eq!(item(MenuAction::Command("save")).shortcut(), Some(save));
        assert_eq!(item(MenuAction::Command("save")).shortcut(), Some("ctrl-s"));
    }

    #[test]
    fn a_command_with_no_binding_shows_no_hint() {
        // `rename` is palette-only; an empty string in the table is "no key",
        // not a key that happens to be blank.
        assert_eq!(item(MenuAction::Command("rename")).shortcut(), None);
    }

    #[test]
    fn an_unknown_command_shows_no_hint_rather_than_guessing() {
        assert_eq!(
            item(MenuAction::Command("no-such-command")).shortcut(),
            None
        );
    }

    #[test]
    fn contextual_entries_name_the_key_that_does_the_same_thing() {
        assert_eq!(
            item(MenuAction::OpenNote("a.md".into())).shortcut(),
            Some("enter")
        );
        assert_eq!(
            item(MenuAction::ExpandUnder("p".into())).shortcut(),
            Some("E")
        );
        assert_eq!(
            item(MenuAction::CollapseDir("p".into())).shortcut(),
            Some("h")
        );
    }

    #[test]
    fn entries_with_no_equivalent_key_show_nothing() {
        assert_eq!(item(MenuAction::DeleteNote("a.md".into())).shortcut(), None);
        assert_eq!(item(MenuAction::RenameNote("a.md".into())).shortcut(), None);
        assert_eq!(item(MenuAction::LinkToNote("a.md".into())).shortcut(), None);
    }

    /// Extraction touches the buffer *and* the vault, so the two must not be
    /// able to disagree: a refused note must leave the text alone.
    #[test]
    fn extracting_onto_an_existing_name_leaves_the_buffer_untouched() {
        let dir = crate::testing::TempDir::with_files(&[
            ("source.md", "# Source\n\nkeep\ntake one\ntake two\n"),
            ("taken.md", "# Taken\n"),
        ]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.open_note("source.md", false);
        app.editor.mode = crate::editor::Mode::VisualLine;
        app.editor.anchor = (3, 0);
        app.editor.buf.goto_line(4);

        let before = app.editor.buf.text();
        app.extract_selection("taken", "take one\ntake two\n");

        assert_eq!(
            app.editor.buf.text(),
            before,
            "the buffer was edited anyway"
        );
        assert!(app.status_text().unwrap().contains("already exists"));
        assert_eq!(app.vault.notes.len(), 2, "no third note should exist");
    }

    #[test]
    fn a_suggested_name_strips_markdown_and_keeps_the_words() {
        assert_eq!(suggest_note_name("## Route Plan\nrest"), "Route Plan");
        assert_eq!(
            suggest_note_name("- **Depart:** Saturday"),
            "Depart Saturday"
        );
        assert_eq!(suggest_note_name("> a quote"), "a quote");
        assert_eq!(suggest_note_name("`code` and _more_"), "code and more");
    }

    /// A guessed name containing a path separator would silently create a
    /// folder, which is not what "make a note from this" means.
    #[test]
    fn a_suggested_name_cannot_contain_a_path_separator() {
        for text in ["I-80 / I-70 corridor", "a\\b", "notes/thing"] {
            let name = suggest_note_name(text);
            assert!(!name.contains('/'), "{text:?} gave {name:?}");
            assert!(!name.contains('\\'), "{text:?} gave {name:?}");
        }
        assert_eq!(
            suggest_note_name("I-80 / I-70 corridor"),
            "I-80 I-70 corridor"
        );
    }

    #[test]
    fn a_suggested_name_skips_leading_blank_lines_and_is_bounded() {
        assert_eq!(suggest_note_name("\n\n  real content\n"), "real content");
        assert!(suggest_note_name(&"word ".repeat(50)).chars().count() <= 60);
        assert_eq!(suggest_note_name(""), "");
    }

    #[test]
    fn extracting_replaces_the_lines_with_a_link_to_the_new_note() {
        let dir = crate::testing::TempDir::with_files(&[(
            "source.md",
            "# Source\n\nkeep this\ntake one\ntake two\n",
        )]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.open_note("source.md", false);
        app.editor.mode = crate::editor::Mode::VisualLine;
        app.editor.anchor = (3, 0);
        app.editor.buf.goto_line(4);

        app.extract_selection("Extracted", "take one\ntake two");

        assert_eq!(
            app.editor.buf.text(),
            "# Source\n\nkeep this\n[[Extracted]]\n",
            "the lines should be gone and a link left in their place"
        );
        let made = app
            .vault
            .get("Extracted.md")
            .expect("the note was not created");
        assert!(made.text.contains("take one"), "{}", made.text);
        // A body with no heading of its own gets one, so the note has a title.
        assert!(made.text.starts_with("# Extracted"), "{}", made.text);
        assert_eq!(app.editor.mode, crate::editor::Mode::Normal);
    }

    #[test]
    fn extracting_a_passage_that_already_has_a_heading_does_not_add_another() {
        let dir = crate::testing::TempDir::with_files(&[("source.md", "a\nb\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.open_note("source.md", false);
        app.editor.mode = crate::editor::Mode::VisualLine;
        app.editor.anchor = (0, 0);
        app.editor.buf.goto_line(1);

        app.extract_selection("Thing", "## Already titled\n\nbody\n");
        let made = app.vault.get("Thing.md").unwrap();
        assert!(made.text.starts_with("## Already titled"), "{}", made.text);
    }

    #[test]
    fn extracting_with_no_name_is_refused() {
        let dir = crate::testing::TempDir::with_files(&[("source.md", "a\nb\n")]);
        let vault = crate::vault::Vault::open(dir.path()).unwrap();
        let mut app = App::new(vault, crate::config::Config::default());
        app.open_note("source.md", false);
        app.editor.mode = crate::editor::Mode::VisualLine;
        app.editor.anchor = (0, 0);
        app.editor.buf.goto_line(1);
        let before = app.editor.buf.text();

        app.extract_selection("   ", "a\nb");
        assert_eq!(app.editor.buf.text(), before);
        assert_eq!(app.vault.notes.len(), 1);
    }

    #[test]
    fn a_disabled_entry_carries_its_reason() {
        let entry = item(MenuAction::Command("save")).unless(true, "no unsaved changes");
        assert!(!entry.is_enabled());
        assert_eq!(entry.disabled.as_deref(), Some("no unsaved changes"));
        // `unless(false, ...)` must leave the entry alone.
        let fine = item(MenuAction::Command("save")).unless(false, "never shown");
        assert!(fine.is_enabled());
    }

    #[test]
    fn the_cursor_steps_over_disabled_entries() {
        let mut m = menu(vec![
            item(MenuAction::Command("save")),
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")),
        ]);
        m.move_cursor(1);
        assert_eq!(m.cursor, 3, "should skip the two greyed entries");
        m.move_cursor(1);
        assert_eq!(m.cursor, 0, "and wrap to the first enabled one");
        m.move_cursor(-1);
        assert_eq!(m.cursor, 3, "backwards too");
    }

    /// Spinning forever looking for an enabled entry would hang the draw loop.
    #[test]
    fn a_menu_of_only_disabled_entries_does_not_move_or_hang() {
        let mut m = menu(vec![
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")).unless(true, "nope"),
        ]);
        m.move_cursor(1);
        assert_eq!(m.cursor, 0);
        assert!(m.selected().is_none(), "nothing is selectable");
    }

    #[test]
    fn a_menu_opens_on_the_first_entry_that_can_run() {
        let mut m = menu(vec![
            item(MenuAction::Command("save")).unless(true, "nope"),
            item(MenuAction::Command("save")),
        ]);
        m.select_first_enabled();
        assert_eq!(m.cursor, 1);
    }

    #[test]
    fn selected_refuses_to_return_a_disabled_entry() {
        let m = menu(vec![item(MenuAction::Command("save")).unless(true, "nope")]);
        assert!(m.selected().is_none());
    }
}
