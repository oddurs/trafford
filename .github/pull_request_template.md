## What this changes

<!-- One paragraph. What behaviour is different after this merges? -->

## Why

<!-- The problem, not the solution. Link an issue if there is one. -->

## How to see it

<!-- The commands or keystrokes a reviewer runs to watch it work.
     e.g. `cargo run -- init /tmp/v && cargo run -- /tmp/v`, then ctrl-g, d -->

## Checks

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] New behaviour has a test next to the code it covers
- [ ] Commits are sliced so each one reviews on its own

<!-- If an agent wrote part of this, say so here and keep the
     Co-Authored-By trailer on the commits. See
     docs/version-control-with-agents.md -->
