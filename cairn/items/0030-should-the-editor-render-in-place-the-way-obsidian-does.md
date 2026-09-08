---
id: 30
title: Should the editor render in place, the way Obsidian does?
type: spike
status: backlog
milestone: later
depends_on:
- 20
- 21
- 22
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: editor
---

## Question

Should the editor render markdown in place — concealing syntax on every line
except the one the cursor is on — instead of keeping preview as a separate mode?

## Why it has to be answered before the work

It is not a feature to add beside the others; it decides whether preview
continues to exist. Building the reading view further and then folding it back
into the editor would be two designs' worth of work for one.

## Options

**Keep the two modes.** The editor shows the file; `ctrl-e` shows it rendered.
Simple, honest, and what #0012 chose. Costs a mode switch every time you want to
see what you wrote.

**Render in place, reveal the cursor's line.** What Obsidian's Live Preview
does. One rule: the line the cursor is on shows its source, every other line is
rendered. The caret then only ever sits on unconcealed text, so the invariant
that protects it holds exactly where it is needed.

**Render in place, reveal the element.** Obsidian's actual behaviour — only the
link or emphasis the cursor is inside reverts, and the rest of the line stays
rendered. Nicer and much fussier.

## What would settle it

Living with v0.5. The argument for two modes was that preview was barely
different from the editor; once folding, properties and callouts land, the
switch may feel like a wall rather than a convenience — or it may feel fine,
because reading and writing turn out to be genuinely separate activities.

That is a question about how it feels in use over a week, not one that can be
reasoned out from here.

## What the vault said before the week was up

The spike says the answer needs living with v0.5, because it is a question about
how something feels. Reading `.obsidian/app.json` on 2026-09-07 found it already
written down:

```
"livePreview": true,
"defaultViewMode": "preview"
```

This vault opens every note in reading view, and edits in live preview. Both,
with reading first.

That is not the answer to "should preview keep existing" — it is the answer to
the question behind it. Preview should keep existing *and be the default*, which
is nearly where trafford already is. What is missing is the other half: an
editor that renders while you type.

So the option to build is the second one below, and the first is not in danger.
It remains a large piece of work, and the reflow it causes is still the thing to
be careful about. But it no longer needs a week of use to justify starting.

## Answer

<!-- Filled in when the spike closes. The evidence above narrows it; living
     with v0.5 is still what settles how the reflow feels. -->

## Notes

#0012 ruled this out on the grounds that concealment breaks the equality between
drawn width and source width, which it does. What has changed since: `Layout`
makes concealment a per-line decision, `Rendered` carries the map back from
drawn characters to what they were, and `PreviewView` already proves a view can
draw a different number of lines than the note has. The obstacle was real and is
now smaller.

The cost that remains is reflow: a line grows when the cursor lands on it and
shrinks when it leaves. That is visible movement under the reader's eye, and it
interacts with folding. Do not treat it as free.
