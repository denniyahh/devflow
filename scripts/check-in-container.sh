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

echo "==> image:  $IMAGE"
echo "==> target: $TARGET"
echo "==> cpus:   $CPUS"

exec docker run --rm -t \
    -v "$REPO_ROOT":/workspace \
    -v devflow-ci-target:/ctarget \
    -v devflow-ci-registry:/usr/local/cargo/registry \
    -w /workspace \
    -e CARGO_TARGET_DIR=/ctarget \
    -e CARGO_TERM_COLOR=always \
    "$IMAGE" \
    "${PIN[@]}" scripts/check.sh "$TARGET"
