#!/usr/bin/env bash
# Extract a clean, review-ready feature branch from a personal workspace branch
# by filtering out all personal/agent planning artifacts (.planning, .agents, etc.).
#
# Usage:
#   ./scripts/cut-pr-branch.sh [TARGET_PR_BRANCH] [BASE_BRANCH] [WORKSPACE_BASE]
#
# Examples:
#   On branch workspace/phase-45:
#     ./scripts/cut-pr-branch.sh                # Creates feature/phase-45 off origin/develop
#     ./scripts/cut-pr-branch.sh feature/fix-ui # Explicit PR branch name
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

CURRENT_BRANCH="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
if [ -z "$CURRENT_BRANCH" ]; then
    echo "error: detached HEAD detected. Switch to your feature worktree/branch first." >&2
    exit 1
fi

BASE_REMOTE="${BASE_REMOTE:-origin}"
BASE_BRANCH="${2:-develop}"
BASE_REF="$BASE_REMOTE/$BASE_BRANCH"

# Infer PR branch name if not explicitly passed
if [ -n "${1:-}" ]; then
    PR_BRANCH="$1"
else
    # Strip workspace/ or personal/ prefix and optional handle
    SLUG="${CURRENT_BRANCH#workspace/}"
    SLUG="${SLUG#personal/}"
    # Strip leading <handle>- if present
    SLUG="${SLUG#*-}"
    PR_BRANCH="feature/$SLUG"
fi

if [ "$PR_BRANCH" = "$CURRENT_BRANCH" ]; then
    echo "error: PR branch name '$PR_BRANCH' matches current branch. Specify a distinct branch name." >&2
    exit 1
fi

# Determine the base workspace branch the current branch forked from
WORKSPACE_BASE="${3:-workspace/denniyahh}"
if ! git show-ref --verify --quiet "refs/heads/$WORKSPACE_BASE"; then
    WORKSPACE_BASE="$CURRENT_BRANCH"
fi

echo "==> Validating working tree..."
if ! git diff-index --quiet HEAD --; then
    echo "error: working tree has uncommitted changes. Commit or stash them before cutting a PR branch." >&2
    exit 1
fi

echo "==> Fetching latest $BASE_BRANCH from $BASE_REMOTE..."
git fetch "$BASE_REMOTE" "$BASE_BRANCH"

# Find fork point from base workspace branch
FORK_POINT=$(git merge-base "$CURRENT_BRANCH" "$WORKSPACE_BASE" 2>/dev/null || true)
if [ -z "$FORK_POINT" ] || [ "$FORK_POINT" = "$(git rev-parse "$CURRENT_BRANCH")" ]; then
    # If not diverged from workspace base, look relative to BASE_REF
    FORK_POINT=$(git merge-base "$CURRENT_BRANCH" "$BASE_REF")
fi

# Check commits ahead of fork point
COMMITS_AHEAD=$(git rev-list --count "$FORK_POINT".."$CURRENT_BRANCH" 2>/dev/null || true)
if [ -z "$COMMITS_AHEAD" ] || [ "$COMMITS_AHEAD" -eq 0 ]; then
    echo "error: no commits found on '$CURRENT_BRANCH' ahead of fork point '$FORK_POINT'." >&2
    exit 1
fi

echo "==> Source branch: $CURRENT_BRANCH ($COMMITS_AHEAD commits ahead of fork point)"
echo "==> Target clean PR branch: $PR_BRANCH (rooted at $BASE_REF)"

# Regex of forbidden paths that must NEVER exist on PR/upstream branches
FORBIDDEN_REGEX='^(\.agents|\.bg-shell|\.claude|\.codex|\.gemini|\.omx|\.opencode|CLAUDE\.md|\.mcp\.json|skills/|skills-lock\.json|\.gsd|\.gsd-backups|\.gsd-id|\.gsd-worktrees|\.planning|\.devflow|\.worktrees)'

# Build list of commit hashes in chronological order
COMMIT_LIST=($(git rev-list --reverse "$FORK_POINT".."$CURRENT_BRANCH"))

# Create target clean branch off BASE_REF
echo "==> Initializing clean branch '$PR_BRANCH' from $BASE_REF..."
git branch -f "$PR_BRANCH" "$BASE_REF"

ORIG_BRANCH="$CURRENT_BRANCH"
cleanup() {
    local cur="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
    if [ "$cur" = "$PR_BRANCH" ] && [ "$cur" != "$ORIG_BRANCH" ]; then
        git checkout "$ORIG_BRANCH" --quiet 2>/dev/null || true
    fi
}
trap cleanup EXIT

git checkout "$PR_BRANCH" --quiet

INCLUDED_COUNT=0
EXCLUDED_COUNT=0

for HASH in "${COMMIT_LIST[@]}"; do
    TOUCHED_FILES=$(git diff-tree --no-commit-id --name-only -r "$HASH")
    NON_FORBIDDEN=$(echo "$TOUCHED_FILES" | grep -v -E "$FORBIDDEN_REGEX" | grep -v '^$' || true)

    if [ -z "$NON_FORBIDDEN" ]; then
        # Commit touches only personal/agent planning files
        EXCLUDED_COUNT=$((EXCLUDED_COUNT + 1))
        continue
    fi

    # Commit contains code changes; cherry-pick without auto-commit
    git cherry-pick "$HASH" --no-commit >/dev/null 2>&1 || true

    # Remove forbidden files and unmerged planning paths from index
    git rm -rf .agents .bg-shell .claude .codex .gemini .omx .opencode CLAUDE.md .mcp.json skills skills-lock.json .gsd .planning .devflow .worktrees >/dev/null 2>&1 || true

    # Check for unmerged files left behind
    UNMERGED=$(git diff --name-only --diff-filter=U || true)
    if [ -n "$UNMERGED" ]; then
        # Check if unmerged files are only forbidden paths
        FORBIDDEN_UNMERGED=$(echo "$UNMERGED" | grep -E "$FORBIDDEN_REGEX" || true)
        if [ -n "$FORBIDDEN_UNMERGED" ]; then
            for f in $FORBIDDEN_UNMERGED; do
                git rm -f "$f" >/dev/null 2>&1 || true
            done
        fi
        REMAINING_UNMERGED=$(git diff --name-only --diff-filter=U || true)
        if [ -n "$REMAINING_UNMERGED" ]; then
            echo "error: unresolved code conflict in commit $HASH on:" >&2
            echo "$REMAINING_UNMERGED" | sed 's/^/  /' >&2
            git cherry-pick --abort >/dev/null 2>&1 || git reset --hard HEAD --quiet
            git checkout "$ORIG_BRANCH" --quiet
            exit 1
        fi
    fi

    # Check if there are staged code changes left to commit
    if git diff --cached --quiet; then
        git reset --hard HEAD --quiet
        EXCLUDED_COUNT=$((EXCLUDED_COUNT + 1))
    else
        git commit -C "$HASH" --no-verify --quiet
        INCLUDED_COUNT=$((INCLUDED_COUNT + 1))
    fi
done

echo "==> Commit classification summary:"
echo "    Included: $INCLUDED_COUNT code commit(s)"
echo "    Excluded: $EXCLUDED_COUNT planning/environment-only commit(s)"

if [ "$INCLUDED_COUNT" -eq 0 ]; then
    echo "error: no code changes remained after filtering personal artifacts. PR branch is empty." >&2
    git checkout "$ORIG_BRANCH" --quiet
    git branch -D "$PR_BRANCH" --quiet
    exit 1
fi

echo "==> Verifying zero forbidden files on $PR_BRANCH (pre-push policy audit)..."
LEAKED="$(git log --name-only --format='' "$BASE_REF..$PR_BRANCH" | grep -E "$FORBIDDEN_REGEX" || true)"
if [ -n "$LEAKED" ]; then
    echo "error: forbidden files detected on clean PR branch '$PR_BRANCH':" >&2
    echo "$LEAKED" | head -10 | sed 's/^/  /' >&2
    git checkout "$ORIG_BRANCH" --quiet
    exit 1
fi

echo "==> Audit passed: '$PR_BRANCH' carries ZERO personal/agent artifacts."

# Return to original branch
git checkout "$ORIG_BRANCH" --quiet
trap - EXIT

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Clean PR Branch Ready: $PR_BRANCH"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Next steps:"
echo "  1. Test:      git checkout $PR_BRANCH && ./scripts/check.sh"
echo "  2. Push:      git push -u origin $PR_BRANCH"
echo "  3. Open PR:   gh pr create --base $BASE_BRANCH --head $PR_BRANCH"
echo ""
