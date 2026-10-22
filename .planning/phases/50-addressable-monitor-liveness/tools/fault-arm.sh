#!/usr/bin/env bash
# The Phase 50 fault arm (D-17, R-10): run each fault case on a built devflow
# binary with fake agents and no GitHub repository, and write one record per
# case for `check-supervisor-evidence.sh --fault` to judge.
#
# Usage:
#   fault-arm.sh run --bin <devflow> --out <dir> [--cases <name,name,...|all>]
#   fault-arm.sh --selftest
#
# `run` refuses an existing <dir>. Everything it launches lives under one
# `mktemp -d /tmp/df50f.XXXXXX` scratch directory: a private HOME,
# XDG_CONFIG_HOME, XDG_CACHE_HOME and DEVFLOW_CACHE_DIR (so every supervisor
# socket it binds is under that directory, never under the operator's real
# ~/.cache/devflow), one scaffolded project per case
# (scripts/scratch-dogfood-repo.sh) and one fake-agent bin dir per case. A
# fake `claude` answers `--version` and the delivery canary, records its own
# pid and start time, waits until it can take its case's trigger file, then
# runs its released body (success, or a failed Define that parks at a gate).
#
# <dir> receives `bin_sha256` (before the first case; a re-hash after any case
# that differs stops the run), `src_tree` and `src_matches_checkout` (R5-W1:
# the crate source of the commit the binary embeds, read from its first
# `workflow_started` event), and `case-<name>.json` per case.
#
# Process rules: a process is signalled only by a pid this run recorded (from
# a sidecar or a fake agent's own registration) after its recorded start time
# is confirmed; nothing is ever signalled by name pattern. The EXIT trap stops
# every run it launched with `devflow stop`, then signals any recorded process
# still alive after the same start-time check, then removes the scratch
# directory. A process this run cannot account for at exit makes `run` fail.
#
# Exit codes: 0 when every case was run and recorded with its expectation met
# and nothing survived; 1 otherwise (records are still written); 2 on usage.
set -euo pipefail

T="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELF="$T/$(basename "${BASH_SOURCE[0]}")"
CHECKOUT="$(git -C "$T" rev-parse --show-toplevel)"
SCAFFOLD="$CHECKOUT/scripts/scratch-dogfood-repo.sh"
CRATE_PATHS=(crates Cargo.toml Cargo.lock rust-toolchain.toml)
KNOWN_CASES=(alive-stop-stream alive-stop-capture kill9-stale-stop kill9-at-gate stop-races-completion
    publish-unlink-window)
# R-10's publish/unlink window cannot be hit live by timing; its record points
# at the fixture that reaches it deterministically.
WINDOW_FIXTURE=pipeline_launch::tests::a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor
GATE_REL=.devflow/gates/01-define.json
GATE_RESPONSE_REL=.devflow/gates/01-define.response.json
LOCK_REL=.devflow/lock-01
EXIT_REL=.devflow/phase-01-exit
KILL_WAIT=30
GATE_WAIT=60
PHASE=1
STATE_REL=.devflow/state-01.json
EVENTS_REL=.devflow/events.jsonl
STOP_WAIT=10
SETTLE_SECS=2

die() {
    echo "fault-arm: $*" >&2
    exit 1
}

usage() {
    sed -n '/^# Usage:/,/^#$/p' "$SELF" >&2
    exit 2
}

# ---------------------------------------------------------------------------
# /proc helpers
# ---------------------------------------------------------------------------

# The fields of /proc/<pid>/stat after the comm field (field 3 onwards).
stat_rest() {
    local line
    { read -r line <"/proc/$1/stat"; } 2>/dev/null || return 1
    printf '%s\n' "${line##*) }"
}

# Field 22, the start time in clock ticks since boot.
start_time() {
    local rest
    rest="$(stat_rest "$1")" || return 1
    printf '%s\n' "$rest" | cut -d' ' -f20
}

# Whether <pid> exists and is not a zombie.
is_running() {
    local rest
    rest="$(stat_rest "$1")" || return 1
    [ "${rest%% *}" != Z ]
}

# Whether <pid> is running and still has start time <start>.
same_process() {
    local pid="$1" start="$2" now
    is_running "$pid" || return 1
    now="$(start_time "$pid")" || return 1
    [ "$now" = "$start" ]
}

# Signal <pid> with <sig> only when it is above 1 and still the process
# recorded with <start>. Returns 1 (and signals nothing) otherwise.
signal_recorded() {
    local sig="$1" pid="$2" start="$3"
    case "$pid" in '' | *[!0-9]*) return 1 ;; esac
    [ "$pid" -gt 1 ] || return 1
    same_process "$pid" "$start" || return 1
    kill -s "$sig" "$pid" 2>/dev/null
}

# The running pids whose process group (stat field 5) is <pgid>. `ps -g`
# selects by session, not process group, on procps-ng, so /proc is read.
group_pids() {
    local pgid="$1" stat_file pid line state pgrp
    case "$pgid" in '' | *[!0-9]*) return 0 ;; esac
    for stat_file in /proc/[0-9]*/stat; do
        pid="${stat_file#/proc/}"
        pid="${pid%/stat}"
        { read -r line <"$stat_file"; } 2>/dev/null || continue
        read -r state _ pgrp _ <<<"${line##*) }"
        [ "$state" != Z ] || continue
        [ "$pgrp" = "$pgid" ] && printf '%s\n' "$pid"
    done
    return 0
}

# One `ps -o pid,pgid,sid,lstart,args` line per member of <pgid>, as a JSON
# array of strings (`[]` for an empty group).
members_json() {
    local pids
    pids="$(group_pids "$1" | paste -sd, -)"
    if [ -z "$pids" ]; then
        printf '[]\n'
        return 0
    fi
    { ps -o pid=,pgid=,sid=,lstart=,args= -p "$pids" 2>/dev/null || true; } | jq -R . | jq -s -c .
}

group_empty() {
    [ -z "$(group_pids "$1")" ]
}

# wait_until <seconds> <cmd...>: poll every 0.1 s until <cmd> succeeds.
wait_until() {
    local deadline=$((SECONDS + $1))
    shift
    until "$@"; do
        [ "$SECONDS" -lt "$deadline" ] || return 1
        sleep 0.1
    done
}

# ---------------------------------------------------------------------------
# run state
# ---------------------------------------------------------------------------
S=""
BIN=""
OUT=""
BIN_SHA=""
FAILED=0
declare -a PROJECTS=()
declare -A KNOWN_START=()

# Record <pid> with <start> as a process this run started.
remember() {
    case "$1" in '' | *[!0-9]*) return 0 ;; esac
    KNOWN_START[$1]="$2"
}

# Record every fake agent that registered in <agents file>.
remember_agents() {
    local file="$1" pid start
    [ -f "$file" ] || return 0
    while read -r pid start; do
        remember "$pid" "$start"
    done <"$file"
}

# hermetic <fake bin dir> <cmd...>: run <cmd> with the private HOME, XDG and
# DEVFLOW_CACHE_DIR, no inherited DEVFLOW_* or repo-local GIT_* variable, no
# system git config, and the fake agents first on PATH.
hermetic() {
    local fake="$1" name
    shift
    local -a scrub=()
    for name in GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_CONFIG GIT_PREFIX \
        GIT_CONFIG_GLOBAL GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE; do
        scrub+=(-u "$name")
    done
    while IFS= read -r name; do
        scrub+=(-u "$name")
    done < <(compgen -e | rg '^DEVFLOW_' || true)
    env "${scrub[@]}" HOME="$S/h" XDG_CONFIG_HOME="$S/h/.config" XDG_CACHE_HOME="$S/h/.cache" \
        DEVFLOW_CACHE_DIR="$S/c" GIT_CONFIG_NOSYSTEM=1 PATH="$fake:$PATH" "$@"
}

# devflow <project> <fake> <args...>: the binary under test, from <project>.
devflow_in() {
    local project="$1" fake="$2"
    shift 2
    (cd "$project" && hermetic "$fake" timeout 120 "$BIN" "$@")
}

# The liveness word on `devflow status`'s supervisor line, read from stdin:
# ALIVE, STALE, GONE, or ABSENT when no supervisor line carries one.
probe_word() {
    local word
    word="$(rg -o -r '$1' -e '^  supervisor: .* — (ALIVE|STALE|GONE)' | head -n 1 || true)"
    printf '%s\n' "${word:-ABSENT}"
}

# The recorded supervisor's liveness for <project>, as `devflow status` reports it.
probe() {
    local out
    out="$(devflow_in "$1" "$2" status "$1" 2>&1)" || {
        echo ERROR
        return 0
    }
    printf '%s\n' "$out" | probe_word
}

events_of() {
    cat -- "$1/$EVENTS_REL" 2>/dev/null || true
}

# How many events named <name> <project> recorded.
event_count() {
    events_of "$1" | jq -s --arg e "$2" '[.[] | select(.event == $e)] | length'
}

# true when <project> recorded an in-process advance event.
advance_ran() {
    events_of "$1" | jq -s '[.[] | select(.event == "advance_evaluated" or .event == "advance_stopped"
        or .event == "advance_failed")] | length > 0'
}

state_field() {
    jq -r "$2" "$1/$STATE_REL" 2>/dev/null || echo ABSENT
}

# ---------------------------------------------------------------------------
# fakes and launch
# ---------------------------------------------------------------------------
RELEASE_SUCCEEDS='git commit -q --allow-empty -m "fake stage work"
printf '"'"'work done\nDEVFLOW_RESULT: {"status":"success","commits":1}\n'"'"'
exit 0'
RELEASE_FAILS_TO_A_GATE='printf '"'"'DEVFLOW_RESULT: {"status":"failed","reason":"gate fixture"}\n'"'"'
exit 0'

# write_fake <dir> <trigger> <agents file> <released body>
write_fake() {
    local dir="$1" trigger="$2" agents="$3" released="$4"
    mkdir -p "$dir"
    cat >"$dir/claude" <<FAKE
#!/bin/sh
if [ "\$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "\$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=\$(printf '%s' "\$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "\$token"
    exit 0
    ;;
esac
start=\$(sed 's/.*) //' /proc/\$\$/stat | cut -d' ' -f20)
printf '%s %s\n' "\$\$" "\$start" >>'$agents'
while ! mv '$trigger' '$trigger.taken' 2>/dev/null; do sleep 0.05; done
$released
FAKE
    chmod 0755 "$dir/claude"
}

# Per-case paths.
case_paths() {
    P="$S/p-$1"
    F="$S/f-$1"
    TRIG="$S/t-$1"
    AGENTS="$S/a-$1"
}

# launch <case> <released body> [start flags...]: scaffold the case's project
# and `devflow start` it in supervise mode; sets P F TRIG AGENTS SOCKET SIDECAR
# SUP_PID SUP_START PGID AGENT_START SHAPE LAUNCH_ID. Returns 1 when the launch
# did not record a supervisor.
launch() {
    local name="$1" released="$2" log
    shift 2
    case_paths "$name"
    write_fake "$F" "$TRIG" "$AGENTS" "$released"
    log="$S/log-$name"
    hermetic "$F" bash "$SCAFFOLD" "$P" >"$log" 2>&1 || {
        echo "fault-arm: $name: scaffold failed (see below)" >&2
        cat -- "$log" >&2
        return 1
    }
    PROJECTS+=("$P")
    devflow_in "$P" "$F" start --phase "$PHASE" --agent claude --mode supervise --no-worktree \
        --until plan "$@" "$P" >>"$log" 2>&1 || {
        echo "fault-arm: $name: devflow start failed" >&2
        cat -- "$log" >&2
        return 1
    }
    SOCKET="$(state_field "$P" '.supervisor.socket_path // "ABSENT"')"
    SIDECAR="${SOCKET%.sock}.json"
    [ -f "$SIDECAR" ] || {
        echo "fault-arm: $name: no sidecar beside the recorded socket $SOCKET" >&2
        return 1
    }
    SUP_PID="$(jq -r .supervisor_pid "$SIDECAR")"
    SUP_START="$(jq -r .supervisor_start_time "$SIDECAR")"
    PGID="$(jq -r .agent_pgid "$SIDECAR")"
    AGENT_START="$(jq -r .agent_start_time "$SIDECAR")"
    SHAPE="$(jq -r .launch "$SIDECAR")"
    LAUNCH_ID="$(jq -r .launch_id "$SIDECAR")"
    remember "$SUP_PID" "$SUP_START"
    remember "$PGID" "$AGENT_START"
    wait_until 10 test -s "$AGENTS" || true
    remember_agents "$AGENTS"
}

# The fake agents of the case that are still alive, as a JSON array of pids.
alive_agents_json() {
    local file="$1" pid start
    {
        if [ -f "$file" ]; then
            while read -r pid start; do
                ! same_process "$pid" "$start" || printf '%s\n' "$pid"
            done <"$file"
        fi
    } | jq -R 'tonumber' | jq -s -c .
}

# `devflow stop` for the current case; its output with ANSI colour removed.
stop_case() {
    local out rc=0
    out="$(devflow_in "$P" "$F" stop --phase "$PHASE" "$P" 2>&1)" || rc=$?
    printf '%s\n' "$out" | sed 's/\x1b\[[0-9;]*m//g'
    return "$rc"
}

# ---------------------------------------------------------------------------
# records
# ---------------------------------------------------------------------------

# write_record <name> <expected_met> <unmet json> <extra json>: the common
# fields from the launch globals plus <extra>.
write_record() {
    local name="$1" met="$2" unmet="$3" extra="$4"
    jq -n --arg case "$name" --arg sha "$BIN_SHA" --arg shape "$SHAPE" --arg socket "$SOCKET" \
        --arg launch_id "$LAUNCH_ID" --argjson sup "$SUP_PID" --argjson met "$met" \
        --argjson unmet "$unmet" --argjson extra "$extra" '{
            case: $case, live: true, bin_sha256: $sha, launch_shape: $shape,
            socket_path: $socket, launch_id: $launch_id, supervisor_pid: $sup
        } + $extra + {expected_met: $met, unmet: $unmet}' >"$OUT/case-$name.json"
}

# R5-W1: the crate source of <commit> in <repo>, written to <outdir>.
write_src_record() {
    local outdir="$1" commit="$2" repo="$3" ids="" head status matches=false
    if [ -n "$commit" ]; then
        ids="$(git -C "$repo" rev-parse "${CRATE_PATHS[@]/#/$commit:}" 2>/dev/null)" || ids=""
    fi
    printf '%s\n' "${ids:-unknown}" >"$outdir/src_tree"
    if [ -n "$ids" ] &&
        status="$(git -C "$repo" status --porcelain -- "${CRATE_PATHS[@]}")" && [ -z "$status" ] &&
        head="$(git -C "$repo" rev-parse "${CRATE_PATHS[@]/#/HEAD:}")" && [ "$head" = "$ids" ]; then
        matches=true
    fi
    printf '%s\n' "$matches" >"$outdir/src_matches_checkout"
}

# ---------------------------------------------------------------------------
# cases
# ---------------------------------------------------------------------------

# alive-stop-<shape>: ALIVE → `devflow stop` → GONE with no member left.
case_alive_stop() {
    local name="$1" shape="$2" probe_before members_before stop_out stop_rc=0 probe_after members_after
    shift 2
    launch "$name" "$RELEASE_SUCCEEDS" "$@" || return 1
    probe_before="$(probe "$P" "$F")"
    members_before="$(members_json "$PGID")"
    stop_out="$(stop_case)" || stop_rc=$?
    wait_until "$STOP_WAIT" group_empty "$PGID" || true
    sleep "$SETTLE_SECS"
    remember_agents "$AGENTS"
    probe_after="$(probe "$P" "$F")"
    members_after="$(members_json "$PGID")"
    finish_alive_stop "$name" "$shape" "$probe_before" "$members_before" "$stop_out" "$stop_rc" \
        "$probe_after" "$members_after"
}

finish_alive_stop() {
    local name="$1" shape="$2" probe_before="$3" members_before="$4" stop_out="$5" stop_rc="$6"
    local probe_after="$7" members_after="$8" launched stopped advance unmet=()
    launched="$(event_count "$P" stage_launched)"
    stopped="$(state_field "$P" '.stopped')"
    advance="$(advance_ran "$P")"
    [ "$SHAPE" = "$shape" ] || unmet+=("launch shape $SHAPE, expected $shape")
    [ "$probe_before" = ALIVE ] || unmet+=("probe before stop $probe_before")
    [ "$members_before" != '[]' ] || unmet+=("no agent group member before stop")
    [ "$stop_rc" -eq 0 ] || unmet+=("devflow stop exited $stop_rc")
    [ "$probe_after" = GONE ] || unmet+=("probe after stop $probe_after")
    [ "$members_after" = '[]' ] || unmet+=("agent group member survived the stop")
    [ "$launched" -eq 1 ] || unmet+=("$launched stage_launched events, expected 1")
    [ "$stopped" = true ] || unmet+=("state.stopped is $stopped")
    record_case "$name" "$(jq -n --arg pb "$probe_before" --arg pa "$probe_after" \
        --argjson mb "$members_before" --argjson ma "$members_after" --argjson adv "$advance" \
        --argjson n "$launched" --arg st "$stopped" --argjson rc "$stop_rc" --arg out "$stop_out" '{
            probe_before: $pb, probe_after: $pa, members_before: $mb, members_after: $ma,
            advance_ran: $adv, next_stage_launched: ($n > 1), stage_launched_count: $n,
            state_stopped: ($st == "true"), stop_exit: $rc, stop_output: $out
        }')" "${unmet[@]+"${unmet[@]}"}"
}

# record_case <name> <extra json> [unmet reason...]
record_case() {
    local name="$1" extra="$2" met=true unmet
    shift 2
    [ "$#" -eq 0 ] || met=false
    unmet="$(printf '%s\n' "$@" | jq -R . | jq -s -c 'map(select(length > 0))')"
    write_record "$name" "$met" "$unmet" "$extra"
    if [ "$met" = true ]; then
        echo "fault-arm: case $name: expectation met"
    else
        echo "fault-arm: case $name: expectation NOT met: $*" >&2
        FAILED=1
    fi
}

# The per-phase lock's holder: none (no lock file, unparsable, or its pid is not
# running), live (running with the recorded start time), recycled (running
# with another start time) or unconfirmable (running, no start time recorded).
lock_status() {
    local file="$1/$LOCK_REL" pid="" start="" now
    [ -f "$file" ] || { echo none; return 0; }
    { read -r pid; read -r start; } <"$file" || true
    case "$pid" in '' | *[!0-9]*) echo none; return 0 ;; esac
    is_running "$pid" || { echo none; return 0; }
    [ -n "$start" ] || { echo unconfirmable; return 0; }
    now="$(start_time "$pid")" || { echo none; return 0; }
    if [ "$now" = "$start" ]; then echo live; else echo recycled; fi
}

supervisor_gone() {
    ! same_process "$SUP_PID" "$SUP_START"
}

probe_is() {
    [ "$(probe "$P" "$F")" = "$1" ]
}

gate_parked() {
    [ -f "$P/$GATE_REL" ] && [ "$(lock_status "$P")" = live ]
}

# SIGKILL the case's supervisor by the pid its sidecar records, after the
# start-time check, and wait until it is gone and its socket reads STALE.
kill_supervisor() {
    signal_recorded KILL "$SUP_PID" "$SUP_START" || return 1
    wait_until "$KILL_WAIT" supervisor_gone || return 1
    wait_until "$STOP_WAIT" probe_is STALE || true
}

# kill9-stale-stop: SIGKILL the supervisor → STALE → `stop` (the backstop) →
# GONE with the orphaned agent group ended.
case_kill9_stale_stop() {
    local name=kill9-stale-stop probe_before members_before killed=true probe_after_kill members_after_kill
    local stop_out stop_rc=0 probe_after members_after launched stopped advance unmet=()
    launch "$name" "$RELEASE_SUCCEEDS" || return 1
    probe_before="$(probe "$P" "$F")"
    members_before="$(members_json "$PGID")"
    kill_supervisor || killed=false
    probe_after_kill="$(probe "$P" "$F")"
    members_after_kill="$(members_json "$PGID")"
    stop_out="$(stop_case)" || stop_rc=$?
    wait_until "$STOP_WAIT" group_empty "$PGID" || true
    sleep "$SETTLE_SECS"
    remember_agents "$AGENTS"
    probe_after="$(probe "$P" "$F")"
    members_after="$(members_json "$PGID")"
    launched="$(event_count "$P" stage_launched)"
    stopped="$(state_field "$P" '.stopped')"
    advance="$(advance_ran "$P")"
    [ "$probe_before" = ALIVE ] || unmet+=("probe before the kill $probe_before")
    [ "$members_before" != '[]' ] || unmet+=("no agent group member before the kill")
    [ "$killed" = true ] || unmet+=("the supervisor was not killed or did not exit")
    [ "$probe_after_kill" = STALE ] || unmet+=("probe after the kill $probe_after_kill")
    [ "$members_after_kill" != '[]' ] || unmet+=("the agent group did not outlive its supervisor")
    [ "$stop_rc" -eq 0 ] || unmet+=("devflow stop exited $stop_rc")
    [ "$probe_after" = GONE ] || unmet+=("probe after stop $probe_after")
    [ "$members_after" = '[]' ] || unmet+=("agent group member survived the backstop")
    [ "$launched" -eq 1 ] || unmet+=("$launched stage_launched events, expected 1")
    [ "$stopped" = true ] || unmet+=("state.stopped is $stopped")
    record_case "$name" "$(jq -n --arg pb "$probe_before" --arg pk "$probe_after_kill" --arg pa "$probe_after" \
        --argjson mb "$members_before" --argjson mk "$members_after_kill" --argjson ma "$members_after" \
        --argjson adv "$advance" --argjson n "$launched" --arg st "$stopped" --argjson rc "$stop_rc" \
        --arg out "$stop_out" --argjson killed "$killed" '{
            probe_before: $pb, supervisor_killed: $killed, probe_after_kill: $pk,
            members_after_kill: $mk, probe_after: $pa, members_before: $mb, members_after: $ma,
            advance_ran: $adv, next_stage_launched: ($n > 1), stage_launched_count: $n,
            state_stopped: ($st == "true"), stop_exit: $rc, stop_output: $out
        }')" "${unmet[@]+"${unmet[@]}"}"
}

# kill9-at-gate: a supervise-mode Define that fails parks at its gate holding
# the per-phase lock; SIGKILL the supervisor → STALE → `stop` → GONE, with no
# gate response written and the lock holder gone or recycled.
case_kill9_at_gate() {
    local name=kill9-at-gate gate_open=false lock_before probe_before members_before killed=true
    local probe_after_kill stop_out stop_rc=0 probe_after members_after lock_after response=false
    local launched advance unmet=()
    launch "$name" "$RELEASE_FAILS_TO_A_GATE" || return 1
    : >"$TRIG"
    wait_until "$GATE_WAIT" gate_parked || true
    [ ! -f "$P/$GATE_REL" ] || gate_open=true
    lock_before="$(lock_status "$P")"
    probe_before="$(probe "$P" "$F")"
    members_before="$(members_json "$PGID")"
    kill_supervisor || killed=false
    probe_after_kill="$(probe "$P" "$F")"
    stop_out="$(stop_case)" || stop_rc=$?
    wait_until "$STOP_WAIT" group_empty "$PGID" || true
    sleep "$SETTLE_SECS"
    remember_agents "$AGENTS"
    probe_after="$(probe "$P" "$F")"
    members_after="$(members_json "$PGID")"
    lock_after="$(lock_status "$P")"
    [ ! -e "$P/$GATE_RESPONSE_REL" ] || response=true
    launched="$(event_count "$P" stage_launched)"
    advance="$(advance_ran "$P")"
    [ "$gate_open" = true ] || unmet+=("the Define gate never opened")
    [ "$lock_before" = live ] || unmet+=("the per-phase lock holder before the kill was $lock_before")
    [ "$killed" = true ] || unmet+=("the supervisor was not killed or did not exit")
    [ "$probe_after_kill" = STALE ] || unmet+=("probe after the kill $probe_after_kill")
    [ "$stop_rc" -eq 0 ] || unmet+=("devflow stop exited $stop_rc")
    [ "$probe_after" = GONE ] || unmet+=("probe after stop $probe_after")
    [ "$members_after" = '[]' ] || unmet+=("agent group member survived")
    case "$lock_after" in none | recycled) ;; *) unmet+=("lock holder after stop is $lock_after") ;; esac
    [ "$response" = false ] || unmet+=("a gate response was written with no live waiter")
    [ "$launched" -eq 1 ] || unmet+=("$launched stage_launched events, expected 1")
    record_case "$name" "$(jq -n --argjson go "$gate_open" --arg lb "$lock_before" --arg pb "$probe_before" \
        --arg pk "$probe_after_kill" --arg pa "$probe_after" --argjson mb "$members_before" \
        --argjson ma "$members_after" --arg la "$lock_after" --argjson resp "$response" \
        --argjson adv "$advance" --argjson n "$launched" --argjson rc "$stop_rc" --arg out "$stop_out" \
        --argjson killed "$killed" '{
            gate_open_before: $go, lock_holder_before: $lb, lock_held_before: ($lb == "live"),
            probe_before: $pb, supervisor_killed: $killed, probe_after_kill: $pk, probe_after: $pa,
            members_before: $mb, members_after: $ma, lock_holder_after: $la,
            gate_response_written: $resp, advance_ran: $adv, next_stage_launched: ($n > 1),
            stage_launched_count: $n, stop_exit: $rc, stop_output: $out
        }')" "${unmet[@]+"${unmet[@]}"}"
}

state_stopped() {
    [ "$(state_field "$P" '.stopped')" = true ]
}

# stop-races-completion: release the agent and run `stop` at once. Either the
# stop or the completion wins; afterwards the state is stopped and no fake
# agent (this stage's or a next stage's) is alive. Whether the advance ran and
# a next stage launched is recorded, not required.
case_stop_races_completion() {
    local name=stop-races-completion probe_before members_before stop_out stop_rc=0 probe_after
    local members_after alive launched advance stopped exit_code unmet=()
    launch "$name" "$RELEASE_SUCCEEDS" || return 1
    probe_before="$(probe "$P" "$F")"
    members_before="$(members_json "$PGID")"
    : >"$TRIG"
    stop_out="$(stop_case)" || stop_rc=$?
    wait_until "$KILL_WAIT" state_stopped || true
    wait_until "$STOP_WAIT" group_empty "$PGID" || true
    sleep "$SETTLE_SECS"
    remember_agents "$AGENTS"
    probe_after="$(probe "$P" "$F")"
    members_after="$(members_json "$PGID")"
    alive="$(alive_agents_json "$AGENTS")"
    launched="$(event_count "$P" stage_launched)"
    advance="$(advance_ran "$P")"
    stopped="$(state_field "$P" '.stopped')"
    exit_code="$(cat -- "$P/$EXIT_REL" 2>/dev/null || echo ABSENT)"
    [ "$probe_before" = ALIVE ] || unmet+=("probe before the race $probe_before")
    [ "$stop_rc" -eq 0 ] || unmet+=("devflow stop exited $stop_rc")
    [ "$stopped" = true ] || unmet+=("state.stopped is $stopped")
    [ "$probe_after" = GONE ] || unmet+=("probe after the race $probe_after")
    [ "$members_after" = '[]' ] || unmet+=("agent group member survived")
    [ "$alive" = '[]' ] || unmet+=("fake agent still alive: $alive")
    record_case "$name" "$(jq -n --arg pb "$probe_before" --arg pa "$probe_after" \
        --argjson mb "$members_before" --argjson ma "$members_after" --argjson alive "$alive" \
        --argjson adv "$advance" --argjson n "$launched" --arg st "$stopped" --argjson rc "$stop_rc" \
        --arg out "$stop_out" --arg exit "$exit_code" '{
            probe_before: $pb, probe_after: $pa, members_before: $mb, members_after: $ma,
            next_agents_alive: $alive, advance_ran: $adv, next_stage_launched: ($n > 1),
            stage_launched_count: $n, state_stopped: ($st == "true"), first_agent_exit: $exit,
            stop_exit: $rc, stop_output: $out
        }')" "${unmet[@]+"${unmet[@]}"}"
}

# publish-unlink-window: not reachable live (R-10 allows fixture coverage);
# the record names the fixture and is judged `ok pointer`, never `ok live`.
record_window_pointer() {
    jq -n --arg sha "$BIN_SHA" --arg fixture "$WINDOW_FIXTURE" '{
        case: "publish-unlink-window", live: false, bin_sha256: $sha, fixture: $fixture,
        reason: "a SIGKILL between the next supervisor publish and the old one unlinking cannot be placed by timing from outside the binary; the fixture reaches the window deterministically"
    }' >"$OUT/case-publish-unlink-window.json"
    echo "fault-arm: case publish-unlink-window: recorded as a pointer to $WINDOW_FIXTURE"
}

run_case() {
    case "$1" in
        alive-stop-stream) case_alive_stop alive-stop-stream stream ;;
        alive-stop-capture) case_alive_stop alive-stop-capture capture --legacy-claude-launch ;;
        kill9-stale-stop) case_kill9_stale_stop ;;
        kill9-at-gate) case_kill9_at_gate ;;
        stop-races-completion) case_stop_races_completion ;;
        publish-unlink-window) record_window_pointer ;;
        *) die "no runner for case $1" ;;
    esac
}

# ---------------------------------------------------------------------------
# cleanup
# ---------------------------------------------------------------------------
CLEANED=0

# Stop every launched run, then signal any recorded process still alive after
# its start-time check, then remove the scratch directory. Prints the
# processes it could not account for.
cleanup() {
    local rc="$1" project pid survivors=()
    set +e
    [ "$CLEANED" -eq 0 ] || return 0
    CLEANED=1
    [ -n "$S" ] || return 0
    for project in "${PROJECTS[@]+"${PROJECTS[@]}"}"; do
        if [ -f "$project/$STATE_REL" ] && [ "$(probe "$project" "$S/f-${project##*/p-}")" != GONE ]; then
            devflow_in "$project" "$S/f-${project##*/p-}" stop --phase "$PHASE" "$project" >/dev/null 2>&1 || true
        fi
        remember_agents "$S/a-${project##*/p-}"
    done
    for pid in "${!KNOWN_START[@]}"; do
        if same_process "$pid" "${KNOWN_START[$pid]}"; then
            signal_recorded KILL "$pid" "${KNOWN_START[$pid]}" || true
        fi
    done
    sleep 0.5
    for pid in "${!KNOWN_START[@]}"; do
        ! same_process "$pid" "${KNOWN_START[$pid]}" || survivors+=("$pid")
    done
    if [ "${#survivors[@]}" -gt 0 ]; then
        echo "fault-arm: recorded processes survived cleanup: ${survivors[*]}" >&2
        rc=1
    fi
    case "$S" in
        /tmp/df50f.??????) rm -rf -- "$S" ;;
        *) echo "fault-arm: refusing to remove unexpected scratch path $S" >&2; rc=1 ;;
    esac
    if [ -e "$S" ]; then
        echo "fault-arm: scratch directory $S was not removed" >&2
        rc=1
    fi
    echo "fault-arm: cleanup recorded_pids=${#KNOWN_START[@]} survivors=${#survivors[@]} scratch_removed=$([ -e "$S" ] && echo no || echo yes)" >&2
    exit "$rc"
}

# ---------------------------------------------------------------------------
# run
# ---------------------------------------------------------------------------
resolve_cases() {
    local list="$1" name known
    if [ "$list" = all ]; then
        printf '%s\n' "${KNOWN_CASES[@]}"
        return 0
    fi
    tr ',' '\n' <<<"$list" | while IFS= read -r name; do
        [ -n "$name" ] || continue
        known=no
        for k in "${KNOWN_CASES[@]}"; do [ "$k" != "$name" ] || known=yes; done
        [ "$known" = yes ] || { echo "fault-arm: unknown case: $name" >&2; return 1; }
        printf '%s\n' "$name"
    done
}

check_binary() {
    local now
    now="$(sha256sum -- "$BIN" | cut -d' ' -f1)"
    [ "$now" = "$BIN_SHA" ] || die "binary changed during the run: $BIN_SHA then $now"
}

run_main() {
    local cases="" list name first=""
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --bin) [ "$#" -ge 2 ] || usage; BIN="$2"; shift ;;
            --out) [ "$#" -ge 2 ] || usage; OUT="$2"; shift ;;
            --cases) [ "$#" -ge 2 ] || usage; cases="$2"; shift ;;
            *) usage ;;
        esac
        shift
    done
    [ -n "$BIN" ] && [ -n "$OUT" ] || usage
    [ -n "$cases" ] || cases=all
    list="$(resolve_cases "$cases")" || exit 1
    [ -n "$list" ] || die "no case selected"
    [ ! -e "$OUT" ] || die "--out already exists: $OUT"
    [ -f "$BIN" ] && [ -x "$BIN" ] || die "--bin is not an executable file: $BIN"
    BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
    mkdir -p "$(dirname "$OUT")"
    mkdir -- "$OUT" || die "could not create --out $OUT"
    OUT="$(cd "$OUT" && pwd)"
    BIN_SHA="$(sha256sum -- "$BIN" | cut -d' ' -f1)"
    printf '%s\n' "$BIN_SHA" >"$OUT/bin_sha256"

    S="$(mktemp -d /tmp/df50f.XXXXXX)"
    trap 'cleanup $?' EXIT
    trap 'exit 130' INT TERM
    mkdir -p "$S/h/.config" "$S/h/.cache" "$S/c"

    while IFS= read -r name; do
        echo "fault-arm: case $name: start"
        if ! run_case "$name"; then
            echo "fault-arm: case $name: could not be run" >&2
            FAILED=1
        fi
        check_binary
        if [ -z "$first" ] && [ -f "$S/p-$name/$EVENTS_REL" ]; then
            first="$(events_of "$S/p-$name" | jq -r 'select(.event == "workflow_started") | .commit' | head -n 1)"
            write_src_record "$OUT" "$first" "$CHECKOUT"
            first="${first:-none}"
        fi
    done <<<"$list"
    [ -f "$OUT/src_tree" ] || write_src_record "$OUT" "" "$CHECKOUT"
    echo "fault-arm: out=$OUT bin_sha256=$BIN_SHA src_matches_checkout=$(cat -- "$OUT/src_matches_checkout")"
    exit "$FAILED"
}

# ---------------------------------------------------------------------------
# --selftest: `  ok   <case>` / `  FAIL <case> — <why>` per case, then
# `SELFTEST passed=N failed=M`; exit 0 only when M is 0 and N > 0. A repeated
# case name is a FAIL.
# ---------------------------------------------------------------------------
ST_TMP=""
ST_PASS=0
ST_FAIL=0
declare -A ST_SEEN=()

st_case() {
    local name="$1" why
    shift
    if [ -n "${ST_SEEN[$name]+set}" ]; then
        echo "  FAIL $name — repeated case name"
        ST_FAIL=$((ST_FAIL + 1))
        return 0
    fi
    ST_SEEN[$name]=1
    if why="$("$@" 2>&1)"; then
        echo "  ok   $name"
        ST_PASS=$((ST_PASS + 1))
    else
        echo "  FAIL $name — $(printf '%s' "$why" | tail -n 4 | tr '\n' ' ')"
        ST_FAIL=$((ST_FAIL + 1))
    fi
}

st_refuses_existing_out() {
    local out="$ST_TMP/existing" err rc=0
    mkdir -p "$out"
    err="$(bash "$SELF" run --bin /bin/true --out "$out" --cases alive-stop-stream 2>&1)" || rc=$?
    [ "$rc" -ne 0 ] || { echo "run exited 0 on an existing --out"; return 1; }
    printf '%s\n' "$err" | rg -q -F -e '--out already exists' || { echo "no refusal text: $err"; return 1; }
    [ -z "$(ls -A "$out")" ] || { echo "the existing --out was written to"; return 1; }
}

st_refuses_unknown_case() {
    local out="$ST_TMP/unknown-case" err rc=0
    err="$(bash "$SELF" run --bin /bin/true --out "$out" --cases no-such-case 2>&1)" || rc=$?
    [ "$rc" -ne 0 ] || { echo "run exited 0 on an unknown case"; return 1; }
    printf '%s\n' "$err" | rg -q -F -e 'unknown case: no-such-case' || { echo "no refusal text: $err"; return 1; }
    [ ! -e "$out" ] || { echo "--out was created before the case list was validated"; return 1; }
}

st_group_members() {
    local pid start
    setsid sleep 30 &
    pid=$!
    wait_until 5 test -n "$(group_pids "$pid")" || { echo "the planted group $pid was never listed"; kill "$pid"; return 1; }
    [ "$(group_pids "$pid")" = "$pid" ] || { echo "group $pid listed $(group_pids "$pid")"; kill "$pid"; return 1; }
    [ "$(members_json "$pid" | jq length)" -eq 1 ] || { echo "members_json did not list one member"; kill "$pid"; return 1; }
    start="$(start_time "$pid")"
    signal_recorded KILL "$pid" "$start" || { echo "signal_recorded refused the recorded process"; kill "$pid"; return 1; }
    wait "$pid" 2>/dev/null || true
    [ -z "$(group_pids "$pid")" ] || { echo "group $pid still listed after it ended"; return 1; }
    [ "$(members_json "$pid")" = '[]' ] || { echo "members_json not empty after the group ended"; return 1; }
}

st_signal_refuses_start_mismatch() {
    local pid start
    sleep 30 &
    pid=$!
    start="$(start_time "$pid")"
    if signal_recorded KILL "$pid" "$((start + 1))"; then
        echo "signal_recorded signalled a pid whose start time did not match"
        return 1
    fi
    is_running "$pid" || { echo "the planted process died"; return 1; }
    signal_recorded KILL "$pid" "$start" || { echo "control: the matching start time was refused"; kill "$pid"; return 1; }
    wait "$pid" 2>/dev/null || true
}

st_git_env() {
    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE -u GIT_COMMON_DIR \
        -u GIT_CONFIG -u GIT_PREFIX -u GIT_CONFIG_GLOBAL \
        HOME="$ST_TMP/home" XDG_CONFIG_HOME="$ST_TMP/home/.config" GIT_CONFIG_NOSYSTEM=1 "$@"
}

st_git() {
    st_git_env git -c user.name=Selftest -c user.email=selftest@localhost \
        -c commit.gpgsign=false -c core.hooksPath=/dev/null "$@"
}

# write_src_record from a fresh shell that sourced this script, under st_git_env.
st_write_src() {
    # shellcheck disable=SC2016 # the positional parameters expand in the child shell
    st_git_env bash -c 'source "$1" --source-only; write_src_record "$2" "$3" "$4"' _ "$SELF" "$@"
}

st_src_record() {
    local repo="$ST_TMP/src-repo" out="$ST_TMP/src-out" commit ids
    mkdir -p "$repo/crates/x" "$out"
    st_git init -q "$repo"
    printf 'fn a() {}\n' >"$repo/crates/x/lib.rs"
    printf '[workspace]\n' >"$repo/Cargo.toml"
    printf '# lock\n' >"$repo/Cargo.lock"
    printf '[toolchain]\n' >"$repo/rust-toolchain.toml"
    st_git -C "$repo" add -A
    st_git -C "$repo" commit -q -m first
    commit="$(st_git -C "$repo" rev-parse HEAD)"
    ids="$(st_git -C "$repo" rev-parse HEAD:crates HEAD:Cargo.toml HEAD:Cargo.lock HEAD:rust-toolchain.toml)"
    st_write_src "$out" "$commit" "$repo"
    [ "$(cat "$out/src_tree")" = "$ids" ] || { echo "src_tree is not the four ids"; return 1; }
    [ "$(wc -l <"$out/src_tree")" -eq 4 ] || { echo "src_tree is not four lines"; return 1; }
    [ "$(cat "$out/src_matches_checkout")" = true ] || { echo "a clean matching checkout read false"; return 1; }
    printf '// dirty\n' >>"$repo/crates/x/lib.rs"
    st_write_src "$out" "$commit" "$repo"
    [ "$(cat "$out/src_matches_checkout")" = false ] || { echo "a dirty checkout read true"; return 1; }
    st_write_src "$out" "$(printf 'd%.0s' {1..40})" "$repo"
    [ "$(cat "$out/src_tree")" = unknown ] || { echo "an unresolvable commit did not give unknown"; return 1; }
    [ "$(cat "$out/src_matches_checkout")" = false ] || { echo "an unknown src_tree read true"; return 1; }
    st_write_src "$out" "" "$repo"
    [ "$(cat "$out/src_tree")" = unknown ] || { echo "an empty commit did not give unknown"; return 1; }
}

st_probe_word() {
    local got
    got="$(printf '  supervisor: /tmp/x.sock — ALIVE (owner: agent_running, launch: stream)\n' | probe_word)"
    [ "$got" = ALIVE ] || { echo "ALIVE read as $got"; return 1; }
    got="$(printf 'phase 1:\n  supervisor: /tmp/x.sock — STALE — run devflow stop\n' | probe_word)"
    [ "$got" = STALE ] || { echo "STALE read as $got"; return 1; }
    got="$(printf '  supervisor: /tmp/x.sock — GONE — the supervisor has exited\n' | probe_word)"
    [ "$got" = GONE ] || { echo "GONE read as $got"; return 1; }
    got="$(printf 'project_root: /tmp/p\nno active phases\n' | probe_word)"
    [ "$got" = ABSENT ] || { echo "no supervisor line read as $got"; return 1; }
}

run_selftest() {
    ST_TMP="$(mktemp -d)"
    trap 'rm -rf -- "$ST_TMP"' EXIT
    mkdir -p "$ST_TMP/home/.config"

    st_case 'run refuses an existing --out directory' st_refuses_existing_out
    st_case 'run refuses an unknown case name' st_refuses_unknown_case
    st_case 'group members lists a planted group and nothing once it ends' st_group_members
    st_case 'signal refuses a pid whose start time does not match' st_signal_refuses_start_mismatch
    st_case 'src record holds the four ids of a resolving commit and unknown otherwise' st_src_record
    st_case 'probe reads each liveness word from status text' st_probe_word

    echo "SELFTEST passed=$ST_PASS failed=$ST_FAIL"
    [ "$ST_FAIL" -eq 0 ] && [ "$ST_PASS" -gt 0 ]
}

main() {
    [ "$#" -gt 0 ] || usage
    case "$1" in
        run) shift; run_main "$@" ;;
        --selftest) [ "$#" -eq 1 ] || usage; run_selftest ;;
        *) usage ;;
    esac
}

# `--source-only` loads the functions without running anything (selftest).
[ "${1:-}" = --source-only ] || main "$@"
