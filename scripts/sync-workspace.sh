#!/usr/bin/env bash
# Sync shared codebase changes from develop into the current personal workspace branch
# using true git merge to maintain graph ancestry without wiping out tracked personal
# agent configurations, skills, or planning files.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

CURRENT_BRANCH="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
if [ -z "$CURRENT_BRANCH" ]; then
    echo "error: detached HEAD detected. Switch to your workspace branch first." >&2
    exit 1
fi

if [[ "$CURRENT_BRANCH" != workspace/* && "$CURRENT_BRANCH" != personal/* ]]; then
    echo "error: sync-workspace.sh must be run from a workspace/* or personal/* branch (currently on '$CURRENT_BRANCH')." >&2
    echo "  For standard branches, use standard git merge or rebase workflows." >&2
    exit 1
fi

BASE_REMOTE="${1:-origin}"
BASE_BRANCH="${2:-develop}"

echo "==> Fetching latest $BASE_BRANCH from $BASE_REMOTE..."
git fetch "$BASE_REMOTE" "$BASE_BRANCH"
if git show-ref --verify --quiet "refs/heads/$BASE_BRANCH"; then
    if ! git worktree list 2>/dev/null | grep -q "\[$BASE_BRANCH\]"; then
        git fetch "$BASE_REMOTE" "$BASE_BRANCH":"$BASE_BRANCH" --quiet 2>/dev/null || true
    fi
fi

echo "==> Merging $BASE_REMOTE/$BASE_BRANCH into $CURRENT_BRANCH..."
if git merge-base --is-ancestor "$BASE_REMOTE/$BASE_BRANCH" "$CURRENT_BRANCH"; then
    echo "==> $CURRENT_BRANCH is already up to date with $BASE_REMOTE/$BASE_BRANCH."
else
    git merge "$BASE_REMOTE/$BASE_BRANCH" -m "chore: sync $BASE_BRANCH into $CURRENT_BRANCH"
    echo "==> Successfully merged $BASE_REMOTE/$BASE_BRANCH into $CURRENT_BRANCH."
fi
