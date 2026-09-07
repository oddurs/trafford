---
id: 46
title: Reload the open page on save, over one connection
type: feature
status: done
milestone: v1.0
depends_on:
- 45
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: site
---

## Problem

The loop that matters when writing docs is save, look, adjust. Without a
reload, every iteration costs a rebuild command and a manual refresh, and the
refresh is the part that gets forgotten — you read the old page and conclude
the edit did nothing, the same failure as serving the wrong worktree in #0045
and just as quiet.

## Proposal

Watch the sources, rebuild, and tell the open page to reload.

- **Watch `docs/`, the templates, and the stylesheet.** Not `target/`, which
  the build writes to — watching your own output is how a rebuild loop becomes
  infinite.
- **Debounce.** An editor writing a file produces several filesystem events,
  and some write a temporary file and rename it. Coalesce a burst into one
  rebuild, and never run two rebuilds at once.
- **One SSE connection per page.** The server holds `text/event-stream` open
  and sends `reload` when a build finishes; the page listens and reloads. No
  polling, no websocket dependency, and a browser reconnects on its own after
  the server restarts.
- **The reload script exists only in `serve`.** It is injected into the
  response, not written into the file on disk, so the deployed HTML has no
  development code in it. A production artifact that carries a livereload
  script is the classic way this leaks.
- **A failed build does not reload.** It reports the error, over the same
  channel, as a banner on the page — with the file and line — and leaves the
  last good page up.

## Acceptance criteria

- [ ] Saving a note updates the open page without a keystroke, in under a second
      for a docs-sized tree
- [ ] Editing the stylesheet or a template rebuilds too
- [ ] A build error shows in the page and in the terminal, and the previous
      page stays readable
- [ ] Nothing in `site build` output contains the reload script
- [ ] Killing and restarting the server reconnects the open page — the same
      port has to be reused for that, which is what `--port` is for
- [ ] A burst of writes causes one rebuild, not one per event

## Notes

`notify` is the crate for this and it is the one dependency here worth taking;
walking the tree on a timer is fine for 41 files and stops being fine the first
time the vault under `docs/` grows.

Full rebuild first. Incremental rebuilds are an optimisation with a correctness
cost — a stale page that only appears when a link's target changes — and the
threshold to justify one is a build slow enough to notice, which a docs tree is
not yet.

## What changed on the way

**Polling, not `notify`.** The notes above said `notify` was the one dependency
worth taking here. It is not, for two reasons found while writing it: a docs
tree is dozens of files and a walk every 200 ms is free; and without it the
site crate adds *no* dependency the workspace did not already have, which turns
"site tooling never ships in the binary" from an argument into something
`cargo tree -p trafford` shows. A snapshot comparison also has one event per
settled state by construction, which is the debouncing the plan listed as
separate work.
