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
    # Capture a planted complete run with the real hook and collector.
    import os, hashlib
    tools = pathlib.Path(script).parent
    home = root / 'positive-home'
    ev = home / 'attempt-1'
    launch = ev / 'project'
    launch.mkdir(parents=True)
    fixture_env = os.environ.copy()
    for key in subprocess.check_output(['git', 'rev-parse', '--local-env-vars'], text=True).splitlines():
        fixture_env.pop(key, None)
    fixture_env.update(GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1')
    def run(*args, cwd=None, env=None):
        return subprocess.check_output(args, cwd=cwd, env=env or fixture_env, stderr=subprocess.PIPE, text=True, timeout=30).strip()
    def git(*args, cwd=launch): return run('git', '-C', str(cwd), *args)
    git('init', '-q'); git('symbolic-ref', 'HEAD', 'refs/heads/main')
    git('config', 'user.name', 'Fixture'); git('config', 'user.email', 'fixture@localhost')
    (launch / '.gitignore').write_text('.devflow/\n.worktrees/\n')
    git('add', '.gitignore'); git('commit', '-qm', 'chore: seed fixture')
    main_sha = git('rev-parse', 'HEAD')
    git('checkout', '-qb', 'integration')
    (launch / 'devflow.toml').write_text('base_branch = "integration"\n')
    git('add', 'devflow.toml'); git('commit', '-qm', 'chore: configure base')
    base_sha = git('rev-parse', 'HEAD')
    (ev / 'prelaunch').mkdir()
    (ev / 'prelaunch/fork-base.txt').write_text(f'base_sha={base_sha}\nmain_sha={main_sha}\n')
    (ev / 'start.stdout').write_text('note: base branch is `integration` (from devflow.toml (base_branch))\n  [holds] GSD config can hold the chain flag\n')
    wt = launch / '.worktrees/phase-01'
    git('worktree', 'add', '-qb', 'feature/phase-01', str(wt), 'integration')
    dev = launch / '.devflow'
    (dev / 'gates').mkdir(parents=True)
    build_commit = 'c' * 40
    recorded = [{'event':'workflow_started', 'phase':1, 'commit':build_commit}]
    def events_write():
        (dev / 'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in recorded))
    for stage in ['define','code','ship']:
        if stage != 'define':
            (wt / f'{stage}.txt').write_text(stage+'\n')
            git('add', f'{stage}.txt', cwd=wt); git('commit', '-qm', f'feat: {stage}', cwd=wt)
        (dev / 'state-01.json').write_text(json.dumps({'phase':1,'stage':stage,'base_branch':'integration'}))
        (dev / f'gates/01-{stage}.json').write_text(json.dumps({'phase':1,'stage':stage}))
        recorded.append({'event':'gate_fired','phase':1,'stage':stage}); events_write()
        env = dict(fixture_env, DEVFLOW_GATE_PHASE='1', DEVFLOW_GATE_STAGE=stage, DEVFLOW_GATE_CONTEXT='fixture', DEVFLOW_NON_SILENT_GATE='1')
        run('bash', str(tools / 'snapshot-hook.sh'), '--no-notify', str(launch), str(ev), env=env)
        recorded.append({'event':'gate_resolved','phase':1,'stage':stage,'responded_by':'--yes-ship' if stage == 'ship' else 'tester'})
    git('merge', '--no-ff', 'feature/phase-01', '-m', 'feat: merge fixture')
    recorded.append({'event':'merge_result','phase':1,'merged':True})
    git('tag', 'v0.1.0')
    (launch / 'CHANGELOG.md').write_text('Version 0.1.0\n')
    git('add', 'CHANGELOG.md'); git('commit', '-qm', 'docs: add changelog entry for 0.1.0')
    assert len(git('rev-list', '--parents', '-n1', 'HEAD').split()) == 2
    subprocess.run(['git','-C',str(launch),'branch','-d','feature/phase-01'],env=fixture_env,capture_output=True)
    recorded.append({'event':'workflow_shipped','phase':1}); events_write()
    (dev / 'state-01.json').unlink()
    fakes = root / 'positive-fakes'; fakes.mkdir()
    binary = fakes / 'devflow'; binary.write_text('#!/bin/sh\nprintf "{}\\n"\n'); binary.chmod(0o755)
    gh = fakes / 'gh'
    gh.write_text('#!/bin/sh\nprintf \'{"baseRefName":"main","headRefName":"feature/phase-01","state":"OPEN","url":"x","number":1}\\n\'\n'); gh.chmod(0o755)
    binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
    (ev / 'attempt.json').write_text(json.dumps({'attempt':1,'launch':str(launch),'bin':str(binary),'bin_sha256':binary_sha}))
    run('bash',str(tools / 'collect-attempt.sh'),'--attempt','1','--home',str(home),'--fake-cmds',str(fakes),'--claude-projects',str(root / 'no-sessions'))
    shutil.copytree(root / 'original-renamed/negctl', ev / 'negctl')
    (ev / 'negctl/bin_sha256').write_text(binary_sha+'\n')
    with (ev / 'negctl/events.jsonl').open('a') as f:
        f.write(json.dumps({'event':'workflow_started','phase':1,'commit':build_commit})+'\n')
    (home / 'current-attempt').write_text('1\n')
    result = subprocess.run(['bash',script,'--arm','positive',str(ev)],capture_output=True,text=True,timeout=20)
    ok = result.returncode == 0 and 'CHECK passed=11 failed=0' in result.stdout
    print(('  ok   ' if ok else '  FAIL ')+'positive arm passes'+('' if ok else ' — complete captured fixture rejected'))
    passed += int(ok); failed += int(not ok)
print(f'SELFTEST passed={passed} failed={failed}')
sys.exit(bool(failed) or not passed)
PY
}

if [ "${1:-}" = --selftest ] && [ "$#" -eq 1 ]; then selftest; exit; fi
if [ "${1:-}" = --arm ] && [ "${2:-}" = positive ]; then exit 1; fi
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
