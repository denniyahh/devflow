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
    def report(name, ok):
        global passed, failed
        print(('  ok   ' if ok else '  FAIL ')+name+('' if ok else ' — expected discrimination absent'))
        passed += int(ok); failed += int(not ok)
    positive_cases = [
        ('control: moved base refuses','base','fork point is one value'),
        ('control: reordered gates refuse','order','gates fired in order define, code, ship'),
        ('control: merge onto main instead of integration refuses','merge','merged into integration'),
        ('control: absent ship snapshot refuses with ABSENT','ship','fork point is one value'),
        ('control: operator-answered ship gate refuses','answer','ship gate answered by --yes-ship'),
        ('positive: extra gate is a deviation not a failure','extra',None),
        ('pair passes','pair',None),
        ('control: differing binary sha refuses the pair','binary','same binary in both arms'),
        ('control: arms agreeing on base_branch refuses the pair','agree','arms disagree on base_branch'),
        ('control: differing build commit refuses the pair','commit','same build commit in both arms'),
        ('control: positive note absent refuses','note','base-branch note names devflow.toml'),
        ('control: positive config condition absent refuses','holds','config condition holds'),
        ('control: positive base_branch develop refuses','branch','base_branch is integration'),
        ('control: positive main changed refuses','main','main untouched locally'),
        ('control: PR targets integration refuses','pr','PR targets main from the feature branch'),
        ('control: positive shipped event absent refuses','shipped','shipped'),
        ('control: merge_result false refuses','result','merge_result records a merge')]
    for i,(name,mutation,expected) in enumerate(positive_cases):
        copy=root/f'positive-control-{i}'
        shutil.copytree(ev,copy)
        ledger=[json.loads(line) for line in (copy/'ledger.jsonl').read_text().splitlines()]
        define=copy/'gates'/pathlib.Path(ledger[0]['dir']).name
        ship=copy/'gates'/pathlib.Path(ledger[-1]['dir']).name
        eventfile=copy/'collected/devflow/events.jsonl'
        rows=[json.loads(line) for line in eventfile.read_text().splitlines()]
        if mutation=='base':
            p=ship/'git.txt'; p.write_text(p.read_text().replace(f'merge_base={base_sha}', 'merge_base='+'d'*40))
        elif mutation=='order':
            ids=[i for i,e in enumerate(rows) if e['event']=='gate_fired' and e['stage'] in ['define','code']]
            rows[ids[0]],rows[ids[1]]=rows[ids[1]],rows[ids[0]]
        elif mutation=='merge':
            p=copy/'collected/git.txt';text=p.read_text()
            text=text.replace('integration_merge_count=1','integration_merge_count=0').replace('main_head='+main_sha,'main_head='+'d'*40)
            p.write_text(text)
        elif mutation=='ship': shutil.rmtree(ship)
        elif mutation=='answer':
            next(e for e in rows if e['event']=='gate_resolved' and e['stage']=='ship')['responded_by']='tester'
        elif mutation=='extra': rows.insert(-1,{'event':'gate_fired','stage':'validate','phase':1})
        elif mutation=='binary': (copy/'negctl/bin_sha256').write_text('d'*64+'\n')
        elif mutation=='agree': (define/'devflow/state-01.json').write_text('{"base_branch":"develop"}')
        elif mutation=='commit':
            p=copy/'negctl/events.jsonl'; p.write_text(p.read_text().replace(build_commit,'d'*40))
        elif mutation in ['note','holds']:
            p=copy/'start.stdout';p.write_text('\n'.join(line for line in p.read_text().splitlines() if ('note:' if mutation=='note' else '[holds]') not in line)+'\n')
        elif mutation=='branch': (define/'devflow/state-01.json').write_text('{"base_branch":"develop"}')
        elif mutation=='main':
            p=copy/'collected/git.txt';p.write_text(p.read_text().replace('main_head='+main_sha,'main_head='+'d'*40))
        elif mutation=='pr': (copy/'collected/pr.json').write_text('{"baseRefName":"integration","headRefName":"feature/phase-01"}')
        elif mutation=='shipped': rows=[e for e in rows if e['event']!='workflow_shipped']
        elif mutation=='result': next(e for e in rows if e['event']=='merge_result')['merged']=False
        eventfile.write_text(''.join(json.dumps(e)+'\n' for e in rows))
        pair=mutation in ['pair','binary','agree','commit']
        args=['--pair',str(copy)] if pair else ['--arm','positive',str(copy)]
        result=subprocess.run(['bash',script,*args],capture_output=True,text=True,timeout=20)
        if expected:
            ok=result.returncode!=0 and f'  FAIL {"pair" if pair else "positive"}: {expected}' in result.stdout
            if mutation=='merge': ok=ok and '  FAIL positive: main untouched locally' in result.stdout
            if mutation=='ship': ok=ok and 'ABSENT' in result.stdout
            if mutation=='base': ok=ok and f'base_sha={base_sha}' in result.stdout and f'define_head={base_sha}' in result.stdout and 'ship_merge_base='+'d'*40 in result.stdout
        else:
            ok=result.returncode==0 and f'CHECK passed={3 if pair else 11} failed=0' in result.stdout
            if mutation=='extra': ok=ok and 'NOTE deviation: gate validate' in result.stdout
        report(name,ok)
    def manifest(directory):
        lines=[]
        for path in sorted(directory.rglob('*')):
            if path.is_file() and path.name not in ['MANIFEST.sha256','DONE']:
                lines.append(hashlib.sha256(path.read_bytes()).hexdigest()+'  '+str(path.relative_to(directory))+'\n')
        (directory/'MANIFEST.sha256').write_text(''.join(lines))
    attempt_cases=[
        ('attempt gate: shipped attempt passes','good','shipped attempt=1'),
        ('control: shipped verdict without workflow_shipped is not ready','disagree','not_ready'),
        ('control: shipped attempt failing a criterion is not ready','criterion','not_ready'),
        ('control: missing verdict is not ready','missing','not_ready'),
        ('control: manifest mismatch is not ready','manifest','not_ready'),
        ('control: one stopped attempt is not ready','one','not_ready'),
        ('attempt gate: three stopped attempts reach the cap','three','cap_reached stopped_attempts=3'),
        ('control: a stopped attempt without a note does not count','note','not_ready')]
    for i,(name,mutation,expected) in enumerate(attempt_cases):
        h=root/f'attempt-control-{i}'
        shutil.copytree(home,h)
        a=h/'attempt-1'; c=a/'collected'
        eventfile=c/'devflow/events.jsonl'
        if mutation in ['disagree','one','three','note']:
            rows=[json.loads(line) for line in eventfile.read_text().splitlines()]
            eventfile.write_text(''.join(json.dumps(e)+'\n' for e in rows if e['event']!='workflow_shipped'))
            manifest(c)
        if mutation=='missing': (a/'verdict.json').unlink()
        elif mutation=='manifest':
            with (c/'git.txt').open('a') as f:f.write('tampered\n')
        elif mutation=='criterion':
            ledger=[json.loads(line) for line in (a/'ledger.jsonl').read_text().splitlines()]
            p=a/'gates'/pathlib.Path(ledger[-1]['dir']).name/'git.txt'
            p.write_text(p.read_text().replace('merge_base='+base_sha,'merge_base='+'d'*40))
        elif mutation in ['one','three','note']:
            verdict=json.loads((a/'verdict.json').read_text())
            verdict.update(outcome='stopped',workflow_shipped=False,stop_note='fixture stop')
            (a/'verdict.json').write_text(json.dumps(verdict))
            if mutation!='one':
                for n in [2,3]:
                    shutil.copytree(a,h/f'attempt-{n}')
                    v=dict(verdict,attempt=n)
                    if mutation=='note' and n==2: v['stop_note']=''
                    (h/f'attempt-{n}/verdict.json').write_text(json.dumps(v))
                (h/'current-attempt').write_text('3\n')
        result=subprocess.run(['bash',script,'--attempt-gate',str(h)],capture_output=True,text=True,timeout=30)
        success=mutation in ['good','three']
        ok=(result.returncode==0)==success and 'ATTEMPT_GATE verdict='+expected in result.stdout
        reason={'disagree':'disagreement','criterion':'fork point is one value','missing':'ABSENT','manifest':'manifest','one':'1 of 3','note':'2 of 3'}
        if mutation in reason:ok=ok and reason[mutation] in result.stdout
        report(name,ok)
print(f'SELFTEST passed={passed} failed={failed}')
sys.exit(bool(failed) or not passed)
PY
}

if [ "${1:-}" = --selftest ] && [ "$#" -eq 1 ]; then selftest; exit; fi
attempt_gate() {
    python3 - "$SELF" "$1" <<'PY'
import json, pathlib, re, subprocess, sys
script=sys.argv[1]; home=pathlib.Path(sys.argv[2]); number='ABSENT'
try:
    number=(home/'current-attempt').read_text().strip()
    if not re.fullmatch('[1-9]',number): raise ValueError('current-attempt must be one digit 1-9')
    attempt=home/f'attempt-{number}'; collected=attempt/'collected'
    if not (collected/'DONE').is_file(): raise FileNotFoundError('collected/DONE')
    verdict=json.loads((attempt/'verdict.json').read_text())
    # Refuse paths outside the collected tree before asking sha256sum to read.
    lines=(collected/'MANIFEST.sha256').read_text().splitlines()
    if not lines: raise ValueError('manifest empty')
    for line in lines:
        if not re.fullmatch(r'[0-9a-f]{64} [ *].+',line): raise ValueError('manifest malformed')
        path=pathlib.Path(line[66:])
        if path.is_absolute() or '..' in path.parts: raise ValueError('manifest path outside collection')
    result=subprocess.run(['sha256sum','--quiet','-c','MANIFEST.sha256'],cwd=collected,capture_output=True,text=True,timeout=30)
    if result.returncode!=0: raise ValueError('manifest mismatch')
    rows=[json.loads(line) for line in (collected/'devflow/events.jsonl').read_text().splitlines()]
    shipped=any(e['event']=='workflow_shipped' and str(e['phase'])=='1' for e in rows)
    if verdict['outcome'] not in ['shipped','stopped'] or (verdict['outcome']=='shipped')!=shipped or verdict['workflow_shipped'] is not shipped:
        raise ValueError('verdict/events disagreement')
    if shipped:
        for args in [['--arm','positive'],['--arm','negative'],['--pair']]:
            result=subprocess.run(['bash',script,*args,str(attempt)],capture_output=True,text=True,timeout=30)
            print(result.stdout,end='')
            if result.returncode!=0 or not re.search(r'^CHECK passed=[1-9][0-9]* failed=0$',result.stdout,re.M):
                failures=[line for line in result.stdout.splitlines() if line.startswith('  FAIL ')]
                raise ValueError('; '.join(failures) or 'arm check failed or CHECK absent')
        print(f'ATTEMPT_GATE verdict=shipped attempt={number}')
    else:
        stopped=0
        for path in home.glob('attempt-*/verdict.json'):
            value=json.loads(path.read_text())
            if value['outcome']=='stopped' and isinstance(value['stop_note'],str) and value['stop_note'].strip(): stopped+=1
        if stopped<3: raise ValueError(f'{stopped} of 3 stopped attempts have a note')
        print(f'ATTEMPT_GATE verdict=cap_reached stopped_attempts={stopped}')
except (OSError,ValueError,KeyError,TypeError,subprocess.TimeoutExpired) as e:
    absent='ABSENT ' if isinstance(e,(OSError,KeyError)) else ''
    reason=str(e).replace('\n',' ')
    print(f'ATTEMPT_GATE verdict=not_ready attempt={number} reason={absent}{reason}')
    sys.exit(1)
PY
}

if [ "${1:-}" = --attempt-gate ] && [ "$#" -eq 2 ]; then attempt_gate "$2"; exit; fi
if [ "${1:-}" = --pair ] && [ "$#" -eq 2 ]; then set -- --arm pair "$2"; fi
if [ "$#" -ne 3 ] || [ "$1" != --arm ] || { [ "$2" != negative ] && [ "$2" != positive ] && [ "$2" != pair ]; }; then
    echo 'usage: check-evidence.sh --arm positive|negative <EV> | --pair <EV> | --attempt-gate <P49> | --selftest' >&2
    exit 2
fi
python3 - "$2" "$3" <<'PY'
import json, pathlib, re, sys
mode = sys.argv[1]
root = pathlib.Path(sys.argv[2])
if mode == 'negative': root = root / 'negctl'
refusal = '  [DOES NOT HOLD] GSD config can hold the chain flag — no `.planning/config.json` under the launch root — nowhere for the chain flag to live'
passed = failed = 0

def check(name, fn):
    global passed, failed
    try:
        if not fn(): raise ValueError('recorded value disagrees')
    except (OSError, ValueError, KeyError, TypeError, IndexError) as e:
        failed += 1
        absent = isinstance(e, (OSError, KeyError, IndexError))
        print(f'  FAIL {mode}: {name} — {"ABSENT " if absent else ""}{e}')
    else:
        passed += 1
        print(f'  ok   {mode}: {name}')

def snapshot(stage='define', last=False, armroot=None):
    armroot = root if armroot is None else armroot
    entries = [json.loads(line) for line in (armroot / 'ledger.jsonl').read_text().splitlines()]
    matches=[]
    for entry in entries:
        name=pathlib.Path(entry['dir']).name
        if not re.fullmatch(r'[0-9]{3,}-[a-z]+',name): raise ValueError('invalid snapshot basename')
        directory=armroot/'gates'/name
        if not directory.is_dir(): raise FileNotFoundError(str(directory))
        if entry['stage']==stage: matches.append(directory)
    if not matches: raise KeyError(stage+' snapshot')
    return matches[-1] if last else matches[0]

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

def fields(path):
    return dict(line.split('=',1) for line in path.read_text().splitlines() if '=' in line)

def sha(value):
    if not isinstance(value,str) or not re.fullmatch('[0-9a-f]{40,64}',value):
        raise ValueError('invalid or ABSENT SHA')
    return value

def event_rows():
    return [json.loads(line) for line in (root/'collected/devflow/events.jsonl').read_text().splitlines()]

def phase_events(kind):
    return [(i,e) for i,e in enumerate(event_rows()) if e['event']==kind and str(e['phase'])=='1']

def fork_point():
    values = [sha(fields(root/'prelaunch/fork-base.txt')['base_sha']),
              sha(fields(snapshot()/'git.txt')['worktree_head']),
              sha(fields(snapshot('ship')/'git.txt')['merge_base'])]
    print('NOTE fork_point base_sha={} define_head={} ship_merge_base={}'.format(*values))
    return len(set(values))==1

def gate_order():
    gates=phase_events('gate_fired')
    positions=[next((i for i,e in gates if e['stage']==stage),None) for stage in ['define','code','ship']]
    if any(i is None for i in positions): raise KeyError('required gate')
    merges=[i for i,e in phase_events('merge_result') if e['merged'] is True]
    return bool(merges) and positions[0]<positions[1]<positions[2]<merges[0]

def answers():
    resolved=phase_events('gate_resolved')
    for stage in ['define','code','ship']:
        entry=next((e for _,e in resolved if e['stage']==stage),None)
        if entry is None: raise KeyError(stage+' gate_resolved')
        if (entry['responded_by']=='--yes-ship') != (stage=='ship'): return False
    return True

def merged():
    git=fields(root/'collected/git.txt')
    parents=git['integration_merge_parents'].split()
    feature=sha(fields(snapshot('ship',last=True)/'git.txt')['feature_head'])
    return (git['integration_merge_count']=='1' and len(parents)==2 and
            sha(parents[1])==feature and sha(git['integration_head'])!=sha(fields(root/'prelaunch/fork-base.txt')['base_sha']))

def positive_checks():
    check('base_branch is integration',lambda:json.loads((snapshot()/'devflow/state-01.json').read_text())['base_branch']=='integration')
    check('base-branch note names devflow.toml',lambda:'note: base branch is `integration` (from devflow.toml (base_branch))' in (root/'start.stdout').read_text())
    check('config condition holds',lambda:'  [holds] GSD config can hold the chain flag' in (root/'start.stdout').read_text().splitlines())
    check('fork point is one value',fork_point)
    check('gates fired in order define, code, ship',gate_order)
    check('ship gate answered by --yes-ship',answers)
    check('shipped',lambda:bool(phase_events('workflow_shipped')))
    check('merged into integration',merged)
    check('main untouched locally',lambda:sha(fields(root/'collected/git.txt')['main_head'])==sha(fields(root/'prelaunch/fork-base.txt')['main_sha']))
    check('PR targets main from the feature branch',lambda:json.loads((root/'collected/pr.json').read_text())['baseRefName']=='main' and json.loads((root/'collected/pr.json').read_text())['headRefName']=='feature/phase-01')
    check('merge_result records a merge',lambda:any(e['merged'] is True for _,e in phase_events('merge_result')))
    try:
        gates=phase_events('gate_fired')
        print(f'NOTE gate_census count={len(gates)} stages='+','.join(e['stage'] for _,e in gates))
        for _,e in gates:
            unexpected=e['stage'] not in ['define','code','ship']
            print(f'NOTE gate {e["stage"]} unexpected={str(unexpected).lower()}')
            if unexpected: print(f'NOTE deviation: gate {e["stage"]} unexpected=true')
    except (OSError,ValueError,KeyError,TypeError) as e:
        print(f'NOTE gate_census ABSENT {e}')

def binary_pair():
    expected=json.loads((root/'attempt.json').read_text())['bin_sha256']
    if not isinstance(expected,str) or not re.fullmatch('[0-9a-f]{64}',expected): raise ValueError('invalid binary sha256')
    return (root/'negctl/bin_sha256').read_text().strip()==expected

def build_commit(path):
    rows=[json.loads(line) for line in path.read_text().splitlines()]
    commits=[e['commit'] for e in rows if e['event']=='workflow_started' and str(e['phase'])=='1']
    if not commits: raise KeyError('workflow_started commit')
    return sha(commits[0])

def branch_at(armroot):
    return json.loads((snapshot(armroot=armroot)/'devflow/state-01.json').read_text())['base_branch']

def branches_differ():
    positive=branch_at(root);negative=branch_at(root/'negctl')
    if not isinstance(positive,str) or not isinstance(negative,str) or positive==negative:
        raise ValueError('measurement broken: discriminating base_branch absent or arms agree')
    return True

def pair_checks():
    check('same binary in both arms',binary_pair)
    check('same build commit in both arms',lambda:build_commit(root/'collected/devflow/events.jsonl')==build_commit(root/'negctl/events.jsonl'))
    check('arms disagree on base_branch',branches_differ)

if mode == 'positive':
    positive_checks()
elif mode == 'pair':
    pair_checks()
else:
    check('DOES NOT HOLD line present', lambda: refusal in (root / 'start.stdout').read_text().splitlines())
    check('no base-branch note', lambda: 'note: base branch is' not in (root / 'start.stdout').read_text())
    check('base_branch is develop', lambda: json.loads((snapshot() / 'devflow/state-01.json').read_text())['base_branch'] == 'develop')
    check('forked from the recorded develop', fork)
    check('aborted after the gate', aborted)
print(f'CHECK passed={passed} failed={failed}')
sys.exit(bool(failed) or not passed)
PY
