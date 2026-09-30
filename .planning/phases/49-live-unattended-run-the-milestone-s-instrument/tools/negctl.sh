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
        if [ "$pid" -gt 0 ] && kill -0 "$pid" 2>/dev/null; then echo active; else echo inactive; exit 3; fi ;;
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
    echo "SELFTEST passed=$passed failed=$failed"
    [ "$failed" -eq 0 ] && [ "$passed" -gt 0 ]
}

if [ "${1:-}" = --selftest ]; then selftest; exit; fi
# RED stub: no launch or evidence.
exit 0
