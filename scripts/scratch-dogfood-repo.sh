#!/usr/bin/env bash
#
# Scaffold a throwaway git repository that is a legal `devflow start`
# target — for probing the Define->Ship pipeline (phase 23a) without
# touching this checkout's own working tree or git state (D-01
# blast-radius isolation, T-23-01/T-23-02).
#
# Scaffolding decision (Claude's Discretion item #2, recorded in
# 23-CONTEXT.md / 23-01-PLAN.md "Recorded planning choices"): the probe
# target is a SYNTHETIC single-task phase, not an imported real backlog
# item — the probe is about the supervisor/pipeline mechanism, not about
# the content of the work.
#
# Minimum scaffold, per `crates/devflow-cli/src/preflight.rs`'s
# `run_preflight` (read in full before writing this script — the only
# thing it checks for `--agent claude` is that the `claude` binary
# resolves on PATH; there is no project-local `.claude/` requirement, so
# none is scaffolded here):
#   - a git repo with `main` + `develop` and one initial commit
#   - `.planning/{PROJECT.md,ROADMAP.md,STATE.md}` (ROADMAP's phase heading must
#     be `### Phase N:` — three hashes — to satisfy the reachability guard in
#     `preflight.rs`, and the current preflight also requires a minimal
#     `.planning/config.json` so the 35.1 unattended-launch chain-flag
#     prerequisite has somewhere to hold `workflow._auto_chain_active`)
#   - one phase directory carrying a trivial, already-written plan, so
#     Define/Plan have a real target instead of inventing scope
#
# Branch names below are hardcoded to "main"/"develop" — verified against
# `crates/devflow-core/src/config.rs:15,17`
# (`pub const MAIN: &str = "main";` / `pub const DEVELOP: &str = "develop";`)
# at plan time, not re-read live by this script.
#
# Usage: scripts/scratch-dogfood-repo.sh [destination]
#   destination defaults to $TMPDIR (or /tmp)/devflow-dogfood-scratch-$$
#   if omitted. Never a sibling `.worktrees/` entry of this repo.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
SELF="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"

die() {
    echo "ERROR: $*" >&2
    exit 1
}

MODE=default
DEST=""
HAVE_DEST=false
CHECK_DEST=""
# shellcheck disable=SC2034  # consumed by the payload's GSD-resolution probes (49-06)
GSD_TOOLS="$HOME/.claude/gsd-core/bin/gsd-tools.cjs"

set_mode() {
    [ "$MODE" = default ] || [ "$MODE" = "$1" ] || die "conflicting modes: --$MODE and --$1"
    MODE="$1"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --live-payload) set_mode live-payload ;;
        --check)
            set_mode check
            shift
            [ $# -gt 0 ] || die "--check needs a path"
            CHECK_DEST="$1"
            ;;
        --selftest) set_mode selftest ;;
        --gsd-tools)
            shift
            [ $# -gt 0 ] || die "--gsd-tools needs a path"
            # shellcheck disable=SC2034  # consumed by the payload's GSD-resolution probes (49-06)
            GSD_TOOLS="$1"
            ;;
        -h | --help)
            sed -n '/^# Usage:/,/^#$/p' "$SELF"
            exit 0
            ;;
        -*) die "unknown option: $1" ;;
        *)
            [ "$HAVE_DEST" = false ] || die "unexpected second destination: $1"
            DEST="$1"
            HAVE_DEST=true
            ;;
    esac
    shift
done

case "$MODE" in
    check | selftest)
        [ "$HAVE_DEST" = false ] || die "--$MODE takes no positional destination (got: $DEST)"
        ;;
esac

# ---------------------------------------------------------------------------
# --selftest: runs this script as a child against scratch destinations and
# asserts both directions. Case line `  ok   <name>` or `  FAIL <name> — <why>`,
# last line `SELFTEST passed=<P> failed=<F>`, exit 0 only if F=0 and P>0.
# Every control first proves its own precondition, and refusals are asserted
# on their text, never only on the exit code.
# ---------------------------------------------------------------------------
ST_TMP=""
ST_PASS=0
ST_FAIL=0

st_ok() {
    printf '  ok   %s\n' "$1"
    ST_PASS=$((ST_PASS + 1))
}

st_fail() {
    printf '  FAIL %s — %s\n' "$1" "$2"
    ST_FAIL=$((ST_FAIL + 1))
}

# st_count <fixed-string> <file>: occurrences, 0 when none (rg -c prints nothing).
st_count() {
    local count
    count="$(rg --count-matches -F -e "$1" -- "$2" || true)"
    printf '%s\n' "${count:-0}"
}

st_live_topology() {
    local name="live payload: integration checked out with devflow.toml"
    local lp="$ST_TMP/lp" rc=0 head
    bash "$SELF" --live-payload "$lp" >"$ST_TMP/lp.log" 2>&1 || rc=$?
    if [ "$rc" -ne 0 ]; then
        st_fail "$name" "--live-payload exited $rc: $(tail -n 1 "$ST_TMP/lp.log")"
        return
    fi
    head="$(git -C "$lp" symbolic-ref -q HEAD 2>/dev/null || true)"
    if [ "$head" != refs/heads/integration ]; then
        st_fail "$name" "HEAD is '${head:-unreadable}', want refs/heads/integration"
        return
    fi
    if ! git -C "$lp" show-ref --verify -q refs/heads/main; then
        st_fail "$name" "refs/heads/main is missing"
        return
    fi
    # Spelled independently of the scaffold's heredoc, so a typo there fails here.
    printf 'base_branch = "integration"\ncapture_retention = 64\n' >"$ST_TMP/want.toml"
    if ! git -C "$lp" show integration:devflow.toml >"$ST_TMP/got.toml" 2>/dev/null ||
        ! cmp -s "$ST_TMP/want.toml" "$ST_TMP/got.toml"; then
        st_fail "$name" "integration:devflow.toml is absent or differs from the two expected lines"
        return
    fi
    if git -C "$lp" cat-file -e main:devflow.toml 2>/dev/null; then
        st_fail "$name" "devflow.toml is committed on main; it belongs on integration only"
        return
    fi
    st_ok "$name"
}

st_live_check_passes() {
    local name="live payload: check passes"
    local rc=0 last
    bash "$SELF" --check "$ST_TMP/lp" >"$ST_TMP/check.out" 2>&1 || rc=$?
    last="$(tail -n 1 "$ST_TMP/check.out")"
    if [ "$rc" -ne 0 ]; then
        st_fail "$name" "--check exited $rc, last line '${last:-<empty>}'"
        return
    fi
    if ! printf '%s\n' "$last" | rg -q '^CHECK passed=[1-9][0-9]* failed=0$'; then
        st_fail "$name" "last line is '${last:-<empty>}', want CHECK passed=<P≥1> failed=0"
        return
    fi
    st_ok "$name"
}

st_control_compact_config() {
    local name="control: check refuses a compact config.json"
    local copy="$ST_TMP/compact" cfg rc=0
    if [ ! -d "$ST_TMP/lp/.git" ]; then
        st_fail "$name" "precondition: no live payload at $ST_TMP/lp to copy"
        return
    fi
    cp -a "$ST_TMP/lp" "$copy"
    cfg="$copy/.planning/config.json"
    if ! { jq -c . "$cfg" >"$ST_TMP/compact.json" && mv "$ST_TMP/compact.json" "$cfg" &&
        git -C "$copy" commit -q -am "chore: compact config for the control"; }; then
        st_fail "$name" "precondition: could not commit a compact config.json in the copy"
        return
    fi
    if cmp -s <(jq --indent 2 . "$cfg") "$cfg"; then
        st_fail "$name" "precondition: the copy's config.json still equals jq --indent 2 of itself"
        return
    fi
    bash "$SELF" --check "$copy" >"$ST_TMP/compact.out" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
        st_fail "$name" "--check accepted a compact config.json (exit 0)"
        return
    fi
    if [ "$(st_count '  FAIL config.json is in pretty form' "$ST_TMP/compact.out")" -ne 1 ]; then
        st_fail "$name" "exit $rc but no FAIL line naming 'config.json is in pretty form'"
        return
    fi
    # Only the form differs, so only the form check may fail.
    if [ "$(st_count '  FAIL ' "$ST_TMP/compact.out")" -ne 1 ]; then
        st_fail "$name" "more than one check failed; the control no longer isolates the form"
        return
    fi
    st_ok "$name"
}

st_control_head_on_main() {
    local name="control: check refuses HEAD on main"
    local copy="$ST_TMP/onmain" head rc=0
    if [ ! -d "$ST_TMP/lp/.git" ]; then
        st_fail "$name" "precondition: no live payload at $ST_TMP/lp to copy"
        return
    fi
    cp -a "$ST_TMP/lp" "$copy"
    git -C "$copy" checkout -q main
    head="$(git -C "$copy" symbolic-ref -q HEAD 2>/dev/null || true)"
    if [ "$head" != refs/heads/main ]; then
        st_fail "$name" "precondition: HEAD is '${head:-unreadable}', want refs/heads/main"
        return
    fi
    bash "$SELF" --check "$copy" >"$ST_TMP/onmain.out" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
        st_fail "$name" "--check accepted HEAD on main (exit 0)"
        return
    fi
    if [ "$(st_count '  FAIL head is integration' "$ST_TMP/onmain.out")" -ne 1 ]; then
        st_fail "$name" "exit $rc but no FAIL line naming 'head is integration'"
        return
    fi
    st_ok "$name"
}

run_selftest() {
    # Hermetic: the host checkout's git environment and global/system config
    # (signing, hooks, excludes) may not leak into the scratch repositories.
    local -a git_local_env_vars
    mapfile -t git_local_env_vars < <(git rev-parse --local-env-vars)
    unset "${git_local_env_vars[@]}"
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
    ST_TMP="$(mktemp -d)"
    trap 'rm -rf "$ST_TMP"' EXIT

    st_live_topology
    st_live_check_passes
    st_control_compact_config
    st_control_head_on_main

    echo "SELFTEST passed=$ST_PASS failed=$ST_FAIL"
    [ "$ST_FAIL" -eq 0 ] && [ "$ST_PASS" -gt 0 ]
}

case "$MODE" in
    selftest)
        run_selftest
        exit 0
        ;;
    check) exit 0 ;;        # RED stub: the checker is not implemented yet
    live-payload) exit 0 ;; # RED stub: the live payload is not implemented yet
esac

DEST="${DEST:-${TMPDIR:-/tmp}/devflow-dogfood-scratch-$$}"

# T-23-01 (Tampering, high, mitigate): refuse any destination inside this
# checkout — resolve both to absolute paths and compare prefixes.
DEST_ABS="$(realpath -m -- "$DEST")"
if [ "$DEST_ABS" = "$REPO_ROOT" ] || case "$DEST_ABS" in "$REPO_ROOT"/*) true ;; *) false ;; esac; then
    echo "ERROR: refusing to scaffold inside this checkout ($REPO_ROOT)." >&2
    echo "       Requested destination: $DEST_ABS" >&2
    echo "       Pick a destination outside the checkout (e.g. under \$TMPDIR or ~)." >&2
    exit 1
fi

if [ -e "$DEST_ABS" ]; then
    echo "ERROR: destination already exists: $DEST_ABS" >&2
    echo "       Remove it first, or pick a different destination." >&2
    exit 1
fi

mkdir -p "$DEST_ABS"

# Force the initial branch to "main" regardless of the operator's
# init.defaultBranch config, without requiring a git version new enough
# for `git init -b`.
git init -q "$DEST_ABS"
git -C "$DEST_ABS" symbolic-ref HEAD refs/heads/main

# T-23-02 (Tampering, high, mitigate): repo-local identity only — never
# --global, and no GIT_* env export (999.37 class).
git -C "$DEST_ABS" config user.name "DevFlow Probe"
git -C "$DEST_ABS" config user.email "devflow-probe@localhost"

mkdir -p "$DEST_ABS/.planning/phases/01-add-probe-marker"

cat > "$DEST_ABS/.planning/PROJECT.md" <<'EOF'
# Devflow Probe Target

## What This Is

A throwaway, single-purpose scratch repository scaffolded by
`scripts/scratch-dogfood-repo.sh` (DevFlow phase 23a) solely to give
`devflow start --phase 1` a legal, isolated target for probing the
Define->Ship pipeline mechanism. Nothing in this repo is meant to be kept.

## Core Value

Prove the pipeline mechanism carries a trivial change end-to-end — not to
build anything real.

## Requirements

### Active

- Phase 1: add a one-line marker file (`PROBE.md`) — deliberately trivial
  so Define/Plan exercise the pipeline, not the work.
EOF

cat > "$DEST_ABS/.planning/ROADMAP.md" <<'EOF'
# Roadmap

### Phase 1: Add PROBE marker file

Goal: create `PROBE.md` at the repo root containing exactly one line,
proving the pipeline can carry a trivial change from Define through Ship.

Status: not started
EOF

cat > "$DEST_ABS/.planning/STATE.md" <<'EOF'
---
gsd_state_version: 1.0
current_phase: 1
current_phase_name: Add PROBE marker file
status: planned
---

# Devflow Probe Target — Project State

## Active Phase

Phase 1 — Add PROBE marker file. Plan 01-01 is pre-written (see
`.planning/phases/01-add-probe-marker/01-01-PLAN.md`) so Define/Plan have
a real artifact to work against rather than inventing scope during a
probe.

## Current Position

Phase: 1
Plan: 01-01
Status: Planned — ready for `devflow start`
EOF

cat > "$DEST_ABS/.planning/phases/01-add-probe-marker/01-01-PLAN.md" <<'EOF'
---
phase: 01-add-probe-marker
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - PROBE.md
autonomous: true
---

<objective>
Create `PROBE.md` containing exactly one line, proving DevFlow's
Define->Ship pipeline can carry a trivial change end-to-end. This is a
throwaway scratch repo (scripts/scratch-dogfood-repo.sh) — the change
itself has no value beyond exercising the pipeline mechanism.
</objective>

<tasks>

<task type="auto">
  <name>Task 1: Create PROBE.md</name>
  <files>PROBE.md</files>
  <action>
Create a file named `PROBE.md` at the repository root containing exactly
one line: `probe ok`.
  </action>
  <acceptance_criteria>
    - `PROBE.md` exists at the repository root
    - Its content is exactly `probe ok`
  </acceptance_criteria>
  <verify>
    <automated>test "$(cat PROBE.md)" = "probe ok" && echo PROBE_OK</automated>
  </verify>
  <done>`PROBE.md` exists with the single line `probe ok`.</done>
</task>

</tasks>

<verification>
1. `PROBE.md` exists and contains exactly `probe ok`.
</verification>

<success_criteria>
- The pipeline produced a single committed file with the exact expected
  content.
</success_criteria>

<output>
Create `.planning/phases/01-add-probe-marker/01-01-SUMMARY.md` when done.
</output>
EOF

cat > "$DEST_ABS/.planning/config.json" <<'EOF'
{
  "workflow": {}
}
EOF

git -C "$DEST_ABS" add -A
git -C "$DEST_ABS" commit -q -m "chore: scaffold devflow probe target"
git -C "$DEST_ABS" branch develop
git -C "$DEST_ABS" checkout -q develop

echo "Scaffolded devflow probe target: $DEST_ABS"
echo "Next: devflow start --phase 1 --agent claude --mode auto $DEST_ABS"
