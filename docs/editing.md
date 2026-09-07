---
order: 30
section: Using it
description: The modal editor — motions, operators, counts, and the things that are not vim.
---

# Editing

The editor is modal. `i` inserts, `esc` returns to normal mode, `v` and `V`
select.

## Motions

`h` `j` `k` `l` · `w` `b` · `0` `^` `$` · `gg` `G`

Counts work: `10G` goes to line ten, `3w` moves three words.

## Operators

`x` `dd` `dw` `D` · `cc` `cw` `C` · `yy` `p` `P` · `>>` `<<` · `u` and
`ctrl-r`.

Counts compose the way you expect: `2dd`, `3>>`.

## Not vim

A few things are deliberately not vim, because a knowledge base is not a
source file.

- **Enter continues a list.** A new line under `- item` starts `- `, and under
  `3. item` starts `4. `. An empty item ends the list.
- **`space` toggles the task** on the current line, in normal mode.
- **Long lines fold** rather than scrolling sideways, because a paragraph is
  not a line of code. `wrap = false` in the config turns that off.
- **`ctrl-s` saves.** `:w` is not a thing here; there is no command line.

## Reading

`ctrl-e` switches to the reading view, which is a different posture rather
than the editor with the syntax hidden — see [Reading](reading.md).

## Everything else

`f1` lists every key, in the same table the palette searches. `ctrl-k` opens
the command palette, which is the same list by name rather than by key.
