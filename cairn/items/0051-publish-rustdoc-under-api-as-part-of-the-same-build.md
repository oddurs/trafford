---
id: 51
title: Publish rustdoc under /api as part of the same build
type: chore
status: done
milestone: v1.0
depends_on:
- 66
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: docs
---

## Problem

`cargo doc` output exists for free and goes nowhere. Once the crate is a
workspace with a real `lib.rs`, the modules the site depends on — `vault`,
`ui::fold`, `ui::theme` — are a public API with doc comments, and the natural
place to link "how link resolution works" from the prose is the type that does
it.

## Proposal

Build rustdoc as part of the site build and publish it under `/api`.

- `cargo doc --no-deps --workspace` into `target/site/api/`
- A redirect at `/api/` to the crate's index, since rustdoc's own entry point
  is a path nobody types
- The prose links into it by item path, checked by the same link checker as
  everything else, so a renamed type breaks the build rather than the reader
- Documentation warnings as errors: `RUSTDOCFLAGS="-D warnings"` catches broken
  intra-doc links the moment they are written

## Acceptance criteria

- [ ] `/api` is on the deployed site and is current with the commit
- [ ] A broken intra-doc link fails the build
- [ ] A prose link to a type that was renamed fails the build
- [ ] `site build` without rustdoc still produces a complete site — the docs
      build is slow, and a developer iterating on the landing page should not
      pay for it

## Notes

That last criterion is why this is a separate item and not part of #0066:
`cargo doc` over the workspace is tens of seconds, and #0046 lives or dies
on the rebuild staying under a second. Rustdoc is a CI-and-deploy step with a
local opt-in flag, not part of the watch loop.
