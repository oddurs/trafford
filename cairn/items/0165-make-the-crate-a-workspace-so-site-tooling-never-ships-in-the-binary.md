---
id: 165
title: Make the crate a workspace so site tooling never ships in the binary
type: chore
status: done
milestone: v1.0
depends_on:
- 164
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: m
area: packaging
---

## Problem

`trafford` is one binary crate with no `lib.rs`. A site generator that wants
`vault::Index` and `ui::fold::headings` cannot reach them, and anything added
to `[dependencies]` to serve HTTP — a listener, a file watcher, a template
engine — is compiled into, and shipped inside, the terminal app that someone
installs with `cargo install --path .`.

That is the wrong direction for a project whose release profile is `lto = true`,
`codegen-units = 1`, `strip = true`. Cold builds are also what CI pays for, on
two runners, on every push.

## Proposal

A workspace of two members:

```
Cargo.toml          # [workspace] members = ["trafford", "site"]
trafford/           # src/lib.rs + src/main.rs — unchanged behaviour
site/               # the generator and the dev server; depends on trafford
```

`src/lib.rs` exports what the site needs and nothing more: `vault`, `ui::fold`,
`ui::table`, `ui::callout`, `ui::theme`. `app`, `keymap`, `mouse` and `llm`
stay private to the binary. The one-way dependency direction CLAUDE.md states —
`vault` and `editor` know nothing about the UI — gains a second consumer that
would fail to compile if it were ever violated, which is a benefit rather than
a cost.

Nothing in `site/` is a dependency of `trafford/`. `cargo build -p trafford`
must not compile a single HTTP or watcher crate.

## Acceptance criteria

- [ ] `cargo install --path trafford` still installs a working `trafford`
- [ ] `cargo tree -p trafford` names no server, watcher or template dependency
- [ ] `cargo test`, `cargo fmt --all --check` and
      `cargo clippy --all-targets -- -D warnings` cover every member
- [ ] The unit tests move with the code they cover and still pass unchanged
- [ ] The install instructions in `README.md` still work as written

## Notes

Making `lib.rs` a re-export of the existing modules is the whole change; the
risk is that it becomes an excuse to make things `pub` that were not. Export
the four modules the site needs and stop — every extra `pub` is API this
project then has to keep working.

One `Cargo.lock` at the root, and keep the release profile there too: a
`[profile.release]` left behind in a member is silently ignored, and nobody
notices until a release binary is three times the size it was.

Do this before, not after, the generator exists. Moving 14,000 lines of source
is a mechanical change on a clean tree and an afternoon of merge conflicts on a
dirty one.
