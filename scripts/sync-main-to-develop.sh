#!/usr/bin/env bash
#
# Sync main back into develop after a release.
#
# Every release lands on `main` via a squash-merge PR (GitHub's merge-button
# settings on this repo only allow "squash", not a real merge commit). A
# squash commit has no parent relationship to develop, so develop never
# learns that main moved — the NEXT release PR then conflicts against the
# stale merge-base. This bit us going into v1.5.0 (main and develop had
# diverged since v1.4.0 / PR #10, producing 11 file conflicts on the
# release PR).
#
# The fix is this script: after every squash-merge release, run it once to
# record main's new tip as a real ancestor of develop. `-X ours` keeps
# develop's own content untouched (verified: the resulting tree is
# byte-identical to develop's pre-merge tree) while still linking the
# history, so the next release PR has a clean merge-base again.
#
# Usage: scripts/sync-main-to-develop.sh
# Run from a clean, up-to-date local checkout after main has been fetched.
#
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

if [ -n "$(git status --porcelain)" ]; then
    echo "ERROR: working tree is not clean. Commit, stash, or discard changes first." >&2
    exit 1
fi

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$CURRENT_BRANCH" != "develop" ]; then
    echo "ERROR: must be run from 'develop' (currently on '$CURRENT_BRANCH')." >&2
    exit 1
fi

echo "Fetching latest main and develop..."
git fetch origin main develop --quiet

if git merge-base --is-ancestor origin/main HEAD; then
    echo "origin/main is already an ancestor of develop — nothing to sync."
    exit 0
fi

echo "Merging origin/main into develop (-X ours; develop's content wins on any overlap)..."
BEFORE_TREE="$(git rev-parse HEAD^{tree})"

git merge -X ours origin/main --no-edit -m "merge: sync main back into develop after release

Standing post-release step (scripts/sync-main-to-develop.sh) — keeps main
a real ancestor of develop so the next release PR doesn't conflict against
a stale merge-base. -X ours: develop's content is authoritative; this
should be a no-op content-wise (verified below)."

AFTER_TREE="$(git rev-parse HEAD^{tree})"
if [ "$BEFORE_TREE" != "$AFTER_TREE" ]; then
    echo "WARNING: the merge changed develop's tree (before: $BEFORE_TREE, after: $AFTER_TREE)." >&2
    echo "This means main had content develop genuinely lacked — inspect 'git show HEAD' before pushing." >&2
    exit 1
fi

echo "Confirmed: develop's tree is unchanged — this was a pure history-linking merge."
echo
echo "Review with: git show HEAD --stat"
echo "Push with:   git push origin develop"
