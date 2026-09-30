#!/usr/bin/env bash
# Judge recorded evidence, never a process exit code. Missing inputs fail by name.
set -euo pipefail
SELF="$(realpath -- "${BASH_SOURCE[0]}")"

selftest() {
    python3 - "$SELF" <<'PY'
import json, pathlib, shutil, subprocess, sys, tempfile
script = sys.argv[1]
refusal = '  [DOES NOT HOLD] GSD config can hold the chain flag — no `.planning/config.json` under the launch root — nowhere for the chain flag to live'
passed = failed = 0
with tempfile.TemporaryDirectory() as tmp:
    root = pathlib.Path(tmp)
    good = root / 'good'
    neg = good / 'negctl'
    snap = neg / 'gates/001-define'
    (snap / 'devflow').mkdir(parents=True)
    (neg / 'start.stdout').write_text(refusal + '\n')
    (neg / 'develop.sha').write_text('a' * 40 + '\n')
    (snap / 'git.txt').write_text('worktree_head=' + 'a' * 40 + '\n')
    (snap / 'devflow/state-01.json').write_text(json.dumps({'base_branch': 'develop'}))
    (neg / 'ledger.jsonl').write_text(json.dumps({'stage': 'define', 'dir': str(snap)}) + '\n')
    events = [{'event': 'gate_fired', 'phase': 1, 'stage': 'define'}, {'event': 'workflow_aborted', 'phase': 1}]
    (neg / 'events.jsonl').write_text(''.join(json.dumps(e) + '\n' for e in events))
    cases = [('negative arm passes', None, None),
             ('control: missing DOES NOT HOLD line refuses', 'text', 'DOES NOT HOLD line present'),
             ('control: base-branch note present refuses', 'note', 'no base-branch note'),
             ('control: base_branch integration refuses', 'base', 'base_branch is develop'),
             ('control: absent define snapshot refuses with ABSENT', 'snapshot', 'base_branch is develop'),
             ('control: fork from a moved develop refuses', 'fork', 'forked from the recorded develop'),
             ('control: no workflow_aborted after the gate refuses', 'abort', 'aborted after the gate'),
             ('relocation: negative arm judged from a moved copy', 'relocate', None)]
    for i, (name, mutation, expected) in enumerate(cases):
        copy = root / str(i)
        shutil.copytree(good, copy)
        n = copy / 'negctl'
        s = n / 'gates/001-define'
        if mutation == 'text': (n / 'start.stdout').write_text('unrelated refusal\n')
        elif mutation == 'note':
            with (n / 'start.stdout').open('a') as f: f.write('note: base branch is `integration`\n')
        elif mutation == 'base': (s / 'devflow/state-01.json').write_text('{"base_branch":"integration"}')
        elif mutation == 'snapshot': shutil.rmtree(s)
        elif mutation == 'fork': (n / 'develop.sha').write_text('b' * 40 + '\n')
        elif mutation == 'abort':
            (n / 'events.jsonl').write_text(''.join(json.dumps(e) + '\n' for e in reversed(events)))
        elif mutation == 'relocate':
            shutil.move(good, root / 'original-renamed')
            assert not snap.exists()
            assert not json.loads((n / 'ledger.jsonl').read_text())['dir'].startswith(str(copy))
            wrong = copy / 'gates/001-define/devflow'
            wrong.mkdir(parents=True)
            (wrong / 'state-01.json').write_text('{"base_branch":"integration"}')
        result = subprocess.run(['bash', script, '--arm', 'negative', str(copy)], capture_output=True, text=True, timeout=20)
        if expected:
            ok = result.returncode != 0 and f'  FAIL negative: {expected}' in result.stdout
            if mutation == 'snapshot': ok = ok and 'ABSENT' in result.stdout
            if mutation == 'abort':
                (n / 'events.jsonl').write_text(json.dumps(events[0]) + '\n')
                again = subprocess.run(['bash', script, '--arm', 'negative', str(copy)], capture_output=True, text=True, timeout=20)
                ok = ok and again.returncode != 0 and '  FAIL negative: aborted after the gate' in again.stdout
        else:
            ok = result.returncode == 0 and 'CHECK passed=5 failed=0' in result.stdout
        print(('  ok   ' if ok else '  FAIL ') + name + ('' if ok else ' — checker did not discriminate'))
        passed += int(ok)
        failed += int(not ok)
print(f'SELFTEST passed={passed} failed={failed}')
sys.exit(bool(failed) or not passed)
PY
}

if [ "${1:-}" = --selftest ] && [ "$#" -eq 1 ]; then selftest; exit; fi
if [ "$#" -ne 3 ] || [ "$1" != --arm ] || [ "$2" != negative ]; then
    echo 'usage: check-evidence.sh --arm negative <EV> | --selftest' >&2
    exit 2
fi
# RED stub: does not inspect evidence yet.
exit 0
