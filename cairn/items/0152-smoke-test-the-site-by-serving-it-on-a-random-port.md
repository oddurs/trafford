---
id: 152
title: Smoke-test the site by serving it on a random port
type: chore
status: done
milestone: v1.0
depends_on:
- 167
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: site
---

## Problem

The site's failure modes are not compile errors. A page builds, deploys, and
returns 500 for a path with a trailing slash; a stylesheet is served as
`text/plain` and the page is unstyled; an asset is written but never linked. The
unit tests in the generator cannot see any of it, for the same reason
CLAUDE.md gives about the TUI: none of it is visible from inside the process
that produced it.

## Proposal

An integration test that starts the real server and makes real requests.

```rust
let server = Server::start(&fixture)?;   // binds 127.0.0.1:0
let res = get(server.url("/docs/getting-started/"))?;
assert_eq!(res.status, 200);
```

- Every path in the generated sitemap returns 200 with a non-empty body
- A directory URL without a trailing slash redirects rather than 404s
- `Content-Type` is right for HTML, CSS, SVG and the fonts
- A path traversal attempt — `..`, an encoded `..`, a symlink out of the tree —
  returns 403 or 404 and never a file
- `HEAD` agrees with `GET` on status and `Content-Length`
- The server shuts down cleanly and frees its port

Because every server binds port 0, these run in parallel, in any number, on a
machine already running two development servers. That is the payoff from
#0167 and the reason a fixed test port would be a mistake even here.

## Acceptance criteria

- [ ] The suite starts a server per test and never names a port
- [ ] `cargo test` passes with development servers already running
- [ ] Deleting a `Content-Type` mapping fails a test
- [ ] The traversal cases are tested and fail without the guard — verified by
      removing the guard, per the practice CLAUDE.md already insists on for
      regression tests

## Notes

`ureq` is already a dependency of the workspace, so the client costs nothing new.

Keep the fixture small and generated: a handful of notes, one table, one
callout, one image. A test suite that depends on the real `docs/` tree fails
every time someone writes a paragraph.
