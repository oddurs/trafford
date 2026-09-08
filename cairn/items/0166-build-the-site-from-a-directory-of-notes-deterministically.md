---
id: 166
title: Build the site from a directory of notes, deterministically
type: feature
status: done
milestone: v1.0
depends_on:
- 165
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: l
area: site
---

## Problem

The site has to come from somewhere, and the docs are already markdown in a
repo that knows how to read markdown. What is missing is the step that turns a
directory of notes into a directory of HTML, deterministically enough that CI
can diff it and a deploy can trust it.

## Proposal

`site build` reads `docs/` — which is a vault, opened by `vault::Index` exactly
as a user's vault is — and writes a static tree:

```
target/site/index.html          # the landing page
target/site/docs/<slug>/index.html
target/site/assets/…
target/site/sitemap.xml
```

Rules that make the output trustworthy rather than merely present:

- **Deterministic.** Same input, byte-identical output. Sorted iteration
  everywhere, no timestamps in the HTML, no hash of the build machine. This is
  what lets #0149 assert that the committed generated assets are current.
- **Never partially written.** Build into a temporary directory and swap, so a
  failed build leaves the last good site in place and the dev server never
  serves a half-written page.
- **Fails on a broken link.** A `[[wikilink]]` that resolves to nothing is an
  error at build time, not a 404 for a reader. Resolution goes through
  `vault::Index`, so it follows the same order as the app — exact relative
  path, then case-insensitive path, then filename stem.
- **One heading scanner.** Anchors come from `ui::fold::headings`, which is
  what the outline and the reading view use. The table of contents on the page
  and the outline in the app cannot disagree, because there is nothing for them
  to disagree with.
- **No client-side framework.** HTML and CSS, and JavaScript only where there
  is behaviour that cannot exist without it.

## Acceptance criteria

- [ ] `site build` produces a tree that opens correctly from `file://`, with no
      server, so it can be inspected without one
- [ ] Two builds of the same commit produce byte-identical output
- [ ] A broken wikilink, a missing image, or a heading anchor that collides
      fails the build and names the file and line
- [ ] A build that fails leaves the previous output untouched
- [ ] Every page is reachable from the landing page in at most two clicks
- [ ] Rendering a note with a table, a callout, a code fence and an aliased
      link inside a table produces the right HTML — the same four cases the
      terminal renderer already has tests for

## Notes

Shape depends on the answer to #0164; the acceptance criteria above do not.
They are properties of a build, not of a generator, and hold whether the HTML
comes from mdBook or from this workspace.

The output directory is `target/site` because `target/` is already ignored and
already the place a build result goes. The deployed artifact is what CI uploads
from there; nothing generated is committed except assets that must be diffable,
which is #0148.

## What changed on the way

**No `--base-url`.** The plan assumed the base path had to be a build input.
Making every href relative to its own page removes the input entirely: one
build is correct at a domain root, under the Pages project subpath, and over
`file://` with no server. `--site-url` survives, for the canonical tags and the
sitemap, which have to be absolute or they are not those things.

**`[text](other.md)` is checked as hard as `[[wikilink]]`.** Not in the plan,
and necessary the moment the docs were written: markdown that has to read well
on GitHub uses the second form, and a docs tree with two link syntaxes and one
link checker is a docs tree with unchecked links. Both resolve through
`vault::Index`, so both follow the app's resolution order.

**`pages.json` rather than an always-present sitemap.** The acceptance criteria
wanted a machine-readable list of pages for the smoke tests; a sitemap needs an
absolute origin, which a local build has no honest answer for. They are two
things, so they are two files, and the sitemap only appears with `--site-url`.
