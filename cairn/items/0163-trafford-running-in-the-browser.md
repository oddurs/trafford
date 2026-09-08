---
id: 163
title: trafford, running in the browser
type: feature
status: backlog
milestone: later
depends_on:
- 157
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: xl
area: site
---

## Problem

The site describes a terminal application. The one thing that would settle
every question a reader has — what is it actually like to use — is a thing they
have to install it to find out.

## Proposal

Run trafford in the page.

`ratatui::backend::TestBackend` renders to a `Buffer` rather than to a
terminal, and a `Buffer` is a grid of styled cells that paints into the DOM
directly. No pty, no terminal emulator, no `xterm.js`. `keymap.rs` is already
the pure key-routing layer, so the browser only has to turn a `keydown` into
the `KeyEvent` the program already understands.

What stands in the way is I/O, and it is countable: `git.rs` shells out in six
places, `App` and `vault::Index` read files in twenty-eight. Behind a `wasm`
feature those become an in-memory vault and a git that reports a clean tree.

- A fixture vault compiled in — the same one the screenshots use
- Real keys: `ctrl-p`, `ctrl-e`, `za`, `hjkl`, everything
- A "this is the real program" line under it, linking to the source, because
  the claim is only worth making if it is checkable

## Acceptance criteria

- [ ] The demo runs the same `keymap` and `ui` code the binary runs
- [ ] `cargo build -p trafford` is unaffected — the wasm feature is off by
      default and CI proves it
- [ ] The bundle is under 500 KiB gzipped and is loaded on interaction, never
      on page load
- [ ] Keyboard focus is trapped only while the demo has focus, and `esc`
      releases it
- [ ] It degrades to the cast when JavaScript or wasm is unavailable

## Notes

Filed under `later` and sized `xl` deliberately: this is a week, most of it
spent feature-gating I/O that currently assumes a filesystem — which is
invasive in exactly the modules that are hardest to test.

Worth doing anyway. Nobody expects a terminal application to be playable on its
own landing page, and the reason nobody does it is that most of them cannot
separate their key handling from their event loop. This one already has.
