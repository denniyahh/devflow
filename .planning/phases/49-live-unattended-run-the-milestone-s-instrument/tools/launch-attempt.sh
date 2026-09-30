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
    import shutil
    def no_units(): return not (root/'units.log').read_text().strip()
    def reject(name,command,reason,**kw):
        (root/'units.log').write_text('')
        result=invoke(command,**kw)
        report(name,result.returncode!=0 and reason in result.stdout+result.stderr and no_units())
    # Copy a prepared project; the fake operator step only uses local bare Git.
    baseline=root/'baseline';shutil.copytree(home,baseline)
    def restore():
        shutil.rmtree(home);shutil.copytree(baseline,home)
        (root/'units.log').write_text('')
    # Clear the relaunch sentinel in each clone, without changing tracked files.
    (baseline/'attempt-1/start.stdout').unlink()
    gh=fakes/'gh';gh_default=gh.read_text();claude=fakes/'claude';claude_default=claude.read_text()
    gh.write_text(gh_default.replace('PRIVATE','PUBLIC'))
    restore();reject('control: public repository refused','start','not private')
    gh.write_text(gh_default)
    own=git('remote','get-url','origin',at=pathlib.Path(source))
    own=re.sub(r'^.*github.com[:/]','',own);own=re.sub(r'\.git$','',own)
    gh.write_text(gh_default.replace('tester/p49-live-attempt-1',own))
    restore();reject('control: repository matching this devflow checkout refused','start','this DevFlow repository')
    gh.write_text(gh_default)
    restore();git('commit','--allow-empty','-qm','chore: unpushed base',at=ev/'project')
    reject('control: unpushed integration refused','start','integration not pushed')
    gh.write_text(gh_default.replace("'repo'","'gist'"))
    restore();reject('control: token without repo scope refused','start','repo scope')
    gh.write_text(gh_default)
    restore();claude.write_text(claude_default.replace('"is_error":false','"is_error":true').replace('P49_AUTH_OK','authentication failed')+'exit 1\n')
    result=invoke('start');log=(root/'units.log').read_text();record=(ev/'prelaunch/auth-probe.txt').read_text()
    report('control: failed auth probe refuses start and launches no run unit',result.returncode!=0 and 'auth probe failed' in result.stderr and 'p49-a1-authprobe' in log and 'p49-a1-surv' not in log and 'p49-a1-start' not in log and 'is_error=true' in record and 'reply_ok=no' in record)
    # The failing response must not report a successful authentication reply.
    claude.write_text(claude_default.split("printf '{\"type\":\"result\"")[0])
    restore();result=invoke('probe-auth');record=(ev/'prelaunch/auth-probe.txt').read_text()
    report('control: probe output without a result line is ABSENT, not a pass',result.returncode!=0 and 'auth probe failed' in result.stderr and 'is_error=ABSENT' in record and 'reply_ok=no' in record)
    claude.write_text(claude_default)
    restore()
    before=git('rev-parse','integration',at=ev/'project')
    mutate='if [ -n "${P49_MUTATE_PROJECT:-}" ]; then git -C "$P49_MUTATE_PROJECT" commit --allow-empty -qm "chore: base changes during auth"; git --git-dir="$P49_MUTATE_REMOTE" fetch -q "$P49_MUTATE_PROJECT" integration:integration; fi\n'
    claude.write_text(claude_default.replace("#!/bin/sh\n", "#!/bin/sh\n"+mutate))
    env['P49_MUTATE_PROJECT']=str(ev/'project');env['P49_MUTATE_REMOTE']=str(bare)
    result=invoke('start')
    after=git('rev-parse','integration',at=ev/'project')
    observed=dict(line.split('=',1) for line in (ev/'prelaunch/fork-base.txt').read_text().splitlines())['base_sha']
    report('start: fork point read after authentication',result.returncode==0 and before!=after and observed==after)
    del env['P49_MUTATE_PROJECT'];del env['P49_MUTATE_REMOTE'];claude.write_text(claude_default)
    restore();(home/'attempt-2').mkdir();shutil.copy(ev/'attempt.json',home/'attempt-2/attempt.json')
    reject('control: attempt number mismatch refused','start','attempt number mismatch',number=2)
    restore();result=invoke('start');assert result.returncode==0,result.stderr
    reject('control: relaunch of the same attempt refused','start','already launched')
    (repo/'crates/probe.rs').write_text('// changed\n')
    reject('control: dirty build tree refused','prepare','dirty build tree',h=root/'dirty-home')
    git('checkout','--','crates/probe.rs')
    restore();reject('control: attempt without a verdict blocks the next prepare','prepare','collect it first',number=2)
    cap=root/'cap'
    for n in [1,2,3]:
        a=cap/f'attempt-{n}';a.mkdir(parents=True)
        (a/'verdict.json').write_text(json.dumps({'outcome':'stopped','stop_note':'fixture stop'}))
    reject('control: cap of three stopped attempts refuses prepare','prepare','D-11',number=4,h=cap)
    restore()
    def inventory():return {str(p.relative_to(ev)):hashlib.sha256(p.read_bytes()).hexdigest() for p in ev.rglob('*') if p.is_file()}
    before=inventory();result=invoke('status')
    report('status: read-only report names both units',result.returncode==0 and 'p49-a1-start' in result.stdout and 'p49-a1-surv' in result.stdout and before==inventory())
    (repo/'notes.txt').write_text('untracked fixture\n')
    h=root/'notes-home';result=invoke('prepare',h=h)
    report('control: dirt outside crates is recorded, not refused',result.returncode==0 and 'build_dirty=true' in (h/'attempt-1/identity.txt').read_text())
print(f'SELFTEST passed={passed} failed={failed}')
sys.exit(bool(failed) or not passed)
PY
}
if [ "${1:-}" = --selftest ]; then selftest; exit; fi
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
# Runs only inside the auth unit; raw output never leaves its scratch directory.
if [ "${1:-}" = __auth-probe-inner ] && [ "$#" -eq 2 ]; then
    compgen -e | LC_ALL=C sort >"$2/env-names"
    rc=0
    timeout 180 claude -p 'Reply with exactly P49_AUTH_OK and nothing else.' --tools '' \
        --no-session-persistence --output-format stream-json --verbose >"$2/claude.jsonl" 2>/dev/null || rc=$?
    printf '%s\n' "$rc" >"$2/claude.exit"
    exit 0
fi
command="${1:-}"; [ "$#" -gt 0 ] || die 'subcommand required'; shift
attempt="" home="$HOME/.cache/devflow-phase49" repo="$ROOT" scaffold="" fake_cmds=""
while [ "$#" -gt 0 ]; do
    [ "$#" -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"
    case "$1" in
        --attempt) attempt="$2" ;; --home) home="$2" ;; --repo) repo="$2" ;;
        --scaffold) scaffold="$2" ;; --fake-cmds) fake_cmds="$2" ;;
        *) die "unknown argument: $1" ;;
    esac
    shift 2
done
[[ "$attempt" =~ ^[1-9]$ ]] || die '--attempt must be one digit 1-9'
home="$(realpath -m -- "$home")"; repo="$(realpath -e -- "$repo")"
ev="$home/attempt-$attempt"; project="$ev/project"
scaffold="${scaffold:-$repo/scripts/scratch-dogfood-repo.sh}"
if [ -n "$fake_cmds" ]; then
    fake_cmds="$(realpath -e -- "$fake_cmds")"
    [ -x "$fake_cmds/claude" ] && [ -x "$fake_cmds/systemd-run" ] || die 'fake claude and systemd-run required'
    export PATH="$fake_cmds:$PATH"
fi
require_attempt() { [ -f "$ev/attempt.json" ] || die 'attempt.json ABSENT'; }
set_unit_env() {
    printf -v notify 'timeout 60 bash %q %q %q' "$T/snapshot-hook.sh" "$project" "$ev"
    UNIT_ENV=("--setenv=PATH=$PATH" "--setenv=HOME=$HOME" "--setenv=DEVFLOW_GATE_NOTIFY_CMD=$notify")
}
prepare() {
    [ ! -e "$ev" ] || die "attempt already exists: $ev"
    local highest=0 stopped=0 dir n outcome dirty sha binary status
    shopt -s nullglob
    for dir in "$home"/attempt-*; do
        [ -d "$dir" ] || continue
        n="${dir##*/attempt-}"
        [[ "$n" =~ ^[1-9]$ ]] || die 'invalid earlier attempt directory'
        [ "$n" -le "$highest" ] || highest="$n"
        [ -f "$dir/verdict.json" ] || die "collect it first: attempt-$n has no verdict"
        outcome="$(jq -er '.outcome | select(. == "stopped" or . == "shipped")' "$dir/verdict.json")" || die 'earlier verdict malformed'
        jq -e 'has("stop_note")' "$dir/verdict.json" >/dev/null || die 'earlier verdict stop_note ABSENT'
        if [ "$outcome" = stopped ] && jq -e '.stop_note | type == "string" and length > 0' "$dir/verdict.json" >/dev/null; then stopped=$((stopped+1)); fi
    done
    [ "$stopped" -lt 3 ] || die 'D-11: cap of three stopped attempts reached'
    [ "$attempt" -eq "$((highest+1))" ] || die 'attempt must follow the highest existing attempt'
    status="$(git -C "$repo" status --porcelain -- crates Cargo.toml Cargo.lock)"
    [ -z "$status" ] || die 'dirty build tree: crates or Cargo files changed'
    dirty=false; [ -z "$(git -C "$repo" status --porcelain)" ] || dirty=true
    (cd "$repo" && timeout 900 cargo build -p devflow --locked)
    binary="$repo/target/debug/devflow"; [ -x "$binary" ] || die 'build binary ABSENT'
    sha="$(sha256sum "$binary" | cut -d ' ' -f1)"
    mkdir -p "$home/bin" "$ev/prelaunch"
    binary="$home/bin/devflow-${sha:0:12}"
    if [ -e "$binary" ]; then
        [ "$(sha256sum "$binary" | cut -d ' ' -f1)" = "$sha" ] || die 'pinned binary sha256 mismatch'
    else
        install -m 0555 "$repo/target/debug/devflow" "$binary"
    fi
    printf 'build_commit=%s\nbuild_dirty=%s\nbin_sha256=%s\n' "$(git -C "$repo" rev-parse HEAD)" "$dirty" "$sha" >"$ev/identity.txt"
    bash "$scaffold" --live-payload "$project"
    bash "$scaffold" --check "$project" >"$ev/prelaunch/check.txt" || die 'payload check failed'
    tail -1 "$ev/prelaunch/check.txt" | rg -qx 'CHECK passed=[1-9][0-9]* failed=0' || die 'payload CHECK line ABSENT'
    jq -n --argjson attempt "$attempt" --arg bin "$binary" --arg bin_sha256 "$sha" --arg launch "$project" \
        '{attempt:$attempt,bin:$bin,bin_sha256:$bin_sha256,launch:$launch,base_branch:"integration"}' >"$ev/attempt.json"
    printf 'start=p49-a%s-start\nsurv=p49-a%s-surv\nauthprobe=p49-a%s-authprobe\n' "$attempt" "$attempt" "$attempt" >"$ev/units.txt"
    printf '%s\n' "$attempt" >"$home/current-attempt.tmp"; mv "$home/current-attempt.tmp" "$home/current-attempt"
    echo "Prepared attempt $attempt. Next: run the negative arm; create the private repository and push main/integration; then start."
}
probe_auth() (
    require_attempt
    local scratch unit="p49-a$attempt-authprobe" rc=0
    scratch="$(mktemp -d "$ev/prelaunch/authprobe.XXXXXX")"
    trap 'rm -rf -- "$scratch"' EXIT
    set_unit_env
    timeout 210 systemd-run --user --unit "$unit" --wait --collect --quiet --working-directory="$scratch" \
        "${UNIT_ENV[@]}" -- bash "$SELF" __auth-probe-inner "$scratch" >/dev/null 2>&1 || rc=$?
    [ -f "$scratch/env-names" ] || die 'auth probe failed: env-names ABSENT'
    if rg -q -v '^[A-Za-z_][A-Za-z0-9_]*$' "$scratch/env-names"; then die 'auth probe failed: invalid environment name'; fi
    cp "$scratch/env-names" "$ev/prelaunch/agent-env-names.txt"
    python3 - "$scratch" "$ev/prelaunch/auth-probe.txt" "$unit" "$rc" <<'PY'
import json,pathlib,re,sys,time
scratch=pathlib.Path(sys.argv[1]);source='ABSENT';is_error='ABSENT';reply='no';exit_code='ABSENT'
if (scratch/'claude.exit').is_file():exit_code=(scratch/'claude.exit').read_text().strip()
if (scratch/'claude.jsonl').is_file():
    for line in (scratch/'claude.jsonl').read_text().splitlines():
        try:obj=json.loads(line)
        except ValueError:continue
        if not isinstance(obj,dict) or 'type' not in obj:continue
        if obj['type']=='system' and obj.get('subtype')=='init' and source=='ABSENT':
            if 'apiKeySource' in obj and isinstance(obj['apiKeySource'],str) and re.fullmatch('[A-Za-z0-9_-]+',obj['apiKeySource']):source=obj['apiKeySource']
        if obj['type']=='result':
            is_error=str(obj['is_error']).lower() if 'is_error' in obj and isinstance(obj['is_error'],bool) else 'ABSENT'
            reply='yes' if 'result' in obj and isinstance(obj['result'],str) and 'P49_AUTH_OK' in obj['result'] else 'no'
record=f'unit={sys.argv[3]}\nprobe_exit={exit_code}\nis_error={is_error}\nreply_ok={reply}\nauth_source={source}\nprobe_timeout_secs=180\nmeasured_at={int(time.time())}\n'
pathlib.Path(sys.argv[2]).write_text(record)
if sys.argv[4]!='0' or exit_code!='0' or is_error!='false' or reply!='yes':
    print(f'auth probe failed: probe_exit={exit_code} is_error={is_error} reply_ok={reply}',file=sys.stderr)
    sys.exit(1)
print(f'Auth probe passed: probe_exit={exit_code} is_error={is_error} reply_ok={reply} auth_source={source}')
PY
)
start() {
    require_attempt
    [ -f "$home/current-attempt" ] && [ "$(cat "$home/current-attempt")" = "$attempt" ] || die 'attempt number mismatch'
    [ ! -e "$ev/start.stdout" ] || die 'already launched'
    bash "$scaffold" --check "$project" >"$ev/prelaunch/check-start.txt" || die 'payload check failed'
    local bin expected actual origin own repo_name remote main_sha base_sha auth
    bin="$(jq -er '.bin | select(type == "string" and length > 0)' "$ev/attempt.json")" || die 'bin ABSENT'
    expected="$(jq -er '.bin_sha256 | select(type == "string" and length > 0)' "$ev/attempt.json")" || die 'bin_sha256 ABSENT'
    actual="$(sha256sum "$bin" | cut -d ' ' -f1)"; [ "$actual" = "$expected" ] || die 'binary sha256 mismatch'
    origin="$(git -C "$project" remote get-url origin)" || die 'origin ABSENT'
    [ -n "$origin" ] || die 'origin ABSENT'
    (cd "$project" && timeout 30 gh repo view --json nameWithOwner,visibility) >"$ev/prelaunch/repo.json"
    jq -e '.visibility == "PRIVATE"' "$ev/prelaunch/repo.json" >/dev/null || die 'repository not private'
    repo_name="$(jq -er '.nameWithOwner | select(type == "string" and length > 0)' "$ev/prelaunch/repo.json")" || die 'repository name ABSENT'
    own="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
    own="$(printf '%s\n' "$own" | sed -E 's@^.*github.com[:/]@@; s@\.git$@@')"
    [ "$repo_name" != "$own" ] || die 'refusing this DevFlow repository'
    remote="$(timeout 30 git -C "$project" ls-remote origin refs/heads/main refs/heads/integration)" || die 'remote heads unreadable'
    main_sha="$(git -C "$project" rev-parse main)";base_sha="$(git -C "$project" rev-parse integration)"
    printf '%s\n' "$remote" | rg -qx "${main_sha}[[:space:]]+refs/heads/main" || die 'main not pushed'
    printf '%s\n' "$remote" | rg -qx "${base_sha}[[:space:]]+refs/heads/integration" || die 'integration not pushed'
    auth="$(timeout 30 gh auth status 2>&1)" || die 'gh auth status failed'
    printf '%s\n' "$auth" | sed -E 's/\x1B\[[0-9;]*[[:alpha:]]//g' | rg 'Logged in to|Token scopes:' >"$ev/prelaunch/gh-auth.txt" || die 'gh auth fields ABSENT'
    rg 'Token scopes:' "$ev/prelaunch/gh-auth.txt" | rg -q "(^|[' ,])repo([', ]|$)" || die 'repo scope required'
    probe_auth
    printf 'base_sha=%s\nmain_sha=%s\n' "$base_sha" "$main_sha" >"$ev/prelaunch/fork-base.txt"
    mkdir -p "$ev/surv";set_unit_env
    timeout 20 systemd-run --user --unit "p49-a$attempt-surv" --setenv="PATH=$PATH" --setenv="HOME=$HOME" -- \
        python3 "$T/surv-watch.py" watch "$project" "$ev/surv"
    : >"$ev/start.stdout"
    # shellcheck disable=SC2016 # Expanded by the launched sh, not this shell.
    timeout 20 systemd-run --user --unit "p49-a$attempt-start" -p RemainAfterExit=yes -p KillMode=process \
        "${UNIT_ENV[@]}" -- sh -c 'exec "$0" start --phase 1 --agent claude --mode auto --yes-ship "$1" >"$2/start.stdout" 2>"$2/start.stderr"' \
        "$bin" "$project" "$ev"
}
case "$command" in
    prepare) prepare ;; probe-auth) probe_auth ;; start) start ;;
    status)
        require_attempt
        for unit in "p49-a$attempt-start" "p49-a$attempt-surv"; do
            state="$(timeout 10 systemctl --user is-active "$unit" 2>/dev/null || true)"
            printf '%s %s\n' "$unit" "${state:-ABSENT}"
        done ;;
    *) die "unknown subcommand: $command" ;;
esac
