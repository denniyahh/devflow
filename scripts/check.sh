#!/usr/bin/env bash
# The single definition of "is this green?" — used by CI, by the pre-push
# hook, and by scripts/check-in-container.sh. If a check is not in here, it
# is not enforced anywhere; if it is, it is enforced identically everywhere.
#
# Deliberately NOT parameterised by environment. The whole point is that the
# same commands run on a developer machine and on a runner, so a green local
# run means the same thing as a green CI run.
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: scripts/check.sh [all|fmt|clippy|test|build]

  all     fmt + clippy + test   (default)
  build   compile workspace and tests only
  fmt     cargo fmt --check
  clippy  cargo clippy --workspace --all-targets -- -D warnings
  test    cargo test --workspace

Run inside the pinned devcontainer for CI parity:
  scripts/check-in-container.sh [target]
EOF
    exit 2
}

TARGET="${1:-all}"

run_fmt() {
    echo "==> cargo fmt --check"
    cargo fmt --check
}

run_clippy() {
    # --all-targets so test and bench code is linted too; a lint that only
    # covers src/ misses the majority of this repo's unsafe blocks.
    echo "==> cargo clippy --workspace --all-targets -- -D warnings"
    cargo clippy --workspace --all-targets -- -D warnings
}

run_test() {
    # --no-fail-fast deliberately: without it cargo stops at the first failing
    # test BINARY, so a failure in devflow-core hides every failure in
    # devflow-cli. Chasing CI one masked failure at a time cost several
    # round trips on 2026-07-26; one run should report everything that is
    # broken, not the alphabetically-first thing.
    #
    # env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test: both halves are
    # load-bearing for the insta snapshot guard (47-CONTEXT.md D-14..D-16).
    # INSTA_UPDATE=no keeps a snapshot mismatch failing even when a developer
    # (or a stale shell, or a CI image) exports INSTA_UPDATE=always, which
    # otherwise SILENTLY rewrites the baseline and exits 0. `env -u
    # INSTA_FORCE_UPDATE` is equally load-bearing: INSTA_FORCE_UPDATE=1
    # OVERRIDES INSTA_UPDATE=no and re-blesses a drifted baseline green. Both
    # were verified by experiment (47-RESEARCH.md § A-2, § A-3). Dropping
    # either reopens a green-over-unread guard.
    #
    # The worktree-guard harness runs even when cargo test fails, for the same
    # one-run-reports-everything reason: under `set -e` a failing cargo run used
    # to end the script before the harness reported anything. Both exit statuses
    # are kept; either failing fails the target, with cargo's status first.
    local cargo_status=0 harness_status=0
    echo "==> env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast"
    env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast || cargo_status=$?
    echo "==> bash scripts/test-phase-worktree-guard.sh"
    bash scripts/test-phase-worktree-guard.sh || harness_status=$?
    if [ "$cargo_status" -ne 0 ] || [ "$harness_status" -ne 0 ]; then
        echo "error: cargo test exited $cargo_status; worktree-guard harness exited $harness_status" >&2
        [ "$cargo_status" -ne 0 ] && return "$cargo_status"
        return "$harness_status"
    fi
}

run_build() {
    echo "==> cargo build --workspace --tests"
    cargo build --workspace --tests
}

case "$TARGET" in
    all)
        # Cheapest first: fail on formatting before paying for a compile.
        run_fmt
        run_clippy
        run_test
        ;;
    fmt) run_fmt ;;
    clippy) run_clippy ;;
    test) run_test ;;
    build) run_build ;;
    -h | --help | help) usage ;;
    *)
        echo "error: unknown target '$TARGET'" >&2
        usage
        ;;
esac

echo "==> check.sh: $TARGET OK"
