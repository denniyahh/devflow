#!/usr/bin/env bash
#
# scripts/cut-release.sh — the step-gated release cut.
#
# `devflow release --check` (pre-cut preflight) and `--verify` (post-cut
# verification) are read-only; this script is the glue that runs the release
# steps IN ORDER and refuses to proceed out of order. The two merge steps
# pause for a human (develop/main are PR-protected and merge on GitHub); the
# tag, sync, and publish steps run here, each gated on the prior step's
# invariant.
#
# Usage (one step at a time, strictly in order):
#   scripts/cut-release.sh check       # devflow release --check (preflight)
#   scripts/cut-release.sh branch      # create release/vX.Y.Z off develop
#   scripts/cut-release.sh pr-develop  # open the bump+changelog PR -> develop
#   scripts/cut-release.sh pr-main     # open develop -> main (squash) PR
#   scripts/cut-release.sh tag         # sign vX.Y.Z on origin/main (maintainer key)
#   scripts/cut-release.sh sync        # sync main back into develop (PR, merge-commit)
#   scripts/cut-release.sh publish     # cargo publish devflow-core then devflow
#   scripts/cut-release.sh verify      # devflow release --verify (post-cut)
#
# Every step re-verifies its precondition and refuses with a fix hint rather
# than silently proceeding out of order — the failure mode that made the
# v2.5.0 cut need three corrective passes (commit on develop, tag on develop,
# skipped sync).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
cd "$REPO_ROOT"

die() { echo "ERROR: $*" >&2; exit 1; }
note() { echo "[release] $*"; }

DEVFLOW="${DEVFLOW:-cargo run -q --}"
STEP="${1:-}"
shift || true

# --- helpers ---------------------------------------------------------------

require_clean() {
    [ -z "$(git status --porcelain)" ] || die "working tree is not clean"
}

require_develop() {
    [ "$(git rev-parse --abbrev-ref HEAD)" = "develop" ] || \
        die "must be on develop (currently on '$(git rev-parse --abbrev-ref HEAD)')"
}

workspace_version() {
    # Reads [workspace.package] version without a TOML parser: the key is the
    # first bare `version = "…"` under [workspace.package].
    sed -n '/^\[workspace.package\]/,/^\[/p' Cargo.toml | sed -n 's/^version = "\([^"]*\)".*/\1/p' | head -1
}

require_merged_to_develop() {
    # A step that opens a PR depends on the previous one having merged; check
    # that the named string is reachable on origin/develop.
    local needle="$1"
    git fetch origin develop main --quiet
    git log --oneline origin/develop | grep -qF "$needle" || \
        die "origin/develop does not yet contain '$needle' — merge the previous PR first"
}

# --- steps -----------------------------------------------------------------

step_check() {
    require_clean
    git fetch origin develop main --quiet
    "$DEVFLOW" release --check
}

step_branch() {
    require_clean
    require_develop
    local v; v="$(workspace_version)"
    [ -n "$v" ] || die "could not read workspace version from Cargo.toml"
    local branch="release/v${v#v}"
    if git show-ref --verify --quiet "refs/heads/$branch"; then
        note "branch $branch already exists — reusing"
        git checkout -q "$branch"
    else
        git checkout -q -b "$branch"
        note "created $branch off develop"
    fi
    note "now bump Cargo.toml (two places) and add the CHANGELOG section, then commit"
    note "commit message convention: release: v$v — <description>"
}

step_pr_develop() {
    require_clean
    local v; v="$(workspace_version)"
    local branch="release/v${v#v}"
    [ "$(git rev-parse --abbrev-ref HEAD)" = "$branch" ] || \
        die "must be on '$branch' (run: git checkout $branch)"
    git log --oneline develop..HEAD | grep -q . || die "no commits on $branch yet — bump + changelog first"
    gh pr create --base develop --head "$branch" \
        --title "release: v$v" --body "Cut v$v (version bump + changelog)."
    note "merge the PR into develop, then run: scripts/cut-release.sh pr-main"
}

step_pr_main() {
    require_clean
    require_develop
    require_merged_to_develop "release: v"
    git fetch origin develop main --quiet
    gh pr create --base main --head develop \
        --title "release: v$(workspace_version)" \
        --body "Squash-merge develop into main for v$(workspace_version)."
    note "squash-merge the PR into main, then run: scripts/cut-release.sh tag"
}

step_tag() {
    require_clean
    local v; v="$(workspace_version)"
    git fetch origin main --quiet

    # The release tag MUST be signed with the maintainer's key. If
    # devflow.releaseSigningKey is unset, `git config --get` returns empty and
    # the tag silently signs with whatever `user.signingkey` defaults to — the
    # wrong-identity trap 999.104 catalogues. Fail loudly instead.
    local release_key; release_key="$(git config --get devflow.releaseSigningKey || true)"
    if [ -z "$release_key" ]; then
        echo "cut-release: devflow.releaseSigningKey is not set." >&2
        echo "  Set it: git config --local devflow.releaseSigningKey <path-to-maintainer-key>" >&2
        exit 1
    fi
    local release_key_expanded="${release_key/#\~/$HOME}"
    if [ ! -r "$release_key_expanded" ]; then
        echo "cut-release: devflow.releaseSigningKey points at an unreadable file: $release_key" >&2
        exit 1
    fi

    # The tag MUST land on main's squash commit, not the develop release
    # commit — enforced here, and re-checked by `release --verify`.
    git -c user.signingkey="$release_key" \
        tag -s "v$v" origin/main -m "release: v$v"
    git verify-tag "v$v"
    git push origin "v$v"
    note "tag v$v signed on origin/main and pushed"
}

step_sync() {
    require_clean
    require_develop
    git fetch origin main develop --quiet
    scripts/sync-main-to-develop.sh
    note "sync merge commit is now on develop; it cannot be pushed directly."
    note "put it on a branch and PR it (MERGE commit, not squash), or run the"
    note "step again after the PR is merged. See CONTRIBUTING.md 'Cutting a Release' step 6."
}

step_publish() {
    require_clean
    # crates.io order: devflow-core before devflow (devflow's path-dependency
    # verifies against the published devflow-core).
    cargo publish -p devflow-core
    cargo publish -p devflow
    note "published devflow-core and devflow (run --verify next)"
}

step_verify() {
    git fetch origin develop main --quiet
    "$DEVFLOW" release --verify
}

# --- dispatch --------------------------------------------------------------

case "$STEP" in
    check)       step_check ;;
    branch)      step_branch ;;
    pr-develop)  step_pr_develop ;;
    pr-main)     step_pr_main ;;
    tag)         step_tag ;;
    sync)        step_sync ;;
    publish)     step_publish ;;
    verify)      step_verify ;;
    "" | -h | --help)
        sed -n '12,26p' "${BASH_SOURCE[0]}"
        exit 0 ;;
    *) die "unknown step '$STEP' (see --help)" ;;
esac
