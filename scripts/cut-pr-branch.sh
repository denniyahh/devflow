#!/usr/bin/env bash
# Extract a clean, review-ready feature branch from a personal workspace branch
# by filtering out all personal/agent planning artifacts (.planning, .agents, etc.).
#
# Usage:
#   ./scripts/cut-pr-branch.sh [--phase N] [TARGET_PR_BRANCH] [BASE_BRANCH] [WORKSPACE_BASE]
#
# Phase mode (--phase N, or automatic on a feature/phase-N branch) replays only
# the commits in BASE_BRANCH..HEAD whose conventional-commit scope names phase N
# (`(48)`, `(48-07)`, `(phase-48)`), the scope GSD's execute-phase commits use.
# Phase work committed on the workspace branch before the phase branch split
# off is included, and unrelated workspace commits are left out, whatever the
# fork point. The default PR branch is feature/phase-N-pr.
#
# Without a phase, the branch's commits since its fork from WORKSPACE_BASE are
# replayed.
#
# Examples:
#   On branch feature/phase-48:
#     ./scripts/cut-pr-branch.sh                # Creates feature/phase-48-pr off origin/develop
#   On branch workspace/phase-45:
#     ./scripts/cut-pr-branch.sh                # Creates feature/phase-45 off origin/develop
#     ./scripts/cut-pr-branch.sh feature/fix-ui # Explicit PR branch name
set -euo pipefail

PHASE="${PHASE:-}"
POSITIONAL=()
while [ $# -gt 0 ]; do
    case "$1" in
        --phase)
            [ $# -ge 2 ] || { echo "error: --phase needs a phase number" >&2; exit 1; }
            PHASE="$2"
            shift 2
            ;;
        --phase=*)
            PHASE="${1#--phase=}"
            shift
            ;;
        *)
            POSITIONAL+=("$1")
            shift
            ;;
    esac
done
set -- "${POSITIONAL[@]+"${POSITIONAL[@]}"}"

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

if [ -z "$PHASE" ] && [[ "$CURRENT_BRANCH" =~ ^feature/phase-([0-9]+(\.[0-9]+)*)$ ]]; then
    PHASE="${BASH_REMATCH[1]}"
fi
if [ -n "$PHASE" ]; then
    if ! [[ "$PHASE" =~ ^[0-9]+(\.[0-9]+)*$ ]]; then
        echo "error: phase '$PHASE' is not a phase number (e.g. 48 or 23.1)." >&2
        exit 1
    fi
    PHASE_INT="${PHASE%%.*}"
    PHASE="$((10#$PHASE_INT))${PHASE#"$PHASE_INT"}"
fi

# Infer PR branch name if not explicitly passed
if [ -n "${1:-}" ]; then
    PR_BRANCH="$1"
elif [ -n "$PHASE" ]; then
    PR_BRANCH="feature/phase-$PHASE-pr"
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

# Regex of forbidden paths that must NEVER exist on PR/upstream branches
FORBIDDEN_REGEX='^(\.agents|\.bg-shell|\.claude|\.codex|\.cursor|\.gemini|\.omx|\.opencode|AGENTS\.md|CLAUDE\.md|\.mcp\.json|skills/|skills-lock\.json|\.gsd|\.gsd-backups|\.gsd-id|\.gsd-worktrees|\.planning|\.devflow|\.worktrees|graphify-out/)'

if [ -n "$PHASE" ]; then
    # Scope anchored the way GSD's execute-phase matches plan commits: the
    # phase's leading integer may carry zero padding, a plan suffix is -NN.
    PHASE_ESC="${PHASE//./\\.}"
    SCOPE_RE="^[a-z]+\((phase-)?0*${PHASE_ESC}(-[0-9]+)?\)!?: "
    COMMIT_LIST=()
    while IFS=$'\t' read -r HASH SUBJECT; do
        if [[ "$SUBJECT" =~ $SCOPE_RE ]]; then
            COMMIT_LIST+=("$HASH")
        fi
    done < <(git log --reverse --no-merges --format='%H%x09%s' "$BASE_REF..$CURRENT_BRANCH")
    if [ "${#COMMIT_LIST[@]}" -eq 0 ]; then
        echo "error: no commits scoped to phase $PHASE in $BASE_REF..$CURRENT_BRANCH." >&2
        exit 1
    fi
    echo "==> Source branch: $CURRENT_BRANCH (${#COMMIT_LIST[@]} commits scoped to phase $PHASE not yet in $BASE_REF)"
else
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

    # Build list of commit hashes in chronological order
    mapfile -t COMMIT_LIST < <(git rev-list --reverse "$FORK_POINT".."$CURRENT_BRANCH")
fi

echo "==> Target clean PR branch: $PR_BRANCH (rooted at $BASE_REF)"

# Create target clean branch off BASE_REF
echo "==> Initializing clean branch '$PR_BRANCH' from $BASE_REF..."
git branch -f "$PR_BRANCH" "$BASE_REF"

ORIG_BRANCH="$CURRENT_BRANCH"
cleanup() {
    local cur
    cur="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
    if [ "$cur" = "$PR_BRANCH" ] && [ "$cur" != "$ORIG_BRANCH" ]; then
        git checkout "$ORIG_BRANCH" --quiet 2>/dev/null || true
    fi
}
trap cleanup EXIT

git checkout "$PR_BRANCH" --quiet

INCLUDED_COUNT=0
EXCLUDED_COUNT=0
INCLUDED_HASHES=()

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

    # Remove forbidden files and unmerged planning paths from index.
    # --ignore-unmatch: one pathspec that matches nothing makes git rm abort
    # and remove none of them, which leaked mixed-commit .planning files.
    git rm -rf --ignore-unmatch .agents .bg-shell .claude .codex .cursor .gemini .omx .opencode AGENTS.md CLAUDE.md .mcp.json skills skills-lock.json .gsd .planning .devflow .worktrees graphify-out >/dev/null 2>&1 || true

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
        INCLUDED_HASHES+=("$HASH")
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

# Fidelity report: every path the cut changes must come from a replayed commit,
# and should read exactly as it does on the source branch. A difference means
# the base diverges there, or a commit that was not replayed also touched it.
TOUCHED_SET="$(for HASH in "${INCLUDED_HASHES[@]}"; do git diff-tree --no-commit-id --name-only -r "$HASH"; done | grep -v -E "$FORBIDDEN_REGEX" | sort -u || true)"
CHANGED_SET="$(git diff --name-only "$BASE_REF" "$PR_BRANCH" | sort -u)"
UNEXPLAINED="$(comm -13 <(echo "$TOUCHED_SET") <(echo "$CHANGED_SET") | grep -v '^$' || true)"
if [ -n "$UNEXPLAINED" ]; then
    echo "error: '$PR_BRANCH' changes paths no replayed commit touched:" >&2
    echo "$UNEXPLAINED" | sed 's/^/  /' >&2
    git checkout "$ORIG_BRANCH" --quiet
    exit 1
fi
DIFFERING=""
while IFS= read -r f; do
    [ -n "$f" ] || continue
    if ! git diff --quiet "$PR_BRANCH" "$ORIG_BRANCH" -- "$f"; then
        DIFFERING+="  $f"$'\n'
    fi
done <<< "$CHANGED_SET"
echo "==> Fidelity: $(echo "$CHANGED_SET" | grep -c . || true) path(s) differ from $BASE_REF, all from replayed commits."
if [ -n "$DIFFERING" ]; then
    echo "warning: these paths do not match $ORIG_BRANCH; review each before pushing:"
    printf '%s' "$DIFFERING"
fi

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
