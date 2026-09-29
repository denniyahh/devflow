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

# Where DevFlow puts the payload phase's worktree and branch (EVIDENCE-LAYOUT).
PAYLOAD_WORKTREE_REL=".worktrees/phase-01"
PAYLOAD_BRANCH="feature/phase-01"
LEDGER_LOCK_WAIT_SECS=30
NOTIFY_SEND_TIMEOUT_SECS=10

# hook_log <root> <message>: append one timestamped line to
# <root>/hook-errors.log, or to stderr when the root is unusable. Never fails.
hook_log() {
    local root="$1" msg="$2" stamp
    stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ 2> /dev/null || echo unknown-time)"
    if [ -n "$root" ] && [ -d "$root" ] \
        && printf '%s %s\n' "$stamp" "$msg" >> "$root/hook-errors.log" 2> /dev/null; then
        return 0
    fi
    printf 'snapshot-hook: %s %s\n' "$stamp" "$msg" >&2 || true
    return 0
}

# Inside take_snapshot (a `set -e` subshell): report and leave the subshell.
fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

# git_value <cmd…>: the command's output, or the literal ABSENT when it fails or
# prints nothing. An unreadable value is never an empty string or a false.
git_value() {
    local out
    if out="$("$@" 2> /dev/null)" && [ -n "$out" ]; then
        printf '%s\n' "$out"
    else
        printf 'ABSENT\n'
    fi
}

# copy_devflow <launch>/.devflow <dest>: the files the contract names, as they
# are at the instant the gate opened.
copy_devflow() {
    local src="$1" dst="$2" f
    mkdir -p "$dst/gates"
    for f in state-01.json events.jsonl phase-01-stdout; do
        if [ -f "$src/$f" ]; then
            cp -p "$src/$f" "$dst/$f"
        fi
    done
    for f in "$src"/gates/01-*.json; do
        if [ -f "$f" ]; then
            cp -p "$f" "$dst/gates/"
        fi
    done
    if [ -d "$src/history/phase-01" ]; then
        mkdir -p "$dst/history"
        cp -a "$src/history/phase-01" "$dst/history/phase-01"
    fi
}

# write_git_txt <launch> <copied state file>: the eight git.txt keys.
write_git_txt() {
    local launch="$1" state="$2" wt="$1/$PAYLOAD_WORKTREE_REL"
    local base="ABSENT" wt_ok=false status
    if [ -f "$state" ]; then
        base="$(jq -r '(.base_branch // "ABSENT") | tostring' "$state" 2> /dev/null || echo ABSENT)"
    fi
    # A linked worktree has a `.git` file; without this guard `git -C` on a plain
    # directory would silently answer for the launch repository instead.
    if [ -e "$wt/.git" ]; then
        wt_ok=true
    fi
    printf 'base_branch=%s\n' "$(printf '%s' "$base" | tr '\n' ' ')"
    if [ "$base" != "ABSENT" ] && [[ "$base" =~ ^[A-Za-z0-9._/-]+$ ]]; then
        printf 'base_head=%s\n' "$(git_value git -C "$launch" rev-parse --verify -q "refs/heads/$base^{commit}")"
    else
        printf 'base_head=ABSENT\n'
    fi
    printf 'main_head=%s\n' "$(git_value git -C "$launch" rev-parse --verify -q 'refs/heads/main^{commit}')"
    if [ "$wt_ok" = true ]; then
        printf 'worktree_head=%s\n' "$(git_value git -C "$wt" rev-parse --verify -q 'HEAD^{commit}')"
    else
        printf 'worktree_head=ABSENT\n'
    fi
    printf 'feature_head=%s\n' \
        "$(git_value git -C "$launch" rev-parse --verify -q "refs/heads/$PAYLOAD_BRANCH^{commit}")"
    if [ "$base" != "ABSENT" ] && [[ "$base" =~ ^[A-Za-z0-9._/-]+$ ]]; then
        printf 'merge_base=%s\n' \
            "$(git_value git -C "$launch" merge-base "refs/heads/$PAYLOAD_BRANCH" "refs/heads/$base")"
    else
        printf 'merge_base=ABSENT\n'
    fi
    if [ "$wt_ok" = true ] && status="$(git -C "$wt" status --porcelain 2> /dev/null)"; then
        if [ -z "$status" ]; then
            printf 'worktree_status_lines=0\n'
        else
            printf 'worktree_status_lines=%s\n' "$(printf '%s\n' "$status" | wc -l)"
        fi
    else
        printf 'worktree_status_lines=ABSENT\n'
    fi
    if [ "$wt_ok" = true ] && status="$(git -C "$wt" status --porcelain -- .planning/config.json 2> /dev/null)"; then
        if [ -z "$status" ]; then
            printf 'config_json_status=clean\n'
        else
            printf 'config_json_status=%s\n' "$(printf '%s\n' "$status" | head -1)"
        fi
    else
        printf 'config_json_status=ABSENT\n'
    fi
}

# write_manifest <dir>: sha256 of every file under <dir>, relative paths, sorted.
write_manifest() {
    local dir="$1"
    (
        cd "$dir"
        find . -type f ! -path ./MANIFEST.sha256 ! -path ./MANIFEST.sha256.tmp -printf '%P\n' \
            | LC_ALL=C sort | xargs -d '\n' -r sha256sum --
    ) > "$dir/MANIFEST.sha256.tmp"
    mv "$dir/MANIFEST.sha256.tmp" "$dir/MANIFEST.sha256"
}

# take_snapshot <launch> <root>: runs in a `set -e` subshell; prints the seq on
# success, a reason on stderr and a non-zero exit on refusal or failure.
take_snapshot() {
    local launch="$1" root="$2"
    local phase="${DEVFLOW_GATE_PHASE-}" stage="${DEVFLOW_GATE_STAGE-}"
    local non_silent="${DEVFLOW_NON_SILENT_GATE:-ABSENT}"
    [ -d "$launch" ] || fail "launch dir not found: $launch"
    [ "$phase" = "1" ] || fail "refused: gate phase ${phase:-<unset>} is not the payload phase 1; nothing copied"
    [[ "$stage" =~ ^[a-z]+$ ]] || fail "refused: gate stage '$stage' is not a lower-case stage name; nothing copied"
    [ -f "$launch/.devflow/gates/01-$stage.json" ] \
        || fail "refused: no gate file $launch/.devflow/gates/01-$stage.json; nothing copied"
    p49_unset_git_local_env
    launch="$(cd "$launch" && pwd)"
    root="$(cd "$root" && pwd)"
    mkdir -p "$root/gates"

    exec 9>> "$root/ledger.lock"
    flock -w "$LEDGER_LOCK_WAIT_SECS" 9 || fail "could not lock $root/ledger.lock within ${LEDGER_LOCK_WAIT_SECS}s"
    local ledger="$root/ledger.jsonl" count=0 n seq dir tmp manifest_sha line
    if [ -f "$ledger" ]; then
        count="$(wc -l < "$ledger")"
    fi
    n=$((count + 1))
    seq="$(printf '%03d' "$n")"
    dir="$root/gates/$seq-$stage"
    [ ! -e "$dir" ] || fail "snapshot directory already exists without a ledger line: $dir"
    tmp="$root/gates/.tmp-$seq-$stage.$BASHPID"
    rm -rf "$tmp"
    mkdir -p "$tmp"
    copy_devflow "$launch/.devflow" "$tmp/devflow"
    write_git_txt "$launch" "$tmp/devflow/state-01.json" > "$tmp/git.txt"
    printf 'phase=%s\nstage=%s\nnon_silent=%s\n' "$phase" "$stage" "$non_silent" > "$tmp/env.txt"
    # Data only: the context is agent-influenced text and is never run.
    printf '%s' "${DEVFLOW_GATE_CONTEXT-}" > "$tmp/context.txt"
    write_manifest "$tmp"
    mv "$tmp" "$dir"
    manifest_sha="$(sha256sum "$dir/MANIFEST.sha256" | cut -d' ' -f1)"
    line="$(jq -c -n --argjson seq "$n" --arg stage "$stage" --arg phase "$phase" \
        --arg non_silent "$non_silent" --arg dir "$dir" --arg manifest_sha256 "$manifest_sha" \
        --argjson t "$(date +%s)" \
        '{seq: $seq, stage: $stage, phase: $phase, non_silent: $non_silent, dir: $dir,
          manifest_sha256: $manifest_sha256, t: $t}')"
    printf '%s\n' "$line" >> "$ledger"
    printf '%s\n' "$seq"
}

hook_main() {
    local notify=true launch root errf seq rc line logged=false
    if [ "${1:-}" = "--no-notify" ]; then
        notify=false
        shift
    fi
    if [ "$#" -ne 2 ]; then
        hook_log "" "usage: snapshot-hook.sh [--no-notify] <launch_dir> <snapshot_root> (got $# arguments)"
        return 0
    fi
    launch="$1"
    root="$2"
    if ! mkdir -p "$root" 2> /dev/null; then
        hook_log "" "snapshot root unusable: $root"
        return 0
    fi
    if ! errf="$(mktemp 2> /dev/null)"; then
        hook_log "$root" "mktemp failed; no snapshot taken"
        return 0
    fi
    # errexit is re-enabled inside the substitution and must not be inherited
    # as "ignored" from a caller's `if`/`||`, so this runs as a plain command.
    set +e
    seq="$(
        set -e
        take_snapshot "$launch" "$root" 2> "$errf"
    )"
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        while IFS= read -r line; do
            hook_log "$root" "$line"
            logged=true
        done < "$errf"
        if [ "$logged" = false ]; then
            hook_log "$root" "snapshot failed with exit $rc and no message"
        fi
    fi
    rm -f "$errf"
    if [ "$rc" -eq 0 ] && [ "$notify" = true ]; then
        timeout "$NOTIFY_SEND_TIMEOUT_SECS" notify-send "DevFlow gate: phase 1 ${DEVFLOW_GATE_STAGE-}" \
            "snapshot $seq" > /dev/null 2>&1 || true
    fi
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
        # Hook mode never fails DevFlow's gate: whatever happens, exit 0.
        trap 'exit 0' EXIT
        hook_main "$@"
        exit 0
        ;;
esac
