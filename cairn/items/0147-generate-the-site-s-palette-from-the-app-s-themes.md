---
id: 147
title: Generate the site's palette from the app's themes
type: chore
status: done
milestone: v1.0
depends_on:
- 166
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: theme
---

## Problem

The site will need colours, and there are two ways to get them: type hex values
into a stylesheet, or take them from `ui::theme::Theme`, which already names
every role the app draws with — `bg`, `surface`, `fg`, `muted`, `accent`,
`heading`, `link`, `broken`, `code`, `tag`, `added`, `modified`.

Typed by hand, the site's blue and the app's blue drift the first time a theme
is adjusted, and the screenshots on the page — which come out of the real
binary, per #0148 — stop matching the page around them. That mismatch is
subtle enough to look like a rendering bug in the screenshot.

## Proposal

Generate the palette. `site build` reads the built-in themes through
`Theme::builtin` and emits CSS custom properties, one block per theme:

```css
:root { --bg: #1a1b26; --fg: #c0caf5; --accent: #7aa2f7; --link: #7aa2f7; … }
[data-theme="dawn"] { … }
```

The stylesheet then only ever refers to `var(--accent)`. A theme added to the
app appears on the site the next time it builds, and a colour changed in the
app is changed on the site by the same edit.

The site's theme switcher is the app's theme list, and `prefers-color-scheme`
picks the first dark or light built-in as the default — the page should not
arrive bright white for someone whose system is not.

## Acceptance criteria

- [ ] Every colour in the stylesheet is `var(--…)`; no hex literal outside the
      generated file
- [ ] Adding a built-in theme to `ui::theme` adds it to the site with no other
      edit
- [ ] The generated CSS is deterministic, so #0149 can check it is current
- [ ] The page honours `prefers-color-scheme` on first load and remembers a
      choice after that
- [ ] Text over every generated background meets WCAG AA, asserted in a test
      rather than eyeballed

## Notes

The contrast test is the part worth insisting on. Terminal themes are designed
against a terminal's background and some of the roles — `faint`, `border` —
are deliberately low contrast; the same value under a browser's font rendering
can be genuinely unreadable. Compute the ratio in the generator and fail the
build, the way CLAUDE.md already advises for the TUI: look at colour, do not
reason about it.

`ratatui::style::Color` is not always RGB — a theme may name an indexed or
named colour. Map those explicitly and fail on anything unmappable rather than
guessing a hex value, which would silently invent a palette.
