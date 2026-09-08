---
order: 70
section: Using it
description: A note-grounded assistant — the open note plus retrieved ones, answering with links you can follow.
---

# The assistant

`ctrl-j` opens it. It needs `ANTHROPIC_API_KEY` in the environment; without
one, the pane explains that rather than failing quietly.

```sh
export ANTHROPIC_API_KEY=sk-ant-...
```

## What is in context

The open note, always. Other notes are retrieved by keyword overlap with the
question and named in the prompt, so answers come back citing `[[notes]]` you
can jump straight to.

```toml
model = "claude-sonnet-5"
context_notes = 6
```

`context_notes` is how many notes are retrieved per question. More is not
always better: six notes that are relevant beat twenty that are adjacent.

## Working with the answer

`ctrl-y` inserts the last answer at the cursor. Selecting lines and asking
about them from the right-click menu sends just those lines, which is the
difference between "what does this mean" and "what is this note about".

## What it does not do

It does not write to your vault on its own. Every edit is one you made, or one
you accepted with `ctrl-y`.
