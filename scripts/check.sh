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
usage: scripts/check.sh [all|fmt|clippy|test|build|deps|service]

  all     fmt + clippy + test   (default)
  build   compile workspace and tests only
  fmt     cargo fmt --check
  clippy  cargo clippy --workspace --all-targets -- -D warnings
  test    cargo test --workspace
  deps    cargo deny check + cargo machete (not in `all`; install the tools
          with scripts/install-dep-tools.sh)
  service the host-only systemd user-unit tests (service_e2e, --run-ignored
          only; not in `all`; needs a running systemd user manager and
          cargo-nextest, and fails rather than skips without either)

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
    echo "==> env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast"
    env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast
}

run_build() {
    echo "==> cargo build --workspace --tests"
    cargo build --workspace --tests
}

run_deps() {
    # Not part of `all`: neither tool ships in the pinned devcontainer image, so
    # the pre-push gate would fail on a missing binary rather than on a finding.
    # A missing tool is a hard error here, never a skip — a skipped check prints
    # nothing a log reader can tell apart from a passing one.
    local tool
    for tool in cargo-deny cargo-machete; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            echo "error: $tool is not installed; run scripts/install-dep-tools.sh" >&2
            exit 1
        fi
    done
    echo "==> cargo deny check"
    cargo deny check
    echo "==> cargo machete"
    cargo machete
}

run_service() {
    # Not part of `all`: the pinned devcontainer has no systemd user manager, so
    # these tests can only run on a host or a VM (D-07b). A missing manager is a
    # hard error, never a skip. The probe sits inside `if !` because under
    # `set -e` a bare failing substitution would end the script before the
    # message below is printed.
    local out
    if ! out=$(systemctl --user show -p Version --value 2>&1); then
        echo "error: no systemd user manager answers (systemctl --user show -p Version failed); scripts/check.sh service needs one" >&2
        echo "$out" >&2
        exit 1
    fi
    if ! command -v cargo-nextest >/dev/null 2>&1; then
        echo "error: cargo-nextest is not installed; scripts/check.sh service needs it (cargo install --locked cargo-nextest)" >&2
        exit 1
    fi
    # --no-tests=fail: a filter that matches nothing must not read as a pass.
    echo "==> cargo nextest run -p devflow --test service_e2e --run-ignored only --no-tests=fail"
    cargo nextest run -p devflow --test service_e2e --run-ignored only --no-tests=fail
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
    deps) run_deps ;;
    service) run_service ;;
    -h | --help | help) usage ;;
    *)
        echo "error: unknown target '$TARGET'" >&2
        usage
        ;;
esac

echo "==> check.sh: $TARGET OK"
