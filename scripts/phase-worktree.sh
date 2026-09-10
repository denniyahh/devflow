#!/usr/bin/env bash
# Create (or re-attach to) the dedicated worktree for a phase.
#
# CLAUDE.md specifies this sequence, and every step of it is skippable by hand
# -- which is how phase work keeps landing on the wrong branch. The steps:
#
#   1. sync workspace/denniyahh with develop        (the fork point must be current)
#   2. derive the branch name from ROADMAP.md       (renumbered phases: 36 -> 35.3)
#   3. create .worktrees/phase-N from the workspace branch, NOT develop
#   4. verify .planning/ is actually tracked on the base
#
# Step 3's base matters more than it looks. `.gitignore` on `develop` ignores
# `.planning/` wholesale, so a develop-based worktree cannot commit CONTEXT.md,
# PLAN.md or STATE.md at all -- `git add` refuses an ignored path and the
# phase's entire planning record sits ignored on disk. Step 4 asserts against
# that rather than trusting it.
set -euo pipefail

WORKSPACE_BASE="${WORKSPACE_BASE:-workspace/denniyahh}"

usage() { echo "usage: $0 <phase-number> [--no-sync]" >&2; exit 2; }

PHASE="${1:-}"; [ -n "$PHASE" ] || usage
case "$PHASE" in *[!0-9.]*|'') echo "error: '$PHASE' is not a phase number" >&2; exit 2 ;; esac
NO_SYNC=0; [ "${2:-}" = "--no-sync" ] && NO_SYNC=1

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

BRANCH="feature/phase-${PHASE}"
WT_PATH="${ROOT}/.worktrees/phase-${PHASE}"

# The phase must exist in ROADMAP.md. A typo'd number would otherwise create a
# worktree for a phase that does not exist, which fails much later and less
# legibly than it does here.
if ! grep -qE "^### Phase ${PHASE}:" .planning/ROADMAP.md; then
    echo "error: no '### Phase ${PHASE}:' heading in .planning/ROADMAP.md." >&2
    echo "  Phases are sometimes renumbered (36 -> 35.3); check the heading first:" >&2
    grep -nE '^### Phase [0-9.]+:' .planning/ROADMAP.md | tail -5 >&2
    exit 1
fi
echo "==> phase ${PHASE}: $(grep -E "^### Phase ${PHASE}:" .planning/ROADMAP.md | head -1)"

if [ "$NO_SYNC" -eq 0 ]; then
    if [ -x scripts/sync-workspace.sh ]; then
        echo "==> syncing ${WORKSPACE_BASE}"
        current="$(git symbolic-ref --short HEAD)"
        if [ "$current" != "$WORKSPACE_BASE" ]; then
            echo "    (on '${current}'; sync must run from '${WORKSPACE_BASE}' -- skipping)" >&2
            echo "    run: git checkout ${WORKSPACE_BASE} && scripts/sync-workspace.sh" >&2
        else
            scripts/sync-workspace.sh
        fi
    else
        echo "==> scripts/sync-workspace.sh not found; skipping sync" >&2
    fi
fi

# The base must track .planning/. Assert it rather than assume: this was wrong
# in CLAUDE.md itself until 2026-09-04.
planning_on_base="$(git ls-tree -r --name-only "$WORKSPACE_BASE" | grep -c '^\.planning/' || true)"
if [ "$planning_on_base" -eq 0 ]; then
    echo "error: '${WORKSPACE_BASE}' tracks zero files under .planning/." >&2
    echo "  A worktree based here cannot commit the phase's planning record." >&2
    exit 1
fi
echo "==> base '${WORKSPACE_BASE}' tracks ${planning_on_base} .planning/ files"

if git -C "$ROOT" worktree list --porcelain | grep -qx "branch refs/heads/${BRANCH}"; then
    existing="$(git worktree list --porcelain \
        | awk -v b="branch refs/heads/${BRANCH}" '/^worktree /{p=substr($0,10)} $0==b{print p; exit}')"
    echo "==> worktree already exists: ${existing}"
    echo "==> bringing it up to date with ${WORKSPACE_BASE}"
    git -C "$existing" merge --ff-only "$WORKSPACE_BASE" || {
        echo "    not a fast-forward -- the branch has its own commits." >&2
        echo "    Resolve by hand; refusing to rewrite phase history." >&2
        exit 1
    }
    WT_PATH="$existing"
elif git show-ref --verify --quiet "refs/heads/${BRANCH}"; then
    echo "==> branch ${BRANCH} exists but has no worktree; attaching at ${WT_PATH}"
    git worktree add "$WT_PATH" "$BRANCH"
else
    echo "==> creating ${BRANCH} at ${WT_PATH} from ${WORKSPACE_BASE}"
    git worktree add -b "$BRANCH" "$WT_PATH" "$WORKSPACE_BASE"
fi

echo
echo "worktree ready:"
echo "  path:   ${WT_PATH}"
echo "  branch: $(git -C "$WT_PATH" rev-parse --abbrev-ref HEAD)"
echo "  head:   $(git -C "$WT_PATH" log --oneline -1)"
echo
echo "Work for phase ${PHASE} goes there. Prefer 'git -C ${WT_PATH} ...' over 'cd'"
echo "-- cd does not persist across an agent's separate tool calls."
