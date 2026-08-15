#!/usr/bin/env bash
#
# scripts/drain-drill.sh — the phase 35.3 drain-gate measurement drill.
#
# ============================================================================
# WHAT THIS DRILL ESTABLISHES
# ============================================================================
#
# Which event families the current Claude CLI actually emits for each of the two
# concurrent-work mechanisms — sub-agent dispatch and backgrounded shell — from
# live capture, at n>=2 per path, so the drain gate's coverage can be measured
# against production rather than inferred from source reading (999.83 / HARDEN-06).
#
# Two prompt variants, one per path, so each capture's events are attributable
# by construction:
#   Variant A (sub-agent dispatch):  delegate a trivial task via the Task tool.
#   Variant B (backgrounded shell):  launch a long-running shell in the background.
#
# ============================================================================
# SAFETY
# ============================================================================
#
# Fixtures are scaffolded OUTSIDE this checkout, guarded as unattended-drill.sh
# guards its destination (T-23-01: refuse any destination inside this worktree
# OR the primary checkout), with a repo-local git identity only (T-23-02). Every
# spawned child is killed by a trap on every exit path (T-35.1-18).
#
# Usage:
#   scripts/drain-drill.sh [options] [destination-base]
#
#   --scaffold-only         Scaffold ONE throwaway fixture and exit (no agent).
#   --prompt <A|B>          Prompt variant for full runs (default: A).
#   --runs <n>              Repetitions per path (default: 2).
#   --out <path>            Evidence directory (default: the phase's 35.3-evidence/).
#
# OUTPUT: per run, writes <out>/<V>-run-<i>-raw_output.jsonl, a `-stderr.log`,
# and a `-run.log` provenance file (argv, claude version, build commit, exit
# status, parse-based counts). Appends one row per run to <out>/counts.tsv. The
# analysis prose (COUNTS.md) is a separate synthesis step, not emitted here.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
# In a linked worktree --show-toplevel is the worktree, not the primary checkout.
# Both must be off-limits as a destination (999.37 class).
GIT_COMMON_DIR="$(git -C "$SCRIPT_DIR" rev-parse --path-format=absolute --git-common-dir)"
MAIN_CHECKOUT="$(dirname "$GIT_COMMON_DIR")"

PHDIR="$REPO_ROOT/.planning/phases/35.3-drain-gate-concurrency-measurement-999-83"
SCAFFOLD_ONLY=false
PROMPT_VARIANT="A"
RUNS=2
OUT_FILE="$PHDIR/35.3-evidence"
DEST_BASE=""

die() { echo "ERROR: $*" >&2; exit 1; }
note() { echo "[drill] $*"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --scaffold-only) SCAFFOLD_ONLY=true ;;
        --prompt)
            shift; [ $# -gt 0 ] || die "--prompt needs A or B"
            PROMPT_VARIANT="$1" ;;
        --runs)
            shift; [ $# -gt 0 ] || die "--runs needs a number"
            RUNS="$1" ;;
        --out)
            shift; [ $# -gt 0 ] || die "--out needs a path"
            OUT_FILE="$1" ;;
        -h | --help)
            sed -n '32,42p' "${BASH_SOURCE[0]}"
            exit 0 ;;
        -*) die "unknown option: $1" ;;
        *) DEST_BASE="$1" ;;
    esac
    shift
done

# Validate the two tunables so a typo cannot silently measure the wrong workload
# or run zero captures (code-review findings, 2026-08-15).
case "$PROMPT_VARIANT" in
    A | B) ;;
    *) die "--prompt must be A or B (got '$PROMPT_VARIANT')" ;;
esac
if ! printf '%s' "$RUNS" | grep -qE '^[1-9][0-9]*$'; then
    die "--runs must be a positive integer (got '$RUNS')"
fi

# ---------------------------------------------------------------------------
# Destination guard (T-23-01, inherited from unattended-drill.sh).
# ---------------------------------------------------------------------------
guard_destination() {
    local dest="$1" dest_abs forbidden
    dest_abs="$(realpath -m -- "$dest")"
    for forbidden in "$REPO_ROOT" "$MAIN_CHECKOUT"; do
        if [ "$dest_abs" = "$forbidden" ] || case "$dest_abs" in "$forbidden"/*) true ;; *) false ;; esac; then
            echo "ERROR: refusing to scaffold inside this checkout ($forbidden)." >&2
            echo "       Requested destination: $dest_abs" >&2
            exit 1
        fi
    done
}

# ---------------------------------------------------------------------------
# Fixture scaffolding (T-23-02 repo-local identity; nothing global).
# ---------------------------------------------------------------------------
scaffold_fixture() {
    local dest="$1"
    guard_destination "$dest"
    mkdir -p "$dest"
    git -C "$dest" init -q
    git -C "$dest" config user.name "drain-drill"
    git -C "$dest" config user.email "drain-drill@local.invalid"
    git -C "$dest" config commit.gpgsign false
    git -C "$dest" config core.hooksPath /dev/null
    printf '# throwaway drain-drill fixture\n' > "$dest/README.md"
    git -C "$dest" add README.md
    git -C "$dest" commit -q -m "scaffold" --no-gpg-sign
    note "scaffolded fixture at $dest"
    printf '%s\n' "$dest"
}

# ---------------------------------------------------------------------------
# Prompt variants. Each is a single instruction the headless agent must honour;
# the capture records whether the CLI emitted the expected concurrent-work
# events for it.
# ---------------------------------------------------------------------------
prompt_A() {
    cat <<'PROMPT'
Use the Task subagent tool to delegate this one small piece of work: ask a single
subagent to compute the sum of 12 + 34 and return the numeric answer. When the
subagent returns, report the answer as your final message. Do the delegation with
the Task tool (not by running a shell command).
PROMPT
}

prompt_B() {
    cat <<'PROMPT'
Launch a shell command IN THE BACKGROUND (run_in_background: true): a command that
sleeps 45 seconds then writes the word "done" to /tmp/drain-drill-bg-marker. Start
it in the background, do NOT wait for it, and immediately report as your final
message that the background command has been launched.
PROMPT
}

# ---------------------------------------------------------------------------
# The child argv DevFlow itself uses (claude.rs exec_command), reproduced verbatim
# so the measurement is of the same child the drain gate supervises.
# ---------------------------------------------------------------------------
CLAUDE_ARGV=(claude -p --input-format stream-json --output-format stream-json --verbose --dangerously-skip-permissions)

launch_and_capture() {
    local variant="$1" run_i="$2" dest="$3" prompt capture err turn_file turn pid rc
    prompt="$([ "$variant" = "A" ] && prompt_A || prompt_B)"
    mkdir -p "$OUT_FILE"
    capture="$OUT_FILE/$variant-run-$run_i-raw_output.jsonl"
    err="$OUT_FILE/$variant-run-$run_i-stderr.log"
    # Build the user-turn JSON with jq so the prompt is escaped, not interpolated
    # (the same reason monitor.rs uses serde_json, not format!).
    turn="$(jq -cn --arg content "$prompt" '{type:"user",message:{role:"user",content:$content}}')"
    # Temp file, not a pipe: a backgrounded pipeline would make the child PID
    # ambiguous and trip `pipefail` on a non-zero exit.
    turn_file="$(mktemp)"
    printf '%s\n' "$turn" > "$turn_file"

    note "variant $variant run $run_i: launching claude (capture -> $capture)"
    # Backgrounded so the child PID is tracked and reaped (T-35.1-18), and a
    # non-zero exit is recorded in the run.log instead of aborting the script
    # before any provenance exists.
    "${CLAUDE_ARGV[@]}" < "$turn_file" > "$capture" 2> "$err" &
    pid=$!
    CHILD_PIDS="$CHILD_PIDS $pid"
    rc=0
    wait "$pid" || rc=$?
    # Runs are sequential, so this child is the only one tracked; clear the list.
    # (A `grep -vx` here would return 1 on the reap — pipefail + set -e abort.)
    CHILD_PIDS=""
    rm -f "$turn_file"
    note "variant $variant run $run_i: claude exited ($rc)"

    write_run_log "$variant" "$run_i" "$capture" "$rc"
    append_counts_row "$variant" "$run_i" "$capture"
}

count_events() {
    local capture="$1"
    echo "  counts for $(basename "$capture"):"
    jq -r 'select(.type=="system") | .subtype' "$capture" 2>/dev/null | sort | uniq -c | sed 's/^/    /'
    local bg
    bg="$(jq -s '[.[] | .. | objects | select(.run_in_background? == true)] | length' "$capture" 2>/dev/null || echo 0)"
    echo "    run_in_background:true observed: $bg"
}

# Write a self-contained provenance file per capture, so a re-run of the drill
# reproduces the evidence without any hand assembly (code-review HIGH, 2026-08-15).
write_run_log() {
    local variant="$1" run_i="$2" capture="$3" rc="$4"
    local log="$OUT_FILE/$variant-run-$run_i-run.log"
    local counts bg
    counts="$(jq -r 'select(.type=="system") | .subtype' "$capture" 2>/dev/null | sort | uniq -c | sed 's/^/  /')"
    bg="$(jq -s '[.[] | .. | objects | select(.run_in_background? == true)] | length' "$capture" 2>/dev/null || echo 0)"
    {
        echo "# Phase 35.3 — drain-gate capture provenance: Variant $variant run $run_i"
        echo
        echo "command invoked : scripts/drain-drill.sh --prompt $variant --runs $RUNS"
        echo "child argv      : ${CLAUDE_ARGV[*]}"
        echo "claude_cli_version : $(claude --version 2>&1 | head -1)"
        echo "devflow build commit : $(git -C "$REPO_ROOT" rev-parse --short HEAD)"
        echo "outcome         : claude exited $rc"
        echo
        echo "counts (parse-based, jq):"
        echo "$counts"
        echo
        echo "run_in_background:true observed : $bg"
    } > "$log"
}

# Append one machine-readable row per run; counts.tsv is the raw mapping the
# analysis prose (COUNTS.md) is synthesized from.
append_counts_row() {
    local variant="$1" run_i="$2" capture="$3"
    local tsv="$OUT_FILE/counts.tsv" ts tp tn tu bgc bg
    ts="$(jq -s '[.[] | select(.type=="system" and .subtype=="task_started")] | length' "$capture" 2>/dev/null || echo 0)"
    tp="$(jq -s '[.[] | select(.type=="system" and .subtype=="task_progress")] | length' "$capture" 2>/dev/null || echo 0)"
    tn="$(jq -s '[.[] | select(.type=="system" and .subtype=="task_notification")] | length' "$capture" 2>/dev/null || echo 0)"
    tu="$(jq -s '[.[] | select(.type=="system" and .subtype=="task_updated")] | length' "$capture" 2>/dev/null || echo 0)"
    bgc="$(jq -s '[.[] | select(.type=="system" and .subtype=="background_tasks_changed")] | length' "$capture" 2>/dev/null || echo 0)"
    bg="$(jq -s '[.[] | .. | objects | select(.run_in_background? == true)] | length' "$capture" 2>/dev/null || echo 0)"
    if [ ! -f "$tsv" ]; then
        printf 'variant\trun\ttask_started\ttask_progress\ttask_notification\ttask_updated\tbackground_tasks_changed\trun_in_background\n' > "$tsv"
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$variant" "$run_i" "$ts" "$tp" "$tn" "$tu" "$bgc" "$bg" >> "$tsv"
}

# ---------------------------------------------------------------------------
# Process hygiene (T-35.1-18): kill spawned children on every exit path.
# ---------------------------------------------------------------------------
CHILD_PIDS=""
cleanup() {
    local p
    for p in $CHILD_PIDS; do
        kill "$p" 2>/dev/null || true
    done
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# Mode dispatch.
# ---------------------------------------------------------------------------
if [ "$SCAFFOLD_ONLY" = true ]; then
    dest="${DEST_BASE:-$(mktemp -d /tmp/drain-drill-fixture-XXXXXX)}"
    scaffold_fixture "$dest"
    exit 0
fi

if [ -z "$DEST_BASE" ]; then
    DEST_BASE="$(mktemp -d /tmp/drain-drill-fixture-XXXXXX)"
fi
fixture="$(scaffold_fixture "$DEST_BASE")"

i=1
while [ "$i" -le "$RUNS" ]; do
    launch_and_capture "$PROMPT_VARIANT" "$i" "$fixture"
    count_events "$OUT_FILE/$PROMPT_VARIANT-run-$i-raw_output.jsonl"
    i=$((i + 1))
done

note "evidence written under $OUT_FILE (raw captures + run.log per capture + counts.tsv)"
