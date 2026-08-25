#!/usr/bin/env bash
#
# Sync main back into develop after a release or PR merge into main.
#
# Every release lands on `main` via a squash-merge PR (GitHub's merge-button
# settings on this repo only allow "squash", not a real merge commit). A
# squash commit has no parent relationship to develop, so develop never
# learns that main moved — the NEXT release PR then conflicts against the
# stale merge-base.
#
# Because `develop` is a protected branch, direct pushes are rejected.
# This script:
#   1. Fetches latest main and develop.
#   2. Fast-forwards local develop if origin/develop is ahead.
#   3. If origin/main is not yet merged into develop:
#      - Creates/resets a temporary `sync/main-into-develop` branch off origin/develop.
#      - Performs a content-preserving `-X ours` merge of `origin/main`.
#      - Pushes the branch to origin and creates a PR into develop via `gh pr create`.
#   4. If --finish is passed (or if origin/main is already merged upstream):
#      - Fast-forwards local develop to origin/develop and cleans up the temporary branch.
#
# Usage:
#   scripts/sync-main-to-develop.sh           # Merge, branch, push, and open PR
#   scripts/sync-main-to-develop.sh --finish  # Post-merge cleanup and local develop sync
#
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

FINISH_MODE=false
if [ "${1:-}" = "--finish" ]; then
    FINISH_MODE=true
fi

echo "==> Fetching latest main and develop from origin..."
git fetch origin main develop --quiet

# Fast-forward local develop if it's behind origin/develop
if git show-ref --verify --quiet refs/heads/develop; then
    git fetch origin develop:develop --quiet || true
fi

# Check if origin/main is already an ancestor of origin/develop
if git merge-base --is-ancestor origin/main origin/develop; then
    echo "==> origin/main is already an ancestor of origin/develop — upstream is in sync."
    if git show-ref --verify --quiet refs/heads/sync/main-into-develop; then
        echo "==> Cleaning up local sync/main-into-develop branch..."
        git branch -D sync/main-into-develop --quiet || true
    fi
    exit 0
fi

if [ "$FINISH_MODE" = true ]; then
    echo "ERROR: origin/main is NOT yet merged into origin/develop." >&2
    echo "  Merge the sync PR on GitHub first (using 'Create a merge commit'), then run --finish." >&2
    exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "ERROR: working tree is not clean. Commit, stash, or discard changes first." >&2
    exit 1
fi

SYNC_BRANCH="sync/main-into-develop"
echo "==> Preparing sync branch '$SYNC_BRANCH' from origin/develop..."
git checkout -B "$SYNC_BRANCH" origin/develop --quiet

echo "==> Merging origin/main into $SYNC_BRANCH (-X ours; develop's content wins on any overlap)..."
BEFORE_TREE="$(git rev-parse HEAD^{tree})"

git merge -X ours origin/main --no-edit -m "merge: sync main back into develop

Standing post-release/post-main sync step (scripts/sync-main-to-develop.sh) —
keeps main a real ancestor of develop so the next release PR doesn't conflict
against a stale merge-base. -X ours: develop's content is authoritative;
this should be a no-op content-wise (verified below)."

AFTER_TREE="$(git rev-parse HEAD^{tree})"
if [ "$BEFORE_TREE" != "$AFTER_TREE" ]; then
    echo "WARNING: the merge changed develop's tree (before: $BEFORE_TREE, after: $AFTER_TREE)." >&2
    echo "This means main had content develop genuinely lacked — inspect 'git show HEAD' before pushing." >&2
    exit 1
fi

echo "==> Confirmed: tree is unchanged — pure history-linking merge."
echo "==> Pushing $SYNC_BRANCH to origin..."
git push -u origin "$SYNC_BRANCH" --force-with-lease

echo "==> Creating sync pull request targeting develop..."
if command -v gh >/dev/null 2>&1; then
    gh pr create --base develop --head "$SYNC_BRANCH" \
        --title "merge: sync main back into develop" \
        --body "Sync \`main\` back into \`develop\` following merge to maintain accurate ancestry and clean merge-bases.

**NOTE:** This PR MUST be merged with **'Create a merge commit'**, NOT squashed." || true
else
    echo "gh CLI not found; please create a PR from '$SYNC_BRANCH' into 'develop' manually."
fi

echo ""
echo "────────────────────────────────────────────────────────────────────────"
echo "Next Steps:"
echo "  1. Merge the PR on GitHub using 'Create a merge commit' (DO NOT SQUASH)."
echo "  2. Once merged, run: scripts/sync-main-to-develop.sh --finish"
echo "────────────────────────────────────────────────────────────────────────"
