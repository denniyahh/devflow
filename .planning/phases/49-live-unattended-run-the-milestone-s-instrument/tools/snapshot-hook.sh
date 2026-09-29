#!/usr/bin/env bash
#
# tools/snapshot-hook.sh — Phase 49 gate-open snapshot hook (the
# DEVFLOW_GATE_NOTIFY_CMD target).
#
# ============================================================================
# WHAT THIS ESTABLISHES
# ============================================================================
#
# That every gate DevFlow opens for the payload phase (phase 1) leaves a copy of
# the state around it on disk, including the `--yes-ship` Ship gate, which
# DevFlow answers immediately after this hook returns, so no poller sees it
# open. DevFlow runs the notify command synchronously through `sh -c` with the
# gate metadata in the environment (`crates/devflow-core/src/gates.rs:388-424`),
# after it saved the state, wrote the gate file and emitted `gate_fired`
# (`crates/devflow-cli/src/pipeline_gate.rs:360-425`). Each snapshot is
#
#   <root>/gates/<seq>-<stage>/{devflow/,git.txt,env.txt,context.txt,MANIFEST.sha256}
#
# plus one `jq -c` line in <root>/ledger.jsonl naming that directory and the
# manifest's sha256, appended under `flock -w 30 <root>/ledger.lock`. The
# layout is defined once, in EVIDENCE-LAYOUT.md next to this file.
#
# ============================================================================
# WHAT THIS DOES *NOT* ESTABLISH
# ============================================================================
#
#   * Nothing about turns that open no gate. The DECN-03 auto-decide resume
#     opens none (R-7); `collect-attempt.sh` gathers that turn after the run.
#   * That a gate fired at all. A gate DevFlow never opened leaves no snapshot,
#     and a hook that failed leaves a line in <root>/hook-errors.log instead.
#     DevFlow only `warn!`s on a failing hook, so that log is the only record.
#   * Protection against a writer with access to <root>. The manifest and the
#     ledger hash expose accidental change after capture, not a forger.
#
# ============================================================================
# SAFETY
# ============================================================================
#
# DEVFLOW_GATE_CONTEXT is agent-influenced text. It is written with
# `printf '%s'` and nothing else: never evaluated, never interpolated into a
# command string. Only phase `1` is snapshotted. In hook mode the script always
# exits 0; run it as `timeout 60 …` inside the notify command.
#
# Usage:
#   snapshot-hook.sh [--no-notify] <launch_dir> <snapshot_root>
#   snapshot-hook.sh --selftest
#
set -euo pipefail

HOOK_SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"

# ---------------------------------------------------------------------------
# Fixture builders. `collect-attempt.sh --selftest` sources this file for them,
# so they come before the sourced-file return below and use no other helper.
# ---------------------------------------------------------------------------

# Unset every repository-local git variable (GIT_DIR, GIT_WORK_TREE, …) so a
# git call can only see the repository its -C path names. Git owns the list.
p49_unset_git_local_env() {
    local vars
    vars="$(git rev-parse --local-env-vars 2>/dev/null)" || return 0
    [ -n "$vars" ] || return 0
    local -a names
    mapfile -t names <<< "$vars"
    unset "${names[@]}"
}

# p49_fixture_launch <launch_dir> <empty_hooks_dir>
# A launch checkout shaped like the live run's: `main`, `integration` one commit
# ahead of it and checked out, and the linked worktree `.worktrees/phase-01` on
# `feature/phase-01` one commit ahead of `integration`. Hermetic: fixture-local
# hooks path, no signing, a local identity (callers export
# GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1).
p49_fixture_launch() {
    local launch="$1" hooks="$2"
    mkdir -p "$launch/.planning" "$hooks"
    git -C "$launch" init -q -b main .
    git -C "$launch" config core.hooksPath "$hooks"
    git -C "$launch" config commit.gpgsign false
    git -C "$launch" config tag.gpgsign false
    git -C "$launch" config user.email t@example.com
    git -C "$launch" config user.name Tester
    printf '{"workflow":{}}\n' > "$launch/.planning/config.json"
    printf 'fixture\n' > "$launch/README.md"
    git -C "$launch" add README.md .planning/config.json
    git -C "$launch" commit -q -m "chore: fixture root"
    git -C "$launch" checkout -q -b integration
    printf 'base_branch = "integration"\n' > "$launch/devflow.toml"
    git -C "$launch" add devflow.toml
    git -C "$launch" commit -q -m "chore: integration base"
    git -C "$launch" worktree add -q -b feature/phase-01 "$launch/.worktrees/phase-01" integration
    printf 'payload work\n' > "$launch/.worktrees/phase-01/feature.txt"
    git -C "$launch/.worktrees/phase-01" add feature.txt
    git -C "$launch/.worktrees/phase-01" commit -q -m "feat: payload work"
}

# p49_fixture_gate <launch_dir> <stage>
# Plants what DevFlow has written when it runs the notify hook: the saved state,
# the gate file and a `gate_fired` event.
p49_fixture_gate() {
    local launch="$1" stage="$2"
    mkdir -p "$launch/.devflow/gates"
    jq -c -n --arg stage "$stage" \
        '{phase: 1, stage: $stage, base_branch: "integration", gate_pending: true}' \
        > "$launch/.devflow/state-01.json"
    jq -c -n --arg stage "$stage" '{phase: 1, stage: $stage, context: "fixture", timestamp: 1}' \
        > "$launch/.devflow/gates/01-$stage.json"
    jq -c -n --arg stage "$stage" '{v: 1, ts: 1, phase: 1, event: "gate_fired", stage: $stage}' \
        >> "$launch/.devflow/events.jsonl"
}

if [[ "${BASH_SOURCE[0]}" != "${0}" ]]; then
    return 0
fi

die() {
    echo "ERROR: $*" >&2
    exit 1
}

note() { echo "[snapshot-hook] $*"; }

# ---------------------------------------------------------------------------
# Hook mode.
# ---------------------------------------------------------------------------

hook_main() {
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

# st_run_hook <phase> <stage> <context> <launch> <root>
# Runs the hook the way DevFlow does: `sh -c` with the gate metadata in the
# environment and nothing interpolated into the command string.
st_run_hook() {
    local notify_cmd="bash \"\$0\" --no-notify \"\$1\" \"\$2\""
    env DEVFLOW_GATE_PHASE="$1" DEVFLOW_GATE_STAGE="$2" DEVFLOW_GATE_CONTEXT="$3" \
        DEVFLOW_NON_SILENT_GATE=0 \
        sh -c "$notify_cmd" "$HOOK_SELF" "$4" "$5" \
        >> "$ST_TMP/hook.out" 2>&1
}

st_kv() { sed -n "s/^$2=//p" "$1" | head -1; }

st_lines() {
    if [ -f "$1" ]; then
        wc -l < "$1"
    else
        echo 0
    fi
}

st_define_snapshot() {
    local d="$ST_ROOT/gates/001-define" ledger="$ST_ROOT/ledger.jsonl" f want got
    [ "$ST_RC" -eq 0 ] || { ST_WHY="hook exited $ST_RC"; return 1; }
    [ -d "$d" ] || { ST_WHY="no snapshot directory $d"; return 1; }
    [ -s "$d/MANIFEST.sha256" ] || { ST_WHY="no MANIFEST.sha256"; return 1; }
    (cd "$d" && sha256sum --quiet -c MANIFEST.sha256) > /dev/null 2>&1 \
        || { ST_WHY="MANIFEST.sha256 does not verify"; return 1; }
    for f in state-01.json gates/01-define.json events.jsonl; do
        cmp -s "$ST_LAUNCH/.devflow/$f" "$d/devflow/$f" || { ST_WHY="devflow/$f not copied"; return 1; }
        grep -qF "devflow/$f" "$d/MANIFEST.sha256" || { ST_WHY="devflow/$f not in the manifest"; return 1; }
    done
    [ "$(st_lines "$ledger")" -eq 1 ] || { ST_WHY="ledger has $(st_lines "$ledger") lines, want 1"; return 1; }
    got="$(jq -r '.dir' "$ledger")"
    [ "$got" = "$d" ] || { ST_WHY="ledger dir '$got' is not '$d'"; return 1; }
    want="$(sha256sum "$d/MANIFEST.sha256" | cut -d' ' -f1)"
    got="$(jq -r '.manifest_sha256' "$ledger")"
    [ "$got" = "$want" ] || { ST_WHY="ledger manifest_sha256 '$got' is not '$want'"; return 1; }
    got="$(jq -r '[.seq, .stage, .phase, .non_silent] | map(tostring) | join(" ")' "$ledger")"
    [ "$got" = "1 define 1 0" ] || { ST_WHY="ledger fields '$got'"; return 1; }
}

st_git_txt() {
    local g="$ST_ROOT/gates/001-define/git.txt" wt="$ST_LAUNCH/.worktrees/phase-01" want got
    [ -f "$g" ] || { ST_WHY="no git.txt"; return 1; }
    got="$(st_kv "$g" base_branch)"
    [ "$got" = "integration" ] || { ST_WHY="base_branch '$got'"; return 1; }
    want="$(git -C "$wt" rev-parse HEAD)"
    got="$(st_kv "$g" worktree_head)"
    [ "$got" = "$want" ] || { ST_WHY="worktree_head '$got' is not the feature commit '$want'"; return 1; }
    got="$(st_kv "$g" feature_head)"
    [ "$got" = "$want" ] || { ST_WHY="feature_head '$got' is not '$want'"; return 1; }
    want="$(git -C "$ST_LAUNCH" rev-parse refs/heads/integration)"
    got="$(st_kv "$g" merge_base)"
    [ "$got" = "$want" ] || { ST_WHY="merge_base '$got' is not integration's head '$want'"; return 1; }
    [ "$(st_kv "$g" base_head)" = "$want" ] || { ST_WHY="base_head is not integration's head"; return 1; }
    [ "$(st_kv "$g" worktree_head)" != "$(st_kv "$g" merge_base)" ] \
        || { ST_WHY="fixture has no feature commit, so the merge base cannot discriminate"; return 1; }
    got="$(st_kv "$g" main_head)"
    [ "$got" = "$(git -C "$ST_LAUNCH" rev-parse refs/heads/main)" ] || { ST_WHY="main_head '$got'"; return 1; }
    got="$(st_kv "$g" config_json_status)"
    [ "$got" = "clean" ] || { ST_WHY="config_json_status '$got'"; return 1; }
    got="$(st_kv "$g" worktree_status_lines)"
    [ "$got" = "0" ] || { ST_WHY="worktree_status_lines '$got'"; return 1; }
}

st_second_gate() {
    local d="$ST_ROOT/gates/002-code" ledger="$ST_ROOT/ledger.jsonl" got
    [ "$ST_RC" -eq 0 ] || { ST_WHY="hook exited $ST_RC"; return 1; }
    [ -d "$d" ] || { ST_WHY="no snapshot directory $d"; return 1; }
    (cd "$d" && sha256sum --quiet -c MANIFEST.sha256) > /dev/null 2>&1 \
        || { ST_WHY="MANIFEST.sha256 does not verify"; return 1; }
    [ "$(st_lines "$ledger")" -eq 2 ] || { ST_WHY="ledger has $(st_lines "$ledger") lines, want 2"; return 1; }
    got="$(sed -n 2p "$ledger" | jq -r '"\(.seq) \(.stage) \(.dir)"')"
    [ "$got" = "2 code $d" ] || { ST_WHY="second ledger line reads '$got'"; return 1; }
}

st_foreign_phase() {
    local ledger="$ST_ROOT/ledger.jsonl" log="$ST_ROOT/hook-errors.log"
    local gates_before lines_before gates_after lines_after rc=0 hits
    gates_before="$(find "$ST_ROOT/gates" -mindepth 1 -maxdepth 1 2> /dev/null | wc -l)"
    lines_before="$(st_lines "$ledger")"
    st_run_hook 2 code "fixture" "$ST_LAUNCH" "$ST_ROOT" || rc=$?
    [ "$rc" -eq 0 ] || { ST_WHY="hook exited $rc"; return 1; }
    gates_after="$(find "$ST_ROOT/gates" -mindepth 1 -maxdepth 1 2> /dev/null | wc -l)"
    lines_after="$(st_lines "$ledger")"
    [ "$gates_after" -eq "$gates_before" ] || { ST_WHY="gates/ went from $gates_before to $gates_after entries"; return 1; }
    [ "$lines_after" -eq "$lines_before" ] || { ST_WHY="ledger went from $lines_before to $lines_after lines"; return 1; }
    hits="$(grep -cF 'phase 2' "$log" 2> /dev/null || true)"
    [ "${hits:-0}" -ge 1 ] || { ST_WHY="hook-errors.log has no line naming phase 2"; return 1; }
}

# st_make_context <var> <marker_dir>: a gate context in which a dollar-paren
# substitution, a backtick substitution and a `;` command would each create a
# marker file if a shell ran it. Built at runtime; ends in two newlines.
st_make_context() {
    local dollar backtick
    dollar="$(printf '\044')"
    backtick="$(printf '\140')"
    printf -v "$1" '%s(touch %s/dollar) %stouch %s/backtick%s; touch %s/semicolon\n\n' \
        "$dollar" "$2" "$backtick" "$2" "$backtick" "$2"
}

st_context_inert() {
    local ctrl="$ST_TMP/markers-control" live="$ST_TMP/markers-hook" ctx ledger="$ST_ROOT/ledger.jsonl"
    local rc=0 m d
    mkdir -p "$ctrl" "$live"
    # Control: the same payload handed to a shell creates all three markers, so
    # their absence below is the hook declining to run it, not a dud payload.
    st_make_context ctx "$ctrl"
    sh -c ": $ctx" > /dev/null 2>&1 || true
    for m in dollar backtick semicolon; do
        [ -e "$ctrl/$m" ] || { ST_WHY="control: the payload did not create $m when run, so this case cannot discriminate"; return 1; }
    done
    st_make_context ctx "$live"
    printf '%s' "$ctx" > "$ST_TMP/context.expected"
    st_run_hook 1 define "$ctx" "$ST_LAUNCH" "$ST_ROOT" || rc=$?
    [ "$rc" -eq 0 ] || { ST_WHY="hook exited $rc"; return 1; }
    for m in dollar backtick semicolon; do
        [ ! -e "$live/$m" ] || { ST_WHY="marker $m exists: the context was executed"; return 1; }
    done
    d="$(tail -1 "$ledger" 2> /dev/null | jq -r '.dir' 2> /dev/null || true)"
    if [ -z "$d" ] || [ ! -f "$d/context.txt" ]; then
        ST_WHY="no snapshot with context.txt for this gate"
        return 1
    fi
    cmp -s "$ST_TMP/context.expected" "$d/context.txt" || { ST_WHY="context.txt is not byte-equal to the context"; return 1; }
}

st_missing_launch() {
    local root="$ST_TMP/ev-missing" launch="$ST_TMP/no-such-launch" rc=0
    st_run_hook 1 define "fixture" "$launch" "$root" || rc=$?
    [ "$rc" -eq 0 ] || { ST_WHY="hook exited $rc"; return 1; }
    [ -f "$root/hook-errors.log" ] || { ST_WHY="no hook-errors.log under $root"; return 1; }
    grep -qF "$launch" "$root/hook-errors.log" || { ST_WHY="hook-errors.log does not name $launch"; return 1; }
    [ ! -f "$root/ledger.jsonl" ] || { ST_WHY="a ledger was written for a missing launch dir"; return 1; }
}

run_selftest() {
    p49_unset_git_local_env
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
    ST_TMP="$(mktemp -d)"
    trap 'rm -rf "$ST_TMP"' EXIT
    ST_PASS=0
    ST_FAIL=0
    ST_LAUNCH="$ST_TMP/launch"
    ST_ROOT="$ST_TMP/ev"

    p49_fixture_launch "$ST_LAUNCH" "$ST_TMP/no-hooks"
    p49_fixture_gate "$ST_LAUNCH" define
    ST_RC=0
    st_run_hook 1 define "fixture context" "$ST_LAUNCH" "$ST_ROOT" || ST_RC=$?
    st_case "snapshot: define gate copied with manifest and ledger line" st_define_snapshot
    st_case "snapshot: git.txt records worktree head and merge base" st_git_txt

    p49_fixture_gate "$ST_LAUNCH" code
    ST_RC=0
    st_run_hook 1 code "fixture context" "$ST_LAUNCH" "$ST_ROOT" || ST_RC=$?
    st_case "snapshot: second gate gets seq 002 in ledger order" st_second_gate

    st_case "control: foreign phase refused and nothing copied" st_foreign_phase
    st_case "control: context is never executed" st_context_inert
    st_case "control: missing launch dir exits 0 and logs" st_missing_launch

    printf 'SELFTEST passed=%d failed=%d\n' "$ST_PASS" "$ST_FAIL"
    [ "$ST_FAIL" -eq 0 ] && [ "$ST_PASS" -gt 0 ]
}

case "${1:-}" in
    --selftest)
        run_selftest
        ;;
    *)
        hook_main "$@"
        exit 0
        ;;
esac
