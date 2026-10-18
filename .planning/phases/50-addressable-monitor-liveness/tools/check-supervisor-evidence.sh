#!/usr/bin/env bash
# Judge Phase 50 supervisor evidence (D-16, D-17, R-10).
#
# Every judging mode prints what it found and one verdict line, and exits 0
# exactly when that verdict passes and 1 otherwise (R6-G6): a gate reads only
# the exit code and never re-reads the evidence this tool judged. A usage
# error exits 2.
#
# Usage:
#   check-supervisor-evidence.sh --fault <fault dir> [--require-all]
#   check-supervisor-evidence.sh --src-check <fault dir> [--repo <checkout>]
#   check-supervisor-evidence.sh --selftest
#
# --fault judges the per-case records `fault-arm.sh run` wrote. Each record
# prints exactly one of:
#   FAULT case=<name> ok live       a live record whose expectation was met
#   FAULT case=<name> ok pointer    a `live: false` record naming its fixture test
#   FAULT case=<name> FAIL <reason>
# then `FAULT bin_sha256=<hex|ABSENT|MALFORMED>` and `FAULT verdict=pass|fail`.
# A pointer is never printed or counted as a live case (RV-09). The checker
# re-derives each live case's expectation from the recorded fields; a record's
# own `expected_met` is necessary, never sufficient.
#
# --src-check (R5-W1): in <checkout> (default: the checkout holding this tool)
# take the `HEAD:` object ids of crates, Cargo.toml, Cargo.lock and
# rust-toolchain.toml and whether `git status --porcelain --` on those paths
# prints nothing; print `SRC tree_is_head=yes|no recorded_matches=<value|ABSENT>
# expected_matches=true|false`; pass only when <dir>/src_tree holds exactly
# those four ids and <dir>/src_matches_checkout equals the expected value.
#
# What this tool does not establish: it judges records, it does not re-run
# anything. A pass means the records say what the fault arm saw; the fault
# arm itself is what observed the binary.
set -euo pipefail

T="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELF="$T/$(basename "${BASH_SOURCE[0]}")"
CRATE_PATHS=(crates Cargo.toml Cargo.lock rust-toolchain.toml)

usage() {
    sed -n '/^# Usage:/,/^#$/p' "$SELF" >&2
    exit 2
}

# ---------------------------------------------------------------------------
# --fault
# ---------------------------------------------------------------------------
fault_judge() {
    local dir="$1" require_all="$2"
    python3 - "$dir" "$require_all" <<'PY'
import json, pathlib, re, sys

d = pathlib.Path(sys.argv[1])
require_all = sys.argv[2] == '1'
LIVE = ('alive-stop-stream', 'alive-stop-capture', 'kill9-stale-stop',
        'kill9-at-gate', 'stop-races-completion')
WINDOW = 'publish-unlink-window'
FIELDS = ('case', 'live', 'bin_sha256', 'launch_shape', 'socket_path',
          'probe_before', 'probe_after', 'members_before', 'members_after',
          'advance_ran', 'next_stage_launched', 'expected_met')
NAME = re.compile(r'[a-z0-9][a-z0-9-]*')
FIXTURE = re.compile(r'[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)+')
SHAPE = {'alive-stop-stream': 'stream', 'alive-stop-capture': 'capture'}

if not d.is_dir():
    print('FAULT dir=ABSENT')
    print('FAULT verdict=fail')
    sys.exit(1)

try:
    sha = (d / 'bin_sha256').read_text().strip()
    sha_ok = bool(re.fullmatch('[0-9a-f]{64}', sha))
    sha_label = sha if sha_ok else 'MALFORMED'
except OSError:
    sha, sha_ok, sha_label = None, False, 'ABSENT'


def need(record, key, why):
    """The value of `key`, or None with `missing <key>` noted: an absent
    field is never read as a benign value."""
    if key not in record:
        why.append('missing ' + key)
        return None
    return record[key]


def expect(record, key, wanted, why, label=None):
    value = need(record, key, why)
    if key in record and value != wanted:
        why.append(f'{label or key} is {json.dumps(value)}, expected {json.dumps(wanted)}')


def non_empty_list(record, key, why):
    value = need(record, key, why)
    if key in record and not (isinstance(value, list) and value):
        why.append(f'{key} is empty: the measurement saw no process to act on')


def live_problems(name, record):
    why = [f'missing {key}' for key in FIELDS if key not in record]
    if why:
        return why
    if sha_ok and record['bin_sha256'] != sha:
        why.append('bin_sha256 differs from the directory bin_sha256')
    if record['expected_met'] is not True:
        why.append('expectation not met (expected_met is not true)')
    expect(record, 'probe_after', 'GONE', why)
    after = record['members_after']
    if not isinstance(after, list):
        why.append('members_after is not a list')
    elif after:
        why.append(f'surviving member: {after[0]}')
    if name in SHAPE:
        expect(record, 'launch_shape', SHAPE[name], why)
        expect(record, 'probe_before', 'ALIVE', why)
        non_empty_list(record, 'members_before', why)
        expect(record, 'next_stage_launched', False, why)
    elif name == 'kill9-stale-stop':
        expect(record, 'probe_before', 'ALIVE', why)
        expect(record, 'probe_after_kill', 'STALE', why)
        non_empty_list(record, 'members_before', why)
        expect(record, 'next_stage_launched', False, why)
    elif name == 'kill9-at-gate':
        expect(record, 'probe_after_kill', 'STALE', why)
        expect(record, 'gate_open_before', True, why)
        expect(record, 'lock_held_before', True, why)
        holder = need(record, 'lock_holder_after', why)
        if 'lock_holder_after' in record and holder not in ('none', 'recycled'):
            why.append(f'lock_holder_after is {json.dumps(holder)}, expected none or recycled')
    elif name == 'stop-races-completion':
        expect(record, 'state_stopped', True, why)
        expect(record, 'next_agents_alive', [], why)
    return why


def pointer_problems(record):
    why = [f'missing {key}' for key in ('case', 'bin_sha256', 'fixture') if key not in record]
    if why:
        return why
    if sha_ok and record['bin_sha256'] != sha:
        why.append('bin_sha256 differs from the directory bin_sha256')
    fixture = record['fixture']
    if not (isinstance(fixture, str) and FIXTURE.fullmatch(fixture)):
        why.append('pointer names no fixture test')
    return why


judged = []  # (name, kind) with kind live | pointer | fail
for path in sorted(d.glob('case-*.json')):
    name = path.name[len('case-'):-len('.json')]
    why = []
    kind = 'fail'
    try:
        record = json.loads(path.read_text())
        if not isinstance(record, dict):
            raise ValueError('not a JSON object')
        if 'case' not in record:
            why.append('missing case')
        elif not (isinstance(record['case'], str) and NAME.fullmatch(record['case'])):
            why.append('invalid case name')
        else:
            name = record['case']
        if not why:
            live = need(record, 'live', why)
            if live is True:
                why += live_problems(name, record)
                kind = 'live'
            elif live is False:
                why += pointer_problems(record)
                kind = 'pointer'
            elif 'live' in record:
                why.append('live is neither true nor false')
    except (OSError, ValueError) as error:
        why.append(f'unreadable record: {error}'.replace('\n', ' '))
    if why:
        kind = 'fail'
        print(f'FAULT case={name} FAIL ' + '; '.join(why))
    else:
        print(f'FAULT case={name} ok {kind}')
    judged.append((name, kind))

print(f'FAULT bin_sha256={sha_label}')
passed = sha_ok and bool(judged) and all(kind != 'fail' for _, kind in judged)
if not judged:
    print('FAULT cases=0')
if require_all:
    counts = {}
    for name, _ in judged:
        counts[name] = counts.get(name, 0) + 1
    missing = [n for n in LIVE if (n, 'live') not in judged]
    if (WINDOW, 'pointer') not in judged:
        missing.append(WINDOW)
    duplicate = [n for n in LIVE + (WINDOW,) if counts.get(n, 0) > 1]
    if missing or duplicate:
        line = 'FAULT complete=no'
        if missing:
            line += ' missing=' + ','.join(missing)
        if duplicate:
            line += ' duplicate=' + ','.join(duplicate)
        print(line)
        passed = False
    else:
        print('FAULT complete=yes')
print('FAULT verdict=' + ('pass' if passed else 'fail'))
sys.exit(0 if passed else 1)
PY
}

# ---------------------------------------------------------------------------
# --src-check (R5-W1)
# ---------------------------------------------------------------------------

# The four crate-source `HEAD:` object ids in <repo>, one per line.
head_src_ids() {
    git -C "$1" rev-parse "${CRATE_PATHS[@]/#/HEAD:}"
}

# The `git status --porcelain` lines for the crate-source paths in <repo>.
crate_status() {
    git -C "$1" status --porcelain -- "${CRATE_PATHS[@]}"
}

src_check() {
    local dir="$1" repo="$2" ids status recorded="" tree_is_head=no matches=ABSENT expected=true
    if ! ids="$(head_src_ids "$repo" 2>/dev/null)"; then
        echo "SRC git_failed=yes repo=$repo"
        return 1
    fi
    if ! status="$(crate_status "$repo" 2>/dev/null)"; then
        echo "SRC git_failed=yes repo=$repo"
        return 1
    fi
    [ -z "$status" ] || expected=false
    if [ -f "$dir/src_tree" ]; then
        recorded="$(cat -- "$dir/src_tree")"
        [ "$recorded" != "$ids" ] || tree_is_head=yes
    fi
    if [ -f "$dir/src_matches_checkout" ]; then
        matches="$(tr -d '[:space:]' <"$dir/src_matches_checkout")"
        [ -n "$matches" ] || matches=ABSENT
    fi
    echo "SRC tree_is_head=$tree_is_head recorded_matches=$matches expected_matches=$expected"
    [ "$tree_is_head" = yes ] && [ "$matches" = "$expected" ]
}

default_repo() {
    git -C "$T" rev-parse --show-toplevel
}

# ---------------------------------------------------------------------------
# --selftest: one `  ok   <case>` or `  FAIL <case> — <why>` line per planted
# case, then `SELFTEST passed=N failed=M`; exit 0 only when M is 0 and N > 0.
# A repeated case name is itself a FAIL. Every case asserts the mode's exit
# code together with its verdict line. Later plans append cases by adding
# `st_case` calls to `run_selftest` (50-25).
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

# expect <rc> <line> <cmd...>: pass when <cmd> exits <rc> and prints <line> as
# a whole line.
expect() {
    local want_rc="$1" want_line="$2" out rc=0
    shift 2
    out="$("$@" 2>&1)" || rc=$?
    if [ "$rc" -ne "$want_rc" ]; then
        echo "exit $rc, wanted $want_rc: $out"
        return 1
    fi
    if ! printf '%s\n' "$out" | rg -q -x -F -e "$want_line"; then
        echo "no line '$want_line' in: $out"
        return 1
    fi
}

# expect_re <rc> <regex> <cmd...>: as `expect`, the line matched as a whole-line regex.
expect_re() {
    local want_rc="$1" want_re="$2" out rc=0
    shift 2
    out="$("$@" 2>&1)" || rc=$?
    if [ "$rc" -ne "$want_rc" ]; then
        echo "exit $rc, wanted $want_rc: $out"
        return 1
    fi
    if ! printf '%s\n' "$out" | rg -q -x -e "$want_re"; then
        echo "no line matching '$want_re' in: $out"
        return 1
    fi
}

# expect_absent <regex> <cmd...>: pass when no output line matches <regex>
# (the exit code is asserted by a companion `expect`).
expect_absent() {
    local re="$1" out
    shift
    out="$("$@" 2>&1)" || true
    if printf '%s\n' "$out" | rg -q -x -e "$re"; then
        echo "unexpected line matching '$re' in: $out"
        return 1
    fi
}

ST_SHA="$(printf 'a%.0s' {1..64})"
ST_OTHER_SHA="$(printf 'b%.0s' {1..64})"

# A good record for case <name> with bin_sha256 <sha>, then the jq filter <edit>.
st_record() {
    local name="$1" sha="$2" edit="${3:-.}" shape=stream
    [ "$name" != alive-stop-capture ] || shape=capture
    jq -n --arg c "$name" --arg s "$sha" --arg shape "$shape" '{
        case: $c, live: true, bin_sha256: $s, launch_shape: $shape,
        socket_path: "/tmp/df50f.AAAAAA/c/sock/0123456789abcdef-01-fedcba9876543210.sock",
        launch_id: "fedcba9876543210",
        probe_before: "ALIVE", probe_after_kill: "STALE", probe_after: "GONE",
        members_before: ["    101     101     100 Sat Oct 18 12:00:00 2026 /bin/sh /tmp/df50f.AAAAAA/f/claude"],
        members_after: [],
        gate_open_before: true, lock_held_before: true, lock_holder_after: "none",
        state_stopped: true, next_agents_alive: [],
        advance_ran: false, next_stage_launched: false, expected_met: true
    }' | jq "$edit"
}

st_pointer() {
    jq -n --arg s "$1" '{
        case: "publish-unlink-window", live: false, bin_sha256: $s,
        fixture: "pipeline_launch::tests::a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor",
        reason: "the publish/unlink window is not reachable live"
    }'
}

# A fresh fault dir under ST_TMP holding bin_sha256 <sha>; prints its path.
st_fault_dir() {
    local dir
    dir="$(mktemp -d "$ST_TMP/fault.XXXXXX")"
    printf '%s\n' "$1" >"$dir/bin_sha256"
    printf '%s\n' "$dir"
}

st_fault_good() {
    local dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_record alive-stop-stream "$ST_SHA" >"$dir/case-alive-stop-stream.json"
    expect 0 'FAULT verdict=pass' bash "$SELF" --fault "$dir" &&
        expect 0 'FAULT case=alive-stop-stream ok live' bash "$SELF" --fault "$dir"
}

st_fault_surviving_member() {
    local dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_record alive-stop-stream "$ST_SHA" '.members_after = ["    101     101     100 Sat Oct 18 12:00:00 2026 /bin/sh claude"]' \
        >"$dir/case-alive-stop-stream.json"
    expect 1 'FAULT verdict=fail' bash "$SELF" --fault "$dir" &&
        expect_re 1 'FAULT case=alive-stop-stream FAIL .*surviving member.*' bash "$SELF" --fault "$dir"
}

st_fault_other_sha() {
    local dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_record alive-stop-stream "$ST_OTHER_SHA" >"$dir/case-alive-stop-stream.json"
    expect 1 'FAULT verdict=fail' bash "$SELF" --fault "$dir" &&
        expect_re 1 'FAULT case=alive-stop-stream FAIL .*bin_sha256 differs.*' bash "$SELF" --fault "$dir"
}

# Run <cmd...> with this checkout's git state out of reach: no repo-local
# GIT_* variable, HOME and XDG_CONFIG_HOME inside the selftest scratch, no
# system config.
st_git_env() {
    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE -u GIT_COMMON_DIR \
        -u GIT_CONFIG -u GIT_PREFIX -u GIT_CONFIG_GLOBAL \
        HOME="$ST_TMP/home" XDG_CONFIG_HOME="$ST_TMP/home/.config" GIT_CONFIG_NOSYSTEM=1 "$@"
}

st_git() {
    st_git_env git -c user.name=Selftest -c user.email=selftest@localhost \
        -c commit.gpgsign=false -c core.hooksPath=/dev/null "$@"
}

# A scratch repository holding the four crate paths with two commits; prints
# its path. <repo>/.git/old-ids holds the first commit's four ids.
st_src_repo() {
    local repo
    repo="$(mktemp -d "$ST_TMP/repo.XXXXXX")"
    st_git init -q "$repo"
    mkdir -p "$repo/crates/x"
    printf 'fn a() {}\n' >"$repo/crates/x/lib.rs"
    printf '[workspace]\n' >"$repo/Cargo.toml"
    printf '# lock\n' >"$repo/Cargo.lock"
    printf '[toolchain]\n' >"$repo/rust-toolchain.toml"
    st_git -C "$repo" add -A
    st_git -C "$repo" commit -q -m first
    st_git -C "$repo" rev-parse HEAD:crates HEAD:Cargo.toml HEAD:Cargo.lock HEAD:rust-toolchain.toml \
        >"$repo/.git/old-ids"
    printf 'fn a() { let _ = 1; }\n' >"$repo/crates/x/lib.rs"
    st_git -C "$repo" commit -q -am second
    printf '%s\n' "$repo"
}

# A fault dir whose src_tree holds <repo>'s current HEAD ids and whose
# src_matches_checkout holds <flag>; prints its path.
st_src_dir() {
    local repo="$1" flag="$2" dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_git -C "$repo" rev-parse HEAD:crates HEAD:Cargo.toml HEAD:Cargo.lock HEAD:rust-toolchain.toml \
        >"$dir/src_tree"
    printf '%s\n' "$flag" >"$dir/src_matches_checkout"
    printf '%s\n' "$dir"
}

st_dirty() {
    printf '// uncommitted\n' >>"$1/crates/x/lib.rs"
}

st_src_clean_pass() {
    local repo dir
    repo="$(st_src_repo)"
    dir="$(st_src_dir "$repo" true)"
    expect 0 'SRC tree_is_head=yes recorded_matches=true expected_matches=true' \
        st_git_env bash "$SELF" --src-check "$dir" --repo "$repo"
}

st_src_dirty_pass() {
    local repo dir
    repo="$(st_src_repo)"
    dir="$(st_src_dir "$repo" false)"
    st_dirty "$repo"
    expect 0 'SRC tree_is_head=yes recorded_matches=false expected_matches=false' \
        st_git_env bash "$SELF" --src-check "$dir" --repo "$repo"
}

st_src_missing() {
    local repo dir
    repo="$(st_src_repo)"
    dir="$(st_src_dir "$repo" true)"
    rm -- "$dir/src_tree"
    expect 1 'SRC tree_is_head=no recorded_matches=true expected_matches=true' \
        st_git_env bash "$SELF" --src-check "$dir" --repo "$repo"
}

st_src_unknown() {
    local repo dir
    repo="$(st_src_repo)"
    dir="$(st_src_dir "$repo" true)"
    printf 'unknown\n' >"$dir/src_tree"
    expect 1 'SRC tree_is_head=no recorded_matches=true expected_matches=true' \
        st_git_env bash "$SELF" --src-check "$dir" --repo "$repo"
}

st_src_older() {
    local repo dir
    repo="$(st_src_repo)"
    dir="$(st_src_dir "$repo" true)"
    cp -- "$repo/.git/old-ids" "$dir/src_tree"
    # Control: the older ids really differ from HEAD's, so the failure below
    # is the comparison's, not a fixture that planted HEAD again.
    if cmp -s "$dir/src_tree" <(st_git -C "$repo" rev-parse HEAD:crates HEAD:Cargo.toml HEAD:Cargo.lock HEAD:rust-toolchain.toml); then
        echo "fixture broken: the older commit's ids equal HEAD's"
        return 1
    fi
    expect 1 'SRC tree_is_head=no recorded_matches=true expected_matches=true' \
        st_git_env bash "$SELF" --src-check "$dir" --repo "$repo"
}

st_src_inverted_clean() {
    local repo dir
    repo="$(st_src_repo)"
    dir="$(st_src_dir "$repo" false)"
    expect 1 'SRC tree_is_head=yes recorded_matches=false expected_matches=true' \
        st_git_env bash "$SELF" --src-check "$dir" --repo "$repo"
}

st_src_inverted_dirty() {
    local repo dir
    repo="$(st_src_repo)"
    dir="$(st_src_dir "$repo" true)"
    st_dirty "$repo"
    expect 1 'SRC tree_is_head=yes recorded_matches=true expected_matches=false' \
        st_git_env bash "$SELF" --src-check "$dir" --repo "$repo"
}

run_selftest() {
    ST_TMP="$(mktemp -d)"
    trap 'rm -rf -- "$ST_TMP"' EXIT
    mkdir -p "$ST_TMP/home/.config"

    st_case 'fault passes a good alive-stop-stream record' st_fault_good
    st_case 'fault fails a surviving member' st_fault_surviving_member
    st_case 'fault fails a record naming another bin_sha256' st_fault_other_sha
    st_case 'src check passes a current record on a clean tree' st_src_clean_pass
    st_case 'src check passes a current record on a dirty tree' st_src_dirty_pass
    st_case 'src check fails a missing src_tree' st_src_missing
    st_case 'src check fails an unknown src_tree' st_src_unknown
    st_case 'src check fails a src_tree from an older commit' st_src_older
    st_case 'src check fails an inverted flag on a clean tree' st_src_inverted_clean
    st_case 'src check fails an inverted flag on a dirty tree' st_src_inverted_dirty

    echo "SELFTEST passed=$ST_PASS failed=$ST_FAIL"
    [ "$ST_FAIL" -eq 0 ] && [ "$ST_PASS" -gt 0 ]
}

# ---------------------------------------------------------------------------
# dispatch
# ---------------------------------------------------------------------------
main() {
    [ "$#" -gt 0 ] || usage
    case "$1" in
        --selftest)
            [ "$#" -eq 1 ] || usage
            run_selftest
            ;;
        --fault)
            local dir="" require_all=0
            shift
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --require-all) require_all=1 ;;
                    -*) usage ;;
                    *) [ -z "$dir" ] || usage; dir="$1" ;;
                esac
                shift
            done
            [ -n "$dir" ] || usage
            fault_judge "$dir" "$require_all"
            ;;
        --src-check)
            local dir="" repo=""
            shift
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --repo) [ "$#" -ge 2 ] || usage; repo="$2"; shift ;;
                    -*) usage ;;
                    *) [ -z "$dir" ] || usage; dir="$1" ;;
                esac
                shift
            done
            [ -n "$dir" ] || usage
            [ -n "$repo" ] || repo="$(default_repo)"
            src_check "$dir" "$repo"
            ;;
        *) usage ;;
    esac
}

main "$@"
