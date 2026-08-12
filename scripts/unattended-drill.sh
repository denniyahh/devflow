#!/usr/bin/env bash
#
# scripts/unattended-drill.sh — the phase 35.1 unattended-mode drill.
#
# ============================================================================
# WHAT THIS DRILL ESTABLISHES
# ============================================================================
#
# That a REAL GSD blocking checkpoint, reached by a REAL Claude agent driving
# REAL GSD commands, is auto-approved when DevFlow has set GSD's chain flag
# (`workflow._auto_chain_active`) for the Code stage — and is NOT auto-approved
# when it has not. Two arms, one fixture generator, one variable changed
# (`--mode auto` vs `--mode supervise`). If the two arms produce the SAME
# checkpoint observation the drill FAILS: a run that never reached a checkpoint
# is otherwise indistinguishable from a successful auto-approval.
#
# ============================================================================
# WHAT THIS DRILL DOES *NOT* ESTABLISH  (35.1 D-10 constraint 3)
# ============================================================================
#
#   * It does NOT establish that DevFlow sets and clears the flag at the right
#     moments. That is plan 35.1-01's coverage (the in-process guard, its Drop,
#     the supervise negative control) and plan 35.1-02's (the SIGKILL leak and
#     the force-clear repair). A green drill must never be cited as coverage of
#     DevFlow's own flag management.
#   * It does NOT establish anything about the PLAN stage. The Plan stage can
#     never receive this bypass — the same upstream boolean that would
#     auto-approve a Plan checkpoint also makes `plan-phase` chain into
#     `execute-phase` (35.1 D-04, upstream G-01).
#   * It does NOT establish anything about a legacy-arm or non-Claude launch.
#     Those are refused at preflight (35.1-03), not covered here.
#   * One run is ONE SAMPLE. It demonstrates the mechanism works on the path
#     taken; it says nothing about reliability under load or concurrency.
#
# ============================================================================
# SAFETY
# ============================================================================
#
# The fixtures are scaffolded OUTSIDE this checkout, guarded exactly as
# `scripts/scratch-dogfood-repo.sh` guards its destination (T-23-01: refuse any
# destination inside this checkout, resolving both to absolute paths), with a
# repo-local git identity only (T-23-02: never --global, never a GIT_* export —
# 999.37). Agent logs are written outside the checkout too. Every process
# either arm spawns is killed by a trap on every exit path (T-35.1-18).
#
# Usage:
#   scripts/unattended-drill.sh [options] [destination-base]
#
#   --scaffold-only        Scaffold ONE fixture and exit. Fast; spawns no agent.
#   --out <path>           Evidence file to write (full runs only).
#   --ceiling-secs <n>     Per-arm poll ceiling in seconds (default 9000).
#   --gate-timeout <n>     DEVFLOW_GATE_TIMEOUT_SECS for both arms (default 180).
#   --no-gap               Scaffold WITHOUT the deferred second success
#                          criterion, so the run converges at the first
#                          Validate instead of entering the fix loop. See the
#                          "fix loop" note in `35.1-DRILL.md`.
#   --sequential           Run the arms one after the other (default: parallel,
#                          which halves wall-clock and cannot cross-contaminate
#                          — the fixtures are independent repositories).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
# In a linked worktree `--show-toplevel` is the worktree, not the primary
# checkout. Both must be off-limits as a destination: a fixture scaffolded into
# the primary checkout is the 999.37 class of accident even when this script is
# running from a worktree.
GIT_COMMON_DIR="$(git -C "$SCRIPT_DIR" rev-parse --path-format=absolute --git-common-dir)"
MAIN_CHECKOUT="$(dirname "$GIT_COMMON_DIR")"

SCAFFOLD_ONLY=false
OUT_FILE="$REPO_ROOT/.planning/phases/35.1-unattended-launch-prerequisites/35.1-DRILL.md"
DEST_BASE=""
CEILING_SECS=9000
GATE_TIMEOUT_SECS=180
POLL_SECS=15
WITH_GAP=true
PARALLEL=true

die() {
    echo "ERROR: $*" >&2
    exit 1
}

note() { echo "[drill] $*"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --scaffold-only) SCAFFOLD_ONLY=true ;;
        --out)
            shift
            [ $# -gt 0 ] || die "--out needs a path"
            OUT_FILE="$1"
            ;;
        --ceiling-secs)
            shift
            [ $# -gt 0 ] || die "--ceiling-secs needs a number"
            CEILING_SECS="$1"
            ;;
        --gate-timeout)
            shift
            [ $# -gt 0 ] || die "--gate-timeout needs a number"
            GATE_TIMEOUT_SECS="$1"
            ;;
        --no-gap) WITH_GAP=false ;;
        --sequential) PARALLEL=false ;;
        -h | --help)
            sed -n '46,60p' "${BASH_SOURCE[0]}"
            exit 0
            ;;
        -*) die "unknown option: $1" ;;
        *) DEST_BASE="$1" ;;
    esac
    shift
done

# ---------------------------------------------------------------------------
# Destination guard (T-23-01, inherited verbatim in spirit from
# scripts/scratch-dogfood-repo.sh and widened to cover the primary checkout).
# ---------------------------------------------------------------------------
guard_destination() {
    local dest="$1"
    local dest_abs
    dest_abs="$(realpath -m -- "$dest")"
    local forbidden
    for forbidden in "$REPO_ROOT" "$MAIN_CHECKOUT"; do
        if [ "$dest_abs" = "$forbidden" ] || case "$dest_abs" in "$forbidden"/*) true ;; *) false ;; esac; then
            echo "ERROR: refusing to scaffold inside this checkout ($forbidden)." >&2
            echo "       Requested destination: $dest_abs" >&2
            echo "       Pick a destination outside the checkout (e.g. under \$TMPDIR or ~)." >&2
            exit 1
        fi
    done
    if [ -e "$dest_abs" ]; then
        echo "ERROR: destination already exists: $dest_abs" >&2
        echo "       Remove it first, or pick a different destination." >&2
        exit 1
    fi
    printf '%s\n' "$dest_abs"
}

# ---------------------------------------------------------------------------
# Fixture scaffolding.
#
# Deliberately does NOT pre-write a plan (35.1 D-11): the Plan stage runs live,
# because a pre-written plan cannot carry a checkpoint a real planner chose to
# emit. What IS pre-baked is CONTEXT.md, which carries the locked decision that
# makes the checkpoint deterministic rather than a coin flip (F-19).
# ---------------------------------------------------------------------------
scaffold_fixture() {
    local dest_abs="$1"

    mkdir -p "$dest_abs"
    git init -q "$dest_abs"
    git -C "$dest_abs" symbolic-ref HEAD refs/heads/main
    # T-23-02: repo-local identity only — never --global, no GIT_* export.
    git -C "$dest_abs" config user.name "DevFlow Drill"
    git -C "$dest_abs" config user.email "devflow-drill@localhost"

    mkdir -p "$dest_abs/.planning/phases/01-drill-marker"

    cat > "$dest_abs/.planning/PROJECT.md" <<'PROJECT_EOF'
# DevFlow Unattended Drill Target

## What This Is

A throwaway, single-purpose scratch repository scaffolded by
`scripts/unattended-drill.sh` (DevFlow phase 35.1-04) solely to give
`devflow start --phase 1` a legal, isolated target for driving a REAL GSD
blocking checkpoint under a real Claude agent. Nothing here is meant to be
kept.

## Core Value

Prove the checkpoint-approval mechanism, not build anything real.

## Requirements

### Active

- Phase 1: create a one-line marker file. Deliberately trivial so the run
  exercises the pipeline and the checkpoint, not the work.
PROJECT_EOF

    if [ "$WITH_GAP" = true ]; then
        cat > "$dest_abs/.planning/ROADMAP.md" <<'ROADMAP_GAP_EOF'
# Roadmap

## v0.1.0 — milestone (Drill, ACTIVE)

### Phase 1: Add the drill marker files

Goal: create the drill's marker files at the repository root, proving the
pipeline can carry a trivial change from Plan through Validate.

**Success Criteria:**

1. `MARKER.md` exists at the repository root and contains exactly `marker ok`.
2. `NOTES.md` exists at the repository root and contains exactly `notes ok`.

Status: not started

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1 | 0/0 | Not started | — |
ROADMAP_GAP_EOF
    else
        cat > "$dest_abs/.planning/ROADMAP.md" <<'ROADMAP_NOGAP_EOF'
# Roadmap

## v0.1.0 — milestone (Drill, ACTIVE)

### Phase 1: Add the drill marker file

Goal: create the drill's marker file at the repository root, proving the
pipeline can carry a trivial change from Plan through Validate.

**Success Criteria:**

1. `MARKER.md` exists at the repository root and contains exactly `marker ok`.

Status: not started

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1 | 0/0 | Not started | — |
ROADMAP_NOGAP_EOF
    fi

    cat > "$dest_abs/.planning/STATE.md" <<'STATE_EOF'
---
gsd_state_version: 1.0
current_phase: 1
current_phase_name: Add the drill marker files
status: discussed
---

# DevFlow Unattended Drill Target — Project State

## Active Phase

Phase 1. `01-CONTEXT.md` is pre-baked; no plan exists yet, deliberately — the
Plan stage runs live so the checkpoint in the generated plan is a real
planner's output rather than a fixture's.

## Current Position

Phase: 1
Plan: none yet
Status: Discussed — ready for `devflow start`
STATE_EOF

    # A real GSD config, with the chain flag PRESENT and CLEAR so plan
    # 35.1-03's preflight condition C1 holds.
    #
    # `auto_mode` MUST be false, and this is the whole ballgame for the negative
    # control (2026-08-09: the first executed run set it true and the drill
    # measured nothing as a result).
    #
    # GSD's `checkpoint_handling` reads `check auto-mode --pick active`, which is
    # documented as "chain flag OR user preference — same boolean". With
    # `auto_mode: true` the PERSISTENT PREFERENCE alone satisfies that OR, so
    # BOTH arms auto-approve and the ephemeral chain flag — the only thing this
    # drill exists to test — is never the deciding variable. The run comes back
    # green-ish and establishes nothing.
    #
    # Setting it false makes `_auto_chain_active` the sole path to auto-approval,
    # which is exactly the isolation F-20 requires: one variable changes between
    # the arms, and it is the one under test.
    cat > "$dest_abs/.planning/config.json" <<'CONFIG_EOF'
{
  "commit_docs": true,
  "workflow": {
    "granularity": "small",
    "auto_mode": false,
    "commit_docs": true,
    "subagent_timeout": 300000,
    "_auto_chain_active": false,
    "nyquist_validation": false,
    "tdd_mode": false
  },
  "git": {
    "main": "main",
    "develop": "develop",
    "feature_prefix": "feature/",
    "auto_branch": true,
    "auto_cleanup": true,
    "branching_strategy": "phase",
    "phase_branch_template": "feature/phase-{phase}"
  }
}
CONFIG_EOF

    # The pre-baked CONTEXT.md (D-11). Two locked decisions and — when the gap
    # arm is active — one deferred idea.
    {
        cat <<'CONTEXT_HEAD_EOF'
# Phase 1: Add the drill marker files — Context

## Phase Goal

Create the drill's marker file(s) at the repository root. The work is
deliberately trivial; the point of this phase is the shape of the plan, not
the content of the change.

## Locked Decisions

**D-1 — the plan MUST contain a blocking verification checkpoint.**

Every plan produced for this phase MUST include one task of type
`checkpoint:human-verify` carrying `gate="blocking"`, placed after the marker
file has been created and committed. This is a locked decision, not a
suggestion: do not omit it, do not downgrade it to an ordinary `auto` task,
and do not merge it into another task.

Use `gate="blocking"` EXACTLY. Never `gate="blocking-human"`, and never
`checkpoint:human-action` — those are human-only markers that no mode
auto-approves, and this phase's launch is refused at preflight if a plan
declares one.

The checkpoint's `<what-built>` should say that `MARKER.md` was created, and
its `<how-to-verify>` should ask the reader to confirm the file's single line
reads `marker ok`.

**D-2 — the only artifact this phase creates is `MARKER.md`.**

One file at the repository root. Its entire content is the single line:

    marker ok

Nothing else. No tests, no scripts, no directories, no README changes.
CONTEXT_HEAD_EOF

        if [ "$WITH_GAP" = true ]; then
            cat <<'CONTEXT_GAP_EOF'

## Deferred Ideas

- **`NOTES.md` — DEFERRED, and it must NOT appear in any plan for this phase.**
  ROADMAP success criterion 2 names a `NOTES.md` file containing `notes ok`.
  It is deliberately out of scope here and is deferred to a later phase. Do
  not plan it, do not create it, and do not reference it in any task. A plan
  that includes it has not honoured this phase's context.
CONTEXT_GAP_EOF
        fi

        cat <<'CONTEXT_TAIL_EOF'

## Constraints

- This is a throwaway scratch repository with no remote. Do not attempt to
  push, open a pull request, or contact any network service.
- Keep the plan to a single plan file with the smallest number of tasks that
  satisfies D-1 and D-2.
CONTEXT_TAIL_EOF
    } > "$dest_abs/.planning/phases/01-drill-marker/01-CONTEXT.md"

    git -C "$dest_abs" add -A
    git -C "$dest_abs" commit -q -m "chore: scaffold devflow unattended-drill target"
    git -C "$dest_abs" branch develop
    git -C "$dest_abs" checkout -q develop
}

# ---------------------------------------------------------------------------
# Process hygiene (T-35.1-18). Runs on EVERY exit path.
# ---------------------------------------------------------------------------
TRACKED_PIDS=()
TRACKED_FIXTURES=()
KEEP_FIXTURES=false
MY_PGID="$(ps -o pgid= -p $$ | tr -d ' ')"

# Processes belonging to a fixture, EXCLUDING this script and everything it
# forked without `setsid`.
#
# Found the hard way while building this script: a bare
# `pgrep -f -- "$fixture"` matches the drill's own command line whenever the
# fixture path appears in its argv, so the "is the arm still running?" poll
# never returns false and the reaper SIGTERMs the drill itself. Filtering on
# the process group is what separates the two: every arm is launched under
# `setsid` and therefore leads its own group, while this script and all of its
# ordinary subshells share `MY_PGID`.
stray_pids() {
    local fixture="$1" pid pgid
    while read -r pid; do
        [ -n "$pid" ] || continue
        pgid="$(ps -o pgid= -p "$pid" 2>/dev/null | tr -d ' ')"
        [ -z "$pgid" ] && continue
        [ "$pgid" = "$MY_PGID" ] && continue
        printf '%s\n' "$pid"
    done < <(pgrep -f -- "$fixture" 2>/dev/null || true)
}

fixture_has_live_process() {
    [ -n "$(stray_pids "$1")" ]
}

signal_fixture() {
    local fixture="$1" sig="$2" pid
    while read -r pid; do
        [ -n "$pid" ] || continue
        kill "-$sig" "$pid" 2>/dev/null || true
    done < <(stray_pids "$fixture")
}

reap() {
    local pid
    for pid in "${TRACKED_PIDS[@]:-}"; do
        [ -n "$pid" ] || continue
        # setsid makes each launcher its own process-group leader, so the
        # negative PID reaches the whole group in one signal.
        kill -TERM -- "-$pid" 2>/dev/null || true
        kill -TERM "$pid" 2>/dev/null || true
    done
    local fixture
    for fixture in "${TRACKED_FIXTURES[@]:-}"; do
        [ -n "$fixture" ] || continue
        # DevFlow's monitor detaches into its own session, so it escapes the
        # process group above.
        signal_fixture "$fixture" TERM
    done
    sleep 2
    for fixture in "${TRACKED_FIXTURES[@]:-}"; do
        [ -n "$fixture" ] || continue
        signal_fixture "$fixture" KILL
    done
}

# shellcheck disable=SC2329
# Justification: `cleanup` is invoked indirectly, by the `trap` two lines below
# its definition. ShellCheck cannot see trap handlers as call sites. Suppressed
# rather than restructured — a trap is exactly how T-35.1-18 requires the reaper
# to run on EVERY exit path, including the signal paths.
cleanup() {
    local rc=$?
    reap
    if [ "$KEEP_FIXTURES" = true ]; then
        local fixture
        for fixture in "${TRACKED_FIXTURES[@]:-}"; do
            [ -n "$fixture" ] && echo "[drill] fixture retained for inspection: $fixture" >&2
        done
    fi
    exit "$rc"
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# Evidence helpers.
# ---------------------------------------------------------------------------
state_file() { printf '%s\n' "$1/.devflow/state-01.json"; }
events_file() { printf '%s\n' "$1/.devflow/events.jsonl"; }

state_field() {
    local fixture="$1" field="$2"
    local sf
    sf="$(state_file "$fixture")"
    [ -f "$sf" ] || {
        printf 'ABSENT\n'
        return 0
    }
    jq -r "(.${field} // \"ABSENT\") | tostring" "$sf" 2>/dev/null || printf 'UNREADABLE\n'
}

# Every capture generation for the phase, newest last: the live capture plus
# every archived generation under .devflow/history/phase-01/.
capture_files() {
    local fixture="$1"
    local f
    for f in "$fixture"/.devflow/history/phase-01/*-stdout; do
        [ -f "$f" ] && printf '%s\n' "$f"
    done
    [ -f "$fixture/.devflow/phase-01-stdout" ] && printf '%s\n' "$fixture/.devflow/phase-01-stdout"
    return 0
}

# Grep every capture generation for a fixed string, printing matching lines
# truncated so a single 200KB stream-json line cannot swamp the evidence file.
grep_captures() {
    local fixture="$1" needle="$2" limit="${3:-4}"
    local f count=0
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        while IFS= read -r line; do
            count=$((count + 1))
            [ "$count" -gt "$limit" ] && return 0
            printf '%s\n' "${line:0:600}"
        done < <(grep -F -h -- "$needle" "$f" 2>/dev/null || true)
    done < <(capture_files "$fixture")
    return 0
}

captures_contain() {
    local fixture="$1" needle="$2"
    local f
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        if grep -qF -- "$needle" "$f" 2>/dev/null; then
            printf 'yes\n'
            return 0
        fi
    done < <(capture_files "$fixture")
    printf 'no\n'
}

# The generated plan file, if the Plan stage produced one. Plans land in the
# worktree, which is where the agent runs.
generated_plan() {
    local fixture="$1"
    local wt="$fixture/.worktrees/phase-01"
    local f
    for f in "$wt"/.planning/phases/01-*/01-*PLAN.md "$fixture"/.planning/phases/01-*/01-*PLAN.md; do
        [ -f "$f" ] && printf '%s\n' "$f" && return 0
    done
    return 0
}

worktree_config() {
    local fixture="$1"
    local wt
    wt="$(state_field "$fixture" worktree_path)"
    if [ "$wt" = "ABSENT" ] || [ ! -d "$wt" ]; then
        wt="$fixture"
    fi
    printf '%s\n' "$wt/.planning/config.json"
}

# ---------------------------------------------------------------------------
# Arm execution.
# ---------------------------------------------------------------------------
launch_arm() {
    local fixture="$1" mode="$2" log="$3"
    (
        cd "$fixture" || exit 1
        exec setsid "$DEVFLOW_BIN" start \
            --phase 1 \
            --agent claude \
            --mode "$mode" \
            --until validate \
            "$fixture"
    ) > "$log" 2>&1 &
    printf '%s\n' "$!"
}

# An arm is done when its launcher has exited (the pipeline stopped, aborted,
# gated out or errored) — the monitor chain is detached, so the launcher
# exiting is NOT on its own proof the run finished. Both are polled.
arm_finished() {
    local fixture="$1" pid="$2"
    if kill -0 "$pid" 2>/dev/null; then
        return 1
    fi
    # The launcher is gone. If a detached monitor for this fixture is still
    # alive, the run is still going.
    if fixture_has_live_process "$fixture"; then
        return 1
    fi
    return 0
}

poll_arm() {
    local name="$1" fixture="$2" pid="$3"
    local waited=0
    while [ "$waited" -lt "$CEILING_SECS" ]; do
        if arm_finished "$fixture" "$pid"; then
            note "$name: finished after ${waited}s (stage=$(state_field "$fixture" stage))"
            printf 'finished\n'
            return 0
        fi
        sleep "$POLL_SECS"
        waited=$((waited + POLL_SECS))
        if [ $((waited % 300)) -eq 0 ]; then
            note "$name: ${waited}s elapsed — stage=$(state_field "$fixture" stage) validate_failures=$(state_field "$fixture" phase_validate_failures)"
        fi
    done
    note "$name: CEILING of ${CEILING_SECS}s reached — killing the process tree"
    kill -TERM -- "-$pid" 2>/dev/null || true
    signal_fixture "$fixture" TERM
    sleep 3
    signal_fixture "$fixture" KILL
    printf 'timeout\n'
}

# ---------------------------------------------------------------------------
# Observation collection. Sets ${ARM}_* globals via printf into a namespace.
# ---------------------------------------------------------------------------
declare -A OBS

collect() {
    local arm="$1" fixture="$2" outcome="$3" log="$4" started="$5" ended="$6"

    OBS["${arm}.fixture"]="$fixture"
    OBS["${arm}.log"]="$log"
    OBS["${arm}.outcome"]="$outcome"
    OBS["${arm}.duration_secs"]="$((ended - started))"
    OBS["${arm}.stage"]="$(state_field "$fixture" stage)"
    OBS["${arm}.gate_pending"]="$(state_field "$fixture" gate_pending)"
    OBS["${arm}.stopped"]="$(state_field "$fixture" stopped)"
    OBS["${arm}.stop_reason"]="$(state_field "$fixture" stop_reason)"
    OBS["${arm}.phase_validate_failures"]="$(state_field "$fixture" phase_validate_failures)"
    OBS["${arm}.consecutive_failures"]="$(state_field "$fixture" consecutive_failures)"

    local plan
    plan="$(generated_plan "$fixture")"
    OBS["${arm}.plan_file"]="${plan:-NONE}"
    if [ -n "$plan" ] && grep -q 'checkpoint:human-verify' "$plan" 2>/dev/null; then
        OBS["${arm}.plan_has_checkpoint"]="yes"
        OBS["${arm}.plan_checkpoint_line"]="$(grep -n 'checkpoint:human-verify' "$plan" | head -1)"
    else
        OBS["${arm}.plan_has_checkpoint"]="no"
        OBS["${arm}.plan_checkpoint_line"]=""
    fi

    # The checkpoint observation — the load-bearing row.
    OBS["${arm}.auto_approved"]="$(captures_contain "$fixture" 'Auto-approved checkpoint')"
    if [ "${OBS["${arm}.auto_approved"]}" = "no" ]; then
        # Fall back to the looser marker before concluding "not auto-approved":
        # the log string is an instruction to a model, and a paraphrase is a
        # different observation from an absence.
        OBS["${arm}.auto_approved_loose"]="$(captures_contain "$fixture" 'Auto-approved')"
    else
        OBS["${arm}.auto_approved_loose"]="yes"
    fi
    OBS["${arm}.checkpoint_surfaced"]="$(captures_contain "$fixture" 'CHECKPOINT REACHED')"
    OBS["${arm}.auto_approval_lines"]="$(grep_captures "$fixture" 'Auto-approved' 3)"
    OBS["${arm}.surfaced_lines"]="$(grep_captures "$fixture" 'CHECKPOINT REACHED' 2)"

    # Did the run get PAST the Code stage? A checkpoint that auto-approved
    # lets the Code stage finish; one that surfaced does not.
    local ev
    ev="$(events_file "$fixture")"
    if [ -f "$ev" ] && jq -e -s 'any(.[]; .event == "transition" and .to == "validate")' "$ev" > /dev/null 2>&1; then
        OBS["${arm}.reached_validate"]="yes"
    else
        OBS["${arm}.reached_validate"]="no"
    fi

    if [ -f "$ev" ]; then
        OBS["${arm}.loop_back_lines"]="$(grep -F '"event":"loop_back"' "$ev" 2>/dev/null | head -5 || true)"
        OBS["${arm}.gaps_only_loop"]="$(grep -F '"event":"loop_back"' "$ev" 2>/dev/null | grep -cF '"fix":"GapsOnly"' || true)"
        OBS["${arm}.full_execute_loop"]="$(grep -F '"event":"loop_back"' "$ev" 2>/dev/null | grep -cF '"fix":"FullExecute"' || true)"
        OBS["${arm}.event_kinds"]="$(jq -r '.event' "$ev" 2>/dev/null | sort | uniq -c | sort -rn | head -20 || true)"
    else
        OBS["${arm}.loop_back_lines"]=""
        OBS["${arm}.gaps_only_loop"]="0"
        OBS["${arm}.full_execute_loop"]="0"
        OBS["${arm}.event_kinds"]=""
    fi

    # Did the gaps-only pass find anything to run?
    OBS["${arm}.gaps_only_found_nothing"]="$(captures_contain "$fixture" 'No matching incomplete plans')"

    # The chain flag AFTER the run. Must be clear on both arms.
    local cfg
    cfg="$(worktree_config "$fixture")"
    if [ -f "$cfg" ]; then
        OBS["${arm}.chain_flag_after"]="$(jq -r '(.workflow._auto_chain_active // "MISSING") | tostring' "$cfg" 2>/dev/null || echo UNREADABLE)"
    else
        OBS["${arm}.chain_flag_after"]="NO_CONFIG"
    fi

    # Strays (T-35.1-18). Excludes this script's own process group — see
    # `stray_pids`.
    local live
    live="$(stray_pids "$fixture" | head -5 | tr '\n' ' ')"
    OBS["${arm}.strays"]="${live:-none}"
}

# A compact, comparable rendering of the CHECKPOINT observation. If the two arms
# produce the SAME string, the measurement is broken, not the subject.
#
# `reached_validate` was deliberately REMOVED from this string on 2026-08-09.
# It is a downstream consequence, not a checkpoint observation, and including it
# let the arms register as "different" on a difference that had nothing to do
# with auto-approval: the first executed run recorded
#
#     arm A: auto_approved=yes surfaced=yes reached_validate=no
#     arm B: auto_approved=yes surfaced=yes reached_validate=yes
#
# — two strings that differ, so FAILURE 3/3 stayed silent, while the fact that
# actually mattered (BOTH arms auto-approved) passed straight through the gate.
# The comparison must be over the checkpoint behaviour alone, or the negative
# control can be satisfied by noise. `reached_validate` is still reported in the
# evidence table; it is simply not part of the discrimination test.
checkpoint_observation() {
    local arm="$1"
    printf 'auto_approved=%s surfaced=%s' \
        "${OBS["${arm}.auto_approved_loose"]}" \
        "${OBS["${arm}.checkpoint_surfaced"]}"
}

# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------
DEST_BASE="${DEST_BASE:-${TMPDIR:-/tmp}/devflow-unattended-drill-$$}"

if [ "$SCAFFOLD_ONLY" = true ]; then
    FIXTURE="$(guard_destination "$DEST_BASE")"
    scaffold_fixture "$FIXTURE"
    echo "Scaffolded unattended-drill fixture: $FIXTURE"
    echo "  ROADMAP criteria: $([ "$WITH_GAP" = true ] && echo 2 || echo 1)"
    echo "  plan files: $(find "$FIXTURE/.planning/phases" -name '*PLAN.md' | wc -l) (must be 0 — D-11)"
    echo "Next: devflow start --phase 1 --agent claude --mode auto $FIXTURE"
    exit 0
fi

# --- F-22: build first, every time. A drill run against a stale binary
# --- measures the previous build.
note "building the workspace from $REPO_ROOT (F-22)"
cargo build --workspace --manifest-path "$REPO_ROOT/Cargo.toml" > /dev/null
DEVFLOW_BIN="$REPO_ROOT/target/debug/devflow"
[ -x "$DEVFLOW_BIN" ] || die "built binary not found at $DEVFLOW_BIN"
BUILD_COMMIT="$(git -C "$REPO_ROOT" rev-parse HEAD)"
BUILD_BRANCH="$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD)"
note "binary: $DEVFLOW_BIN (built from $BUILD_COMMIT on $BUILD_BRANCH)"

command -v claude > /dev/null 2>&1 || die "no \`claude\` on PATH — this drill needs a real agent"
CLAUDE_VERSION="$(claude --version 2>&1 | head -1)"
note "agent: $CLAUDE_VERSION"

LOG_DIR="${TMPDIR:-/tmp}/devflow-unattended-drill-logs-$$"
mkdir -p "$LOG_DIR"
note "logs (outside this checkout, F-21): $LOG_DIR"

FIXTURE_A="$(guard_destination "${DEST_BASE}-auto")"
scaffold_fixture "$FIXTURE_A"
TRACKED_FIXTURES+=("$FIXTURE_A")
note "arm A fixture (auto): $FIXTURE_A"

FIXTURE_B="$(guard_destination "${DEST_BASE}-supervise")"
scaffold_fixture "$FIXTURE_B"
TRACKED_FIXTURES+=("$FIXTURE_B")
note "arm B fixture (supervise, NEGATIVE CONTROL): $FIXTURE_B"

export DEVFLOW_GATE_TIMEOUT_SECS="$GATE_TIMEOUT_SECS"
note "DEVFLOW_GATE_TIMEOUT_SECS=$GATE_TIMEOUT_SECS (identical for both arms)"

LOG_A="$LOG_DIR/arm-a-auto.log"
LOG_B="$LOG_DIR/arm-b-supervise.log"

START_A=0
START_B=0
END_A=0
END_B=0
OUTCOME_A=""
OUTCOME_B=""

if [ "$PARALLEL" = true ]; then
    note "launching both arms in parallel"
    START_A=$(date +%s)
    PID_A="$(launch_arm "$FIXTURE_A" auto "$LOG_A")"
    TRACKED_PIDS+=("$PID_A")
    START_B=$(date +%s)
    PID_B="$(launch_arm "$FIXTURE_B" supervise "$LOG_B")"
    TRACKED_PIDS+=("$PID_B")
    OUTCOME_B="$(poll_arm "arm B (supervise)" "$FIXTURE_B" "$PID_B")"
    END_B=$(date +%s)
    OUTCOME_A="$(poll_arm "arm A (auto)" "$FIXTURE_A" "$PID_A")"
    END_A=$(date +%s)
else
    START_A=$(date +%s)
    PID_A="$(launch_arm "$FIXTURE_A" auto "$LOG_A")"
    TRACKED_PIDS+=("$PID_A")
    OUTCOME_A="$(poll_arm "arm A (auto)" "$FIXTURE_A" "$PID_A")"
    END_A=$(date +%s)
    START_B=$(date +%s)
    PID_B="$(launch_arm "$FIXTURE_B" supervise "$LOG_B")"
    TRACKED_PIDS+=("$PID_B")
    OUTCOME_B="$(poll_arm "arm B (supervise)" "$FIXTURE_B" "$PID_B")"
    END_B=$(date +%s)
fi

reap

collect A "$FIXTURE_A" "$OUTCOME_A" "$LOG_A" "$START_A" "$END_A"
collect B "$FIXTURE_B" "$OUTCOME_B" "$LOG_B" "$START_B" "$END_B"

OBS_A="$(checkpoint_observation A)"
OBS_B="$(checkpoint_observation B)"

# ---------------------------------------------------------------------------
# The three hard failures (T-35.1-19). Each has its OWN message naming which
# of the three fired, so a green exit can never mean "nothing happened".
# ---------------------------------------------------------------------------
FAILURES=()

if [ "${OBS[A.plan_has_checkpoint]}" != "yes" ] || [ "${OBS[B.plan_has_checkpoint]}" != "yes" ]; then
    FAILURES+=("FAILURE 1/3 — NO CHECKPOINT IN THE GENERATED PLAN. The fixture failed to set up its own measurement: arm A plan_has_checkpoint=${OBS[A.plan_has_checkpoint]}, arm B plan_has_checkpoint=${OBS[B.plan_has_checkpoint]}. A run that never reached a checkpoint proves nothing about auto-approval (F-19).")
fi

if [ "${OBS[A.gaps_only_loop]}" = "0" ] && [ "$WITH_GAP" = true ]; then
    FAILURES+=("FAILURE 2/3 — THE GAPS-ONLY LOOP NEVER RAN. No loop_back event with \"fix\":\"GapsOnly\" appears in arm A's events.jsonl (FullExecute loop-backs seen: ${OBS[A.full_execute_loop]}). The fix-loop half of ROADMAP criterion 1 was not exercised (F-18).")
fi

if [ "$OBS_A" = "$OBS_B" ]; then
    FAILURES+=("FAILURE 3/3 — BOTH ARMS OBSERVED THE SAME THING at the checkpoint: '$OBS_A'. The MEASUREMENT is broken, not the subject: an agent that simply never reached a checkpoint is indistinguishable from a successful auto-approval (F-20).")
fi

# ---------------------------------------------------------------------------
# Evidence.
# ---------------------------------------------------------------------------
mkdir -p "$(dirname "$OUT_FILE")"
{
    echo "# Phase 35.1 — Unattended-Mode Drill Record"
    echo
    echo "> Generated by \`scripts/unattended-drill.sh\` on $(date -u +%Y-%m-%dT%H:%M:%SZ)."
    echo "> Every claim below is traceable to a line in \`.devflow/events.jsonl\` or to a"
    echo "> capture file from the run. Nothing here is a recollection."
    echo
    echo "## Run identity"
    echo
    echo "| Field | Value |"
    echo "|---|---|"
    echo "| Build commit | \`$BUILD_COMMIT\` (branch \`$BUILD_BRANCH\`) |"
    echo "| Binary | \`$DEVFLOW_BIN\` |"
    echo "| Agent | \`$CLAUDE_VERSION\` |"
    echo "| Arm A fixture (auto) | \`$FIXTURE_A\` |"
    echo "| Arm B fixture (supervise) | \`$FIXTURE_B\` |"
    echo "| Gate timeout (both arms) | ${GATE_TIMEOUT_SECS}s |"
    echo "| Poll ceiling (per arm) | ${CEILING_SECS}s |"
    echo "| Deferred-criterion arm | $([ "$WITH_GAP" = true ] && echo "on (fix loop expected)" || echo "off (--no-gap)") |"
    echo
    echo "## Arm A vs Arm B — one row per observation"
    echo
    echo "The two arms differ by EXACTLY one thing: \`--mode auto\` vs \`--mode supervise\`."
    echo "Same generator, same agent, same environment, same \`--until validate\` cap."
    echo
    echo "| Observation | Arm A (auto) | Arm B (supervise — NEGATIVE CONTROL) |"
    echo "|---|---|---|"
    echo "| Outcome | ${OBS[A.outcome]} | ${OBS[B.outcome]} |"
    echo "| Wall clock | ${OBS[A.duration_secs]}s | ${OBS[B.duration_secs]}s |"
    echo "| Generated plan declares a blocking checkpoint | ${OBS[A.plan_has_checkpoint]} | ${OBS[B.plan_has_checkpoint]} |"
    echo "| **Checkpoint auto-approved in capture** | **${OBS[A.auto_approved_loose]}** | **${OBS[B.auto_approved_loose]}** |"
    echo "| **Checkpoint surfaced to a human in capture** | **${OBS[A.checkpoint_surfaced]}** | **${OBS[B.checkpoint_surfaced]}** |"
    echo "| **Run advanced past Code to Validate** | **${OBS[A.reached_validate]}** | **${OBS[B.reached_validate]}** |"
    echo "| Final stage | ${OBS[A.stage]} | ${OBS[B.stage]} |"
    echo "| Gate pending at end | ${OBS[A.gate_pending]} | ${OBS[B.gate_pending]} |"
    echo "| \`stopped\` | ${OBS[A.stopped]} | ${OBS[B.stopped]} |"
    echo "| \`phase_validate_failures\` | ${OBS[A.phase_validate_failures]} | ${OBS[B.phase_validate_failures]} |"
    echo "| \`loop_back\` with GapsOnly | ${OBS[A.gaps_only_loop]} | ${OBS[B.gaps_only_loop]} |"
    echo "| \`loop_back\` with FullExecute | ${OBS[A.full_execute_loop]} | ${OBS[B.full_execute_loop]} |"
    echo "| Chain flag after the run | ${OBS[A.chain_flag_after]} | ${OBS[B.chain_flag_after]} |"
    echo "| Surviving processes | ${OBS[A.strays]} | ${OBS[B.strays]} |"
    echo
    echo "**Compact checkpoint observation** — the string the drill compares:"
    echo
    echo '```'
    echo "arm A: $OBS_A"
    echo "arm B: $OBS_B"
    echo '```'
    echo
    echo "## Quoted capture lines"
    echo
    echo "### Arm A (auto) — auto-approval"
    echo
    echo '```'
    printf '%s\n' "${OBS[A.auto_approval_lines]:-(no matching line)}"
    echo '```'
    echo
    echo "### Arm A (auto) — surfaced checkpoint (expected: none)"
    echo
    echo '```'
    printf '%s\n' "${OBS[A.surfaced_lines]:-(no matching line)}"
    echo '```'
    echo
    echo "### Arm B (supervise) — auto-approval (expected: none)"
    echo
    echo '```'
    printf '%s\n' "${OBS[B.auto_approval_lines]:-(no matching line)}"
    echo '```'
    echo
    echo "### Arm B (supervise) — surfaced checkpoint"
    echo
    echo '```'
    printf '%s\n' "${OBS[B.surfaced_lines]:-(no matching line)}"
    echo '```'
    echo
    echo "## The fix loop"
    echo
    echo "\`loop_back\` events from arm A's \`.devflow/events.jsonl\`, verbatim:"
    echo
    echo '```json'
    printf '%s\n' "${OBS[A.loop_back_lines]:-(none)}"
    echo '```'
    echo
    echo "Gaps-only pass reported \"No matching incomplete plans\": ${OBS[A.gaps_only_found_nothing]}"
    echo
    echo "## Criterion 6 — \`phase_validate_failures\`"
    echo
    echo "**${OBS[A.phase_validate_failures]}** — read from \`${FIXTURE_A}/.devflow/state-01.json\` at the end of the auto arm's run on $(date -u +%Y-%m-%d)."
    echo
    echo "This is an OBSERVATION, not a pass/fail. One run is one sample. It licenses"
    echo "nothing about whether \`MAX_PHASE_VALIDATE_FAILURES = 10\` is the right ceiling;"
    echo "it converts \"no history exists\" into \"one data point exists\". Do not"
    echo "editorialize the ceiling up or down on the strength of this number."
    echo
    echo "## Event kinds recorded"
    echo
    echo "Arm A:"
    echo '```'
    printf '%s\n' "${OBS[A.event_kinds]:-(none)}"
    echo '```'
    echo
    echo "Arm B:"
    echo '```'
    printf '%s\n' "${OBS[B.event_kinds]:-(none)}"
    echo '```'
    echo
    echo "## Drill assertions"
    echo
    if [ "${#FAILURES[@]}" -eq 0 ]; then
        echo "All three hard assertions passed."
    else
        for f in "${FAILURES[@]}"; do
            echo "- **$f**"
        done
    fi
    echo
    echo "## What this run does NOT establish"
    echo
    echo "- **It does not establish that DevFlow sets and clears the flag at the right"
    echo "  moments.** That is plan 35.1-01's coverage (the in-process guard, its \`Drop\`,"
    echo "  and the supervise negative control) and 35.1-02's (the real-SIGKILL leak and the"
    echo "  force-clear repair at both launch entry points). 35.1 D-10 constraint 3 is"
    echo "  explicit that a green drill must not be cited as coverage of DevFlow's own flag"
    echo "  management, and it is not cited that way here."
    echo "- **It does not establish anything about the Plan stage.** The Plan stage cannot"
    echo "  receive this bypass at all: the same upstream boolean that would auto-approve a"
    echo "  Plan checkpoint also makes \`plan-phase\` chain into \`execute-phase\`, double-"
    echo "  executing Code and misattributing its commits (D-04, upstream G-01)."
    echo "- **It does not establish anything about a legacy-arm or non-Claude launch.**"
    echo "  Those shapes are refused at preflight by 35.1-03; nothing here exercises them."
    echo "- **One run is one sample.** Both arms are n=1. They demonstrate the mechanism"
    echo "  discriminates on the path taken; they say nothing about behaviour under load,"
    echo "  concurrency, quota pressure, or a longer phase."
    echo
} > "$OUT_FILE"

note "evidence written: $OUT_FILE"

if [ "${#FAILURES[@]}" -gt 0 ]; then
    KEEP_FIXTURES=true
    echo >&2
    echo "DRILL FAILED — ${#FAILURES[@]} assertion(s):" >&2
    for f in "${FAILURES[@]}"; do
        echo "  * $f" >&2
    done
    echo >&2
    echo "Fixtures retained for inspection:" >&2
    echo "  arm A: $FIXTURE_A" >&2
    echo "  arm B: $FIXTURE_B" >&2
    echo "  logs:  $LOG_DIR" >&2
    exit 1
fi

note "drill passed — removing fixtures"
rm -rf "$FIXTURE_A" "$FIXTURE_B"
note "logs retained: $LOG_DIR"
exit 0
