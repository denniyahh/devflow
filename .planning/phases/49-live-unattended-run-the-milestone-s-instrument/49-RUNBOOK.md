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

## 8. Close the checkpoint or stop the attempt

Reply `done` to the Phase 49 checkpoint only after `verdict=shipped` or `verdict=cap_reached`.
A shipped verdict establishes criteria 1–3 for this attempt; cap reached is three documented stopped
attempts and does not meet VERIFY-01. A typed reply alone establishes neither.

On `verdict=not_ready`, do not reply done. If the attempt stopped below the cap, end the current
execute-phase session, confirm the fix vehicle described below, fix it and resume with
`/gsd-execute-phase 49`. The checkpoint plan has no SUMMARY or commits and restarts at its single
human task for the next attempt. Do not alter GSD state to simulate closure.
