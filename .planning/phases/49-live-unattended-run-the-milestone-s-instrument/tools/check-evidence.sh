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
python3 - "$3" <<'PY'
import json, pathlib, re, sys
root = pathlib.Path(sys.argv[1]) / 'negctl'
refusal = '  [DOES NOT HOLD] GSD config can hold the chain flag — no `.planning/config.json` under the launch root — nowhere for the chain flag to live'
passed = failed = 0

def check(name, fn):
    global passed, failed
    try:
        if not fn(): raise ValueError('recorded value disagrees')
    except (OSError, ValueError, KeyError, TypeError, IndexError) as e:
        failed += 1
        absent = isinstance(e, (OSError, KeyError, IndexError))
        print(f'  FAIL negative: {name} — {"ABSENT " if absent else ""}{e}')
    else:
        passed += 1
        print(f'  ok   negative: {name}')

def snapshot():
    entries = [json.loads(line) for line in (root / 'ledger.jsonl').read_text().splitlines()]
    entry = next((e for e in entries if e['stage'] == 'define'), None)
    if entry is None: raise KeyError('define snapshot')
    name = pathlib.Path(entry['dir']).name
    if not re.fullmatch(r'[0-9]{3,}-define', name): raise ValueError('invalid snapshot basename')
    return root / 'gates' / name

def fork():
    fields = dict(line.split('=', 1) for line in (snapshot() / 'git.txt').read_text().splitlines() if '=' in line)
    expected = (root / 'develop.sha').read_text().strip()
    if not re.fullmatch('[0-9a-f]{40,64}', expected): raise ValueError('invalid develop SHA')
    return fields['worktree_head'] == expected

def aborted():
    events = [json.loads(line) for line in (root / 'events.jsonl').read_text().splitlines()]
    gates = [i for i, e in enumerate(events) if e['event'] == 'gate_fired' and str(e['phase']) == '1']
    if not gates: raise KeyError('gate_fired')
    return any(e['event'] == 'workflow_aborted' and str(e['phase']) == '1' for e in events[max(gates) + 1:])

check('DOES NOT HOLD line present', lambda: refusal in (root / 'start.stdout').read_text().splitlines())
check('no base-branch note', lambda: 'note: base branch is' not in (root / 'start.stdout').read_text())
check('base_branch is develop', lambda: json.loads((snapshot() / 'devflow/state-01.json').read_text())['base_branch'] == 'develop')
check('forked from the recorded develop', fork)
check('aborted after the gate', aborted)
print(f'CHECK passed={passed} failed={failed}')
sys.exit(bool(failed) or not passed)
PY
