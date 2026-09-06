#!/usr/bin/env bash
# Blocks git operations that destroy work which cannot be recovered from the
# reflog by someone who does not know to look. See
# docs/version-control-with-agents.md for the reasoning behind each rule.
#
# Reads the PreToolUse payload on stdin; exit 2 blocks the call and feeds the
# message on stderr back to the model.
set -uo pipefail

command=$(jq -r '.tool_input.command // empty')
[ -z "$command" ] && exit 0

block() {
  echo "blocked by .claude/hooks/git-guard.sh: $1" >&2
  echo "If this is genuinely what the task needs, ask first." >&2
  exit 2
}

# Normalise whitespace so "git   push  --force" is caught too.
normalised=$(printf '%s' "$command" | tr '\n' ' ' | tr -s ' ')

case "$normalised" in
  *"git push"*--force*|*"git push"*" -f "*|*"git push"*" -f")
    block "force-push. It overwrites commits on the remote that nobody can recover." ;;
  *"git reset --hard"*)
    block "git reset --hard. Uncommitted work is unrecoverable. Use 'git stash' instead." ;;
  *"git clean"*-*f*d*|*"git clean"*-*d*f*)
    block "git clean -fd. It deletes untracked files permanently." ;;
  *"git checkout ."*|*"git restore ."*)
    block "a whole-tree discard. Name the specific paths you mean to revert." ;;
  *"git branch -D"*)
    block "a forced branch delete. Use -d, which refuses to drop unmerged work." ;;
esac

# Committing straight to the default branch: branch first, per CONTRIBUTING.
case "$normalised" in
  *"git commit"*|*"git merge"*)
    branch=$(git branch --show-current 2>/dev/null)
    # An unborn HEAD means this is the repository's first commit, which has
    # nowhere else to go.
    if ! git rev-parse HEAD >/dev/null 2>&1; then
      exit 0
    fi
    if [ "$branch" = "main" ] || [ "$branch" = "master" ]; then
      block "a commit on '$branch'. Branch first: git switch -c type/short-description"
    fi
    ;;
esac

exit 0
