# The site's typeface

**JetBrains Mono**, variable weight axis, under the SIL Open Font License 1.1 —
`OFL.txt` beside it. Shipping a font without its licence is not shipping it.

`JetBrainsMono[wght].ttf` is the upstream file, kept because the subset is
derived from it. `jetbrains-mono.woff2` is what the site serves, cut down by
`tools/subset-font.py` to the characters the built site actually draws.

```sh
.venv/bin/python tools/subset-font.py           # regenerate
.venv/bin/python tools/subset-font.py --check   # fail if a glyph is missing
```

The check is the part that matters: without it, prose gains a character the
subset does not have, the browser falls back mid-word, and it reads as a
rendering bug in one glyph rather than as a missing glyph.
