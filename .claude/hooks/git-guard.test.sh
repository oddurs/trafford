#!/usr/bin/env bash
# Tests for git-guard.sh.
#
# The guard is the only thing standing between an agent and work that cannot be
# recovered from the reflog, and it has already failed silently once: an
# `if: Bash(git *)` condition in settings.json matched only the start of the
# command line, so `cairn close 0013 && git commit` walked straight through and
# put a commit on main. Nothing noticed, because nothing was watching.
#
# The branch cases need the guard to believe it is on a particular branch, so a
# stub `git` goes on PATH ahead of the real one. Everything else runs against
# the script directly.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
guard="$here/git-guard.sh"
stub_dir=$(mktemp -d)
trap 'rm -rf "$stub_dir"' EXIT

cat > "$stub_dir/git" <<'STUB'
#!/bin/sh
case "$*" in
  "branch --show-current") echo "$FAKE_BRANCH" ;;
  "rev-parse HEAD") echo deadbeef ;;
  *) exit 0 ;;
esac
STUB
chmod +x "$stub_dir/git"

pass=0
fail=0

# check <expected: allow|block> <branch> <command> <description>
check() {
  local expect=$1 branch=$2 command=$3 what=$4 status
  printf '%s' "$command" \
    | jq -Rs '{tool_input:{command:.}}' \
    | env FAKE_BRANCH="$branch" PATH="$stub_dir:$PATH" "$guard" >/dev/null 2>&1
  status=$?
  local got=allow
  [ "$status" -eq 2 ] && got=block
  if [ "$got" = "$expect" ]; then
    pass=$((pass + 1))
    printf '  ok    %s\n' "$what"
  else
    fail=$((fail + 1))
    printf '  FAIL  %s\n        expected %s, got %s (exit %d)\n' "$what" "$expect" "$got" "$status"
  fi
}

echo "destructive commands, wherever they sit in the line"
check block feat/x 'git push --force'                      "force-push"
check block feat/x 'git push --force-with-lease origin x'  "force-with-lease"
check block feat/x 'git push -f origin main'               "push -f"
check block feat/x 'git reset --hard HEAD~1'               "reset --hard"
check block feat/x 'git clean -fd'                         "clean -fd"
check block feat/x 'git clean -df'                         "clean -df, flags reversed"
check block feat/x 'git checkout .'                        "whole-tree checkout"
check block feat/x 'git restore .'                         "whole-tree restore"
check block feat/x 'git branch -D old'                     "forced branch delete"
check block feat/x 'git   push   --force'                  "extra whitespace"
check block feat/x $'cargo test &&\ngit push --force'      "newline in the chain"

echo
echo "the failure this file exists for: git hidden behind a shell operator"
check block main   'cairn close 13 && git add -A && git commit -m x'  "commit on main, after &&"
check block main   'echo hi; git commit -m x'                         "commit on main, after ;"
check block main   'cargo test && git merge feat/x'                   "merge on main, after &&"
check block feat/x 'cargo build && git push --force'                  "force-push after &&"

echo
echo "work that should not be stopped"
check allow feat/x 'git commit -m "a change"'      "commit on a branch"
check allow feat/x 'git status --short'            "status"
check allow feat/x 'git push'                      "an ordinary push"
check allow feat/x 'git checkout src/ui/mod.rs'    "checkout of a named path"
check allow feat/x 'git branch -d merged'          "safe branch delete"
check allow feat/x 'ls -la'                        "a command with no git in it"

echo
echo "known over-blocking, kept deliberately"
# The guard matches text, not shell syntax, so a dangerous command quoted inside
# some other command is blocked too. Parsing the line properly would mean a
# shell parser inside a PreToolUse hook; erring toward a stopped-but-harmless
# call is the cheaper mistake, and the block message says how to proceed.
check block feat/x 'grep -r "git push --force" .'  "the words, inside a search string"
check block feat/x 'git checkout ./src/ui/mod.rs' "a path that starts with ./"

echo
echo "the wiring, which is where it actually broke"
# Every case above drives the script directly, so not one of them would have
# caught the original failure: the script was correct and never ran. What broke
# was the registration in settings.json. This is the case that pins it.
settings="$here/../settings.json"
if [ ! -f "$settings" ]; then
  fail=$((fail + 1))
  printf '  FAIL  settings.json is missing\n'
elif ! jq -e '.hooks.PreToolUse[]
              | select(.matcher == "Bash")
              | .hooks[]
              | select(.command | test("git-guard"))' "$settings" >/dev/null; then
  fail=$((fail + 1))
  printf '  FAIL  the guard is not registered on PreToolUse/Bash\n'
elif jq -e '.hooks.PreToolUse[]
            | select(.matcher == "Bash")
            | .hooks[]
            | select(.command | test("git-guard"))
            | has("if")' "$settings" >/dev/null; then
  fail=$((fail + 1))
  printf '  FAIL  the guard has an `if` condition again\n'
  printf '        It matches the start of the command line, so anything after\n'
  printf '        a && or a ; is invisible to it. That is how a commit reached main.\n'
else
  pass=$((pass + 1))
  printf '  ok    registered on every Bash call, with no `if` gate\n'
fi

echo
if [ "$fail" -gt 0 ]; then
  echo "$pass passed, $fail failed"
  exit 1
fi
echo "$pass passed"
