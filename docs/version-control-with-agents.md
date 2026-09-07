# Version control with agents

Two kinds of history live near this project, and they want different things.

- **This repository** — Rust source, reviewed like any other code.
- **A vault** — the markdown folder trafford opens, which is also a git repo,
  and which an assistant may be writing into.

Both are covered below. The common thread: an agent can produce a week of
diffs in an afternoon, so the bottleneck moves from *writing* changes to
*reviewing* them. Everything here optimises for the review.

---

## Part 1 — Agents working on this repository

### The loop

```sh
git switch -c feat/backlink-jump        # 1. never work on main
cargo test && cargo clippy --all-targets  # 2. know the baseline is green
#    ... make one coherent change ...
cargo fmt && cargo test                 # 3. prove it before committing
git add -p                              # 4. stage deliberately, not -A
git commit                              # 5. see "Commits" below
gh pr create --fill                     # 6. CI gates the merge
```

### Branches

`type/short-description`, where type is `feat`, `fix`, `refactor`, `docs`,
`test`, or `chore`. One branch per intent. If you notice a second thing worth
fixing, finish the first, then branch again — a branch that does two things
takes more than twice as long to review.

**Branch from `main`, not from another branch.** Chaining work off an unmerged
branch looks efficient and is not: a rebase-merge rewrites those commits, so
every branch built on them is left holding the old hashes and becomes
unmergeable. GitHub then **skips CI on an unmergeable pull request**, so what
you see is not a red build but no build at all — a pull request that looks like
it is still waiting. Rebase onto `main` and force-with-lease your own unmerged
branch to fix it.

If the next piece of work genuinely needs the last one, wait for the merge.

### Commits

A commit is a unit of review, not a save point. Each one should build, pass
tests, and be describable in a single line.

```
fix: keep backlink context in its original case

The index stored only a lowercased copy of each note for searching, and the
context pane read backlinks out of it, so every quoted line appeared folded.
WikiLink now carries the source line it was parsed from.
```

- Subject: imperative, lowercase after the type prefix, under 72 characters.
- Body: why the change exists and what it affects. Skip it for a one-line
  change whose subject is self-evident.
- Never mention the tooling that produced the change in the subject line.

**Provenance.** When an agent authored a commit, record it in a trailer rather
than in the subject, so `git log --oneline` stays readable and
`git log --grep` can still find them:

```
Co-Authored-By: Claude <noreply@anthropic.com>
```

### Run what CI runs

`cargo test` passing locally is not the same gate as CI. Match the commands
exactly — `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all
--check` — **and** the toolchain, because CI tracks the latest stable and a
lint that does not exist in your version cannot warn you about itself. When a
run fails, read the log before changing anything: `gh run view <id>
--log-failed`.

### Slicing

The single highest-leverage habit: commit in slices a human can hold in their
head. A useful test — if the diff cannot be reviewed in under five minutes,
it should have been more than one commit.

- Mechanical changes (rename, format, move a file) go in their own commit,
  never mixed with behavioural ones. A reviewer can then skip them safely.
- Behavioural changes carry their tests in the same commit. A commit that adds
  a fix without the test that pins it is incomplete.

### What an agent must not do without being asked

- Commit or push to `main`.
- `push --force`, or `--force-with-lease`, onto a shared branch.
- `git reset --hard`, `git clean -fd`, or checkout over uncommitted work that
  it did not itself create.
- Rewrite published history (`rebase`, `commit --amend`) after a push.
- Merge its own pull request.
- Commit secrets, `.env` files, or anything under `target/`.

These are not stylistic. Each one destroys work that cannot be recovered from
the reflog by someone who does not know to look.

### Leaving the tree clean

Finish a task with `git status` clean and the branch pushed, or with the work
explicitly described as in progress. A half-staged index is the worst possible
handoff state: the next agent — or the next human — cannot tell what was
intended from what was left behind.

### Recovery

```sh
git reflog                       # every HEAD the repo has had, for 90 days
git reset --hard HEAD@{3}        # go back to one of them
git stash list                   # work parked mid-task
git fsck --lost-found            # commits with no branch pointing at them
```

Nothing that was ever committed is truly gone. Nothing that was *not* committed
is recoverable at all — which is the argument for committing early and often on
a branch, and squashing later if the history is untidy.

---

## Part 2 — Versioning a vault an agent writes into

A vault is not source code. Nobody reviews a note before it lands, the author
is usually the only reader, and the value is in the history being continuous
rather than curated. Different rules follow.

### Commit often, automatically

Set `autocommit_secs` in `<vault>/.trafford/config.toml` and stop thinking
about it:

```toml
autocommit_secs = 300   # commit the vault after five minutes idle
```

trafford stages everything and writes a message naming what moved
(`vault: update journal/2026-09-06, Reading list`). This is the right default
for a personal vault: a dense, boring history you never have to curate, and
from which any note can be recovered at any point in time.

### Keep agent writes reviewable

The moment an assistant writes into the vault, "commit everything on idle"
stops being enough — you want to see what it wrote before it disappears into a
lump commit with your own edits.

- **Commit your own work first.** A clean tree before the agent runs means the
  agent's diff *is* the change.
- **Let the agent write to new notes, not into old ones.** trafford's
  "save last answer as a new note" exists for this: a new file is a diff that
  reads as an addition rather than a rewrite.
- **Review with `ctrl-g`, `d`.** The diff view in the git panel is the
  cheapest review surface there is. Stage the notes you want, commit those,
  and discard the rest with `X`.

### Syncing across machines

`ctrl-g` then `p` runs `pull --rebase --autostash`, which is the correct
incantation for a vault: your local edits replay on top of what the other
machine wrote, and uncommitted work is stashed and restored rather than
refused. Push before you close the laptop; pull before you start typing.

### Conflicts

Markdown conflicts are almost always additive — the same note appended to from
two machines. Resolve them by keeping both sides; there is rarely a reason to
choose. Conflicted files show in the git panel with a `!`.

### What not to version

`.gitignore` in a vault should cover `.DS_Store`, editor swap files, and any
local cache. Do not gitignore `.trafford/config.toml` — the config travelling
with the vault is the point.
