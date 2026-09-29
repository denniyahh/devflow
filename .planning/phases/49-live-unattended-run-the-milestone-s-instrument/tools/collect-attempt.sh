#!/usr/bin/env bash
#
# tools/collect-attempt.sh — Phase 49 post-run collector.
#
# ============================================================================
# WHAT THIS ESTABLISHES
# ============================================================================
#
# That once an attempt has ended, its evidence is on disk in one place,
# complete or explicitly ABSENT, before anyone judges it. It copies what no gate
# snapshot can see (R-7): the launch checkout's whole `.devflow/` (including the
# DECN-03 resumed turn archived under `history/phase-01/`), the Claude session
# directory for the payload worktree, the Ship merge on `integration`, and the
# PR. It writes
#
#   $EV/collected/{devflow/,sessions/,git.txt,pr.json,evidence.json,absent.txt,MANIFEST.sha256,DONE}
#   $EV/verdict.json
#
# as defined in EVIDENCE-LAYOUT.md next to this file. The verdict's `outcome`
# is derived from DevFlow's own record, a `workflow_shipped` event for phase 1
# in the collected events; no argument can set it.
#
# ============================================================================
# WHAT THIS DOES *NOT* ESTABLISH
# ============================================================================
#
#   * That the attempt met any criterion. It gathers; `check-evidence.sh`
#     judges.
#   * That an ABSENT item never existed. A capture evicted by retention, a
#     session directory under a path spelling this tool did not try, or a
#     `gh` failure all read as ABSENT; each is named in absent.txt.
#   * Anything about the run while it was live. It refuses while
#     `state-01.json` exists unless told `--allow-state`.
#
# ============================================================================
# SAFETY
# ============================================================================
#
# Read-only toward the project and toward GitHub: it copies out of the launch
# checkout and its only GitHub call is `gh pr view`. It never overwrites a
# finished collection: a rerun over one prints `already collected` and exits 0.
#
# Usage:
#   collect-attempt.sh --attempt N [--home DIR] [--stop-note TEXT] [--allow-state]
#                      [--claude-projects DIR] [--fake-cmds DIR]
#   collect-attempt.sh --selftest
#
set -euo pipefail

COLLECT_SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
TOOLS_DIR="$(dirname "$COLLECT_SELF")"

die() {
    echo "ERROR: $*" >&2
    exit 1
}

note() { echo "[collect-attempt] $*"; }

usage() {
    sed -n '/^# Usage:/,/^set -euo/p' "$COLLECT_SELF" | sed '$d; s/^# \{0,1\}//'
}

# ---------------------------------------------------------------------------
# Collection.
# ---------------------------------------------------------------------------

collect_main() {
    return 0
}

# ---------------------------------------------------------------------------
# Selftest.
# ---------------------------------------------------------------------------

st_ok() {
    printf '  ok   %s\n' "$1"
    ST_PASS=$((ST_PASS + 1))
}

st_fail() {
    printf '  FAIL %s — %s\n' "$1" "$2"
    ST_FAIL=$((ST_FAIL + 1))
}

# st_case <name> <function>: a case function returns non-zero with ST_WHY set.
st_case() {
    local name="$1"
    shift
    ST_WHY="no reason recorded"
    if "$@"; then
        st_ok "$name"
    else
        st_fail "$name" "$ST_WHY"
    fi
}

st_kv() { sed -n "s/^$2=//p" "$1" | head -1; }

# The selftest's own spelling of Claude Code's project-directory name: every
# non-alphanumeric character of the path becomes `-`.
st_escaped() { printf '%s' "$1" | sed 's/[^A-Za-z0-9]/-/g'; }

# st_collect <N> [args…]: run the collector; sets ST_CRC and ST_OUT.
st_collect() {
    local n="$1"
    shift
    ST_RUNS=$((ST_RUNS + 1))
    ST_OUT="$ST_TMP/out.$ST_RUNS"
    ST_CRC=0
    bash "$COLLECT_SELF" --attempt "$n" --home "$ST_HOME" --claude-projects "$ST_PROJECTS" \
        --fake-cmds "$ST_FAKE" "$@" > "$ST_OUT" 2>&1 || ST_CRC=$?
}

# st_event <launch> <event> [jq object of extra fields]
st_event() {
    local extra="${3:-}"
    if [ -z "$extra" ]; then
        extra='{}'
    fi
    jq -c -n --arg event "$2" --argjson extra "$extra" \
        '{v: 1, ts: 1, phase: 1, event: $event} + $extra' >> "$1/.devflow/events.jsonl"
}

# st_attempt <N>: $ST_HOME/attempt-N with a launch fixture that opened the
# define, code and ship gates, attempt.json and prelaunch/fork-base.txt.
st_attempt() {
    local n="$1" ev="$ST_HOME/attempt-$1"
    mkdir -p "$ev/prelaunch"
    p49_fixture_launch "$ev/project" "$ST_TMP/no-hooks"
    printf 'base_sha=%s\nmain_sha=%s\n' \
        "$(git -C "$ev/project" rev-parse refs/heads/integration)" \
        "$(git -C "$ev/project" rev-parse refs/heads/main)" > "$ev/prelaunch/fork-base.txt"
    jq -n --argjson attempt "$n" --arg bin "$ST_FAKE_BIN" --arg launch "$ev/project" \
        '{attempt: $attempt, bin: $bin, bin_sha256: "fixture", build_commit: "fixture",
          launch: $launch, base_branch: "integration", created_at: 1}' > "$ev/attempt.json"
    p49_fixture_gate "$ev/project" define
    p49_fixture_gate "$ev/project" code
    p49_fixture_gate "$ev/project" ship
}

# st_terminal_batch <launch> <merge|no-merge>: DevFlow's terminal batch in its
# order (hooks.rs:115-122): Merge (checkout integration, merge --no-ff),
# VersionBump (tag), ChangelogAppend (commit CHANGELOG.md alone), BranchCleanup
# (`branch -d`, which fails for a branch checked out in a worktree and only warns).
st_terminal_batch() {
    local launch="$1" mode="$2"
    git -C "$launch" checkout -q integration
    if [ "$mode" = merge ]; then
        git -C "$launch" merge -q --no-ff --no-edit feature/phase-01
        st_event "$launch" merge_result '{"merged":true,"branch":"feature/phase-01"}'
    fi
    git -C "$launch" tag v0.1.0
    printf '# Changelog\n\n## 0.1.0\n' > "$launch/CHANGELOG.md"
    git -C "$launch" add CHANGELOG.md
    git -C "$launch" commit -q -m "docs: add changelog entry for 0.1.0" -- CHANGELOG.md
    git -C "$launch" branch -d feature/phase-01 > /dev/null 2>&1 || true
}

# st_shipped <launch>: finish_workflow's order — clear the state, then emit.
st_shipped() {
    rm -f "$1/.devflow/state-01.json"
    st_event "$1" workflow_shipped '{"stage":"ship"}'
    st_event "$1" workflow_finished
}

st_fakes() {
    mkdir -p "$ST_FAKE"
    cat > "$ST_FAKE/gh" << 'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$(dirname "$0")/gh.log"
if [ "${1:-}" = pr ] && [ "${2:-}" = view ]; then
    printf '{"baseRefName":"main","headRefName":"feature/phase-01","state":"OPEN","url":"x","number":1}\n'
    exit 0
fi
exit 1
EOF
    cat > "$ST_FAKE_BIN" << 'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = evidence ]; then
    printf '{"phase":1,"shipped":true}\n'
    exit 0
fi
exit 2
EOF
    chmod +x "$ST_FAKE/gh" "$ST_FAKE_BIN"
}

st_c1_shipped() {
    local ev="$ST_HOME/attempt-1" c="$ST_HOME/attempt-1/collected" got
    [ "$A1_RC" -eq 0 ] || { ST_WHY="collector exited $A1_RC: $(tail -1 "$A1_OUT")"; return 1; }
    [ -f "$c/DONE" ] || { ST_WHY="no collected/DONE"; return 1; }
    diff -r "$ev/project/.devflow" "$c/devflow" > /dev/null 2>&1 \
        || { ST_WHY="collected/devflow differs from the source .devflow"; return 1; }
    (cd "$c" && sha256sum --quiet -c MANIFEST.sha256) > /dev/null 2>&1 \
        || { ST_WHY="collected/MANIFEST.sha256 does not verify"; return 1; }
    got="$(jq -r '[.attempt, .outcome, .workflow_shipped, .stop_note, .state_present]
        | map(tostring) | join(" ")' "$ev/verdict.json" 2> /dev/null || echo ABSENT)"
    [ "$got" = "1 shipped true null false" ] || { ST_WHY="verdict reads '$got'"; return 1; }
    got="$(jq -r '.baseRefName' "$c/pr.json" 2> /dev/null || echo ABSENT)"
    [ "$got" = "main" ] || { ST_WHY="pr.json baseRefName '$got'"; return 1; }
    jq -e '.phase == 1' "$c/evidence.json" > /dev/null 2>&1 || { ST_WHY="evidence.json not collected"; return 1; }
    [ "$(grep -c '' "$ST_FAKE/gh.log" 2> /dev/null || echo 0)" -ge 1 ] \
        || { ST_WHY="gh was never called"; return 1; }
    got="$(grep -vc '^pr view ' "$ST_FAKE/gh.log" || true)"
    [ "${got:-0}" -eq 0 ] || { ST_WHY="gh was called with something other than pr view"; return 1; }
}

st_c2_history() {
    local c="$ST_HOME/attempt-1/collected" src="$ST_HOME/attempt-1/project/.devflow/history/phase-01"
    [ -f "$c/devflow/history/phase-01/$STAMP_KEPT-stdout" ] \
        || { ST_WHY="history/phase-01/$STAMP_KEPT-stdout not collected"; return 1; }
    cmp -s "$src/$STAMP_KEPT-stdout" "$c/devflow/history/phase-01/$STAMP_KEPT-stdout" \
        || { ST_WHY="collected capture differs from the source"; return 1; }
    [ -f "$c/absent.txt" ] || { ST_WHY="no absent.txt"; return 1; }
    ! grep -qF "$STAMP_KEPT" "$c/absent.txt" || { ST_WHY="a present capture is reported absent"; return 1; }
}

st_c3_evicted() {
    local a="$ST_HOME/attempt-2/collected/absent.txt"
    [ -f "$a" ] || { ST_WHY="no absent.txt for attempt 2"; return 1; }
    grep -F "$STAMP_EVICTED" "$a" | grep -qF ABSENT \
        || { ST_WHY="absent.txt has no ABSENT line naming $STAMP_EVICTED"; return 1; }
}

st_c4_sessions() {
    local c1="$ST_HOME/attempt-1/collected" a2="$ST_HOME/attempt-2/collected/absent.txt" got
    [ -f "$c1/sessions/$A1_SESSION/session.jsonl" ] \
        || { ST_WHY="collected/sessions/$A1_SESSION/session.jsonl missing"; return 1; }
    grep -qF "$A2_SESSION" "$a2" 2> /dev/null \
        || { ST_WHY="attempt 2's absent.txt does not name $A2_SESSION"; return 1; }
    # Pinned from a real Claude Code project directory on this host.
    got="$(escape_project_path "/var/home/u/Github/devflow/.worktrees/phase-49" 2> /dev/null || echo ABSENT)"
    [ "$got" = "-var-home-u-Github-devflow--worktrees-phase-49" ] \
        || { ST_WHY="escape_project_path gives '$got'"; return 1; }
    got="$(session_candidates /home/u/p 2> /dev/null | LC_ALL=C sort -u | tr '\n' ' ' || echo ABSENT)"
    case "$got" in
        *"/home/u/p "*"/var/home/u/p "*) ;;
        *)
            ST_WHY="session_candidates /home/u/p gives '$got', want both home spellings"
            return 1
            ;;
    esac
}

st_c5_stop_note_required() {
    local ev="$ST_HOME/attempt-2"
    st_collect 2
    [ "$ST_CRC" -ne 0 ] || { ST_WHY="collector exited 0 for a stopped attempt without a note"; return 1; }
    grep -qF "stop note required" "$ST_OUT" || { ST_WHY="no 'stop note required' in: $(tail -1 "$ST_OUT")"; return 1; }
    [ ! -e "$ev/verdict.json" ] || { ST_WHY="verdict.json was written"; return 1; }
    if [ -e "$ev/collected" ] || [ -e "$ev/collected.tmp" ]; then
        ST_WHY="a collection was left behind"
        return 1
    fi
}

st_c6_stopped_with_note() {
    local ev="$ST_HOME/attempt-2" got want
    st_collect 2 --stop-note "$STOP_NOTE"
    [ "$ST_CRC" -eq 0 ] || { ST_WHY="collector exited $ST_CRC: $(tail -1 "$ST_OUT")"; return 1; }
    got="$(jq -r '[.attempt, .outcome, .workflow_shipped, .state_present] | map(tostring) | join(" ")' \
        "$ev/verdict.json" 2> /dev/null || echo ABSENT)"
    [ "$got" = "2 stopped false false" ] || { ST_WHY="verdict reads '$got'"; return 1; }
    got="$(jq -r '.stop_note' "$ev/verdict.json")"
    want="$STOP_NOTE"
    [ "$got" = "$want" ] || { ST_WHY="stop_note '$got' is not '$want'"; return 1; }
    [ -f "$ev/collected/DONE" ] || { ST_WHY="no collected/DONE"; return 1; }
}

st_c7_shipped_with_note() {
    local ev="$ST_HOME/attempt-3"
    st_collect 3 --stop-note "not allowed here"
    [ "$ST_CRC" -ne 0 ] || { ST_WHY="collector exited 0 for a shipped attempt with a note"; return 1; }
    grep -qF "shipped attempt cannot carry a stop note" "$ST_OUT" \
        || { ST_WHY="refusal text missing: $(tail -1 "$ST_OUT")"; return 1; }
    [ ! -e "$ev/verdict.json" ] || { ST_WHY="verdict.json was written"; return 1; }
    if [ -e "$ev/collected" ] || [ -e "$ev/collected.tmp" ]; then
        ST_WHY="a collection was left behind"
        return 1
    fi
}

st_c8_live_state() {
    local ev="$ST_HOME/attempt-4" got
    [ -f "$ev/project/.devflow/state-01.json" ] || { ST_WHY="fixture has no state file"; return 1; }
    st_collect 4 --stop-note "live fixture"
    [ "$ST_CRC" -ne 0 ] || { ST_WHY="collector exited 0 with state-01.json present"; return 1; }
    grep -qF "run still live or parked" "$ST_OUT" || { ST_WHY="refusal text missing: $(tail -1 "$ST_OUT")"; return 1; }
    if [ -e "$ev/collected" ] || [ -e "$ev/collected.tmp" ]; then
        ST_WHY="wrote under collected/"
        return 1
    fi
    [ ! -e "$ev/verdict.json" ] || { ST_WHY="verdict.json was written"; return 1; }
    st_collect 4 --stop-note "live fixture" --allow-state
    [ "$ST_CRC" -eq 0 ] || { ST_WHY="--allow-state run exited $ST_CRC: $(tail -1 "$ST_OUT")"; return 1; }
    got="$(jq -r '[.outcome, .state_present] | map(tostring) | join(" ")' "$ev/verdict.json" 2> /dev/null || echo ABSENT)"
    [ "$got" = "stopped true" ] || { ST_WHY="--allow-state verdict reads '$got'"; return 1; }
}

st_c9_rerun() {
    local ev="$ST_HOME/attempt-1" before after vbefore vafter
    before="$(sha256sum "$ev/collected/MANIFEST.sha256" 2> /dev/null | cut -d' ' -f1)"
    vbefore="$(sha256sum "$ev/verdict.json" 2> /dev/null | cut -d' ' -f1)"
    [ -n "$before" ] || { ST_WHY="no first collection to rerun over"; return 1; }
    st_collect 1
    [ "$ST_CRC" -eq 0 ] || { ST_WHY="rerun exited $ST_CRC"; return 1; }
    grep -qF "already collected" "$ST_OUT" || { ST_WHY="rerun did not print 'already collected'"; return 1; }
    after="$(sha256sum "$ev/collected/MANIFEST.sha256" | cut -d' ' -f1)"
    vafter="$(sha256sum "$ev/verdict.json" | cut -d' ' -f1)"
    [ "$after" = "$before" ] || { ST_WHY="manifest changed on rerun"; return 1; }
    [ "$vafter" = "$vbefore" ] || { ST_WHY="verdict.json changed on rerun"; return 1; }
}

st_c10_merge() {
    local launch="$ST_HOME/attempt-1/project" g="$ST_HOME/attempt-1/collected/git.txt"
    local feature tip_parents parents merge head
    [ -f "$g" ] || { ST_WHY="no collected/git.txt"; return 1; }
    # The fixture must put the merge behind the tip, or this case cannot tell a
    # history lookup from a read of the tip's parents.
    tip_parents="$(git -C "$launch" log -1 --format=%P refs/heads/integration | wc -w)"
    [ "$tip_parents" -eq 1 ] || { ST_WHY="fixture tip has $tip_parents parents, want the single-parent CHANGELOG commit"; return 1; }
    feature="$(git -C "$launch/.worktrees/phase-01" rev-parse HEAD)"
    [ "$(st_kv "$g" integration_merge_count)" = "1" ] \
        || { ST_WHY="integration_merge_count '$(st_kv "$g" integration_merge_count)'"; return 1; }
    parents="$(st_kv "$g" integration_merge_parents)"
    [ "$(printf '%s\n' "$parents" | wc -w)" -eq 2 ] || { ST_WHY="integration_merge_parents '$parents'"; return 1; }
    [ "$(printf '%s\n' "$parents" | awk '{print $2}')" = "$feature" ] \
        || { ST_WHY="second merge parent is not the feature tip $feature"; return 1; }
    merge="$(st_kv "$g" integration_merge)"
    head="$(st_kv "$g" integration_head)"
    [ "$head" = "$(git -C "$launch" rev-parse refs/heads/integration)" ] || { ST_WHY="integration_head '$head'"; return 1; }
    [ "$merge" != "$head" ] || { ST_WHY="integration_merge equals integration_head"; return 1; }
    case " $(st_kv "$g" tags) " in
        *" v0.1.0 "*) ;;
        *)
            ST_WHY="tags '$(st_kv "$g" tags)' does not name v0.1.0"
            return 1
            ;;
    esac
}

st_c11_no_merge() {
    local c="$ST_HOME/attempt-2/collected" g
    g="$c/git.txt"
    [ -f "$g" ] || { ST_WHY="no collected/git.txt for attempt 2"; return 1; }
    case " $(st_kv "$g" tags) " in
        *" v0.1.0 "*) ;;
        *)
            ST_WHY="control fixture lacks the tag, so ABSENT would not isolate the missing merge"
            return 1
            ;;
    esac
    [ "$(st_kv "$g" integration_merge_count)" = "0" ] \
        || { ST_WHY="integration_merge_count '$(st_kv "$g" integration_merge_count)'"; return 1; }
    [ "$(st_kv "$g" integration_merge)" = "ABSENT" ] || { ST_WHY="integration_merge is not ABSENT"; return 1; }
    [ "$(st_kv "$g" integration_merge_parents)" = "ABSENT" ] \
        || { ST_WHY="integration_merge_parents is not ABSENT"; return 1; }
    grep -qF "integration_merge" "$c/absent.txt" 2> /dev/null \
        || { ST_WHY="absent.txt has no line naming integration_merge"; return 1; }
}

run_selftest() {
    # shellcheck source=/dev/null
    source "$TOOLS_DIR/snapshot-hook.sh"
    p49_unset_git_local_env
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
    ST_TMP="$(mktemp -d)"
    trap 'rm -rf "$ST_TMP"' EXIT
    ST_PASS=0
    ST_FAIL=0
    ST_RUNS=0
    ST_HOME="$ST_TMP/p49"
    ST_PROJECTS="$ST_TMP/claude-projects"
    ST_FAKE="$ST_TMP/fake-cmds"
    ST_FAKE_BIN="$ST_TMP/fake-bin-devflow"
    STAMP_KEPT="1790700000000000001-0"
    STAMP_EVICTED="1790700000000000002-1"
    STOP_NOTE="operator stopped: fixture \"quoted\" note"
    mkdir -p "$ST_PROJECTS"
    st_fakes

    # Attempt 1: shipped, the resumed-turn capture archived, a session
    # directory, and the full terminal batch (merge behind tag and CHANGELOG).
    st_attempt 1
    local l1="$ST_HOME/attempt-1/project"
    mkdir -p "$l1/.devflow/history/phase-01"
    printf 'resumed turn capture\n' > "$l1/.devflow/history/phase-01/$STAMP_KEPT-stdout"
    st_event "$l1" capture_archived "{\"stage\":\"code\",\"to_stage\":\"code\",\"stamp\":\"$STAMP_KEPT\"}"
    st_terminal_batch "$l1" merge
    st_shipped "$l1"
    A1_SESSION="$(st_escaped "$(realpath "$l1/.worktrees/phase-01")")"
    mkdir -p "$ST_PROJECTS/$A1_SESSION"
    printf '{"type":"fixture"}\n' > "$ST_PROJECTS/$A1_SESSION/session.jsonl"

    # Attempt 2: stopped, its capture evicted, no session directory, and the
    # batch without its merge.
    st_attempt 2
    local l2="$ST_HOME/attempt-2/project"
    st_event "$l2" capture_archived "{\"stage\":\"code\",\"to_stage\":\"code\",\"stamp\":\"$STAMP_EVICTED\"}"
    st_terminal_batch "$l2" no-merge
    rm -f "$l2/.devflow/state-01.json"
    A2_SESSION="$(st_escaped "$(realpath "$l2/.worktrees/phase-01")")"

    # Attempt 3: shipped. Attempt 4: still live (state file present).
    st_attempt 3
    st_shipped "$ST_HOME/attempt-3/project"
    st_attempt 4

    st_collect 1
    A1_RC="$ST_CRC"
    A1_OUT="$ST_OUT"

    st_case "collect: shipped attempt copied with verdict shipped" st_c1_shipped
    st_case "collect: resumed-turn capture under history is collected" st_c2_history
    st_case "collect: merge located behind the tag and changelog commits" st_c10_merge
    st_case "collect: rerun on a finished collection changes nothing" st_c9_rerun
    st_case "control: stopped attempt without a stop note refused" st_c5_stop_note_required
    st_case "collect: stopped attempt with a stop note records it" st_c6_stopped_with_note
    st_case "control: retention-evicted capture reported absent" st_c3_evicted
    st_case "control: integration without a merge records ABSENT" st_c11_no_merge
    st_case "collect: session transcripts found under the escaped worktree path" st_c4_sessions
    st_case "control: shipped attempt with a stop note refused" st_c7_shipped_with_note
    st_case "control: live state file refused" st_c8_live_state

    printf 'SELFTEST passed=%d failed=%d\n' "$ST_PASS" "$ST_FAIL"
    [ "$ST_FAIL" -eq 0 ] && [ "$ST_PASS" -gt 0 ]
}

# ---------------------------------------------------------------------------
# Entry.
# ---------------------------------------------------------------------------

case "${1:-}" in
    --selftest)
        run_selftest
        ;;
    -h | --help)
        usage
        ;;
    *)
        collect_main "$@"
        ;;
esac
