---
id: 35
title: The development server binds port zero
type: feature
status: done
milestone: v1.0
depends_on:
- 33
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: m
area: site
---

## Problem

A development server on a fixed port is a shared global variable with a
process-wide lifetime, and this project is developed in a way that guarantees
collisions: work happens in git worktrees, several at once, sometimes with more
than one agent editing in parallel. Two checkouts both starting on `:3000`
gives one of two outcomes, and the bad one is not the error:

- the second server fails to bind, which is merely annoying; or
- the second server is *not started* and the browser tab keeps showing the
  first worktree's build, so you review a change that is not there.

The second failure is silent, survives a reload, and is indistinguishable from
"my change did nothing". Tests that bind a fixed port have the same problem in
smaller: they cannot run in parallel, and they fail on a developer's machine
because something unrelated already holds the port.

## Proposal

`site serve` binds `127.0.0.1:0` and asks the operating system what it got.

```
$ site serve
  http://127.0.0.1:52741   ← docs/  (watching 41 files)
```

- **The port is read after binding, never guessed before it.** Probing for a
  free port and then binding it is a race with every other process on the
  machine, and it is the standard way this is got wrong.
- **The URL is machine-readable too.** `target/site/.serve.json` carries
  `{ port, pid, root, started }` so a script, a test or an agent can find the
  running server without scraping stdout. It is removed on exit, and a file
  whose pid is no longer alive is treated as stale rather than believed.
- **`--port N` for when you want a stable URL** — a pinned browser tab, a
  bookmark. An explicit port that is taken is a loud failure: the user asked
  for that one.
- **Loopback only.** Never `0.0.0.0`. A docs preview has no business being
  reachable from the café's network, and defaulting to "any interface" is how
  it ends up there.
- **`--open` opens a browser**, using the port that was actually bound.

## Acceptance criteria

- [ ] `site serve` with no arguments never fails because a port is in use
- [ ] Two servers in two worktrees run simultaneously, each serving its own tree
- [ ] The bound URL is printed on the first line and written to `.serve.json`
- [ ] `--port` overrides, and fails with a clear message when that port is taken
- [ ] Nothing is reachable from another machine on the network
- [ ] Ctrl-C removes the state file; a state file left by a killed process is
      detected as stale and overwritten, not trusted
- [ ] A request for a path outside the output directory is refused — `..`,
      symlinks, and URL-encoded traversal all included

## Notes

`TcpListener::bind(("127.0.0.1", 0))` then `local_addr()` is the whole
mechanism; the discipline is in never introducing a second place that decides
what the port is.

Serve from `std::net` rather than adding a web framework. This project shells
out to `git` instead of taking `git2`, and uses `ureq` instead of `reqwest`;
a static file server for `GET` and `HEAD` over loopback is a couple of hundred
lines, and it lives in `site/`, which #0033 keeps out of the shipped binary.
Correct MIME types, `Range` support for the video an asciinema cast might
become, and byte-exact `Content-Length` — a truncated response looks like a
generator bug and costs an hour to find.

This is the item that makes #0042 possible: a smoke test can boot a real
server per test, in parallel, precisely because no test names a port.
