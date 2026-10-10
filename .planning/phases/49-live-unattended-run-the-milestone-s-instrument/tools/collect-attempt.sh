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

# Where DevFlow puts the payload phase's worktree and branch (EVIDENCE-LAYOUT).
PAYLOAD_WORKTREE_REL=".worktrees/phase-01"
PAYLOAD_BRANCH="feature/phase-01"
BASE_BRANCH="integration"
GH_TIMEOUT_SECS=60
EVIDENCE_TIMEOUT_SECS=120

# Unset every repository-local git variable (GIT_DIR, GIT_WORK_TREE, …) so a
# git call can only see the repository its -C path names. Git owns the list.
unset_git_local_env() {
    local vars
    vars="$(git rev-parse --local-env-vars 2> /dev/null)" || return 0
    [ -n "$vars" ] || return 0
    local -a names
    mapfile -t names <<< "$vars"
    unset "${names[@]}"
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

# Claude Code names a project directory after the working directory with every
# non-alphanumeric character replaced by `-`.
escape_project_path() { printf '%s' "$1" | sed 's/[^A-Za-z0-9]/-/g'; }

# session_candidates <path>: the path as given, its realpath, and the other
# home spelling of each (Fedora Atomic exposes both /home and /var/home).
session_candidates() {
    local given="$1" real p
    real="$(realpath -m -- "$given" 2> /dev/null || printf '%s' "$given")"
    for p in "$given" "$real"; do
        printf '%s\n' "$p"
        case "$p" in
            /var/home/*) printf '/home/%s\n' "${p#/var/home/}" ;;
            /home/*) printf '/var/home/%s\n' "${p#/home/}" ;;
        esac
    done | awk '!seen[$0]++'
}

# collect_captures <collected devflow dir> <absent.txt>: every capture a
# `capture_archived` event names must be in the collected history.
collect_captures() {
    local dv="$1" absent="$2" stamp
    if [ ! -f "$dv/events.jsonl" ]; then
        printf 'events.jsonl ABSENT: %s\n' "$dv/events.jsonl" >> "$absent"
        return 0
    fi
    while IFS= read -r stamp; do
        if [ "$stamp" = "ABSENT" ]; then
            printf 'capture_archived stamp ABSENT: an event carries no stamp\n' >> "$absent"
        elif [[ ! "$stamp" =~ ^[A-Za-z0-9._-]+$ ]]; then
            printf 'capture_archived stamp ABSENT: an event carries an unusable stamp\n' >> "$absent"
        elif [ ! -f "$dv/history/phase-01/$stamp-stdout" ]; then
            printf 'capture %s-stdout ABSENT: named by a capture_archived event, not in history/phase-01 (retention eviction?)\n' \
                "$stamp" >> "$absent"
        fi
    done < <(jq -r -R 'fromjson? | select(type == "object" and .event == "capture_archived" and .phase == 1)
        | (.stamp // "ABSENT") | tostring' "$dv/events.jsonl")
}

# collect_sessions <claude projects dir> <worktree path> <dest> <absent.txt>
collect_sessions() {
    local projects="$1" wt="$2" dest="$3" absent="$4" p name tried="" found=false
    while IFS= read -r p; do
        name="$(escape_project_path "$p")"
        tried="$tried $name"
        if [ -d "$projects/$name" ] && [ ! -e "$dest/$name" ]; then
            mkdir -p "$dest"
            cp -a "$projects/$name" "$dest/$name"
            found=true
        fi
    done < <(session_candidates "$wt")
    if [ "$found" = false ]; then
        printf 'sessions ABSENT: no Claude project directory under %s for %s (tried:%s)\n' \
            "$projects" "$wt" "$tried" >> "$absent"
    fi
}

# write_collected_git <launch> <fork-base.txt> <absent.txt>: git.txt on stdout.
# R-2: the Ship merge is located in `integration`'s first-parent history since
# the fork-point base, never read from the tip. DevFlow's terminal batch
# (hooks.rs:115-122) merges (git.rs:185-190), tags, then commits CHANGELOG.md
# on `integration` (hooks.rs:286), so a correct run's tip has one parent.
write_collected_git() {
    local launch="$1" fork_base="$2" absent="$3"
    local base_sha="ABSENT" merges count merge="ABSENT" parents="ABSENT" feature tags
    printf 'integration_head=%s\n' \
        "$(git_value git -C "$launch" rev-parse --verify -q "refs/heads/$BASE_BRANCH^{commit}")"
    if [ -f "$fork_base" ]; then
        base_sha="$(sed -n 's/^base_sha=//p' "$fork_base" | head -1)"
    fi
    if [[ ! "$base_sha" =~ ^[0-9a-f]{40}$ ]]; then
        printf 'integration_merge_count=ABSENT\n'
        printf 'integration_merge ABSENT: no usable base_sha in %s\n' "$fork_base" >> "$absent"
    elif ! merges="$(git -C "$launch" rev-list --first-parent --merges \
        "$base_sha..refs/heads/$BASE_BRANCH" 2> /dev/null)"; then
        printf 'integration_merge_count=ABSENT\n'
        printf 'integration_merge ABSENT: git rev-list over %s..%s failed\n' "$base_sha" "$BASE_BRANCH" >> "$absent"
    else
        if [ -z "$merges" ]; then
            count=0
        else
            count="$(printf '%s\n' "$merges" | wc -l)"
        fi
        printf 'integration_merge_count=%s\n' "$count"
        if [ "$count" -eq 1 ]; then
            merge="$merges"
            parents="$(git_value git -C "$launch" log -1 --format=%P "$merge")"
        else
            printf 'integration_merge ABSENT: %s first-parent merges on %s since base_sha %s, want exactly 1\n' \
                "$count" "$BASE_BRANCH" "$base_sha" >> "$absent"
        fi
    fi
    printf 'integration_merge=%s\n' "$merge"
    printf 'integration_merge_parents=%s\n' "$parents"
    printf 'main_head=%s\n' "$(git_value git -C "$launch" rev-parse --verify -q 'refs/heads/main^{commit}')"
    feature="$(git_value git -C "$launch" rev-parse --verify -q "refs/heads/$PAYLOAD_BRANCH^{commit}")"
    printf 'feature_head=%s\n' "$feature"
    # Supporting only: BranchCleanup may delete the feature ref (RESEARCH A4).
    if [ "$feature" = "ABSENT" ]; then
        printf 'feature_is_ancestor_of_integration=ABSENT\n'
    else
        local rc=0
        git -C "$launch" merge-base --is-ancestor "refs/heads/$PAYLOAD_BRANCH" "refs/heads/$BASE_BRANCH" \
            2> /dev/null || rc=$?
        case "$rc" in
            0) printf 'feature_is_ancestor_of_integration=yes\n' ;;
            1) printf 'feature_is_ancestor_of_integration=no\n' ;;
            *) printf 'feature_is_ancestor_of_integration=ABSENT\n' ;;
        esac
    fi
    # Space-separated; empty means the repository has no tags.
    if tags="$(git -C "$launch" for-each-ref --format='%(refname:short)' refs/tags 2> /dev/null)"; then
        printf 'tags=%s\n' "$(printf '%s' "$tags" | tr '\n' ' ')"
    else
        printf 'tags=ABSENT\n'
    fi
}

# collect_pr <launch> <dest pr.json> <absent.txt>: the only GitHub call.
collect_pr() {
    local launch="$1" dest="$2" absent="$3" rc=0
    (cd "$launch" && timeout "$GH_TIMEOUT_SECS" gh pr view "$PAYLOAD_BRANCH" \
        --json baseRefName,headRefName,state,url,number) > "$dest.tmp" 2> /dev/null || rc=$?
    if [ "$rc" -eq 0 ] && jq -e 'type == "object"' "$dest.tmp" > /dev/null 2>&1; then
        mv "$dest.tmp" "$dest"
    else
        rm -f "$dest.tmp"
        printf 'pr.json ABSENT: gh pr view %s failed or printed no JSON object (exit %s)\n' \
            "$PAYLOAD_BRANCH" "$rc" >> "$absent"
    fi
}

# collect_evidence <bin> <launch> <dest evidence.json> <absent.txt>
collect_evidence() {
    local bin="$1" launch="$2" dest="$3" absent="$4" rc=0
    timeout "$EVIDENCE_TIMEOUT_SECS" "$bin" evidence --phase 1 --json --root "$launch" \
        > "$dest.tmp" 2> /dev/null || rc=$?
    if [ "$rc" -eq 0 ] && jq -e '.' "$dest.tmp" > /dev/null 2>&1; then
        mv "$dest.tmp" "$dest"
    else
        rm -f "$dest.tmp"
        printf 'evidence.json ABSENT: %s evidence --phase 1 --json failed (exit %s)\n' "$bin" "$rc" >> "$absent"
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

# shipped_in <events.jsonl>: `true` iff DevFlow recorded `workflow_shipped` for
# phase 1 (pipeline_gate.rs, finish_workflow — its only emitter). Unparseable
# lines are skipped, not fatal.
shipped_in() {
    local events="$1"
    if [ -f "$events" ] && jq -e -n -R '[inputs | fromjson? | select(type == "object"
            and .event == "workflow_shipped" and .phase == 1)] | length > 0' "$events" > /dev/null 2>&1; then
        printf 'true\n'
    else
        printf 'false\n'
    fi
}

collect_main() {
    local attempt="" home="" stop_note="" allow_state=false projects="" fake_cmds=""
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --attempt) attempt="${2:-}"; shift ;;
            --home) home="${2:-}"; shift ;;
            --stop-note) stop_note="${2:-}"; shift ;;
            --allow-state) allow_state=true ;;
            --claude-projects) projects="${2:-}"; shift ;;
            --fake-cmds) fake_cmds="${2:-}"; shift ;;
            *) die "unknown argument: $1 (see --help)" ;;
        esac
        shift
    done
    [[ "$attempt" =~ ^[1-9]$ ]] || die "--attempt must be one digit 1-9 (got '${attempt}')"
    local p49="${home:-$HOME/.cache/devflow-phase49}"
    projects="${projects:-$HOME/.claude/projects}"
    if [ -n "$fake_cmds" ]; then
        [ -d "$fake_cmds" ] || die "--fake-cmds is not a directory: $fake_cmds"
        PATH="$fake_cmds:$PATH"
        export PATH
    fi
    unset_git_local_env

    local ev="$p49/attempt-$attempt"
    [ -f "$ev/attempt.json" ] || die "attempt.json ABSENT: $ev/attempt.json"
    local launch bin
    launch="$(jq -r '.launch // empty' "$ev/attempt.json" 2> /dev/null)" || die "attempt.json unreadable: $ev/attempt.json"
    bin="$(jq -r '.bin // empty' "$ev/attempt.json" 2> /dev/null)" || die "attempt.json unreadable: $ev/attempt.json"
    [ -n "$launch" ] || die "attempt.json names no launch: $ev/attempt.json"
    [ -n "$bin" ] || die "attempt.json names no bin: $ev/attempt.json"
    [ -d "$launch" ] || die "launch dir not found: $launch"

    local state_present=false
    if [ -e "$launch/.devflow/state-01.json" ]; then
        [ "$allow_state" = true ] \
            || die "run still live or parked: $launch/.devflow/state-01.json exists (--allow-state collects anyway)"
        state_present=true
    fi
    if [ -f "$ev/collected/DONE" ]; then
        note "already collected: $ev/collected (nothing written)"
        return 0
    fi
    [ ! -e "$ev/collected" ] || die "incomplete collection at $ev/collected (no DONE); move it aside first"

    local tmp="$ev/collected.tmp" absent
    rm -rf "$tmp"
    mkdir -p "$tmp"
    absent="$tmp/absent.txt"
    : > "$absent"
    if [ -d "$launch/.devflow" ]; then
        cp -a "$launch/.devflow" "$tmp/devflow"
    else
        printf 'devflow ABSENT: %s\n' "$launch/.devflow" >> "$absent"
    fi
    collect_captures "$tmp/devflow" "$absent"
    collect_sessions "$projects" "$launch/$PAYLOAD_WORKTREE_REL" "$tmp/sessions" "$absent"
    write_collected_git "$launch" "$ev/prelaunch/fork-base.txt" "$absent" > "$tmp/git.txt"
    collect_pr "$launch" "$tmp/pr.json" "$absent"
    collect_evidence "$bin" "$launch" "$tmp/evidence.json" "$absent"
    write_manifest "$tmp"
    : > "$tmp/DONE"

    # The outcome comes from the collected events alone; the note rules only
    # accept or refuse it.
    local shipped outcome note_json
    shipped="$(shipped_in "$tmp/devflow/events.jsonl")"
    if [ "$shipped" = true ]; then
        outcome=shipped
        if [ -n "$stop_note" ]; then
            rm -rf "$tmp"
            die "shipped attempt cannot carry a stop note (a workflow_shipped event for phase 1 is in the events)"
        fi
        note_json=null
    else
        outcome=stopped
        if [ -z "$stop_note" ]; then
            rm -rf "$tmp"
            die "stop note required: no workflow_shipped event for phase 1 in the events (pass --stop-note TEXT)"
        fi
        note_json="$(jq -n --arg note "$stop_note" '$note')"
    fi
    jq -n --argjson attempt "$attempt" --arg outcome "$outcome" --argjson workflow_shipped "$shipped" \
        --argjson stop_note "$note_json" --argjson state_present "$state_present" \
        '{attempt: $attempt, outcome: $outcome, workflow_shipped: $workflow_shipped,
          stop_note: $stop_note, state_present: $state_present}' > "$ev/verdict.json.tmp"
    mv "$ev/verdict.json.tmp" "$ev/verdict.json"
    mv "$tmp" "$ev/collected"
    note "collected attempt $attempt: outcome=$outcome, $(grep -c '' "$ev/collected/absent.txt" || true) ABSENT item(s)"
    note "verdict: $ev/verdict.json"
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
    grep -qF -e "$A2_SESSION" "$a2" 2> /dev/null \
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
    tip_parents="$(git -C "$launch" cat-file -p refs/heads/integration | grep -c "^parent " || true)"
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
    # Another phase's `workflow_shipped` must not make phase 1 read as shipped.
    st_event "$l2" workflow_shipped '{"phase":2,"stage":"ship"}'
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
