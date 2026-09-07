---
id: 49
title: CI gates the site the way it gates the crate
type: chore
status: done
milestone: v1.0
depends_on:
- 66
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: site
---

## Problem

CI currently gates the crate: formatting, clippy with `-D warnings`, tests on
two runners, and a release build. The site would arrive with none of that. A
docs site rots in ways the compiler cannot see — a link to a renamed page, a
screenshot of an interface that changed, a stylesheet referring to a theme that
was deleted — and every one of those reaches a reader rather than a developer.

## Proposal

A `site` job in `.github/workflows/ci.yml`, held to the same standard as the
crate.

- **Build it.** `site build` with warnings as errors. Broken internal links
  already fail the build per #0066; CI is where that is enforced on a
  branch nobody ran locally.
- **Check the anchors, not just the pages.** A link to `page#section` where
  that heading was renamed is a link that lands at the top of the page and
  looks like it worked.
- **Check external links, on a schedule and not on every pull request.** They
  fail for reasons that have nothing to do with the change under review;
  breaking an unrelated PR on someone else's expired certificate is how link
  checking gets switched off. A weekly job that opens an issue is the right
  shape.
- **Assert the generated things are current** — the palette from #0047, the
  screenshots from #0048. Regenerate, `git diff --exit-code`, fail with the
  command to run.
- **Serve it and hit it.** Boot the server, request every page in the sitemap,
  fail on any status that is not 200 and on any page whose body is empty. That
  is #0052, run here.
- **Cache by lockfile**, reusing the existing cargo cache keys, so the job adds
  a minute and not five.

## Acceptance criteria

- [ ] A pull request that breaks an internal link or an anchor fails CI
- [ ] A pull request that changes the status bar without regenerating the
      screenshots fails, and the message says how to fix it
- [ ] External link failures never fail an unrelated pull request
- [ ] The site job runs on pull requests and on `main`, and finishes in under
      two minutes warm
- [ ] `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings`
      cover the site crate too

## Notes

CLAUDE.md's warning applies here as much as anywhere: CI runs the latest stable
toolchain and knows lints yours does not. Two `unnecessary_sort_by` errors
reached `main` that way already, and a new crate is new surface for exactly
that.

Keep the site job independent of the `check` job so a formatting failure and a
broken link are reported together rather than one after the other.
