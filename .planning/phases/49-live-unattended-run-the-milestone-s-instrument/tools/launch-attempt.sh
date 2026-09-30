#!/usr/bin/env bash
# Prepare and authenticate an attempt; start only after the operator's repo step.
# Auth records contain variable names and derived fields, never credential values.
set -euo pipefail
T="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELF="$T/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(git -C "$T" rev-parse --show-toplevel)"

selftest() {
    python3 - "$SELF" "$ROOT" <<'PY'
import hashlib,json,os,pathlib,re,subprocess,sys,tempfile,uuid
script,source=sys.argv[1:]
passed=failed=0
with tempfile.TemporaryDirectory() as tmp:
    root=pathlib.Path(tmp); repo=root/'source';repo.mkdir();(repo/'crates').mkdir()
    (repo/'crates/probe.rs').write_text('// fixture\n');(repo/'.gitignore').write_text('target/\n')
    env=os.environ.copy()
    for k in subprocess.check_output(['git','rev-parse','--local-env-vars'],text=True).splitlines():env.pop(k,None)
    env.update(GIT_CONFIG_GLOBAL='/dev/null',GIT_CONFIG_NOSYSTEM='1',P49_SELFTEST_SENTINEL=uuid.uuid4().hex)
    def run(args,**kw):return subprocess.run(args,env=env,capture_output=True,text=True,timeout=60,**kw)
    def git(*args,at=repo):
        result=run(['git','-C',str(at),*args]);assert result.returncode==0,result.stderr
        return result.stdout.strip()
    git('init','-q');git('config','user.name','Fixture');git('config','user.email','fixture@localhost')
    git('add','.');git('commit','-qm','chore: build fixture')
    fakes=root/'fakes';fakes.mkdir();env['P49_FAKE_LOG']=str(root/'units.log')
    fake_scripts={
    'cargo':'''#!/bin/bash
set -eu
mkdir -p target/debug
printf '#!/bin/sh\\nprintf "{}\\\\n"\\n' >target/debug/devflow
chmod +x target/debug/devflow
''',
    'systemd-run':'''#!/bin/bash
set -eu
printf '%s\\n' "$*" >>"$P49_FAKE_LOG"
wait=false; directory=""; vars=()
while [ "$#" -gt 0 ]; do
 case "$1" in --wait) wait=true ;; --setenv=*) vars+=("${1#--setenv=}") ;; --working-directory=*) directory="${1#--working-directory=}" ;; --) shift; break ;; esac
 shift
done
if [ "$wait" = true ]; then cd "$directory"; env "${vars[@]}" "$@"; fi
''',
    'claude':'''#!/bin/sh
printf '{"type":"system","subtype":"init","apiKeySource":"ANTHROPIC_API_KEY"}\\n'
printf '{"type":"result","is_error":false,"result":"P49_AUTH_OK"}\\n'
''',
    'gh':'''#!/bin/sh
if [ "$1" = repo ]; then printf '{"nameWithOwner":"tester/p49-live-attempt-1","visibility":"PRIVATE"}\\n'; else printf "Logged in to github.com account tester\\nToken scopes: 'gist', 'read:org', 'repo'\\n"; fi
''',
    'systemctl':'''#!/bin/sh
printf 'inactive\\n'
exit 3
'''}
    for name,text in fake_scripts.items():
        p=fakes/name;p.write_text(text);p.chmod(0o755)
    home=root/'home';ev=home/'attempt-1'
    def invoke(command,number=1,h=home,extra=()):
        args=['bash',script,command,'--attempt',str(number),'--home',str(h),'--fake-cmds',str(fakes)]
        if command=='prepare':args+=['--repo',str(repo),'--scaffold',source+'/scripts/scratch-dogfood-repo.sh']
        return run(args+list(extra))
    def report(name,ok):
        global passed,failed
        print(('  ok   ' if ok else '  FAIL ')+name+('' if ok else ' — expected record absent or wrong'))
        passed+=int(ok);failed+=int(not ok)
    result=invoke('prepare');ok=False
    if result.returncode==0 and (ev/'attempt.json').is_file():
        a=json.loads((ev/'attempt.json').read_text());b=pathlib.Path(a['bin'])
        ok=(a['attempt']==1 and a['launch']==str(ev/'project') and a['base_branch']=='integration' and b.parent==home/'bin' and
            hashlib.sha256(b.read_bytes()).hexdigest()==a['bin_sha256'] and b.stat().st_mode&0o777==0o555 and
            'build_dirty=false' in (ev/'identity.txt').read_text() and
            re.search(r'^CHECK passed=[1-9][0-9]* failed=0$',(ev/'prelaunch/check.txt').read_text(),re.M) and (home/'current-attempt').read_text().strip()=='1')
    report('prepare: builds, pins, scaffolds and records the attempt',bool(ok))
    result=invoke('probe-auth');ok=False
    if result.returncode==0 and (ev/'prelaunch/auth-probe.txt').is_file():
        names=(ev/'prelaunch/agent-env-names.txt').read_text().splitlines();record=(ev/'prelaunch/auth-probe.txt').read_text()
        ok=all(re.fullmatch('[A-Za-z_][A-Za-z0-9_]*',n) for n in names) and all(n in names for n in ['P49_SELFTEST_SENTINEL','PATH','HOME'])
        ok=ok and all(s in record for s in ['probe_exit=0','is_error=false','reply_ok=yes','auth_source=ANTHROPIC_API_KEY'])
        ok=ok and not list((ev/'prelaunch').glob('authprobe.*'))
        ok=ok and env['P49_SELFTEST_SENTINEL'] not in result.stdout+result.stderr
        ok=ok and not any(env['P49_SELFTEST_SENTINEL'].encode() in p.read_bytes() for p in ev.rglob('*') if p.is_file())
    report('probe-auth: environment recorded by name only and authentication established',bool(ok))
    ok=False
    if (ev/'project/.git').exists():
        bare=root/'origin.git';git('init','--bare','-q',str(bare));git('remote','add','origin',str(bare),at=ev/'project')
        git('fetch',str(ev/'project'),'main:main','integration:integration',at=bare)
        (root/'units.log').write_text('')
        result=invoke('start')
        lines=(root/'units.log').read_text().splitlines()
        if result.returncode==0 and len(lines)==3:
            ok=all(unit in lines[i] for i,unit in enumerate(['p49-a1-authprobe','p49-a1-surv','p49-a1-start']))
            ok=ok and 'surv-watch.py watch' in lines[1] and 'RemainAfterExit=yes' in lines[2] and 'KillMode=process' in lines[2]
            ok=ok and 'start --phase 1 --agent claude --mode auto --yes-ship' in lines[2] and json.loads((ev/'attempt.json').read_text())['bin'] in lines[2]
            ok=ok and 'base_sha='+git('rev-parse','integration',at=ev/'project') in (ev/'prelaunch/fork-base.txt').read_text()
            ok=ok and json.loads((ev/'prelaunch/repo.json').read_text())['visibility']=='PRIVATE'
    report('start: both units launched with the notify hook and the pinned binary',bool(ok))
print(f'SELFTEST passed={passed} failed={failed}')
sys.exit(bool(failed) or not passed)
PY
}
if [ "${1:-}" = --selftest ]; then selftest; exit; fi
# RED stub: emits no records and launches nothing.
exit 0
