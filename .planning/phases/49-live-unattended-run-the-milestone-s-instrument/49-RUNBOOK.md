# Phase 49 — One Live Attempt

## What an attempt is

An attempt is a real Claude-driven DevFlow run against a disposable Python/GSD project, never this
DevFlow repository. The operator owns repository creation, pushing, the Define/Code approvals and
cleanup; the run records whether this pinned binary reaches Ship.

Run from a plain Bash terminal outside every agent session. While the run is live, perform no manual
git operation against its project tree. Capture, do not narrate: the tools keep the original outputs,
gate snapshots and events. A stall or failure is a result; preserve each attempt's record and never
replace it with a retry. Attempts are numbered 1–9; at most three may end stopped (D-11).

## Before every attempt

Open a plain terminal in this phase's worktree. The private API-key file and configured Claude
apiKeyHelper must remain available; never paste the key into the project, evidence or terminal history.

```bash
cd "$HOME/Github/devflow/.worktrees/phase-49"
W=$(git rev-parse --show-toplevel)
T="$W/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools"
P49="$HOME/.cache/devflow-phase49"
N=1
EV="$P49/attempt-$N"
gh auth status
```

Use the next attempt number, not necessarily 1. The Token scopes line must include `repo`; otherwise:

```bash
gh auth refresh -s repo
```

## 1. Prepare

Build from this phase's tree, record build identity, install the binary owner-executable/read-only,
and scaffold/check the payload. The destination must not already exist; never delete an old attempt
just to make preparation pass.

```bash
bash "$T/launch-attempt.sh" prepare --attempt "$N"
BIN=$(jq -er '.bin | select(type == "string" and length > 0)' "$EV/attempt.json")
```

## 2. Negative arm

The attempt's pinned binary must first produce the unconfigured project's Define refusal and abort.
Its evidence belongs under `$EV/negctl/`.

```bash
bash "$T/negctl.sh" --attempt "$N"
bash "$T/check-evidence.sh" --arm negative "$EV"
```

The checker receives the attempt directory, selects the negative ledger and resolves snapshots under
`$EV/negctl/gates/`. A FAIL stops this procedure before launching the positive arm.

## 3. Create and push the throwaway repository (operator only)

Create a private `p49-live-attempt-$N` repository. These commands create external GitHub state;
execute them yourself after checking the account reported above.

```bash
ACCOUNT=$(gh api user --jq .login)
REPO="$ACCOUNT/p49-live-attempt-$N"
gh repo create "$REPO" --private
git -C "$EV/project" remote add origin "https://github.com/$REPO.git"
git -C "$EV/project" push -u origin main
git -C "$EV/project" push origin integration
(cd "$EV/project" && gh repo view --json nameWithOwner,visibility)
```

Confirm `PRIVATE` and the intended owner/name. Keep both pushed branches: main is the PR target;
integration is DevFlow's local merge base and target. The launcher refuses this DevFlow repository.

## 4. Launch

```bash
bash "$T/launch-attempt.sh" start --attempt "$N"
bash "$T/launch-attempt.sh" status --attempt "$N"
```

Start first runs one bounded Claude authentication call in transient unit `p49-a$N-authprobe`, with
exactly the run's PATH, HOME and notify-command settings. It launches the watcher and run only when
authentication succeeds. On `auth probe failed`, neither run unit launched and the attempt is unused.
Read only the derived fields and variable names:

```bash
cat "$EV/prelaunch/auth-probe.txt"
cat "$EV/prelaunch/agent-env-names.txt"
jq -r '(.env // {}) | keys[]' ~/.claude/settings.json
jq -r 'has("apiKeyHelper")' ~/.claude/settings.json
find "$EV/project" -path '*/.claude/settings*.json'
```

Also inspect env key names and apiKeyHelper presence in `/etc/claude-code/managed-settings.json`, if
present, and every project settings file found above, including the phase worktree. The scaffold
creates none. Never display environment values or raw auth output.

Credential precedence is cloud-provider switch, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_API_KEY,
apiKeyHelper, CLAUDE_CODE_OAUTH_TOKEN, Anthropic profiles, then the /login credential. A variable can
come from the user manager or a settings env block; a names file cannot show the latter. At planning
time the API-key name existed in both sources; subsequent value checks found both empty and the
operator chose the private file-backed helper. auth_source reports the mechanism, not the account
or which source provided a value. Any credential-routing change is the operator's choice, outside
the project/evidence; then rerun start. To repeat just the probe:

```bash
bash "$T/launch-attempt.sh" probe-auth --attempt "$N"
```

## 5. Answer the Define and Code gates

A desktop notification, a new ledger line and gate list identify a waiting gate. Gates wait three
days by default; do not mistake that fixed timeout for progress or run reliability.

```bash
"$BIN" gate list "$EV/project"
"$BIN" gate show 1 --stage define "$EV/project"
"$BIN" gate approve 1 --stage define --note "attempt $N: seeded blocking-human checkpoint is intentional" --project "$EV/project"
```

Wait for Code and inspect its own context before approving:

```bash
"$BIN" gate list "$EV/project"
"$BIN" gate show 1 --stage code "$EV/project"
"$BIN" gate approve 1 --stage code --note "attempt $N: this approval records the checkpoint set" --project "$EV/project"
```

The Code approval records the auto-decide set DECN-03 needs. Do not pre-write responses or answer
any other gate as though it were expected. Approvals require a live waiter holding the phase lock.

## 6. Wait for the end

```bash
"$BIN" status "$EV/project"
"$BIN" evidence --phase 1 --json --root "$EV/project"
tail -30 "$EV/project/.devflow/events.jsonl"
```

Expect Define, Plan preserving the authored plan, Code, the seeded checkpoint and auto-decide
resume, Validate, then Ship. Ship reviews and opens a PR to main; its gate is authorized by the
launch's `--yes-ship`, followed by local merge into integration, tag and changelog. Completion is a
`workflow_shipped` event, not a quiet terminal or a stage's success message.

Warning signs: delivery canary refusing after Define approval, a review loop-back, Ship blocked by
security/verification/dirty-tree checks, or a MAJOR-bump gate. A waiting unexpected gate and a dead
run are handled below; keep their original evidence.

## 7. Collect and judge the attempt

After the run ended, stop the observer:

```bash
systemctl --user stop "p49-a$N-surv"
python3 "$T/surv-watch.py" analyze "$EV/surv"
```

Read analysis.txt without inferring interleaving from absence of observations. After confirming
workflow_shipped and no monitor/agent remains, release the retained exited unit:

```bash
systemctl --user stop "p49-a$N-start"
systemctl --user reset-failed "p49-a$N-start"
```

For a shipped run:

```bash
bash "$T/collect-attempt.sh" --attempt "$N"
```

For an intentionally aborted stopped run, supply its actual reason instead:

```bash
bash "$T/collect-attempt.sh" --attempt "$N" --stop-note "aborted: describe the observed failure"
```

If state remains after a dead run, use the death-confirmation procedure below instead. Then judge:

```bash
bash "$T/check-evidence.sh" --attempt-gate "$P49"
```

If the verdict says shipped but the attempt gate says not_ready (for example, a transient GitHub
lookup omitted collected/pr.json), do not label that run stopped or immediately spend another
attempt. Preserve the entire collection and verdict, diagnose the exact checker reason and retry
only the collection from the same ended project. Choose a fresh archive suffix:

```bash
STAMP=$(date +%s)
if [ -e "$EV/collected.before-$STAMP" ] || [ -e "$EV/verdict.before-$STAMP.json" ]; then
  printf 'Archive suffix already exists; choose a fresh STAMP and stop here\n'
else
  mv "$EV/collected" "$EV/collected.before-$STAMP" &&
  mv "$EV/verdict.json" "$EV/verdict.before-$STAMP.json" &&
  bash "$T/collect-attempt.sh" --attempt "$N" &&
  bash "$T/check-evidence.sh" --attempt-gate "$P49"
fi
```

Preserve both versions and record why collection was repeated. The collector otherwise returns
already collected at DONE. Do not rewrite collected files to satisfy a manifest or overwrite old
evidence; a genuine criterion failure still requires diagnosis and the existing fix/stop decision.

## 8. Close the checkpoint or stop the attempt

Reply `done` to the Phase 49 checkpoint only after `verdict=shipped` or `verdict=cap_reached`.
A shipped verdict establishes criteria 1–3 for this attempt; cap reached is three documented stopped
attempts and does not meet VERIFY-01. A typed reply alone establishes neither.

On `verdict=not_ready`, do not reply done. If the attempt stopped below the cap, end the current
execute-phase session, confirm the fix vehicle described below, fix it and resume with
`/gsd-execute-phase 49`. The checkpoint plan has no SUMMARY or commits and restarts at its single
human task for the next attempt. Do not alter GSD state to simulate closure.

## If a gate you did not expect opens

Every gate beyond Define and Code is a deviation to record in `$EV/deviations.txt`, including a
review loop-back, MAJOR-bump or terminal-hook retry. Inspect it before choosing approve or abort;
record the observed stage, context and your reason first. The launch authorization does not answer
a MAJOR-bump or terminal-hook retry gate. Never fabricate an approval response file.

```bash
"$BIN" gate list "$EV/project"
STAGE=validate
"$BIN" gate show 1 --stage "$STAGE" "$EV/project"
printf '%s\n' "attempt $N: unexpected $STAGE gate; record actual context and decision" >> "$EV/deviations.txt"
```

Replace STAGE with the observed stage. If you choose approval after recording the deviation:

```bash
"$BIN" gate approve 1 --stage "$STAGE" --note "attempt $N: deviation recorded; describe approval reason" --project "$EV/project"
```

To abort, use an actual reason; the `abort:` substring distinguishes termination from loop-back:

```bash
"$BIN" gate reject 1 --stage "$STAGE" --note "abort: describe observed blocking defect" --project "$EV/project"
```

Wait for workflow_aborted/state removal, then collect the stopped attempt using section 7. Do not
count a live or parked run as stopped merely because a response was written.

## If the run dies without reaching Ship

Confirm no workflow_shipped, no live waiter, and no surviving monitor/agent for this project.
A gate FILE remaining on disk is not proof of a live parked run: a dead waiter can leave a stale
Define/Code gate which gate reject refuses to answer. Do not delete or answer that stale gate to
make collection work. Preserve it in the evidence and use the confirmed-dead path below.

Inspect the stored monitor record, preserving NULL versus ABSENT versus an actual PID:

```bash
"$BIN" gate list "$EV/project"
"$BIN" doctor --json "$EV/project" > "$EV/death-doctor.json"
MONITOR_RECORD=$(jq -er '
  if (has("monitor_pid") | not) then "ABSENT"
  elif .monitor_pid == null then "NULL"
  elif (.monitor_pid | type) == "number" and .monitor_pid >= 1 and (.monitor_pid | floor) == .monitor_pid then (.monitor_pid | tostring)
  else error("invalid monitor_pid") end' "$EV/project/.devflow/state-01.json")
case "$MONITOR_RECORD" in
  NULL|ABSENT) printf 'No recorded monitor PID: %s; liveness remains unknown until process checks\n' "$MONITOR_RECORD" ;;
  *) ps -ww -o pid=,lstart=,args= -p "$MONITOR_RECORD" ;;
esac
systemctl --user status "p49-a$N-start"
systemctl --user show "p49-a$N-start" -p MainPID -p ControlGroup
```

NULL is valid before monitor spawn; it is not malformed state and never proves death by itself.
ABSENT is distinct missing evidence. Invalid JSON or PID types require diagnosis. Check the unit's
cgroup processes and the recorded `.devflow/phase-01-agent-pid` file, if present; inspect any numeric
PID's full argv and start time. A monitor can be either __monitor naming this project/phase or an
sh -c wrapper naming this project's phase-01 files and ending with advance --phase 1. Do not infer
absence from only one spelling. Do not signal an unrelated/recycled PID or a parentless agent on
PID alone. Doctor is read-only; its no-waiter finding can distinguish a stale gate from a live one.
If it cannot establish waiter absence, or any monitor/agent remains or cannot be identified safely,
stop and diagnose; never collect over a live or unconfirmable run.

Only after positively confirming all run processes are gone, no waiter remains and no shipped
event exists, a remaining gate file is stale evidence, not a parked run. Record and collect it:

```bash
printf '%s\n' "attempt $N died: record events, PID identities, cgroup emptiness and any stale gate" >> "$EV/deviations.txt"
systemctl --user stop "p49-a$N-surv"
python3 "$T/surv-watch.py" analyze "$EV/surv"
systemctl --user stop "p49-a$N-start"
systemctl --user reset-failed "p49-a$N-start"
bash "$T/collect-attempt.sh" --attempt "$N" --allow-state --stop-note "died: describe confirmed failure and stale gate if any"
bash "$T/check-evidence.sh" --attempt-gate "$P49"
```

Never pass --allow-state while a waiter, monitor or agent is alive or unconfirmable. A stale gate
with verified no waiter does not forbid this confirmed-dead collection. Preserve stale state and
gate files for the collector. This writes the stopped verdict the next prepare requires. A dead run
ends the attempt without Ship; do not resume DevFlow inside it and call that unattended progression.
RemainAfterExit can show active (exited) after the program ended; unit activity alone proves neither
life nor death. KillMode=process can leave descendants; stopping a unit is not a process-reaping
proof. That process-model limitation is the explicit 49-10 plan deferral, not silently fixed here.

## Fix vehicle for a blocking defect

Confirm the vehicle at the stop. A payload problem is fixed in scripts/scratch-dogfood-repo.sh on
feature/phase-49, with a fresh payload next time. A contained DevFlow defect uses `/gsd-quick` on its
own branch and normal tests/pre-push gates. While the phase worktree exists, use the repository's
explicit off-worktree contract `DEVFLOW_ALLOW_OFF_WORKTREE=1` for a fix-branch crates commit; it
is not permission to bypass hooks. A cross-crate fix or design decision becomes a decimal phase
through `/gsd-phase --insert 49`.

The default rebuild uses this phase's tree. A fix on another branch therefore needs an explicit
clean fix checkout; verify the fix is present before creating an attempt. Set FIX and FIX_COMMIT to
the actual absolute checkout and full fix SHA:

```bash
FIX=/absolute/path/to/fix-checkout
FIX_COMMIT=actual-full-fix-commit
W=$(git -C "$T" rev-parse --show-toplevel)
git -C "$FIX" merge-base --is-ancestor "$FIX_COMMIT" HEAD
git -C "$FIX" status --porcelain
bash "$T/launch-attempt.sh" prepare --attempt "$N" --repo "$FIX" --scaffold "$W/scripts/scratch-dogfood-repo.sh"
jq -er .build_commit "$EV/attempt.json"
git -C "$FIX" rev-parse HEAD
```

The ancestry check must exit 0, the fix tree must be clean and the final two SHAs must match.
Preparing an old binary creates an attempt on the wrong code; do this check first, not after an
attempt fails. The explicit scaffold is needed because the fix branch need not carry this phase's
payload changes. A payload-only fix in the phase branch needs neither alternate flag.

## Outward effects of an attempt

The Ship agent pushes feature/phase-01, opens a PR to main, pushes a ship-note commit, and may push
an empty CI commit. GitHub notifications and Actions usage follow. The pinned binary then merges
locally into integration, tags and appends a changelog. Record the GitHub account/scopes already
captured in `$EV/prelaunch/gh-auth.txt`; every retry uses its own private repository and evidence.
API usage is billed through the credential source selected by Claude; the auth probe does not prove
which account owns the credential.

## Cleanup (only after Phase 49 verification)

Do not delete either local or GitHub records until Phase 49 verification and until 49-12 has committed
and verified the sanitized evidence. Repository deletion destroys its branch/PR history permanently.
Stop/reset only this attempt's units, and restrict gate sweep to this project:

```bash
systemctl --user stop "p49-a$N-start" "p49-a$N-surv" "p49-a$N-negctl"
systemctl --user reset-failed "p49-a$N-start" "p49-a$N-surv" "p49-a$N-negctl"
"$BIN" gate sweep --root "$EV/project"
```

The named units are recorded in units.txt; the authprobe is transient and self-collects. Confirm
no monitor or agent survives before removing records. Gate sweep only addresses aged gates with
a live waiter: it may legitimately be a no-op here and does not remove stale gate files or registry
entries. The final verified local-directory removal clears this attempt's archived stale files.

Successful Ship or abort normally deregisters itself. If a stale registration remains under
`~/.cache/devflow/roots/`, inspect JSON project_root and phase, and remove only the exact file whose
root equals `$EV/project` and whose phase is 1. Honor DEVFLOW_CACHE_DIR/XDG_CACHE_HOME if the recorded
unit used a different cache location; never sweep arbitrary files or other projects.

For the outward cleanup, check the recorded owner/name and obtain the deletion scope only now:

```bash
REPO=$(jq -er '.nameWithOwner | select(type == "string" and length > 0)' "$EV/prelaunch/repo.json")
gh auth refresh -s delete_repo
gh repo view "$REPO" --json nameWithOwner,visibility
gh repo delete "$REPO" --yes
```

Finally remove only this exact local attempt directory after confirming N, the absolute EV path,
its attempt.json identity, the stopped units, and the committed sanitized evidence. Preserve other
attempts and the pinned binaries until their records are no longer needed; never remove by wildcard.
The private API-key file is configuration, not attempt evidence, and is not part of this cleanup.

## What an attempt does not establish

One attempt is one sample, not a reliability guarantee. It exercises Claude alone, so DECN-03 closes
for Claude only. An unobserved interleaving establishes nothing about SURV-01. A green attempt gate
proves criteria 1–3 for this binary/payload/base; it cannot generalize to other agents or bases.

A delivery canary refusal after Define cannot be probed before launch: it runs only on the first
stream launch. Record it as an environment finding or a DevFlow defect; the attempt ends without
Ship and consumes one of the three stopped attempts. The authentication probe proves authentication
at its own timestamp, not for the whole run. Three stops produce cap_reached with VERIFY-01 unmet.
