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
#   check-supervisor-evidence.sh --fault-current <P50 home> [--repo <checkout>]
#   check-supervisor-evidence.sh collect-sidecars <EV> --project <dir> [--bin <devflow>]
#   check-supervisor-evidence.sh --attempt <EV> [--p49-check <script>]
#   check-supervisor-evidence.sh --attempt-gate <P50 home> [--p49-check <script>]
#                                [--current-source [--repo <checkout>]]
#   check-supervisor-evidence.sh --published <dir> [--attempt <N>]
#   check-supervisor-evidence.sh --identity-scan <dir> [--tokens <file> | --repo <checkout>]
#   check-supervisor-evidence.sh --live-run <50-LIVE-RUN.md> [--attempt <N>]
#   check-supervisor-evidence.sh --handoff <50-LIVE-RUN.md> --plan <50-20-PLAN.md>
#   check-supervisor-evidence.sh --defects <50-LIVE-RUN.md> --roadmap <ROADMAP.md>
#   check-supervisor-evidence.sh --selftest
#
# <P50 home> is $HOME/.cache/devflow-phase50 in the runbook: Phase 49's tools
# write attempt-<N>/ there (--home), fault-arm.sh writes fault-<M>/.
# --p49-check replaces Phase 49's check-evidence.sh; only the selftest passes
# it (a stub), so its attempt cases need no Phase 49 evidence tree.
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
# --require-all (R5-W2) also prints `FAULT complete=yes`, or `FAULT
# complete=no [missing=<names>] [duplicate=<names>]` and fails, unless each of
# the five live cases (alive-stop-stream, alive-stop-capture, kill9-stale-stop,
# kill9-at-gate, stop-races-completion) printed `ok live` and
# publish-unlink-window printed `ok pointer`, each in exactly one record.
#
# --fault-current (RV-07, R5-W1): fails on any uncommitted change under the
# crate paths (`FAULT_CURRENT uncommitted_crate_source=<n>`); otherwise selects
# the newest `fault-*` directory in version order that passes --src-check with
# `src_matches_checkout` true (`FAULT_CURRENT dir=<name|ABSENT>`) and judges
# that one directory with --fault --require-all, so an older passing directory
# never stands in for a failing newer one.
#
# --attempt runs Phase 49's `check-evidence.sh --arm positive <EV>`, then
# checks every collected stage_launched event: socket_path at most 107 bytes,
# a launch label, a non-empty launch_id its socket file name ends with as
# `-<launch_id>.sock`, both `stream` and `capture` present, no monitor_pid key.
# It prints `ATTEMPT sidecars_left=<n>` as information only (RV-02) and
# `ATTEMPT verdict=pass|fail`.
#
# --attempt-gate prints `ATTEMPT_GATE verdict=shipped|cap_reached|not_ready`
# with Phase 49's semantics; shipped also needs a fault-* directory that
# --fault --require-all passes recording the attempt's bin_sha256 (D-17;
# `ATTEMPT_GATE fault=<dir> bin_sha256=<hex>`). Exit 0 for shipped and
# cap_reached, 1 for not_ready.
#
# --attempt-gate --current-source (50-27, operator ruling 2026-10-21) judges
# an attempt after a crate fix: it refuses up front with `reason=uncommitted
# crate source` when `git status --porcelain` on the crate paths in <checkout>
# (default: the checkout holding this tool) prints anything, and a fault
# directory counts as the attempt's match only if it also passes --src-check
# against <checkout>. A match prints `ATTEMPT_GATE current_source=yes`; none
# prints the not-ready reason ending `from the current crate source`. So an
# attempt whose paired fault record was taken from another crate tree, such
# as one built before a later fix, cannot ship. cap_reached is unchanged: it
# counts stopped attempts, whatever source they ran. --repo is valid only
# with --current-source.
#
# --published (50-25, D-18) judges the sanitized tree 50-20 commits: at least
# one fault-* and one attempt-* directory, a fault-*/verdict-*.txt line
# starting `FAULT verdict=pass`, a line anywhere starting `ATTEMPT_GATE
# verdict=shipped` or `cap_reached`, no file naming the value of $HOME (rg
# -uuu: hidden, ignored and binary files are read; an empty $HOME or an rg
# error fails) and no symlink. Prints `PUBLISHED verdict=pass|fail`.
#
# --published <dir> --attempt <N> (50-29, T-50-109) also requires
# <dir>/attempt-<N>/ (not a symlink) with a file directly in it holding a line
# starting `ATTEMPT_GATE verdict=shipped attempt=<N>` or `ATTEMPT_GATE
# verdict=cap_reached`. For shipped, that same file must hold the exact line
# `ATTEMPT_GATE current_source=yes` and one `ATTEMPT_GATE fault=<name>
# bin_sha256=<hex>` line naming a <dir>/<name>/ whose bin_sha256 file equals
# <hex> and which holds a verdict-*.txt line starting `FAULT verdict=pass`.
# Prints `PUBLISHED attempt=<N> verdict_line=shipped|cap_reached|ABSENT`,
# `PUBLISHED attempt_current_source=yes|no|n/a` and `PUBLISHED
# attempt_fault=<name|ABSENT>` (ABSENT for cap_reached, which pairs no fault
# directory), then the one verdict line. Without --attempt nothing changes.
#
# --identity-scan (50-29, T-50-107) refuses a tree holding any fragment of an
# email address. Addresses come from <file> (one per line, blank lines
# ignored; source `file`) or, without --tokens, from the host: the
# `oauthAccount.emailAddress` of $HOME/.claude.json (source `account`) and
# every `git -C <checkout> config --get-all user.email` value (source `git`;
# <checkout> defaults to the one holding this tool). Each address, lowercased,
# gives the full address (part `address`), the local part and every [a-z0-9]
# run of 4 or more characters in it (part `local`), and the domain and every
# such run in each domain label but the last (part `domain`); duplicates keep
# their first derivation, account before git. Every file under <dir> (hidden,
# ignored and binary included) is searched for each token with `rg --no-config
# -uuu -i -F -l`, the token read from a pattern file in a 0700 scratch
# directory, never from argv; every file and directory name is checked too.
# It prints `IDENTITY tokens=<n>`, one `IDENTITY hit token=<k>
# source=<account|git|file> part=<address|local|domain> file=<path relative to
# dir>` per hit (` in=name` for a name hit, the token masked as `<token-k>`),
# and `IDENTITY verdict=pass|fail`; it never prints a token's text.
# Exit codes (R6-G6): 0 when the verdict is pass; 1 for every fail, which
# covers any hit, zero tokens, an address without exactly one `@` (`IDENTITY
# malformed_address=<line number>`), a missing host account email (`IDENTITY
# source=account ABSENT`: no file, no key) and an rg error (`IDENTITY
# rg_error=<code>`); 2 for a usage error only (a missing or unknown argument,
# --tokens with --repo, <dir> not a directory). A gate tells a hit from the
# other failures by the `IDENTITY hit` line, never by the exit code.
#
# --live-run (50-25) passes a record holding each of `does not establish`,
# `verdict=`, `stream`, `capture`, `unverified on macOS`, `V-28`, `V-29`,
# `bin_sha256` and, for each criterion N in 1-8, a line matching
# `^\| *(criterion )?N *\|`. Prints `LIVE_RUN verdict=pass|fail`.
#
# --live-run <file> --attempt <N> (50-29, T-50-109) also requires a line
# containing `ATTEMPT_GATE verdict=shipped attempt=<N>` (N ending at a
# non-digit) or `ATTEMPT_GATE verdict=cap_reached`; otherwise it prints
# `LIVE_RUN missing=attempt-<N>-verdict`. Without --attempt nothing changes.
#
# --handoff (50-25, R6-D) passes when <plan> holds exactly one line matching
# `^    bash -c .R=.*changelog_3_0_0_lines=.*$`, the record holds that line
# without its four-space indent verbatim, and the record holds every release
# handoff fragment. Prints `HANDOFF verdict=pass|fail`.
#
# --defects (50-25) reads `git diff --unified=0 HEAD -- <roadmap>` in the
# repository holding <roadmap> (a git failure, or a roadmap absent at HEAD,
# fails): each added `### Phase 999.N:` heading's id must appear in the record
# (not as part of a longer number); with none added the record must say "no
# non-blocking defect found" (any case). Prints `DEFECTS verdict=pass|fail`.
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
# --fault-current (RV-07, R5-W1): the fault directory recorded from the current
# crate source, judged with --require-all
# ---------------------------------------------------------------------------
fault_current() {
    local home="$1" repo="$2" status count=0 name chosen=""
    if ! status="$(crate_status "$repo" 2>/dev/null)"; then
        echo "FAULT_CURRENT git_failed=yes repo=$repo"
        echo "FAULT_CURRENT verdict=fail"
        return 1
    fi
    [ -z "$status" ] || count="$(printf '%s\n' "$status" | wc -l)"
    echo "FAULT_CURRENT uncommitted_crate_source=$count"
    if [ "$count" -gt 0 ]; then
        echo "FAULT_CURRENT verdict=fail"
        return 1
    fi
    if [ -d "$home" ]; then
        while IFS= read -r name; do
            if src_check "$home/$name" "$repo" >/dev/null; then
                chosen="$name"
                break
            fi
        done < <(find "$home" -mindepth 1 -maxdepth 1 -type d -name 'fault-*' -printf '%f\n' | sort -V -r)
    fi
    echo "FAULT_CURRENT dir=${chosen:-ABSENT}"
    if [ -z "$chosen" ]; then
        echo "FAULT_CURRENT verdict=fail"
        return 1
    fi
    if fault_judge "$home/$chosen" 1; then
        echo "FAULT_CURRENT verdict=pass"
    else
        echo "FAULT_CURRENT verdict=fail"
        return 1
    fi
}

# ---------------------------------------------------------------------------
# collect-sidecars: copy the leftover sidecars of <project> and its `devflow
# status` into <EV>/supervisor/. Evidence only: a run to Ship leaves none,
# because finish() removes each finished launch's socket and sidecar (50-07).
# ---------------------------------------------------------------------------

# registry::cache_dir(): DEVFLOW_CACHE_DIR, else XDG_CACHE_HOME/devflow, else
# HOME/.cache/devflow.
devflow_cache_dir() {
    if [ -n "${DEVFLOW_CACHE_DIR+set}" ]; then
        printf '%s\n' "$DEVFLOW_CACHE_DIR"
    elif [ -n "${XDG_CACHE_HOME+set}" ]; then
        printf '%s\n' "$XDG_CACHE_HOME/devflow"
    else
        printf '%s\n' "$HOME/.cache/devflow"
    fi
}

collect_sidecars() {
    local ev="$1" project="$2" bin="$3" cache root sidecar recorded copied=0 status_rc=0
    [ -d "$ev" ] || { echo "COLLECT ev=ABSENT $ev" >&2; return 1; }
    [ -d "$project" ] || { echo "COLLECT project=ABSENT $project" >&2; return 1; }
    root="$(cd "$project" && pwd -P)"
    if [ -z "$bin" ]; then
        bin="$(jq -r '.bin // empty' "$ev/attempt.json" 2>/dev/null || true)"
        [ -n "$bin" ] || bin=devflow
    fi
    cache="$(devflow_cache_dir)"
    mkdir -p "$ev/supervisor"
    for sidecar in "$cache"/sock/*.json; do
        [ -f "$sidecar" ] || continue
        recorded="$(jq -r '.project_root // empty' "$sidecar" 2>/dev/null || true)"
        if [ -n "$recorded" ] && { [ "$recorded" = "$project" ] || [ "$recorded" = "$root" ]; }; then
            cp -p -- "$sidecar" "$ev/supervisor/"
            copied=$((copied + 1))
        fi
    done
    "$bin" status "$project" >"$ev/supervisor/status.txt" 2>&1 || status_rc=$?
    echo "COLLECT sidecars=$copied status_exit=$status_rc dir=$ev/supervisor"
}

# ---------------------------------------------------------------------------
# --attempt: Phase 49's positive-arm check plus the Phase 50 launch checks on
# the collected stage_launched events. No sidecar is required (RV-02).
# ---------------------------------------------------------------------------
p49_check_default() {
    local dir="$T/../../49-live-unattended-run-the-milestone-s-instrument/tools"
    printf '%s\n' "$(cd "$dir" && pwd)/check-evidence.sh"
}

# The Phase 50 half of --attempt: every stage_launched event carries a socket
# path of at most 107 bytes, a launch label and a non-empty launch id that the
# socket's file name ends with (`-<launch_id>.sock`); both labels appear; no
# event carries a monitor_pid key. Prints ATTEMPT lines, no verdict.
attempt_launch_checks() {
    python3 - "$1" <<'PY'
import json, pathlib, sys

ev = pathlib.Path(sys.argv[1])
failed = False


def report(name, problem):
    global failed
    if problem:
        failed = True
        print(f'ATTEMPT check={name} FAIL {problem}')
    else:
        print(f'ATTEMPT check={name} ok')


rows = []
try:
    text = (ev / 'collected' / 'devflow' / 'events.jsonl').read_text()
    for line in text.splitlines():
        if line.strip():
            row = json.loads(line)
            if not isinstance(row, dict) or 'event' not in row:
                raise ValueError('an event line has no event field')
            rows.append(row)
    report('events readable', '')
except (OSError, ValueError) as error:
    text = ''
    report('events readable', f'ABSENT or malformed: {error}'.replace('\n', ' '))

launches = [row for row in rows if row['event'] == 'stage_launched']
report('stage_launched present', '' if launches else 'no stage_launched event')
problems = []
labels = set()
for index, row in enumerate(launches):
    where = f'stage_launched #{index + 1}'
    for key in ('socket_path', 'launch', 'launch_id'):
        if key not in row:
            problems.append(f'{where} missing {key}')
    if any(key not in row for key in ('socket_path', 'launch', 'launch_id')):
        continue
    socket, label, launch_id = row['socket_path'], row['launch'], row['launch_id']
    if not isinstance(socket, str) or not socket:
        problems.append(f'{where} socket_path is not a path')
        continue
    if len(socket.encode()) > 107:
        problems.append(f'{where} socket_path is {len(socket.encode())} bytes')
    if label not in ('stream', 'capture'):
        problems.append(f'{where} launch label {json.dumps(label)}')
    else:
        labels.add(label)
    if not isinstance(launch_id, str) or not launch_id:
        problems.append(f'{where} launch_id is empty')
    elif not socket.rsplit('/', 1)[-1].endswith(f'-{launch_id}.sock'):
        problems.append(f'{where} socket file name does not end with -{launch_id}.sock')
report('every launch names its socket, label and id', '; '.join(problems))
missing = [label for label in ('stream', 'capture') if label not in labels]
report('both launch shapes', 'missing ' + ','.join(missing) if missing else '')
report('no monitor_pid key', 'an event carries monitor_pid' if '"monitor_pid"' in text else '')
print('ATTEMPT launches={} stream={} capture={}'.format(
    len(launches), sum(1 for r in launches if r.get('launch') == 'stream'),
    sum(1 for r in launches if r.get('launch') == 'capture')))
sys.exit(1 if failed else 0)
PY
}

attempt_check() {
    local ev="$1" p49="$2" out rc=0 p49_ok=no launch_ok=no left=0
    out="$(bash "$p49" --arm positive "$ev" 2>&1)" || rc=$?
    printf '%s\n' "$out"
    if [ "$rc" -eq 0 ] && printf '%s\n' "$out" | rg -q -x -e 'CHECK passed=[1-9][0-9]* failed=0'; then
        p49_ok=yes
    fi
    echo "ATTEMPT p49_positive=$([ "$p49_ok" = yes ] && echo pass || echo fail)"
    if attempt_launch_checks "$ev"; then launch_ok=yes; fi
    if [ -d "$ev/supervisor" ]; then
        left="$(find "$ev/supervisor" -maxdepth 1 -type f -name '*.json' | wc -l)"
    fi
    echo "ATTEMPT sidecars_left=$left"
    if [ "$p49_ok" = yes ] && [ "$launch_ok" = yes ]; then
        echo "ATTEMPT verdict=pass"
    else
        echo "ATTEMPT verdict=fail"
        return 1
    fi
}

# ---------------------------------------------------------------------------
# --attempt-gate: Phase 49's verdict semantics (shipped | cap_reached |
# not_ready, exit 0 for the first two and 1 for not_ready), with shipped
# judged by --attempt and D-17's same-binary rule: a fault directory that
# `--fault --require-all` passes must record the attempt's bin_sha256.
# ---------------------------------------------------------------------------
attempt_gate() {
    python3 - "$SELF" "$1" "$2" "$3" "$4" "${CRATE_PATHS[@]}" <<'PY'
import json, pathlib, re, subprocess, sys

script, home, p49 = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3]
current_source, repo, crate_paths = sys.argv[4] == '1', sys.argv[5], sys.argv[6:]
number = 'ABSENT'


def version_key(path):
    return [int(part) if part.isdigit() else part for part in re.split(r'(\d+)', path.name)]


try:
    number = (home / 'current-attempt').read_text().strip()
    if not re.fullmatch('[1-9]', number):
        raise ValueError('current-attempt must be one digit 1-9')
    if current_source:
        status = subprocess.run(['git', '-C', repo, 'status', '--porcelain', '--', *crate_paths],
                                capture_output=True, text=True, timeout=60)
        if status.returncode != 0:
            raise ValueError(f'git status failed in {repo}')
        if status.stdout.strip():
            raise ValueError('uncommitted crate source')
    attempt = home / f'attempt-{number}'
    collected = attempt / 'collected'
    if not (collected / 'DONE').is_file():
        raise FileNotFoundError('collected/DONE')
    verdict = json.loads((attempt / 'verdict.json').read_text())
    lines = (collected / 'MANIFEST.sha256').read_text().splitlines()
    if not lines:
        raise ValueError('manifest empty')
    for line in lines:
        if not re.fullmatch(r'[0-9a-f]{64} [ *].+', line):
            raise ValueError('manifest malformed')
        path = pathlib.Path(line[66:])
        if path.is_absolute() or '..' in path.parts:
            raise ValueError('manifest path outside collection')
    result = subprocess.run(['sha256sum', '--quiet', '-c', 'MANIFEST.sha256'], cwd=collected,
                            capture_output=True, text=True, timeout=60)
    if result.returncode != 0:
        raise ValueError('manifest mismatch')
    rows = [json.loads(line) for line in (collected / 'devflow' / 'events.jsonl').read_text().splitlines() if line.strip()]
    shipped = any(row['event'] == 'workflow_shipped' and str(row['phase']) == '1' for row in rows)
    if (verdict['outcome'] not in ('shipped', 'stopped') or (verdict['outcome'] == 'shipped') != shipped
            or verdict['workflow_shipped'] is not shipped):
        raise ValueError('verdict/events disagreement')
    if shipped:
        result = subprocess.run(['bash', script, '--attempt', str(attempt), '--p49-check', p49],
                                capture_output=True, text=True, timeout=300)
        print(result.stdout, end='')
        if result.returncode != 0:
            failures = [line for line in result.stdout.splitlines()
                        if ' FAIL ' in line or line.startswith('  FAIL ')]
            raise ValueError('attempt check failed: ' + ('; '.join(failures) or 'ATTEMPT verdict=fail'))
        expected = json.loads((attempt / 'attempt.json').read_text())['bin_sha256']
        if not (isinstance(expected, str) and re.fullmatch('[0-9a-f]{64}', expected)):
            raise ValueError('attempt.json bin_sha256 is not 64 hex')
        match = None
        for fault in sorted((p for p in home.glob('fault-*') if p.is_dir()), key=version_key):
            try:
                recorded = (fault / 'bin_sha256').read_text().strip()
            except OSError:
                continue
            if recorded != expected:
                continue
            judged = subprocess.run(['bash', script, '--fault', str(fault), '--require-all'],
                                    capture_output=True, text=True, timeout=60)
            if judged.returncode != 0:
                continue
            if current_source:
                source = subprocess.run(['bash', script, '--src-check', str(fault), '--repo', repo],
                                        capture_output=True, text=True, timeout=60)
                if source.returncode != 0:
                    continue
            match = fault
        if match is None:
            suffix = ' from the current crate source' if current_source else ''
            print(f"ATTEMPT_GATE verdict=not_ready attempt={number} reason=no passing fault arm on attempt {number}'s binary{suffix}")
            sys.exit(1)
        if current_source:
            print('ATTEMPT_GATE current_source=yes')
        print(f'ATTEMPT_GATE fault={match.name} bin_sha256={expected}')
        print(f'ATTEMPT_GATE verdict=shipped attempt={number}')
    else:
        stopped = 0
        for path in home.glob('attempt-*/verdict.json'):
            value = json.loads(path.read_text())
            if value['outcome'] == 'stopped' and isinstance(value['stop_note'], str) and value['stop_note'].strip():
                stopped += 1
        if stopped < 3:
            raise ValueError(f'{stopped} of 3 stopped attempts have a note')
        print(f'ATTEMPT_GATE verdict=cap_reached stopped_attempts={stopped}')
except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
    absent = 'ABSENT ' if isinstance(error, (OSError, KeyError)) else ''
    reason = str(error).replace('\n', ' ')
    print(f'ATTEMPT_GATE verdict=not_ready attempt={number} reason={absent}{reason}')
    sys.exit(1)
PY
}

# ---------------------------------------------------------------------------
# --published (50-25, D-18): judge a sanitized evidence tree before it is
# committed. One `PUBLISHED <problem>` line per problem, then
# `PUBLISHED verdict=pass|fail`. Every rg call here passes --no-config so a
# user ripgrep config cannot narrow the search.
# ---------------------------------------------------------------------------

# Some directory (not a symlink, not a file) directly under <dir> named <glob>.
published_has_dir() {
    [ -n "$(find "$1" -mindepth 1 -maxdepth 1 -type d -name "$2" -print -quit)" ]
}

published_check() {
    local dir="$1" home="${HOME:-}" bad=0 rc found line file fault_pass=0
    if [ -L "$dir" ]; then
        echo "PUBLISHED symlink=$dir"
        echo "PUBLISHED verdict=fail"
        return 1
    fi
    if [ ! -d "$dir" ]; then
        echo "PUBLISHED dir=ABSENT $dir"
        echo "PUBLISHED verdict=fail"
        return 1
    fi
    published_has_dir "$dir" 'fault-*' || { echo "PUBLISHED missing=fault-dir"; bad=1; }
    published_has_dir "$dir" 'attempt-*' || { echo "PUBLISHED missing=attempt-dir"; bad=1; }

    for file in "$dir"/fault-*/verdict-*.txt; do
        [ -f "$file" ] && [ ! -L "$file" ] || continue
        if rg --no-config -q -e '^FAULT verdict=pass' -- "$file"; then
            fault_pass=1
        fi
    done
    [ "$fault_pass" -eq 1 ] || { echo "PUBLISHED missing=fault-pass-verdict"; bad=1; }

    rc=0
    rg --no-config -uuu -q -e '^ATTEMPT_GATE verdict=(shipped|cap_reached)' -- "$dir" || rc=$?
    case "$rc" in
        0) ;;
        1) echo "PUBLISHED missing=attempt-gate-verdict"; bad=1 ;;
        *) echo "PUBLISHED rg_error=$rc search=attempt-gate-verdict"; bad=1 ;;
    esac

    # -uuu: hidden, ignored and binary files are read too (T-50-92).
    if [ -z "$home" ]; then
        echo "PUBLISHED home=EMPTY"
        bad=1
    else
        rc=0
        found="$(rg --no-config -uuu -l -F -e "$home" -- "$dir")" || rc=$?
        case "$rc" in
            0 | 1) ;;
            *) echo "PUBLISHED rg_error=$rc search=home-path"; bad=1 ;;
        esac
        if [ -n "$found" ]; then
            while IFS= read -r line; do echo "PUBLISHED names_home=$line"; done <<<"$found"
            bad=1
        fi
    fi

    rc=0
    found="$(find "$dir" -type l)" || rc=$?
    [ "$rc" -eq 0 ] || { echo "PUBLISHED find_error=$rc search=symlink"; bad=1; }
    if [ -n "$found" ]; then
        while IFS= read -r line; do echo "PUBLISHED symlink=$line"; done <<<"$found"
        bad=1
    fi

    if [ -n "${2:-}" ]; then
        published_attempt "$dir" "$2" || bad=1
    fi

    if [ "$bad" -eq 0 ]; then
        echo "PUBLISHED verdict=pass"
    else
        echo "PUBLISHED verdict=fail"
        return 1
    fi
}

# --published <dir> --attempt <N> (50-29): attempt-<N>/ is published and a file
# directly in it holds the attempt's own gate verdict. For shipped, that file
# also holds `ATTEMPT_GATE current_source=yes` and exactly one `ATTEMPT_GATE
# fault=<name> bin_sha256=<hex>` line whose <dir>/<name>/ records <hex> and
# holds a verdict-*.txt line starting `FAULT verdict=pass`. Prints the
# PUBLISHED attempt lines; no verdict line (published_check prints it).
published_attempt() {
    python3 - "$1" "$2" <<'PY'
import pathlib, re, sys

root, number = pathlib.Path(sys.argv[1]), sys.argv[2]
attempt = root / f'attempt-{number}'
shipped_re = re.compile(rf'ATTEMPT_GATE verdict=shipped attempt={number}(\s|$)')
cap_re = re.compile(r'ATTEMPT_GATE verdict=cap_reached(\s|$)')
fault_re = re.compile(r'ATTEMPT_GATE fault=(\S+) bin_sha256=([0-9a-f]{64})')


def lines_of(path):
    try:
        return path.read_text(errors='replace').splitlines()
    except OSError:
        return []


def judge_fault(lines):
    """(name or None, problem lines) for one shipped verdict file."""
    found = [m for m in (fault_re.fullmatch(line) for line in lines) if m]
    if len(found) != 1:
        return None, [f'PUBLISHED attempt_fault_lines={len(found)}']
    name, sha = found[0].group(1), found[0].group(2)
    if not re.fullmatch(r'fault-[A-Za-z0-9._-]+', name) or name in ('fault-.', 'fault-..'):
        return None, ['PUBLISHED attempt_fault_name=MALFORMED']
    fault = root / name
    if fault.is_symlink() or not fault.is_dir():
        return None, [f'PUBLISHED attempt_fault_dir=ABSENT name={name}']
    sha_file = fault / 'bin_sha256'
    recorded = '\n'.join(lines_of(sha_file)).strip() if sha_file.is_file() and not sha_file.is_symlink() else ''
    problems = []
    if recorded != sha:
        problems.append(f'PUBLISHED attempt_fault_sha=MISMATCH name={name}')
    verdicts = [p for p in sorted(fault.glob('verdict-*.txt')) if p.is_file() and not p.is_symlink()]
    if not any(line.startswith('FAULT verdict=pass') for p in verdicts for line in lines_of(p)):
        problems.append(f'PUBLISHED attempt_fault_pass=ABSENT name={name}')
    return (None if problems else name), problems


if attempt.is_symlink() or not attempt.is_dir():
    print(f'PUBLISHED missing=attempt-{number}-dir')
    files = []
else:
    files = [p for p in sorted(attempt.iterdir()) if p.is_file() and not p.is_symlink()]
shipped = [(p, lines_of(p)) for p in files]
shipped = [(p, lines) for p, lines in shipped if any(shipped_re.match(line) for line in lines)]
capped = [p for p in files if any(cap_re.match(line) for line in lines_of(p))]

if shipped:
    results = []
    for path, lines in shipped:
        source = 'ATTEMPT_GATE current_source=yes' in lines
        name, problems = judge_fault(lines)
        results.append((source and name is not None, source, name, problems))
    ok, source, name, problems = next((r for r in results if r[0]), results[0])
    print(f'PUBLISHED attempt={number} verdict_line=shipped')
    print(f"PUBLISHED attempt_current_source={'yes' if source else 'no'}")
    for line in problems:
        print(line)
    print(f"PUBLISHED attempt_fault={name or 'ABSENT'}")
    sys.exit(0 if ok else 1)
if capped:
    print(f'PUBLISHED attempt={number} verdict_line=cap_reached')
    print('PUBLISHED attempt_current_source=n/a')
    print('PUBLISHED attempt_fault=ABSENT')
    sys.exit(0)
print(f'PUBLISHED attempt={number} verdict_line=ABSENT')
print('PUBLISHED attempt_current_source=n/a')
print('PUBLISHED attempt_fault=ABSENT')
sys.exit(1)
PY
}

# ---------------------------------------------------------------------------
# --identity-scan (50-29, deferred item 2026-10-20): refuse a tree holding any
# fragment of the operator's email addresses. Prints `IDENTITY tokens=<n>`,
# one `IDENTITY hit token=<k> source=<s> part=<p> file=<path>` per hit (a
# file whose name holds a token adds ` in=name`, the token masked as
# `<token-k>`), and `IDENTITY verdict=pass|fail`. A token's text is never
# printed and never put on argv: rg reads it from a pattern file in a 0700
# scratch directory.
# ---------------------------------------------------------------------------
identity_scan() {
    python3 - "$1" "$2" "$3" <<'PY'
import json, os, pathlib, re, subprocess, sys, tempfile

directory, token_file, repo = sys.argv[1], sys.argv[2], sys.argv[3]
RUN = re.compile(r'[a-z0-9]{4,}')


def fail(*lines):
    for line in lines:
        print(line)
    print('IDENTITY verdict=fail')
    sys.exit(1)


addresses = []  # (address, source, line number within its source)
if token_file:
    try:
        text = pathlib.Path(token_file).read_text()
    except (OSError, UnicodeDecodeError):
        fail('IDENTITY tokens_file=ABSENT')
    addresses += [(line.strip(), 'file', n) for n, line in enumerate(text.splitlines(), 1) if line.strip()]
else:
    home = os.environ.get('HOME', '')
    try:
        if not home.startswith('/'):
            raise KeyError('HOME')
        account = json.loads((pathlib.Path(home) / '.claude.json').read_text())['oauthAccount']['emailAddress']
        if not isinstance(account, str) or not account.strip():
            raise KeyError('emailAddress')
    except (OSError, ValueError, KeyError, TypeError):
        fail('IDENTITY source=account ABSENT')
    addresses.append((account.strip(), 'account', 1))
    git = subprocess.run(['git', '-C', repo, 'config', '--get-all', 'user.email'],
                         capture_output=True, text=True, timeout=60)
    if git.returncode not in (0, 1):
        fail(f'IDENTITY source=git error={git.returncode}')
    addresses += [(line.strip(), 'git', n) for n, line in enumerate(git.stdout.splitlines(), 1) if line.strip()]

tokens, seen, malformed = [], set(), []


def add(token, source, part):
    if token and token not in seen:
        seen.add(token)
        tokens.append((token, source, part))


for address, source, number in addresses:
    address = address.lower()
    local, _, domain = address.partition('@')
    if address.count('@') != 1 or not local or not domain:
        malformed.append(f'IDENTITY malformed_address={number}' + ('' if source == 'file' else f' source={source}'))
        continue
    add(address, source, 'address')
    add(local, source, 'local')
    for run in RUN.findall(local):
        add(run, source, 'local')
    add(domain, source, 'domain')
    for label in domain.split('.')[:-1]:
        for run in RUN.findall(label):
            add(run, source, 'domain')

print(f'IDENTITY tokens={len(tokens)}')
if malformed or not tokens:
    fail(*malformed)

by_length = sorted(range(len(tokens)), key=lambda i: -len(tokens[i][0]))


def masked(path):
    for i in by_length:
        path = re.sub(re.escape(tokens[i][0]), f'<token-{i + 1}>', path, flags=re.IGNORECASE)
    return path


bad = False
with tempfile.TemporaryDirectory() as scratch:
    os.chmod(scratch, 0o700)
    pattern = os.path.join(scratch, 'pattern')
    for index, (token, source, part) in enumerate(tokens, 1):
        fd = os.open(pattern, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, 'w') as handle:
            handle.write(token + '\n')
        result = subprocess.run(['rg', '--no-config', '-uuu', '-i', '-F', '-l', '-0', '-f', pattern, '--', directory],
                                capture_output=True, timeout=300)
        if result.returncode == 1:
            continue
        if result.returncode != 0:
            print(f'IDENTITY rg_error={result.returncode}')
            bad = True
            continue
        for path in sorted(p for p in result.stdout.split(b'\0') if p):
            relative = os.path.relpath(os.fsdecode(path), directory)
            print(f'IDENTITY hit token={index} source={source} part={part} file={masked(relative)}')
            bad = True

# rg -l searches contents only, so the names of every file and directory are
# judged here.
for parent, dirnames, filenames in os.walk(directory):
    for name in sorted(dirnames + filenames):
        relative = os.path.relpath(os.path.join(parent, name), directory)
        for index, (token, source, part) in enumerate(tokens, 1):
            if token in name.lower():
                print(f'IDENTITY hit token={index} source={source} part={part} file={masked(relative)} in=name')
                bad = True

print(f"IDENTITY verdict={'fail' if bad else 'pass'}")
sys.exit(1 if bad else 0)
PY
}

# ---------------------------------------------------------------------------
# --live-run, --handoff, --defects (50-25): judge 50-20's phase record
# (50-LIVE-RUN.md). Each prints one `<LABEL> <problem>` line per problem, then
# `<LABEL> verdict=pass|fail`, and exits 0 only on pass.
# ---------------------------------------------------------------------------
LIVE_RUN_FRAGMENTS=('does not establish' 'verdict=' 'stream' 'capture' 'unverified on macOS' 'V-28' 'V-29' 'bin_sha256')
LIVE_RUN_CRITERIA=8
HANDOFF_CHECK_RE='^    bash -c .R=.*changelog_3_0_0_lines=.*$'
HANDOFF_INDENT='    '
HANDOFF_FRAGMENTS=('## Release handoff (3.0.0)' 'gh pr merge' '--merge' 'never squash'
    'docs(50-17): state socket supervision is unverified on macOS' 'gh release edit v3.0.0')
DEFECTS_NONE='no non-blocking defect found'

# judge_found <label> <rg exit code> <what>: 0 when rg found <what>; otherwise
# prints `<label> missing=<what>` (rg 1) or `<label> rg_error=<rc> ...` and
# returns 1.
judge_found() {
    case "$2" in
        0) return 0 ;;
        1) echo "$1 missing=$3" ;;
        *) echo "$1 rg_error=$2 search=$3" ;;
    esac
    return 1
}

# judge_verdict <label> <bad>: print the verdict line; exit status is the verdict.
judge_verdict() {
    if [ "$2" -eq 0 ]; then
        echo "$1 verdict=pass"
    else
        echo "$1 verdict=fail"
        return 1
    fi
}

live_run_check() {
    local file="$1" number="${2:-}" bad=0 frag n rc
    if [ ! -f "$file" ]; then
        echo "LIVE_RUN file=ABSENT $file"
        echo "LIVE_RUN verdict=fail"
        return 1
    fi
    for frag in "${LIVE_RUN_FRAGMENTS[@]}"; do
        rc=0
        rg --no-config -q -F -e "$frag" -- "$file" || rc=$?
        judge_found LIVE_RUN "$rc" "$frag" || bad=1
    done
    for ((n = 1; n <= LIVE_RUN_CRITERIA; n++)); do
        rc=0
        rg --no-config -q -e "^\| *(criterion )?$n *\|" -- "$file" || rc=$?
        judge_found LIVE_RUN "$rc" "criterion-row-$n" || bad=1
    done
    # --attempt <N> (50-29): attempt N's own gate verdict is quoted; attempt=<N>
    # ends at a non-digit, so attempt=20 does not stand in for attempt=2.
    if [ -n "$number" ]; then
        rc=0
        rg --no-config -q -e "ATTEMPT_GATE verdict=shipped attempt=$number([^0-9]|\$)" \
            -e 'ATTEMPT_GATE verdict=cap_reached' -- "$file" || rc=$?
        judge_found LIVE_RUN "$rc" "attempt-$number-verdict" || bad=1
    fi
    judge_verdict LIVE_RUN "$bad"
}

handoff_check() {
    local file="$1" plan="$2" bad=0 lines="" count=0 frag rc
    rc=0
    lines="$(rg --no-config -e "$HANDOFF_CHECK_RE" -- "$plan")" || rc=$?
    if [ "$rc" -gt 1 ]; then
        echo "HANDOFF rg_error=$rc search=check-line plan=$plan"
        bad=1
    else
        [ -z "$lines" ] || count="$(printf '%s\n' "$lines" | wc -l)"
        if [ "$count" -ne 1 ]; then
            echo "HANDOFF check_lines=$count plan=$plan"
            bad=1
        fi
    fi
    if [ ! -f "$file" ]; then
        echo "HANDOFF file=ABSENT $file"
        echo "HANDOFF verdict=fail"
        return 1
    fi
    if [ "$bad" -eq 0 ]; then
        rc=0
        rg --no-config -q -F -e "${lines#"$HANDOFF_INDENT"}" -- "$file" || rc=$?
        judge_found HANDOFF "$rc" check-line || bad=1
    fi
    for frag in "${HANDOFF_FRAGMENTS[@]}"; do
        rc=0
        rg --no-config -q -F -e "$frag" -- "$file" || rc=$?
        judge_found HANDOFF "$rc" "$frag" || bad=1
    done
    judge_verdict HANDOFF "$bad"
}

defects_check() {
    local file="$1" roadmap="$2" bad=0 rdir rbase diff ids id rc
    if [ ! -f "$file" ]; then
        echo "DEFECTS file=ABSENT $file"
        echo "DEFECTS verdict=fail"
        return 1
    fi
    if [ ! -f "$roadmap" ]; then
        echo "DEFECTS roadmap=ABSENT $roadmap"
        echo "DEFECTS verdict=fail"
        return 1
    fi
    rdir="$(dirname -- "$roadmap")"
    rbase="$(basename -- "$roadmap")"
    rc=0
    diff="$(git -C "$rdir" diff --no-color --no-ext-diff --unified=0 HEAD -- "$rbase")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "DEFECTS git=FAILED rc=$rc"
        echo "DEFECTS verdict=fail"
        return 1
    fi
    # A path git does not track diffs as empty; require the roadmap at HEAD so
    # a wrong path cannot pass as "no heading added".
    if ! git -C "$rdir" cat-file -e "HEAD:./$rbase" 2>/dev/null; then
        echo "DEFECTS roadmap=NOT_AT_HEAD $roadmap"
        echo "DEFECTS verdict=fail"
        return 1
    fi
    ids="$(printf '%s\n' "$diff" | sed -n 's/^+### Phase \(999\.[0-9][0-9]*\):.*/\1/p')"
    if [ -n "$ids" ]; then
        while IFS= read -r id; do
            rc=0
            rg --no-config -q -e "(^|[^0-9])${id//./\\.}([^0-9]|\$)" -- "$file" || rc=$?
            case "$rc" in
                0) echo "DEFECTS referenced=$id" ;;
                1) echo "DEFECTS unreferenced=$id"; bad=1 ;;
                *) echo "DEFECTS rg_error=$rc search=$id"; bad=1 ;;
            esac
        done <<<"$ids"
    else
        echo "DEFECTS added=0"
        rc=0
        rg --no-config -q -i -F -e "$DEFECTS_NONE" -- "$file" || rc=$?
        judge_found DEFECTS "$rc" "$DEFECTS_NONE" || bad=1
    fi
    judge_verdict DEFECTS "$bad"
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

# --- Task 2 fixtures --------------------------------------------------------
ST_LIVE_CASES=(alive-stop-stream alive-stop-capture kill9-stale-stop kill9-at-gate stop-races-completion)

# A fault dir holding the full D-17/R-10 case set with bin_sha256 <sha>; in
# directory <dir> when given, else a fresh one. Prints its path.
st_full_fault_dir() {
    local sha="$1" dir="${2:-}" name
    if [ -z "$dir" ]; then
        dir="$(st_fault_dir "$sha")"
    else
        mkdir -p "$dir"
        printf '%s\n' "$sha" >"$dir/bin_sha256"
    fi
    for name in "${ST_LIVE_CASES[@]}"; do
        st_record "$name" "$sha" >"$dir/case-$name.json"
    done
    st_pointer "$sha" >"$dir/case-publish-unlink-window.json"
    printf '%s\n' "$dir"
}

st_fault_unmet() {
    local dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_record alive-stop-stream "$ST_SHA" '.expected_met = false' >"$dir/case-alive-stop-stream.json"
    expect 1 'FAULT verdict=fail' bash "$SELF" --fault "$dir" &&
        expect_re 1 'FAULT case=alive-stop-stream FAIL .*expectation not met.*' bash "$SELF" --fault "$dir"
}

st_require_all_window_live() {
    local dir
    dir="$(st_full_fault_dir "$ST_SHA")"
    expect 0 'FAULT complete=yes' bash "$SELF" --fault "$dir" --require-all || return 1
    st_record publish-unlink-window "$ST_SHA" >"$dir/case-publish-unlink-window.json"
    expect 1 'FAULT verdict=fail' bash "$SELF" --fault "$dir" --require-all &&
        expect_re 1 'FAULT complete=no missing=publish-unlink-window' bash "$SELF" --fault "$dir" --require-all &&
        expect 1 'FAULT case=publish-unlink-window ok live' bash "$SELF" --fault "$dir" --require-all
}

st_require_all_twice() {
    local dir
    dir="$(st_full_fault_dir "$ST_SHA")"
    expect 0 'FAULT complete=yes' bash "$SELF" --fault "$dir" --require-all || return 1
    cp -- "$dir/case-alive-stop-stream.json" "$dir/case-alive-stop-stream-again.json"
    expect 1 'FAULT complete=no duplicate=alive-stop-stream' bash "$SELF" --fault "$dir" --require-all &&
        expect 1 'FAULT verdict=fail' bash "$SELF" --fault "$dir" --require-all
}

st_require_all_partial() {
    local dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_record alive-stop-stream "$ST_SHA" >"$dir/case-alive-stop-stream.json"
    expect 0 'FAULT verdict=pass' bash "$SELF" --fault "$dir" &&
        expect_re 1 'FAULT complete=no( .*)?' bash "$SELF" --fault "$dir" --require-all &&
        expect 1 'FAULT verdict=fail' bash "$SELF" --fault "$dir" --require-all
}

st_pointer_not_live() {
    local dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_pointer "$ST_SHA" >"$dir/case-publish-unlink-window.json"
    expect 0 'FAULT case=publish-unlink-window ok pointer' bash "$SELF" --fault "$dir" &&
        expect_absent 'FAULT case=publish-unlink-window ok live' bash "$SELF" --fault "$dir"
}

st_pointer_no_fixture() {
    local dir
    dir="$(st_fault_dir "$ST_SHA")"
    st_pointer "$ST_SHA" | jq '.fixture = ""' >"$dir/case-publish-unlink-window.json"
    expect 1 'FAULT verdict=fail' bash "$SELF" --fault "$dir" &&
        expect_re 1 'FAULT case=publish-unlink-window FAIL .*names no fixture.*' bash "$SELF" --fault "$dir"
}

# --- attempt fixtures -------------------------------------------------------

# One stage_launched event line: <label> <launch id> [<socket path>].
st_launch_event() {
    local label="$1" id="$2" socket="${3:-/tmp/df50f.AAAAAA/c/sock/0123456789abcdef-01-$2.sock}"
    jq -n -c --arg l "$label" --arg id "$id" --arg s "$socket" '{v: 1, ts: 1, phase: 1,
        event: "stage_launched", stage: "define", agent: "claude", supervisor_pid: 42,
        socket_path: $s, launch_id: $id, launch: $l}'
}

# An attempt dir whose collected events are stdin; an empty supervisor/.
st_attempt_ev() {
    local ev
    ev="$(mktemp -d "$ST_TMP/attempt.XXXXXX")"
    mkdir -p "$ev/collected/devflow" "$ev/supervisor"
    cat >"$ev/collected/devflow/events.jsonl"
    printf '%s\n' "$ev"
}

st_good_events() {
    printf '%s\n' '{"v":1,"ts":1,"phase":1,"event":"workflow_started","commit":"x"}'
    st_launch_event stream 1111111111111111
    st_launch_event capture 2222222222222222
}

st_attempt() {
    bash "$SELF" --attempt "$1" --p49-check "$ST_TMP/p49-pass.sh"
}

st_attempt_zero_sidecars() {
    local ev
    ev="$(st_good_events | st_attempt_ev)"
    [ -z "$(ls -A "$ev/supervisor")" ] || { echo "fixture broken: supervisor/ is not empty"; return 1; }
    expect 0 'ATTEMPT verdict=pass' st_attempt "$ev" &&
        expect 0 'ATTEMPT sidecars_left=0' st_attempt "$ev"
}

st_attempt_no_capture() {
    local ev
    ev="$({ st_launch_event stream 1111111111111111; st_launch_event stream 3333333333333333; } | st_attempt_ev)"
    expect 1 'ATTEMPT verdict=fail' st_attempt "$ev" &&
        expect 1 'ATTEMPT check=both launch shapes FAIL missing capture' st_attempt "$ev"
}

st_attempt_monitor_pid() {
    local ev
    ev="$({ st_good_events; printf '%s\n' '{"v":1,"ts":2,"phase":1,"event":"stage_launched","stage":"plan","monitor_pid":7,"socket_path":"/tmp/x/sock/a-01-4444444444444444.sock","launch_id":"4444444444444444","launch":"stream"}'; } | st_attempt_ev)"
    expect 1 'ATTEMPT verdict=fail' st_attempt "$ev" &&
        expect 1 'ATTEMPT check=no monitor_pid key FAIL an event carries monitor_pid' st_attempt "$ev"
}

# A socket path of exactly <bytes> bytes whose file name ends with -<id>.sock.
st_socket_of() {
    local bytes="$1" id="$2" pad
    pad="$(printf 'x%.0s' $(seq 1 $((bytes - 27))))"
    printf '/tmp/%s-%s.sock\n' "$pad" "$id"
}

st_attempt_108() {
    local id=5555555555555555 ok_ev bad_ev s107 s108
    s107="$(st_socket_of 107 "$id")"
    s108="$(st_socket_of 108 "$id")"
    [ "${#s107}" -eq 107 ] && [ "${#s108}" -eq 108 ] || { echo "fixture broken: ${#s107}/${#s108} bytes"; return 1; }
    ok_ev="$({ st_launch_event stream "$id" "$s107"; st_launch_event capture 2222222222222222; } | st_attempt_ev)"
    bad_ev="$({ st_launch_event stream "$id" "$s108"; st_launch_event capture 2222222222222222; } | st_attempt_ev)"
    expect 0 'ATTEMPT verdict=pass' st_attempt "$ok_ev" &&
        expect 1 'ATTEMPT verdict=fail' st_attempt "$bad_ev" &&
        expect_re 1 'ATTEMPT check=every launch names its socket, label and id FAIL .*108 bytes.*' st_attempt "$bad_ev"
}

st_attempt_id_not_in_socket() {
    local ev
    ev="$({ st_launch_event stream 1111111111111111 /tmp/df50f.AAAAAA/c/sock/0123456789abcdef-01-9999999999999999.sock
        st_launch_event capture 2222222222222222; } | st_attempt_ev)"
    expect 1 'ATTEMPT verdict=fail' st_attempt "$ev" &&
        expect_re 1 'ATTEMPT check=every launch names its socket, label and id FAIL .*does not end with -1111111111111111.sock.*' st_attempt "$ev"
}

# --- attempt-gate fixtures --------------------------------------------------

# Attempt <n> under <home>: outcome shipped|stopped, collected events, DONE,
# MANIFEST.sha256, attempt.json (bin_sha256 ST_SHA) and verdict.json.
st_gate_attempt() {
    local home="$1" n="$2" outcome="$3" note="${4-fixture stop}" ev
    ev="$home/attempt-$n"
    mkdir -p "$ev/collected/devflow" "$ev/supervisor"
    {
        st_good_events
        [ "$outcome" != shipped ] || printf '%s\n' '{"v":1,"ts":9,"phase":1,"event":"workflow_shipped"}'
    } >"$ev/collected/devflow/events.jsonl"
    (cd "$ev/collected" && find . -type f ! -name MANIFEST.sha256 -printf '%P\n' | sort | xargs sha256sum >../manifest.tmp)
    mv -- "$ev/manifest.tmp" "$ev/collected/MANIFEST.sha256"
    printf 'done\n' >"$ev/collected/DONE"
    jq -n --argjson n "$n" --arg s "$ST_SHA" '{attempt: $n, bin_sha256: $s}' >"$ev/attempt.json"
    if [ "$outcome" = shipped ]; then
        jq -n --argjson n "$n" '{attempt: $n, outcome: "shipped", workflow_shipped: true, stop_note: null, state_present: false}' >"$ev/verdict.json"
    else
        jq -n --argjson n "$n" --arg note "$note" '{attempt: $n, outcome: "stopped", workflow_shipped: false, stop_note: $note, state_present: false}' >"$ev/verdict.json"
    fi
    printf '%s\n' "$n" >"$home/current-attempt"
}

st_gate() {
    bash "$SELF" --attempt-gate "$1" --p49-check "$ST_TMP/p49-pass.sh"
}

st_gate_ships() {
    local home
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_gate_attempt "$home" 1 shipped
    st_full_fault_dir "$ST_SHA" "$home/fault-1" >/dev/null
    expect 0 'ATTEMPT_GATE verdict=shipped attempt=1' st_gate "$home" &&
        expect 0 "ATTEMPT_GATE fault=fault-1 bin_sha256=$ST_SHA" st_gate "$home"
}

st_gate_other_binary() {
    local home
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_gate_attempt "$home" 1 shipped
    st_full_fault_dir "$ST_OTHER_SHA" "$home/fault-1" >/dev/null
    expect 0 'FAULT verdict=pass' bash "$SELF" --fault "$home/fault-1" --require-all &&
        expect 1 "ATTEMPT_GATE verdict=not_ready attempt=1 reason=no passing fault arm on attempt 1's binary" st_gate "$home"
}

st_gate_partial() {
    local home
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_gate_attempt "$home" 1 shipped
    mkdir -p "$home/fault-1"
    printf '%s\n' "$ST_SHA" >"$home/fault-1/bin_sha256"
    st_record alive-stop-stream "$ST_SHA" >"$home/fault-1/case-alive-stop-stream.json"
    expect 0 'FAULT verdict=pass' bash "$SELF" --fault "$home/fault-1" &&
        expect 1 "ATTEMPT_GATE verdict=not_ready attempt=1 reason=no passing fault arm on attempt 1's binary" st_gate "$home"
}

st_gate_cap() {
    local home
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_gate_attempt "$home" 1 stopped
    st_gate_attempt "$home" 2 stopped
    expect 1 'ATTEMPT_GATE verdict=not_ready attempt=2 reason=2 of 3 stopped attempts have a note' st_gate "$home" || return 1
    # Control: a third stop without a note does not count toward the cap.
    st_gate_attempt "$home" 3 stopped ''
    expect 1 'ATTEMPT_GATE verdict=not_ready attempt=3 reason=2 of 3 stopped attempts have a note' st_gate "$home" || return 1
    st_gate_attempt "$home" 3 stopped
    expect 0 'ATTEMPT_GATE verdict=cap_reached stopped_attempts=3' st_gate "$home"
}

# --attempt-gate --current-source against scratch checkout <repo>.
st_gate_cs() {
    st_git_env bash "$SELF" --attempt-gate "$1" --p49-check "$ST_TMP/p49-pass.sh" --current-source --repo "$2"
}

# A shipped attempt 1 under a fresh home whose fault-1 records <repo>'s HEAD
# crate tree; prints the home.
st_gate_cs_home() {
    local repo="$1" home
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_gate_attempt "$home" 1 shipped
    st_fc_dir "$home" fault-1 "$repo" true
    printf '%s\n' "$home"
}

st_gate_cs_ships() {
    local repo home
    repo="$(st_src_repo)"
    home="$(st_gate_cs_home "$repo")"
    expect 0 'ATTEMPT_GATE current_source=yes' st_gate_cs "$home" "$repo" &&
        expect 0 'ATTEMPT_GATE verdict=shipped attempt=1' st_gate_cs "$home" "$repo" &&
        expect 0 "ATTEMPT_GATE fault=fault-1 bin_sha256=$ST_SHA" st_gate_cs "$home" "$repo"
}

st_gate_cs_other_source() {
    local repo home
    repo="$(st_src_repo)"
    home="$(st_gate_cs_home "$repo")"
    cp -- "$repo/.git/old-ids" "$home/fault-1/src_tree"
    # Control: the older ids really differ from HEAD's, so the refusal below
    # is the source check's, not a fixture that planted HEAD again.
    if cmp -s "$home/fault-1/src_tree" <(st_git -C "$repo" rev-parse HEAD:crates HEAD:Cargo.toml HEAD:Cargo.lock HEAD:rust-toolchain.toml); then
        echo "fixture broken: the older commit's ids equal HEAD's"
        return 1
    fi
    expect 1 "ATTEMPT_GATE verdict=not_ready attempt=1 reason=no passing fault arm on attempt 1's binary from the current crate source" \
        st_gate_cs "$home" "$repo" || return 1
    expect_absent 'ATTEMPT_GATE current_source=yes' st_gate_cs "$home" "$repo" || return 1
    # Control: without the flag the same home ships, so the flag is what refuses.
    expect 0 'ATTEMPT_GATE verdict=shipped attempt=1' st_gate "$home"
}

st_gate_cs_uncommitted() {
    local repo home
    repo="$(st_src_repo)"
    home="$(st_gate_cs_home "$repo")"
    # Control: the same home ships while the crate tree is clean.
    expect 0 'ATTEMPT_GATE verdict=shipped attempt=1' st_gate_cs "$home" "$repo" || return 1
    st_dirty "$repo"
    expect_re 1 'ATTEMPT_GATE verdict=not_ready attempt=1 reason=uncommitted crate source.*' st_gate_cs "$home" "$repo"
}

# --- fault-current fixtures -------------------------------------------------

# Fault dir <home>/<name>: the full case set, src_tree = <repo>'s HEAD ids,
# src_matches_checkout = <flag>.
st_fc_dir() {
    local home="$1" name="$2" repo="$3" flag="$4"
    st_full_fault_dir "$ST_SHA" "$home/$name" >/dev/null
    st_git -C "$repo" rev-parse HEAD:crates HEAD:Cargo.toml HEAD:Cargo.lock HEAD:rust-toolchain.toml \
        >"$home/$name/src_tree"
    printf '%s\n' "$flag" >"$home/$name/src_matches_checkout"
}

st_fc() {
    st_git_env bash "$SELF" --fault-current "$1" --repo "$2"
}

st_fc_picks_current() {
    local repo home
    repo="$(st_src_repo)"
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_fc_dir "$home" fault-1 "$repo" true
    cp -r -- "$home/fault-1" "$home/fault-2"
    cp -- "$repo/.git/old-ids" "$home/fault-2/src_tree"
    expect 0 'FAULT_CURRENT dir=fault-1' st_fc "$home" "$repo" &&
        expect 0 'FAULT_CURRENT verdict=pass' st_fc "$home" "$repo"
}

st_fc_only_other() {
    local repo home
    repo="$(st_src_repo)"
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_fc_dir "$home" fault-1 "$repo" true
    cp -- "$repo/.git/old-ids" "$home/fault-1/src_tree"
    expect 1 'FAULT_CURRENT dir=ABSENT' st_fc "$home" "$repo" &&
        expect 1 'FAULT_CURRENT verdict=fail' st_fc "$home" "$repo"
}

st_fc_newest_only() {
    local repo home
    repo="$(st_src_repo)"
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_fc_dir "$home" fault-9 "$repo" true
    st_fc_dir "$home" fault-10 "$repo" true
    rm -- "$home/fault-10/case-kill9-at-gate.json"
    expect 1 'FAULT_CURRENT dir=fault-10' st_fc "$home" "$repo" &&
        expect 1 'FAULT_CURRENT verdict=fail' st_fc "$home" "$repo"
}

st_fc_uncommitted() {
    local repo home
    repo="$(st_src_repo)"
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_fc_dir "$home" fault-1 "$repo" true
    # fault-2 records a dirty checkout (flag false): --src-check alone would
    # accept it once the tree is dirty, so only the refusal can fail the run.
    st_fc_dir "$home" fault-2 "$repo" false
    expect 0 'FAULT_CURRENT dir=fault-1' st_fc "$home" "$repo" || return 1
    st_dirty "$repo"
    expect 0 'SRC tree_is_head=yes recorded_matches=false expected_matches=false' \
        st_git_env bash "$SELF" --src-check "$home/fault-2" --repo "$repo" || return 1
    expect 1 'FAULT_CURRENT uncommitted_crate_source=1' st_fc "$home" "$repo" &&
        expect 1 'FAULT_CURRENT verdict=fail' st_fc "$home" "$repo"
}

st_fc_planning_commit() {
    local repo home before
    repo="$(st_src_repo)"
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_fc_dir "$home" fault-1 "$repo" true
    before="$(st_git -C "$repo" rev-parse HEAD)"
    mkdir -p "$repo/.planning"
    printf 'note\n' >"$repo/.planning/notes.md"
    st_git -C "$repo" add .planning/notes.md
    st_git -C "$repo" commit -q -m 'docs: planning only'
    printf 'scratch\n' >"$repo/untracked.txt"
    [ "$(st_git -C "$repo" rev-parse HEAD)" != "$before" ] || { echo "fixture broken: HEAD did not move"; return 1; }
    expect 0 'FAULT_CURRENT dir=fault-1' st_fc "$home" "$repo" &&
        expect 0 'FAULT_CURRENT verdict=pass' st_fc "$home" "$repo"
}

st_fc_mismatching_checkout() {
    local repo home
    repo="$(st_src_repo)"
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_fc_dir "$home" fault-1 "$repo" false
    expect 1 'FAULT_CURRENT dir=ABSENT' st_fc "$home" "$repo" &&
        expect 1 'FAULT_CURRENT verdict=fail' st_fc "$home" "$repo"
}

st_fc_partial() {
    local repo home name
    repo="$(st_src_repo)"
    home="$(mktemp -d "$ST_TMP/p50.XXXXXX")"
    st_fc_dir "$home" fault-1 "$repo" true
    for name in alive-stop-capture kill9-stale-stop kill9-at-gate stop-races-completion publish-unlink-window; do
        rm -- "$home/fault-1/case-$name.json"
    done
    expect 1 'FAULT_CURRENT dir=fault-1' st_fc "$home" "$repo" &&
        expect_re 1 'FAULT complete=no( .*)?' st_fc "$home" "$repo" &&
        expect 1 'FAULT_CURRENT verdict=fail' st_fc "$home" "$repo"
}

# --published cases (50-25). Each call runs with HOME set to ST_PUB_HOME, a
# value no planted file names unless the case plants it.
ST_PUB_HOME=/nonexistent/df50-selftest-home

st_pub() {
    env HOME="$ST_PUB_HOME" bash "$SELF" --published "$1"
}

# A sanitized published tree; prints its path. fault-1/case-notes.txt holds a
# `FAULT verdict=pass` line outside any verdict-*.txt file, so only the
# verdict file's own line can satisfy the fault check.
st_pub_tree() {
    local dir
    dir="$(mktemp -d "$ST_TMP/pub.XXXXXX")"
    mkdir -p "$dir/fault-1" "$dir/attempt-1"
    st_record alive-stop-stream "$ST_SHA" >"$dir/fault-1/case-alive-stop-stream.json"
    printf 'FAULT case=alive-stop-stream ok live\nFAULT bin_sha256=%s\nFAULT verdict=pass\n' "$ST_SHA" \
        >"$dir/fault-1/verdict-fault.txt"
    printf 'quoted from an older run: FAULT verdict=pass\nFAULT verdict=pass\n' >"$dir/fault-1/case-notes.txt"
    printf 'ATTEMPT sidecars_left=0\nATTEMPT verdict=pass\n' >"$dir/attempt-1/verdict-attempt.txt"
    printf 'ATTEMPT_GATE fault=fault-1 bin_sha256=%s\nATTEMPT_GATE verdict=shipped attempt=1\n' "$ST_SHA" \
        >"$dir/attempt-1/verdict-attempt-gate.txt"
    printf 'project=<HOME>/.cache/devflow-phase50/attempt-1/project\n' >"$dir/attempt-1/.redacted-env"
    printf '%s\n' "$dir"
}

st_pub_pass() {
    local dir
    dir="$(st_pub_tree)"
    expect 0 'PUBLISHED verdict=pass' st_pub "$dir"
}

st_pub_home_file() {
    local dir
    dir="$(st_pub_tree)"
    printf 'project=%s/.cache/devflow-phase50/attempt-1/project\n' "$ST_PUB_HOME" >"$dir/attempt-1/start.stderr"
    expect 1 'PUBLISHED verdict=fail' st_pub "$dir" &&
        expect 1 "PUBLISHED names_home=$dir/attempt-1/start.stderr" st_pub "$dir"
}

# The leak is hidden, listed in an .ignore file and binary (a NUL byte before
# the path), so dropping any one of rg's three -u flags misses it.
st_pub_home_hidden() {
    local dir
    dir="$(st_pub_tree)"
    printf 'x\0%s/.cache\n' "$ST_PUB_HOME" >"$dir/attempt-1/.leak"
    printf '.leak\n' >"$dir/attempt-1/.ignore"
    expect 1 'PUBLISHED verdict=fail' st_pub "$dir" &&
        expect 1 "PUBLISHED names_home=$dir/attempt-1/.leak" st_pub "$dir"
}

st_pub_symlink() {
    local dir
    dir="$(st_pub_tree)"
    ln -s verdict-attempt.txt "$dir/attempt-1/link"
    expect 1 'PUBLISHED verdict=fail' st_pub "$dir" &&
        expect 1 "PUBLISHED symlink=$dir/attempt-1/link" st_pub "$dir"
}

st_pub_no_fault_pass() {
    local dir
    dir="$(st_pub_tree)"
    printf 'FAULT case=alive-stop-stream FAIL surviving member\nFAULT verdict=fail\n' >"$dir/fault-1/verdict-fault.txt"
    expect 1 'PUBLISHED verdict=fail' st_pub "$dir" &&
        expect 1 'PUBLISHED missing=fault-pass-verdict' st_pub "$dir"
}

st_pub_no_shipped() {
    local dir
    dir="$(st_pub_tree)"
    printf 'ATTEMPT_GATE verdict=not_ready attempt=1 reason=no fault directory\n' \
        >"$dir/attempt-1/verdict-attempt-gate.txt"
    expect 1 'PUBLISHED verdict=fail' st_pub "$dir" &&
        expect 1 'PUBLISHED missing=attempt-gate-verdict' st_pub "$dir"
}

# The attempt directory is renamed, so its gate line stays in the tree, and an
# `attempt-notes` file (not a directory) must not stand in for it.
st_pub_no_attempt_dir() {
    local dir
    dir="$(st_pub_tree)"
    mv -- "$dir/attempt-1" "$dir/run-1"
    printf 'notes\n' >"$dir/attempt-notes"
    expect 1 'PUBLISHED verdict=fail' st_pub "$dir" &&
        expect 1 'PUBLISHED missing=attempt-dir' st_pub "$dir"
}

# --live-run cases (50-25). A complete record, one fragment or row per line so
# a case can delete exactly one. Rows use both `| N |` and `| criterion N |`,
# and `| 17 |` must not stand in for criterion 7.
st_lr_record() {
    local file
    file="$(mktemp "$ST_TMP/live-run.XXXXXX")"
    cat >"$file" <<'MD'
# Phase 50 live run
Quoted: `FAULT verdict=pass`
Recorded bin_sha256 per attempt.
Both launch shapes ran: stream
and capture.
Socket supervision is unverified on macOS.
V-28 holds.
V-29 holds.
## What this evidence does not establish
| Criterion | Evidence |
|---|---|
| 1 | a |
| criterion 2 | b |
|3|c|
| 4 | d |
| 5 | e |
| 6 | f |
| 7 | g |
| 8 | h |
| 17 | not a criterion |
MD
    printf '%s\n' "$file"
}

st_lr_pass() {
    local file
    file="$(st_lr_record)"
    expect 0 'LIVE_RUN verdict=pass' bash "$SELF" --live-run "$file"
}

st_lr_missing_fragment() {
    local file
    file="$(st_lr_record)"
    sed -i '/^V-29 holds\.$/d' "$file"
    expect 1 'LIVE_RUN verdict=fail' bash "$SELF" --live-run "$file" &&
        expect 1 'LIVE_RUN missing=V-29' bash "$SELF" --live-run "$file"
}

st_lr_missing_row() {
    local file
    file="$(st_lr_record)"
    sed -i '/^| 7 | g |$/d' "$file"
    expect 1 'LIVE_RUN verdict=fail' bash "$SELF" --live-run "$file" &&
        expect 1 'LIVE_RUN missing=criterion-row-7' bash "$SELF" --live-run "$file"
}

st_lr_absent() {
    local file="$ST_TMP/no-such-live-run.md"
    expect 1 'LIVE_RUN verdict=fail' bash "$SELF" --live-run "$file" &&
        expect 1 "LIVE_RUN file=ABSENT $file" bash "$SELF" --live-run "$file"
}

# --live-run --attempt cases (50-29, T-50-109). The complete record quoting
# attempt 2's gate verdict passes `--attempt 2`; quoting only attempt 1's (or
# attempt 20's, which must not stand in for 2) fails it.
st_lr_attempt_pass() {
    local file
    file="$(st_lr_record)"
    printf '%s\n' 'Quoted: ATTEMPT_GATE verdict=shipped attempt=2' >>"$file"
    expect 0 'LIVE_RUN verdict=pass' bash "$SELF" --live-run "$file" --attempt 2
}

st_lr_attempt_earlier() {
    local file
    file="$(st_lr_record)"
    printf '%s\n' 'Quoted: ATTEMPT_GATE verdict=shipped attempt=1' \
        'Quoted: ATTEMPT_GATE verdict=shipped attempt=20' >>"$file"
    expect 0 'LIVE_RUN verdict=pass' bash "$SELF" --live-run "$file" &&
        expect 1 'LIVE_RUN verdict=fail' bash "$SELF" --live-run "$file" --attempt 2 &&
        expect 1 'LIVE_RUN missing=attempt-2-verdict' bash "$SELF" --live-run "$file" --attempt 2
}

# --handoff cases (50-25; R6-D). A synthetic plan holding one indented check
# line, and a record holding that line unindented plus every handoff fragment.
st_ho_plan() {
    local plan
    plan="$(mktemp "$ST_TMP/plan.XXXXXX")"
    cat >"$plan" <<'MD'
<context>
Release handoff check: prints `changelog_3_0_0_lines=<N>`.

    bash -c 'R="${1:-origin/main}"; n=$(git show "${R}:CHANGELOG.md" | wc -l); echo "ref=$R changelog_3_0_0_lines=$n"' _ origin/main
</context>
MD
    printf '%s\n' "$plan"
}

st_ho_record() {
    local file
    file="$(mktemp "$ST_TMP/handoff.XXXXXX")"
    cat >"$file" <<'MD'
## Release handoff (3.0.0)
Merge the phase PR with `gh pr merge <PR> --merge --match-head-commit <sha>`;
never squash, never auto-merge.
50-17's commit `docs(50-17): state socket supervision is unverified on macOS` carries the note.
Run:
```bash
bash -c 'R="${1:-origin/main}"; n=$(git show "${R}:CHANGELOG.md" | wc -l); echo "ref=$R changelog_3_0_0_lines=$n"' _ origin/main
```
Repair: `gh release edit v3.0.0 --notes-file <file>`.
MD
    printf '%s\n' "$file"
}

st_ho() {
    bash "$SELF" --handoff "$1" --plan "$2"
}

st_ho_pass() {
    local plan file
    plan="$(st_ho_plan)"
    file="$(st_ho_record)"
    expect 0 'HANDOFF verdict=pass' st_ho "$file" "$plan"
}

# The record copies the real 50-20 plan's check line, taken with sed (not the
# judge's rg) and required to be exactly one line.
st_ho_real_plan() {
    local plan file line
    plan="$(dirname -- "$T")/50-20-PLAN.md"
    line="$(sed -n 's/^    \(bash -c .R=.*changelog_3_0_0_lines=.*\)$/\1/p' "$plan")"
    [ -n "$line" ] && [ "$(printf '%s\n' "$line" | wc -l)" -eq 1 ] ||
        { echo "fixture broken: not exactly one check line in $plan"; return 1; }
    file="$(st_ho_record)"
    printf '%s\n' "$line" >>"$file"
    expect 0 'HANDOFF verdict=pass' st_ho "$file" "$plan"
}

st_ho_altered() {
    local plan file
    plan="$(st_ho_plan)"
    file="$(st_ho_record)"
    sed -i 's/_ origin\/main$/_ origin\/mian/' "$file"
    expect 1 'HANDOFF verdict=fail' st_ho "$file" "$plan" &&
        expect 1 'HANDOFF missing=check-line' st_ho "$file" "$plan"
}

st_ho_missing_fragment() {
    local plan file
    plan="$(st_ho_plan)"
    file="$(st_ho_record)"
    sed -i '/^never squash, never auto-merge\.$/d' "$file"
    expect 1 'HANDOFF verdict=fail' st_ho "$file" "$plan" &&
        expect 1 'HANDOFF missing=never squash' st_ho "$file" "$plan"
}

# The check line is still in the plan, but indented by two spaces, not four.
st_ho_plan_without() {
    local plan file
    plan="$(st_ho_plan)"
    file="$(st_ho_record)"
    sed -i 's/^    bash -c /  bash -c /' "$plan"
    expect 1 'HANDOFF verdict=fail' st_ho "$file" "$plan" &&
        expect 1 "HANDOFF check_lines=0 plan=$plan" st_ho "$file" "$plan"
}

st_ho_plan_twice() {
    local plan file
    plan="$(st_ho_plan)"
    file="$(st_ho_record)"
    sed -i '/^    bash -c /p' "$plan"
    expect 1 'HANDOFF verdict=fail' st_ho "$file" "$plan" &&
        expect 1 "HANDOFF check_lines=2 plan=$plan" st_ho "$file" "$plan"
}

st_ho_absent() {
    local plan file="$ST_TMP/no-such-handoff.md"
    plan="$(st_ho_plan)"
    expect 1 'HANDOFF verdict=fail' st_ho "$file" "$plan" &&
        expect 1 "HANDOFF file=ABSENT $file" st_ho "$file" "$plan"
}

# --defects cases (50-25). A scratch repository whose committed ROADMAP.md
# holds backlog heading 999.66; GIT_CEILING_DIRECTORIES keeps git from
# finding a repository above the selftest scratch.
st_def() {
    st_git_env env GIT_CEILING_DIRECTORIES="$ST_TMP" bash "$SELF" --defects "$1" --roadmap "$2"
}

st_def_repo() {
    local repo
    repo="$(mktemp -d "$ST_TMP/def.XXXXXX")"
    st_git init -q "$repo"
    printf '# Roadmap\n\n### Phase 999.66: Older entry (BACKLOG)\n\nFound: earlier.\n' >"$repo/ROADMAP.md"
    st_git -C "$repo" add ROADMAP.md
    st_git -C "$repo" commit -q -m roadmap
    printf '%s\n' "$repo"
}

st_def_add_heading() {
    printf '\n### Phase 999.67: New defect (BACKLOG)\n\nFound: 2026-10-20.\n' >>"$1/ROADMAP.md"
}

st_def_pass_referenced() {
    local repo
    repo="$(st_def_repo)"
    st_def_add_heading "$repo"
    printf 'Defects: non-blocking, filed as 999.67.\n' >"$repo/record.md"
    expect 0 'DEFECTS verdict=pass' st_def "$repo/record.md" "$repo/ROADMAP.md" &&
        expect 0 'DEFECTS referenced=999.67' st_def "$repo/record.md" "$repo/ROADMAP.md"
}

# The record names 999.675 and 999.66, neither of which is the new 999.67.
st_def_fail_unreferenced() {
    local repo
    repo="$(st_def_repo)"
    st_def_add_heading "$repo"
    printf 'Defects: see 999.675 and 999.66.\n' >"$repo/record.md"
    expect 1 'DEFECTS verdict=fail' st_def "$repo/record.md" "$repo/ROADMAP.md" &&
        expect 1 'DEFECTS unreferenced=999.67' st_def "$repo/record.md" "$repo/ROADMAP.md"
}

# ROADMAP.md changes, but no backlog heading is added.
st_def_pass_none() {
    local repo
    repo="$(st_def_repo)"
    printf 'A note, not a heading.\n' >>"$repo/ROADMAP.md"
    printf 'No non-blocking defect found.\n' >"$repo/record.md"
    expect 0 'DEFECTS verdict=pass' st_def "$repo/record.md" "$repo/ROADMAP.md" &&
        expect 0 'DEFECTS added=0' st_def "$repo/record.md" "$repo/ROADMAP.md"
}

st_def_fail_no_statement() {
    local repo
    repo="$(st_def_repo)"
    printf 'Defects: none listed.\n' >"$repo/record.md"
    expect 1 'DEFECTS verdict=fail' st_def "$repo/record.md" "$repo/ROADMAP.md" &&
        expect 1 "DEFECTS missing=$DEFECTS_NONE" st_def "$repo/record.md" "$repo/ROADMAP.md"
}

st_def_outside_git() {
    local dir
    dir="$(mktemp -d "$ST_TMP/norepo.XXXXXX")"
    printf '# Roadmap\n\n### Phase 999.67: New defect (BACKLOG)\n' >"$dir/ROADMAP.md"
    printf 'No non-blocking defect found. 999.67\n' >"$dir/record.md"
    expect 1 'DEFECTS verdict=fail' st_def "$dir/record.md" "$dir/ROADMAP.md" &&
        expect_re 1 'DEFECTS git=FAILED rc=[0-9]+' st_def "$dir/record.md" "$dir/ROADMAP.md"
}

# --identity-scan cases (50-29). The fake address yields seven tokens, in
# this order: 1 the address; 2 alpha.bravo, 3 alpha, 4 bravo (local); 5
# charlie-delta.example, 6 charlie, 7 delta (domain). The last domain label
# (`example`) is not a token.
ST_ID_ADDRESS='alpha.bravo@charlie-delta.example'
ST_ID_TOKENS_RE='alpha|bravo|charlie|delta'

st_id_tokens() {
    local file
    file="$(mktemp "$ST_TMP/tokens.XXXXXX")"
    printf '\n%s\n\n' "$ST_ID_ADDRESS" >"$file"
    printf '%s\n' "$file"
}

# A tree holding none of the seven tokens (short pieces and the last domain
# label only); prints its path.
st_id_tree() {
    local dir
    dir="$(mktemp -d "$ST_TMP/id.XXXXXX")"
    mkdir -p "$dir/sub"
    printf 'alp bra cha del example.org\n' >"$dir/notes.txt"
    printf 'nothing here\n' >"$dir/sub/log.txt"
    printf '%s\n' "$dir"
}

st_id() {
    bash "$SELF" --identity-scan "$1" --tokens "$2"
}

st_id_clean() {
    local dir tokens
    dir="$(st_id_tree)"
    tokens="$(st_id_tokens)"
    expect 0 'IDENTITY verdict=pass' st_id "$dir" "$tokens" &&
        expect 0 'IDENTITY tokens=7' st_id "$dir" "$tokens"
}

st_id_local_case() {
    local dir tokens
    dir="$(st_id_tree)"
    tokens="$(st_id_tokens)"
    printf 'user=xxBravoyy\n' >"$dir/notes.txt"
    expect 1 'IDENTITY verdict=fail' st_id "$dir" "$tokens" &&
        expect 1 'IDENTITY hit token=4 source=file part=local file=notes.txt' st_id "$dir" "$tokens"
}

st_id_domain() {
    local dir tokens
    dir="$(st_id_tree)"
    tokens="$(st_id_tokens)"
    printf 'host delta.internal\n' >"$dir/sub/log.txt"
    expect 1 'IDENTITY verdict=fail' st_id "$dir" "$tokens" &&
        expect 1 'IDENTITY hit token=7 source=file part=domain file=sub/log.txt' st_id "$dir" "$tokens"
}

# Hidden, listed in an .ignore file and binary, so dropping any one of rg's
# three -u flags misses it.
st_id_hidden() {
    local dir tokens
    dir="$(st_id_tree)"
    tokens="$(st_id_tokens)"
    printf 'x\0CHARLIE\n' >"$dir/sub/.leak"
    printf '.leak\n' >"$dir/sub/.ignore"
    expect 1 'IDENTITY verdict=fail' st_id "$dir" "$tokens" &&
        expect 1 'IDENTITY hit token=6 source=file part=domain file=sub/.leak' st_id "$dir" "$tokens"
}

st_id_empty() {
    local dir tokens
    dir="$(st_id_tree)"
    tokens="$(mktemp "$ST_TMP/tokens.XXXXXX")"
    printf '\n\n' >"$tokens"
    expect 1 'IDENTITY tokens=0' st_id "$dir" "$tokens" &&
        expect 1 'IDENTITY verdict=fail' st_id "$dir" "$tokens"
}

# A file whose name holds a token: rg -l searches contents, so the name is
# judged separately and printed with the token masked.
st_id_name() {
    local dir tokens
    dir="$(st_id_tree)"
    tokens="$(st_id_tokens)"
    printf 'clean\n' >"$dir/report-Bravo.txt"
    expect 1 'IDENTITY verdict=fail' st_id "$dir" "$tokens" &&
        expect 1 'IDENTITY hit token=4 source=file part=local file=report-<token-4>.txt in=name' st_id "$dir" "$tokens"
}

# Run the scan with HOME at <home>, git's global config at <home>/gitconfig
# and no system config, against the scratch repository <repo>.
st_id_host() {
    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE -u GIT_COMMON_DIR -u GIT_CONFIG -u GIT_PREFIX \
        HOME="$1" XDG_CONFIG_HOME="$1/.config" GIT_CONFIG_GLOBAL="$1/gitconfig" GIT_CONFIG_NOSYSTEM=1 \
        bash "$SELF" --identity-scan "$3" --repo "$2"
}

# A scratch HOME with an empty git config; with `account`, also a
# .claude.json naming the fake address. Prints its path.
st_id_home() {
    local home
    home="$(mktemp -d "$ST_TMP/idhome.XXXXXX")"
    : >"$home/gitconfig"
    if [ "$1" = account ]; then
        printf '{"oauthAccount": {"emailAddress": "%s"}}\n' "$ST_ID_ADDRESS" >"$home/.claude.json"
    fi
    printf '%s\n' "$home"
}

st_id_account() {
    local home repo clean dir
    home="$(st_id_home account)"
    repo="$(st_src_repo)"
    clean="$(st_id_tree)"
    dir="$(st_id_tree)"
    printf 'Charlie was here\n' >"$dir/notes.txt"
    expect 0 'IDENTITY verdict=pass' st_id_host "$home" "$repo" "$clean" &&
        expect 0 'IDENTITY tokens=7' st_id_host "$home" "$repo" "$clean" &&
        expect 1 'IDENTITY hit token=6 source=account part=domain file=notes.txt' st_id_host "$home" "$repo" "$dir" &&
        expect 1 'IDENTITY verdict=fail' st_id_host "$home" "$repo" "$dir"
}

st_id_no_account() {
    local home repo dir
    home="$(st_id_home none)"
    repo="$(st_src_repo)"
    dir="$(st_id_tree)"
    expect 1 'IDENTITY source=account ABSENT' st_id_host "$home" "$repo" "$dir" &&
        expect 1 'IDENTITY verdict=fail' st_id_host "$home" "$repo" "$dir" &&
        expect_absent 'IDENTITY hit .*' st_id_host "$home" "$repo" "$dir"
}

# Every failing case's output, together, names no token in any case, and
# holds hit lines (so the check is not vacuous).
st_id_never_prints() {
    local tokens home repo out="" dir
    tokens="$(st_id_tokens)"
    home="$(st_id_home account)"
    repo="$(st_src_repo)"
    dir="$(st_id_tree)"
    printf 'xxBravoyy delta\n' >"$dir/notes.txt"
    printf 'x\0CHARLIE alpha.bravo@charlie-delta.example\n' >"$dir/sub/.leak"
    printf '.leak\n' >"$dir/sub/.ignore"
    printf 'clean\n' >"$dir/report-Alpha-Delta.txt"
    out+="$(st_id "$dir" "$tokens" 2>&1)" || true
    out+="$(st_id_host "$home" "$repo" "$dir" 2>&1)" || true
    printf '%s\n' "$out" | rg -q -e '^IDENTITY hit token=' ||
        { echo "no hit line, so nothing was judged: $out"; return 1; }
    if printf '%s\n' "$out" | rg -q -i -e "$ST_ID_TOKENS_RE"; then
        echo "a token was printed"
        return 1
    fi
}

# --published --attempt cases (50-29). The base tree plus attempt-2, whose
# saved gate output pairs it with fault-7 on another bin_sha256.
st_pub_attempt() {
    env HOME="$ST_PUB_HOME" bash "$SELF" --published "$1" --attempt "$2"
}

st_pub_attempt_tree() {
    local dir
    dir="$(st_pub_tree)"
    mkdir -p "$dir/attempt-2" "$dir/fault-7"
    printf '%s\n' "$ST_OTHER_SHA" >"$dir/fault-7/bin_sha256"
    printf 'FAULT bin_sha256=%s\nFAULT complete=yes\nFAULT verdict=pass\n' "$ST_OTHER_SHA" \
        >"$dir/fault-7/verdict-fault.txt"
    printf 'ATTEMPT verdict=pass\nATTEMPT_GATE current_source=yes\nATTEMPT_GATE fault=fault-7 bin_sha256=%s\nATTEMPT_GATE verdict=shipped attempt=2\n' \
        "$ST_OTHER_SHA" >"$dir/attempt-2/verdict-attempt-gate.txt"
    printf '%s\n' "$dir"
}

st_pub_attempt_pass() {
    local dir
    dir="$(st_pub_attempt_tree)"
    expect 0 'PUBLISHED verdict=pass' st_pub_attempt "$dir" 2 &&
        expect 0 'PUBLISHED attempt=2 verdict_line=shipped' st_pub_attempt "$dir" 2 &&
        expect 0 'PUBLISHED attempt_current_source=yes' st_pub_attempt "$dir" 2 &&
        expect 0 'PUBLISHED attempt_fault=fault-7' st_pub_attempt "$dir" 2
}

# Only attempt-1 is published; the plain check passes the same tree.
st_pub_attempt_unpublished() {
    local dir
    dir="$(st_pub_tree)"
    expect 0 'PUBLISHED verdict=pass' st_pub "$dir" &&
        expect 1 'PUBLISHED verdict=fail' st_pub_attempt "$dir" 2 &&
        expect 1 'PUBLISHED missing=attempt-2-dir' st_pub_attempt "$dir" 2
}

st_pub_attempt_no_fault() {
    local dir
    dir="$(st_pub_attempt_tree)"
    rm -r -- "$dir/fault-7"
    expect 1 'PUBLISHED verdict=fail' st_pub_attempt "$dir" 2 &&
        expect 1 'PUBLISHED attempt_fault_dir=ABSENT name=fault-7' st_pub_attempt "$dir" 2 &&
        expect 1 'PUBLISHED attempt_fault=ABSENT' st_pub_attempt "$dir" 2
}

# fault-7 records the attempt-1 fixture's bin_sha256, not the one attempt-2's
# gate line names.
st_pub_attempt_other_sha() {
    local dir
    dir="$(st_pub_attempt_tree)"
    printf '%s\n' "$ST_SHA" >"$dir/fault-7/bin_sha256"
    expect 1 'PUBLISHED verdict=fail' st_pub_attempt "$dir" 2 &&
        expect 1 'PUBLISHED attempt_fault_sha=MISMATCH name=fault-7' st_pub_attempt "$dir" 2
}

st_pub_attempt_other() {
    local dir
    dir="$(st_pub_attempt_tree)"
    sed -i 's/^ATTEMPT_GATE verdict=shipped attempt=2$/ATTEMPT_GATE verdict=shipped attempt=1/' \
        "$dir/attempt-2/verdict-attempt-gate.txt"
    expect 1 'PUBLISHED verdict=fail' st_pub_attempt "$dir" 2 &&
        expect 1 'PUBLISHED attempt=2 verdict_line=ABSENT' st_pub_attempt "$dir" 2
}

st_pub_attempt_no_source() {
    local dir
    dir="$(st_pub_attempt_tree)"
    sed -i '/^ATTEMPT_GATE current_source=yes$/d' "$dir/attempt-2/verdict-attempt-gate.txt"
    expect 1 'PUBLISHED verdict=fail' st_pub_attempt "$dir" 2 &&
        expect 1 'PUBLISHED attempt_current_source=no' st_pub_attempt "$dir" 2
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

    printf '#!/usr/bin/env bash\necho "CHECK passed=11 failed=0"\n' >"$ST_TMP/p49-pass.sh"
    st_case 'attempt fails without a capture launch' st_attempt_no_capture
    st_case 'attempt fails a monitor_pid key' st_attempt_monitor_pid
    st_case 'attempt fails a 108-byte socket path' st_attempt_108
    st_case 'fault fails a record whose expectation was not met' st_fault_unmet
    st_case 'attempt gate is not ready on a fault directory from another binary' st_gate_other_binary
    st_case 'attempt gate ships with a full fault directory on its binary' st_gate_ships
    st_case 'attempt gate reports cap reached after three noted stops' st_gate_cap
    st_case 'fault require-all fails a window record marked live' st_require_all_window_live
    st_case 'fault require-all fails a case recorded twice' st_require_all_twice
    st_case 'attempt passes with zero leftover sidecars' st_attempt_zero_sidecars
    st_case 'attempt fails when a launch id is not in its socket name' st_attempt_id_not_in_socket
    st_case 'fault pointer is not counted as live' st_pointer_not_live
    st_case 'fault pointer without its fixture fails' st_pointer_no_fixture
    st_case 'fault require-all fails a partial directory' st_require_all_partial
    st_case 'attempt gate refuses a partial fault directory' st_gate_partial
    st_case 'attempt gate with current source ships when the paired fault directory records the checkout crate tree' st_gate_cs_ships
    st_case 'attempt gate with current source refuses a paired fault directory from other crate source' st_gate_cs_other_source
    st_case 'attempt gate with current source refuses an uncommitted crate change' st_gate_cs_uncommitted
    st_case 'fault current picks the current-source directory over a newer other-source one' st_fc_picks_current
    st_case 'fault current fails with only other-source directories' st_fc_only_other
    st_case 'fault current judges only the newest current-source directory' st_fc_newest_only
    st_case 'fault current refuses an uncommitted crate tree' st_fc_uncommitted
    st_case 'fault current passes after a planning-only commit' st_fc_planning_commit
    st_case 'fault current fails a directory whose run saw a mismatching checkout' st_fc_mismatching_checkout
    st_case 'fault current fails a partial current-source directory' st_fc_partial
    st_case 'published passes a sanitized tree' st_pub_pass
    st_case 'published fails a file naming the home path' st_pub_home_file
    st_case 'published fails a hidden file naming the home path' st_pub_home_hidden
    st_case 'published fails a symlink' st_pub_symlink
    st_case 'published fails without a passing fault verdict' st_pub_no_fault_pass
    st_case 'published fails without a shipped or cap_reached verdict' st_pub_no_shipped
    st_case 'published fails without an attempt directory' st_pub_no_attempt_dir
    st_case 'live run passes a complete record' st_lr_pass
    st_case 'live run fails a missing fragment' st_lr_missing_fragment
    st_case 'live run fails a missing criterion row' st_lr_missing_row
    st_case 'live run fails an absent file' st_lr_absent
    st_case 'live run passes a record quoting the attempt gate verdict' st_lr_attempt_pass
    st_case 'live run fails a record quoting only an earlier attempt verdict' st_lr_attempt_earlier
    st_case 'handoff passes a complete section' st_ho_pass
    st_case 'handoff passes on the check line of the 50-20 plan' st_ho_real_plan
    st_case 'handoff fails an altered check line' st_ho_altered
    st_case 'handoff fails a missing handoff fragment' st_ho_missing_fragment
    st_case 'handoff fails a plan without the check line' st_ho_plan_without
    st_case 'handoff fails a plan holding the check line twice' st_ho_plan_twice
    st_case 'handoff fails an absent live run file' st_ho_absent
    st_case 'defects passes a new backlog id referenced from the record' st_def_pass_referenced
    st_case 'defects fails a new backlog id missing from the record' st_def_fail_unreferenced
    st_case 'defects passes no new id with the no-defect statement' st_def_pass_none
    st_case 'defects fails no new id without the statement' st_def_fail_no_statement
    st_case 'defects fails outside a git repository' st_def_outside_git
    st_case 'identity passes a tree without the tokens' st_id_clean
    st_case 'identity fails a planted local part run in another case' st_id_local_case
    st_case 'identity fails a planted domain run' st_id_domain
    st_case 'identity fails a token in a hidden file' st_id_hidden
    st_case 'identity fails an empty token file' st_id_empty
    st_case 'identity fails a token in a file name' st_id_name
    st_case 'identity never prints a token' st_id_never_prints
    st_case 'identity reads the host account email' st_id_account
    st_case 'identity fails without a host account email' st_id_no_account
    st_case 'published passes an attempt with its shipped verdict and its paired fault directory' st_pub_attempt_pass
    st_case 'published fails an attempt that is not published' st_pub_attempt_unpublished
    st_case 'published fails an attempt whose paired fault directory is absent' st_pub_attempt_no_fault
    st_case 'published fails an attempt verdict naming another attempt' st_pub_attempt_other
    st_case 'published fails an attempt whose paired fault directory records another bin_sha256' st_pub_attempt_other_sha
    st_case 'published fails a shipped attempt verdict without the current source line' st_pub_attempt_no_source

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
        --fault-current)
            local home="" repo=""
            shift
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --repo) [ "$#" -ge 2 ] || usage; repo="$2"; shift ;;
                    -*) usage ;;
                    *) [ -z "$home" ] || usage; home="$1" ;;
                esac
                shift
            done
            [ -n "$home" ] || usage
            [ -n "$repo" ] || repo="$(default_repo)"
            fault_current "$home" "$repo"
            ;;
        collect-sidecars)
            local ev="" project="" bin=""
            shift
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --project) [ "$#" -ge 2 ] || usage; project="$2"; shift ;;
                    --bin) [ "$#" -ge 2 ] || usage; bin="$2"; shift ;;
                    -*) usage ;;
                    *) [ -z "$ev" ] || usage; ev="$1" ;;
                esac
                shift
            done
            [ -n "$ev" ] && [ -n "$project" ] || usage
            collect_sidecars "$ev" "$project" "$bin"
            ;;
        --attempt | --attempt-gate)
            local mode="$1" target="" p49="" current_source=0 repo=""
            shift
            while [ "$#" -gt 0 ]; do
                case "$mode $1" in
                    *" --p49-check") [ "$#" -ge 2 ] || usage; p49="$2"; shift ;;
                    "--attempt-gate --current-source") current_source=1 ;;
                    "--attempt-gate --repo") [ "$#" -ge 2 ] || usage; repo="$2"; shift ;;
                    *" -"*) usage ;;
                    *) [ -z "$target" ] || usage; target="$1" ;;
                esac
                shift
            done
            [ -n "$target" ] || usage
            # --repo names the checkout --current-source judges against; alone it means nothing.
            [ -z "$repo" ] || [ "$current_source" -eq 1 ] || usage
            [ -n "$p49" ] || p49="$(p49_check_default)"
            if [ "$mode" = --attempt ]; then
                attempt_check "$target" "$p49"
            else
                [ "$current_source" -eq 0 ] || [ -n "$repo" ] || repo="$(default_repo)"
                attempt_gate "$target" "$p49" "$current_source" "$repo"
            fi
            ;;
        --published)
            local dir="" number=""
            shift
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --attempt) [ "$#" -ge 2 ] && [ -z "$number" ] || usage; number="$2"; shift ;;
                    -*) usage ;;
                    *) [ -z "$dir" ] || usage; dir="$1" ;;
                esac
                shift
            done
            [ -n "$dir" ] || usage
            [ -z "$number" ] || [[ "$number" =~ ^[1-9][0-9]*$ ]] || usage
            published_check "$dir" "$number"
            ;;
        --identity-scan)
            local dir="" tokens="" repo=""
            shift
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --tokens) [ "$#" -ge 2 ] && [ -z "$tokens" ] || usage; tokens="$2"; shift ;;
                    --repo) [ "$#" -ge 2 ] && [ -z "$repo" ] || usage; repo="$2"; shift ;;
                    -*) usage ;;
                    *) [ -z "$dir" ] || usage; dir="$1" ;;
                esac
                shift
            done
            [ -n "$dir" ] && [ -d "$dir" ] || usage
            # --tokens replaces the host's addresses; --repo names where git's are read.
            [ -z "$tokens" ] || [ -z "$repo" ] || usage
            [ -n "$tokens" ] || [ -n "$repo" ] || repo="$(default_repo)"
            identity_scan "$dir" "$tokens" "$repo"
            ;;
        --live-run)
            # Two arguments keep 50-25's parsing unchanged; 50-29 appends the
            # `<file> --attempt <N>` form only.
            if [ "$#" -eq 2 ]; then
                live_run_check "$2"
            else
                [ "$#" -eq 4 ] && [ "$3" = --attempt ] && [[ "$4" =~ ^[1-9][0-9]*$ ]] || usage
                live_run_check "$2" "$4"
            fi
            ;;
        --handoff | --defects)
            local mode="$1" file="" other=""
            shift
            while [ "$#" -gt 0 ]; do
                case "$mode $1" in
                    "--handoff --plan" | "--defects --roadmap") [ "$#" -ge 2 ] || usage; other="$2"; shift ;;
                    *" -"*) usage ;;
                    *) [ -z "$file" ] || usage; file="$1" ;;
                esac
                shift
            done
            [ -n "$file" ] && [ -n "$other" ] || usage
            if [ "$mode" = --handoff ]; then
                handoff_check "$file" "$other"
            else
                defects_check "$file" "$other"
            fi
            ;;
        *) usage ;;
    esac
}

main "$@"
