---
id: 32
title: What builds the site, and where does its code live?
type: spike
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: s
area: site
---

## Question

trafford needs a page on the internet: a landing page that shows what it is,
and documentation you can read without cloning the repo. Before any of that
there is one decision that fixes the cost of all of it.

**Does the site come from an off-the-shelf generator, or from this codebase?**

## Why it has to be answered before the work

Everything downstream — where the code lives, what CI runs, what the dev server
serves, whether a screenshot can go stale — follows from this and only this. It
also decides whether the project acquires a *second idea of what a markdown
document is*, which CLAUDE.md already names as the failure mode:

> **There is one heading scanner.** `ui::fold::headings` is it — the outline in
> the context pane, the fold state, and the reading view all read it. A second
> idea of the document's structure diverges exactly the way a second idea of
> the layout does.

A generator with its own parser is that second idea, shipped deliberately. It
may still be the right trade; it should not be made by default.

## Options

**A. mdBook.** Rust, one binary, `mdbook serve` already watches and reloads.
Costs nothing to stand up. Its output is recognisably mdBook, and a landing
page means fighting the theme rather than writing one. Its parser is
pulldown-cmark: `[[wikilinks]]`, `> [!note]` callouts and `#tags` — the three
things this project's notes are made of — are not markdown to it, and the docs
are written as a vault.

**B. Zola.** A general SSG in Rust; Tera templates, full control of the HTML,
one more config language and one more template dialect in the repo. Same parser
problem as A, without the free theme.

**C. A generator in this workspace, over the vault.** `vault::Index` already
resolves links, collects backlinks and indexes tags; `ui::fold::headings`
already finds the structure; `ui::table` and `ui::callout` already parse the
blocks. The docs are then a vault, which means the app can open its own manual
and the site is dogfood. Costs an HTML backend and a build we own.

The seam is where C stops being free: `ui::markdown::Renderer` returns
`Rendered`, a line of styled spans plus link ranges. That is a *line* model
built for a terminal, not a block tree, and HTML wants nesting — lists inside
lists, a paragraph inside a callout. So C is either "add an HTML backend behind
the existing block scanners" or "add pulldown-cmark for the site and accept two
parsers", and those are different sizes.

## What would settle it

- Render three real notes from the 127-note vault through each candidate — one
  with a table of CJK, one with nested callouts, one with `[[Note\|alias]]`
  links inside a table. A/B either drop those or need a preprocessor per
  feature, which is where mdBook setups rot.
- Count what the docs actually need: if the manual is fifteen pages of prose,
  a generator is cheap and A wins. If the docs are the vault, C is already
  written and A is a translation layer.
- Time `mdbook serve` against the dev server in #0035. If both are instant,
  the argument for A is only the theme, and the theme is the part being
  replaced.

## Answer

<!-- Filled in when the spike closes. -->
