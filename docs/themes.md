---
order: 90
section: Reference
description: Four built-ins, your own theme files, and Ghostty themes read directly.
---

# Themes

A theme is a set of *roles* rather than a list of colours, so it can be
swapped without touching anything else.

```toml
theme = "gotham"
```

Ships with **Gotham** (the default), **Night**, **Paper**, and **Mono**.

Three other things work in that field, in this order:

```toml
theme = "my-theme"                            # <vault>/.trafford/themes/my-theme.toml
theme = "~/.config/ghostty/themes/gotham"     # a file, anywhere
theme = "Catppuccin Mocha"                    # a Ghostty theme, by name
```

That last one is the useful one: **trafford reads Ghostty theme files
directly**, so whatever your terminal is already wearing, trafford can wear
too. No transcribing, and every theme Ghostty ships works. Names match
loosely, so `catppuccin-mocha` finds `Catppuccin Mocha`.

`ctrl-k` → *Change theme* lists everything it can find and previews each one
as you move through the list; `esc` puts back the one you had.

## Writing one

A theme file states as little as it likes. Anything left out is derived by
mixing the background and the text, so five lines is a coherent theme:

```toml
name = "Squid"
background = "#0d1117"
text       = "#c9d1d9"
accent     = "#d29922"
link       = "#58a6ff"
```

The full set of roles is in `trafford/themes/gotham.toml`, which is an
ordinary theme file — the built-ins are compiled in and parsed by the same
code you would use, so the format cannot drift from the documentation.

## This website wears them too

The palette on this page is generated from those same theme files at build
time, and the switcher in the header offers the ones that translate.

> [!note] Why Mono is not offered here
> Mono sets its background and foreground to `reset`, meaning *whatever the
> terminal already is*. That is a useful thing to ask a terminal and an
> impossible thing to ask a browser, so rather than inventing a palette and
> calling it Mono, the site leaves it out.
