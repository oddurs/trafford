---
order: 50
section: Using it
description: Everything is clickable, and right-click offers what applies to whatever is under the pointer.
---

# The mouse

All of it is clickable. Click a pane to focus it, a folder to fold it, a note
to open it. Click an outline entry to jump to that heading, or a backlink to
open that note at the line mentioning this one. The wheel scrolls whatever is
under the pointer.

Dragging in the editor selects lines for `y`, `d` and `c`. Clicking outside an
overlay dismisses it.

## Right-click

The menu offers what can be done to whatever is under the pointer, rather than
one fixed list. Every surface that has actions answers it: the tree, the
editor, the context pane, the git panel, the tag list, search results, the
quick switcher and the assistant.

The git panel is the one to know — its actions are single letters otherwise.

An action that exists but cannot run right now is shown greyed with the reason
(`Delete… — the only note`), so the menu's shape does not change under you.
Keys are shown beside the entries that have them.

## With lines selected

The menu leads with what applies to them: copy, ask the assistant, indent,
delete — and **make a note from this**, which moves the passage into its own
note and leaves a `[[link]]` where it was.

That last one is the gesture a vault is for.

## Handing a note elsewhere

Open it in Obsidian (when the vault has an `.obsidian/`), in `$EDITOR`, or
reveal it in the file manager. `$EDITOR` gets the terminal to itself and hands
it back; the vault is re-read afterwards, and unsaved local changes are kept
rather than overwritten.

## Moving and duplicating

Moving a note is a rename into another folder, so the links pointing at it
keep working. *Move to…* gives a fuzzy-searchable list of every folder in the
vault. *Duplicate* copies a note beside itself under a free name.

There is no *new folder*: a vault is indexed from its notes, so a folder with
nothing in it has nowhere to live. Naming `folder/note` in the new-note prompt
makes both at once.

## Copying over ssh

The menu offers a note's `[[link]]`, its path, and the selected lines. Each
goes to the local pasteboard *and* out as an OSC 52 escape, so it lands on the
clipboard of the machine you are sitting at rather than the one trafford
happens to be running on.

> [!tip] Getting your terminal's own selection back
> Mouse capture takes it away. Hold `shift`, which every terminal worth using
> supports.
