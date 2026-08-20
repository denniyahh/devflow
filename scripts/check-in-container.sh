#!/usr/bin/env bash
# Run scripts/check.sh inside the pinned devcontainer image — the same image
# CI uses — so a green local run and a green CI run mean the same thing.
#
# WHY THIS EXISTS: the host toolchain, libc, and OS are not CI's. Phase 23
# shipped a commit that passed `cargo test` on a Fedora host and went red on
# a Debian runner minutes later, and the resulting hunt cost hours (999.47).
# Verification belongs in the pinned image; editing, git, and agent tooling
# stay on the host where your credentials are.
#
# Image is read from .devcontainer/devcontainer.json so there is exactly one
# place to bump it. Requires Docker (a real daemon). Rootless podman is NOT
# supported here: it does not delegate the cpuset controller by default, so
# --cpuset-cpus fails outright.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

TARGET="${1:-all}"

# Single source of truth for the image tag: the devcontainer definition.
# Tolerates the // comments devcontainer.json is allowed to contain.
IMAGE="$(sed 's|//.*||' .devcontainer/devcontainer.json \
    | grep -o '"image"[[:space:]]*:[[:space:]]*"[^"]*"' \
    | head -1 | sed 's|.*"image"[[:space:]]*:[[:space:]]*"||; s|"$||')"

if [ -z "$IMAGE" ]; then
    echo "error: could not read \"image\" from .devcontainer/devcontainer.json" >&2
    exit 1
fi

if ! docker info >/dev/null 2>&1; then
    cat >&2 <<'EOF'
error: Docker daemon is not reachable.

Verification runs in the pinned devcontainer image so that local results
match CI. Start Docker and retry.

To deliberately verify on the host instead — accepting that a green result
does NOT imply CI will be green — run scripts/check.sh directly.
EOF
    exit 1
fi

# Persist the container's target/ and cargo registry across runs so repeat
# invocations are incremental. Named volumes, not bind mounts: the container
# toolchain's artifacts must never collide with the host's target/, which is
# built by a different libc.
docker volume create devflow-ci-target >/dev/null
docker volume create devflow-ci-registry >/dev/null

# CPU pinning: match CI's core count so test-thread interleaving is
# comparable. GitHub's standard hosted runners are 2-core; a 4-core host
# hides races that CI sees. Override with DEVFLOW_CI_CPUS=all to use every
# core (faster, less faithful).
CPUS="${DEVFLOW_CI_CPUS:-0,1}"
if [ "$CPUS" = "all" ]; then
    PIN=()
else
    PIN=(taskset -c "$CPUS")
fi

# HYG-02 (41-02, re-derived from real container runs 2026-08-20): a git
# WORKTREE's `.git` is a FILE — `gitdir: <main>/.git/worktrees/<N>` — pointing
# at the main repo's gitdir, which lives OUTSIDE the worktree and therefore
# outside the single `-v "$REPO_ROOT":/workspace` mount below. Inside the
# container git resolves that absolute gitdir path to nothing and every
# git-dependent check dies with `fatal: not a git repository`. Two paths must
# come through: the worktree gitdir itself AND the COMMON gitdir it shares
# (`commondir` — the main repo's `.git`, where refs/objects live). The MAIN
# checkout is unaffected (its gitdir is inside REPO_ROOT) — verified both
# ways in the pinned image: worktree FAILS, main checkout as uid 0 PASSES.
# The fix is the mount, not the tests and not a uid-0 skip: gitignore /
# ci-parity / pre-commit-branch guards stay active under CI's root-over-
# normal-checkout runs.
GITDIR="$(git rev-parse --absolute-git-dir)"
COMMONDIR="$(git rev-parse --path-format=absolute --git-common-dir)"
GITDIR_MOUNT=()
for dir in "$GITDIR" "$COMMONDIR"; do
    if [ "${dir#"$REPO_ROOT"/}" = "$dir" ]         && ! printf '%s
' "${GITDIR_MOUNT[@]}" | grep -qx -- "$dir"; then
        echo "==> worktree detected: gitdir component $dir is outside the mount; binding it through"
        GITDIR_MOUNT+=(-v "$dir:$dir")
    fi
done

echo "==> image:  $IMAGE"
echo "==> target: $TARGET"
echo "==> cpus:   $CPUS"

exec docker run --rm -t \
    "${GITDIR_MOUNT[@]}" \
    -v "$REPO_ROOT":/workspace \
    -v devflow-ci-target:/ctarget \
    -v devflow-ci-registry:/usr/local/cargo/registry \
    -w /workspace \
    -e CARGO_TARGET_DIR=/ctarget \
    -e CARGO_TERM_COLOR=always \
    "$IMAGE" \
    "${PIN[@]}" scripts/check.sh "$TARGET"
