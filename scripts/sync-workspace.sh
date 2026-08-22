#!/usr/bin/env bash
# Sync shared codebase changes from develop into the current personal workspace branch
# without wiping out tracked personal agent configurations, skills, or planning files.
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
    git fetch "$BASE_REMOTE" "$BASE_BRANCH":"$BASE_BRANCH" --quiet || true
fi

echo "==> Syncing project code into $CURRENT_BRANCH..."
git checkout "$BASE_REMOTE/$BASE_BRANCH" -- \
    crates/ \
    Cargo.toml \
    Cargo.lock \
    .github/ \
    scripts/ \
    docs/ \
    doc-check-allowlist.toml \
    rust-toolchain.toml \
    .devcontainer/ \
    .gitignore \
    .gitconfig \
    README.md \
    CONTRIBUTING.md \
    ARCHITECTURE.md \
    CHANGELOG.md \
    CODE_OF_CONDUCT.md \
    LICENSE \
    LICENSE-APACHE \
    SECURITY.md \
    DEPENDENCIES.md \
    OPERATIONS.md

echo "==> Workspace sync complete on $CURRENT_BRANCH. Changes staged for review/commit."
