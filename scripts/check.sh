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
    echo "==> cargo test --workspace --no-fail-fast"
    cargo test --workspace --no-fail-fast
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
