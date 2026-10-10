#!/usr/bin/env bash
# Run the unconfigured scratch arm. Own exactly one unit; never publish.
set -euo pipefail
T="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELF="$T/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(git -C "$T" rev-parse --show-toplevel)"

selftest() {
    local tmp rc=0 passed=0 failed=0
    tmp="$(mktemp -d)"
    trap 'rm -rf -- "$tmp"' RETURN
    mkdir "$tmp/fakes"
    cat >"$tmp/fakes/systemd-run" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" >"$FAKE_ROOT/argv"
envs=()
unit=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --unit) shift; unit="$1" ;;
        --setenv=*) envs+=("${1#--setenv=}") ;;
        --) shift; break ;;
    esac
    shift
done
env "${envs[@]}" "$@" &
printf '%s\n' "$!" >"$FAKE_ROOT/$unit.pid"
FAKE
    cat >"$tmp/fakes/systemctl" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
[ "$1" != --user ] || shift
cmd="$1"; shift
unit="$1"
printf '%s %s\n' "$cmd" "$unit" >>"$FAKE_ROOT/ctl"
if [ -f "$FAKE_ROOT/$unit.pid" ]; then pid="$(cat "$FAKE_ROOT/$unit.pid")"; else pid=0; fi
case "$cmd" in
    is-active)
        if [ "$pid" -gt 0 ] && kill -0 "$pid" 2>/dev/null && ! rg -q "^State:.*Z" "/proc/$pid/status"; then echo active; else echo inactive; exit 3; fi ;;
    stop) if [ "$pid" -gt 0 ]; then kill "$pid" 2>/dev/null || true; fi ;;
    reset-failed) ;;
    *) exit 2 ;;
esac
FAKE
    cat >"$tmp/fakes/devflow" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = gate ]; then
    while [ "$1" != --project ]; do shift; done
    touch "$2/.devflow/abort"
    exit 0
fi
repo="${@: -1}"
if [ "${FAKE_NEVER:-false}" = true ]; then sleep 15; exit; fi
printf '  [DOES NOT HOLD] GSD config can hold the chain flag — no `.planning/config.json` under the launch root — nowhere for the chain flag to live\n'
git -C "$repo" worktree add -q -b feature/phase-01 "$repo/.worktrees/phase-01" develop
mkdir -p "$repo/.devflow/gates"
printf '{"phase":1,"stage":"define","base_branch":"develop","gate_pending":true}\n' >"$repo/.devflow/state-01.json"
printf '{"phase":1,"stage":"define"}\n' >"$repo/.devflow/gates/01-define.json"
printf '{"phase":1,"stage":"define","event":"gate_fired"}\n' >"$repo/.devflow/events.jsonl"
DEVFLOW_GATE_PHASE=1 DEVFLOW_GATE_STAGE=define DEVFLOW_GATE_CONTEXT=fixture DEVFLOW_NON_SILENT_GATE=1 \
    bash -c "$DEVFLOW_GATE_NOTIFY_CMD"
while [ ! -f "$repo/.devflow/abort" ]; do sleep 0.1; done
printf '{"phase":1,"event":"workflow_aborted"}\n' >>"$repo/.devflow/events.jsonl"
rm -- "$repo/.devflow/state-01.json"
FAKE
    chmod +x "$tmp/fakes/"*
    export FAKE_ROOT="$tmp"
    local -a git_vars
    mapfile -t git_vars < <(git rev-parse --local-env-vars)
    unset "${git_vars[@]}"
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
    timeout 20 bash "$SELF" --attempt 1 --home "$tmp/home" --bin "$tmp/fakes/devflow" \
        --fake-cmds "$tmp/fakes" --gate-timeout 5 --exit-timeout 5 >"$tmp/run.log" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ] && bash "$T/check-evidence.sh" --arm negative "$tmp/home/attempt-1" >"$tmp/check.log" 2>&1 &&
        rg -qx 'CHECK passed=5 failed=0' "$tmp/check.log"; then
        echo '  ok   negctl: fake arm end to end passes the negative-arm check'; passed=$((passed + 1))
    else
        echo '  FAIL negctl: fake arm end to end passes the negative-arm check — evidence absent or rejected'
        failed=$((failed + 1))
    fi
    # Task-2 controls assert text and preconditions, never a bare exit code.
    st_report() {
        if [ "$1" -eq 0 ]; then
            printf '  ok   %s\n' "$2"; passed=$((passed + 1))
        else
            printf '  FAIL %s — control failed\n' "$2"; failed=$((failed + 1))
        fi
    }
    local why=0 fixture="$tmp/fixture" before after
    env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=commit.gpgsign GIT_CONFIG_VALUE_0=false \
        bash "$ROOT/scripts/scratch-dogfood-repo.sh" "$tmp/default" >"$tmp/default.log" 2>&1
    git -C "$tmp/default" cat-file -e develop:.planning/config.json || why=1
    st_report "$why" 'control: unmodified scaffold still tracks config.json on develop'
    before="$(wc -l <"$tmp/argv")"
    rc=0
    bash "$SELF" --attempt 1 --home "$fixture" --bin "$tmp/fakes/devflow" --fake-cmds "$tmp/fakes" \
        --fixture-only >"$tmp/fixture.log" 2>&1 || rc=$?
    why=0
    [ "$rc" -eq 0 ] || why=1
    if git -C "$fixture/attempt-1/negctl/repo" cat-file -e develop:.planning/config.json 2>/dev/null; then why=1; fi
    after="$(wc -l <"$tmp/argv")"
    [ "$before" = "$after" ] || why=1
    st_report "$why" 'negctl: fixture-only removes config.json on develop and launches nothing'
    before="$(git -C "$fixture/attempt-1/negctl/repo" rev-parse HEAD)"
    rc=0
    bash "$SELF" --attempt 1 --home "$fixture" --bin "$tmp/fakes/devflow" --fixture-only >"$tmp/existing.log" 2>&1 || rc=$?
    why=0
    [ "$rc" -ne 0 ] && rg -q 'already exists' "$tmp/existing.log" &&
        [ "$before" = "$(git -C "$fixture/attempt-1/negctl/repo" rev-parse HEAD)" ] || why=1
    st_report "$why" 'control: existing negctl dir refused'
    mkdir -p "$tmp/mismatch/attempt-1"
    jq -n --arg bin "$tmp/fakes/devflow" '{bin:$bin,bin_sha256:"wrong"}' >"$tmp/mismatch/attempt-1/attempt.json"
    rc=0
    bash "$SELF" --attempt 1 --home "$tmp/mismatch" --fixture-only >"$tmp/mismatch.log" 2>&1 || rc=$?
    why=0
    [ "$rc" -ne 0 ] && rg -q 'sha256 mismatch' "$tmp/mismatch.log" &&
        [ ! -e "$tmp/mismatch/attempt-1/negctl" ] || why=1
    st_report "$why" 'control: bin sha mismatch with attempt.json refused'
    why=0
    [ "$(rg --count-matches '^--setenv=' "$tmp/argv")" -eq 3 ] &&
        ! rg -q '^--setenv=DEVFLOW_BASE_BRANCH=' "$tmp/argv" &&
        rg -q 'env -u DEVFLOW_BASE_BRANCH' "$tmp/argv" &&
        rg -q '^--setenv=DEVFLOW_GATE_NOTIFY_CMD=.*attempt-1/negctl$' "$tmp/argv" || why=1
    st_report "$why" 'negctl: unit env scrubs DEVFLOW_BASE_BRANCH and routes the hook to negctl'
    rc=0
    FAKE_NEVER=true bash "$SELF" --attempt 1 --home "$tmp/never" --bin "$tmp/fakes/devflow" \
        --unit p49-test-never --fake-cmds "$tmp/fakes" --gate-timeout 3 --exit-timeout 2 >"$tmp/never.log" 2>&1 || rc=$?
    why=0
    [ "$rc" -ne 0 ] && rg -q 'define gate did not open' "$tmp/never.log" &&
        [ "$(tail -2 "$tmp/ctl")" = $'stop p49-test-never\nreset-failed p49-test-never' ] || why=1
    st_report "$why" 'negctl: unit stopped and reset when the gate never opens'
    cat >"$tmp/failing-signer" <<'SIGNER'
#!/usr/bin/env bash
printf 'called\n' >>"$FAKE_ROOT/signer.log"
exit 1
SIGNER
    chmod +x "$tmp/failing-signer"
    git config --file "$tmp/global" commit.gpgsign true
    git config --file "$tmp/global" tag.gpgsign true
    git config --file "$tmp/global" gpg.format openpgp
    git config --file "$tmp/global" gpg.program "$tmp/failing-signer"
    git config --file "$tmp/global" user.name 'Control Signer'
    git config --file "$tmp/global" user.email control@localhost
    git init -q "$tmp/sign-control"
    rc=0
    GIT_CONFIG_GLOBAL="$tmp/global" git -C "$tmp/sign-control" commit -q --allow-empty -m 'chore: signer control' >"$tmp/sign-control.log" 2>&1 || rc=$?
    why=0
    [ "$rc" -ne 0 ] && [ -s "$tmp/signer.log" ] || why=1
    before="$(wc -l <"$tmp/signer.log")"
    rc=0
    GIT_CONFIG_GLOBAL="$tmp/global" bash "$SELF" --attempt 1 --home "$tmp/signed" --bin "$tmp/fakes/devflow" \
        --fixture-only >"$tmp/signed.log" 2>&1 || rc=$?
    [ "$rc" -eq 0 ] && [ "$before" = "$(wc -l <"$tmp/signer.log")" ] || why=1
    git -C "$tmp/signed/attempt-1/negctl/repo" log --format=%H develop | while IFS= read -r commit; do
        if git -C "$tmp/signed/attempt-1/negctl/repo" cat-file commit "$commit" | rg -q '^gpgsig '; then exit 1; fi
    done || why=1
    st_report "$why" 'negctl: no commit invokes host signing'
    echo "SELFTEST passed=$passed failed=$failed"
    [ "$failed" -eq 0 ] && [ "$passed" -gt 0 ]
}

if [ "${1:-}" = --selftest ]; then selftest; exit; fi
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
attempt="" home="$HOME/.cache/devflow-phase49" bin="" unit="" scaffold="$ROOT/scripts/scratch-dogfood-repo.sh"
gate_timeout=120 exit_timeout=60 fixture=false fake_cmds=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --attempt|--home|--bin|--unit|--scaffold|--gate-timeout|--exit-timeout|--fake-cmds)
            [ "$#" -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"
            case "$1" in
                --attempt) attempt="$2" ;; --home) home="$2" ;; --bin) bin="$2" ;;
                --unit) unit="$2" ;; --scaffold) scaffold="$2" ;; --gate-timeout) gate_timeout="$2" ;;
                --exit-timeout) exit_timeout="$2" ;; --fake-cmds) fake_cmds="$2" ;;
            esac
            shift ;;
        --fixture-only) fixture=true ;;
        *) die "unknown option: $1" ;;
    esac
    shift
done
[[ "$attempt" =~ ^[1-9][0-9]*$ ]] || die '--attempt must be a positive integer'
[[ "$gate_timeout" =~ ^[1-9][0-9]*$ ]] && [[ "$exit_timeout" =~ ^[1-9][0-9]*$ ]] || die 'timeouts must be positive integers'
unit="${unit:-p49-a$attempt-negctl}"
[[ "$unit" =~ ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$ ]] || die 'invalid unit name'
ev="$(realpath -m -- "$home")/attempt-$attempt"
neg="$ev/negctl"
[ ! -e "$neg" ] || die "already exists: $neg"
if [ -f "$ev/attempt.json" ]; then
    recorded_bin="$(jq -er '.bin | select(type == "string" and length > 0)' "$ev/attempt.json")" || die 'attempt.json bin ABSENT'
    if [ -n "$bin" ] && [ "$bin" != "$recorded_bin" ]; then die 'binary path mismatch with attempt.json'; fi
    bin="$recorded_bin"
fi
[ -n "$bin" ] && [ -x "$bin" ] || die 'executable --bin or attempt.json bin required'
bin="$(realpath -e -- "$bin")"
sha="$(sha256sum "$bin" | cut -d ' ' -f1)"
if [ -f "$ev/attempt.json" ]; then
    expected="$(jq -er '.bin_sha256 | select(type == "string" and length > 0)' "$ev/attempt.json")" || die 'attempt.json bin_sha256 ABSENT'
    [ "$sha" = "$expected" ] || die 'sha256 mismatch with attempt.json'
fi
[ -f "$scaffold" ] || die 'scaffold ABSENT'
if [ -n "$fake_cmds" ]; then
    fake_cmds="$(realpath -e -- "$fake_cmds")"
    [ -x "$fake_cmds/systemd-run" ] && [ -x "$fake_cmds/systemctl" ] || die 'fake commands ABSENT'
    export PATH="$fake_cmds:$PATH"
fi
mkdir -p "$neg"
printf '%s\n' "$sha" >"$neg/bin_sha256"
repo="$neg/repo"
env GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=commit.gpgsign GIT_CONFIG_VALUE_0=false \
    GIT_CONFIG_KEY_1=tag.gpgsign GIT_CONFIG_VALUE_1=false bash "$scaffold" "$repo"
git -C "$repo" config commit.gpgsign false
git -C "$repo" config tag.gpgsign false
git -C "$repo" cat-file -e develop:.planning/config.json || die 'precondition: develop config.json ABSENT'
git -C "$repo" rm -q .planning/config.json
git -C "$repo" commit -q -m 'chore: remove config.json for negative control'
if git -C "$repo" cat-file -e develop:.planning/config.json 2>/dev/null; then die 'config removal failed'; fi
git -C "$repo" rev-parse 'develop^{commit}' >"$neg/develop.sha"
if [ "$fixture" = true ]; then exit 0; fi
state="$(timeout 10 systemctl --user is-active "$unit" 2>/dev/null || true)"
case "$state" in active|activating|deactivating|reloading) die "unit already exists: $unit ($state)" ;; esac
cleanup() {
    timeout 15 systemctl --user stop "$unit" >/dev/null 2>&1 || true
    timeout 15 systemctl --user reset-failed "$unit" >/dev/null 2>&1 || true
}
trap cleanup EXIT
# Quote paths as shell arguments because the notify contract is a command string.
printf -v notify 'timeout 60 bash %q --no-notify %q %q' "$T/snapshot-hook.sh" "$repo" "$neg"

# shellcheck disable=SC2016 # Expanded by the launched sh, not this shell.
timeout 20 systemd-run --user --unit "$unit" --setenv="PATH=$PATH" --setenv="HOME=$HOME" \
    --setenv="DEVFLOW_GATE_NOTIFY_CMD=$notify" -- \
    sh -c 'exec env -u DEVFLOW_BASE_BRANCH "$0" start --phase 1 --agent claude --mode auto "$1" >"$2/start.stdout" 2>"$2/start.stderr"' \
    "$bin" "$repo" "$neg"
deadline=$((SECONDS + gate_timeout))
dir=""
while [ "$SECONDS" -lt "$deadline" ]; do
    if [ -f "$neg/ledger.jsonl" ]; then
        dir="$(jq -rs '[.[] | select(.stage == "define")][0].dir // empty' "$neg/ledger.jsonl")" || die 'ledger malformed'
        [ -z "$dir" ] || break
    fi
    sleep 0.2
done
[ -n "$dir" ] && [ -f "$dir/git.txt" ] || die 'define gate did not open'
head="$(sed -n 's/^worktree_head=//p' "$dir/git.txt")"
[[ "$head" =~ ^[0-9a-f]{40,64}$ ]] || die 'worktree head ABSENT'
printf '%s\n' "$head" >"$neg/worktree-head.sha"
timeout 30 "$bin" gate reject 1 --stage define --note 'abort: negative control complete' --project "$repo"
deadline=$((SECONDS + exit_timeout))
while [ "$SECONDS" -lt "$deadline" ]; do
    state="$(timeout 10 systemctl --user is-active "$unit" 2>/dev/null || true)"
    case "$state" in inactive|failed|unknown) break ;; esac
    sleep 0.2
done
case "$state" in inactive|failed|unknown) ;; *) die 'unit did not exit after abort' ;; esac
[ -f "$repo/.devflow/events.jsonl" ] || die 'events ABSENT'
cp "$repo/.devflow/events.jsonl" "$neg/events.jsonl"
bash "$T/check-evidence.sh" --arm negative "$ev"
