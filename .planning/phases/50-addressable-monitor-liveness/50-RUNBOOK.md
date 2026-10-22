# Phase 50 — One Live Attempt (D-16) and its Fault Arm (D-17)

## What an attempt is

An attempt is one real Claude-driven DevFlow run to Ship against a disposable project, on a freshly built and pinned
binary, using Phase 49's rig unchanged: `scripts/scratch-dogfood-repo.sh --live-payload` and the tools in
`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/`, invoked with
`--home "$HOME/.cache/devflow-phase50"`. No Phase 49 file is edited. The run uses `--agent claude`, the payload's seeded
`blocking-human` checkpoint and `--yes-ship`. The seeded checkpoint's auto-decide resume goes through
`relaunch_checkpoint_session`, which launches the capture shape, so one run crosses both launch shapes: stream for the
Claude stages, capture for the resume.

Before launch, the same pinned binary runs the fault arm (D-17, R-10): fake agents, no GitHub repository, a private
cache, five live fault cases and one fixture pointer. The fault arm does not count against the attempt cap.

Attempt 1 ran every step from a plain Bash terminal outside every agent session.
For attempt 2 the orchestrating Claude session issues these steps under the operator's 2026-10-21 delegation (see
"D-18 rules"). It issues them from its main session and never from a subagent, a planner adaptation listed beside the
ruling in "D-18 rules". The run itself is owned by the `p49-a<N>-*` systemd user units, not by any agent session, and
the session waits with bounded polls (`"$BIN" status`, `"$BIN" gate list`, `tail` of `events.jsonl`). While the run is
live, perform no manual git operation against its project tree. Capture, do not narrate: the tools keep the original
outputs. A stall or failure is a result; preserve each attempt's record and never replace it with a retry.

The live run uses the operator's real DevFlow cache (`~/.cache/devflow`), as production does: its supervisor sockets
live under `~/.cache/devflow/sock/`. Only the fault arm uses a private cache.

## D-18 rules (Phase 49's failure handling, unchanged)

- Full attempts are capped at 3. Only an attempt that ends without reaching Ship counts against the cap (49 D-11).
  The fault arm never counts.
- Each attempt is one `checkpoint:human-action` (50-19 for attempt 1, 50-28 for attempt 2), judged afterwards by
  `check-supervisor-evidence.sh --attempt-gate` (49 D-18), with `--current-source` from attempt 2. Only
  `verdict=shipped` or `verdict=cap_reached` closes it.
- A blocking defect is fixed in-phase and the attempt is repeated on a rebuilt binary. A non-blocking defect goes to
  the backlog as a ROADMAP.md `999.x` entry (49 D-10).
- Every attempt rebuilds the binary and records its identity (`prepare` writes `attempt.json` and `identity.txt`);
  the fault arm runs on that pinned binary, never on another build.
- Evidence enters the repository only through `sanitize-evidence.py` (49 D-26).
- The throwaway GitHub repository is created by the operator, or for attempt 2 by the orchestrating session under the
  2026-10-21 delegation, which tells the operator before creating it. Only the operator deletes it, and only after
  Phase 50 verification.
- Operator ruling 2026-10-21: because CR-01 and WR-03 were fixed after live attempt 1, attempt 2 runs on the fixed
  binary before Phase 50 verifies. The operator delegated running attempt 2 to the orchestrating Claude session
  ("Yes, but you do the run"): preparing and pinning the binary, the fault arm, creating and pushing the private
  repository `denniyahh/p50-live-attempt-2` after telling the operator, launching, answering the Define and Code gates
  as section 5 says, collecting and judging. Deleting the repository stays with the operator, after verification. Any
  attempt after attempt 2 goes back to the operator.
- Planner adaptations (not operator decisions): the operator did not rule on these; the planner added them to keep the
  delegation safe, and they stay in force. The session acts from its main session and never from a subagent, because
  the run is owned by the `p49-a<N>-*` systemd user units but a subagent's turn would end while the attempt is still
  live, leaving its gates unanswered and its polls orphaned. A deviation from this runbook, an unexpected gate (any gate
  but Define and Code) and a credential, auth or scope problem go back to the operator, because the delegation covers
  running this runbook as written, not deciding a departure from it, and `gh auth refresh -s repo` needs the
  operator's browser.
- Work stays within GSD's confines (49 D-19): no hand-made SUMMARY, no edited state.

## Before every attempt

Open a plain terminal in this phase's worktree. The private API-key file and configured Claude apiKeyHelper must remain
available; never paste the key into the project, evidence or terminal history.

```bash
cd "$HOME/Github/devflow/.worktrees/phase-50"
W=$(git rev-parse --show-toplevel)
T49="$W/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools"
T50="$W/.planning/phases/50-addressable-monitor-liveness/tools"
P50="$HOME/.cache/devflow-phase50"
N=1
EV="$P50/attempt-$N"
gh auth status
```

Use the next attempt number, not necessarily 1 (`prepare` refuses a wrong one). The Token scopes line must include
`repo`; otherwise run `gh auth refresh -s repo`.

## 1. Prepare (rebuild and pin the binary)

Commit every crate change first: the binary embeds its commit, and the fault arm records the crate source it ran.

```bash
git -C "$W" status --porcelain -- crates Cargo.toml Cargo.lock rust-toolchain.toml
bash "$T49/launch-attempt.sh" prepare --attempt "$N" --home "$P50"
BIN=$(jq -er '.bin | select(type == "string" and length > 0)' "$EV/attempt.json")
jq -er .bin_sha256 "$EV/attempt.json"
```

The first command must print nothing. The destination must not already exist; never delete an old attempt to make
preparation pass.

## 2. Run the fault arm on the pinned binary (D-17, R-10)

Right after `prepare`, run the full case set on `$BIN`, the binary this attempt launches, and judge it with
`--require-all` (D-17 and R-10's full set, R5-W2):

```bash
M=1; while [ -e "$P50/fault-$M" ]; do M=$((M + 1)); done
bash "$T50"/fault-arm.sh run --bin "$BIN" --out "$P50/fault-$M" --cases all
bash "$T50/check-supervisor-evidence.sh" --fault "$P50/fault-$M" --require-all >"$P50/fault-$M/verdict-require-all.txt"
echo "fault verdict exit=$?"
cat "$P50/fault-$M/verdict-require-all.txt"
```

Expected: five `FAULT case=<name> ok live` lines (`alive-stop-stream`, `alive-stop-capture`, `kill9-stale-stop`,
`kill9-at-gate`, `stop-races-completion`), `FAULT case=publish-unlink-window ok pointer`, `FAULT complete=yes`,
`FAULT verdict=pass` and exit 0. The fault arm takes about 20 s, uses a private `DEVFLOW_CACHE_DIR` under
`/tmp/df50f.*`, never touches `~/.cache/devflow`, and removes its scratch on exit.

A failing case is a blocking defect: do not launch. Fix it in-phase (see "Fix vehicle"), prepare a new attempt number
and run the fault arm again with a new `M`. Never edit a fault record. `--attempt-gate` refuses `shipped` unless a
fault directory that passes `--fault --require-all` records this attempt's `bin_sha256`, so skipping this step makes
the attempt unjudgeable, not faster.

## 3. Create and push the throwaway repository (operator, or the orchestrating session under the 2026-10-21 delegation)

These commands create external GitHub state. Run them after checking the account reported above: the operator, or for
attempt 2 the orchestrating session, which tells the operator before creating the repository.

```bash
ACCOUNT=$(gh api user --jq .login)
REPO="$ACCOUNT/p50-live-attempt-$N"
gh repo create "$REPO" --private
git -C "$EV/project" remote add origin "https://github.com/$REPO.git"
git -C "$EV/project" push -u origin main
git -C "$EV/project" push origin integration
(cd "$EV/project" && gh repo view --json nameWithOwner,visibility)
```

Confirm `PRIVATE` and the intended owner and name (`start` records the same query in `$EV/prelaunch/repo.json` and
refuses a repository that is not private). Keep both pushed branches: `main` is the PR target; `integration`
is DevFlow's local merge base and target.

## 4. Launch

```bash
bash "$T49/launch-attempt.sh" start --attempt "$N" --home "$P50"
bash "$T49/launch-attempt.sh" status --attempt "$N" --home "$P50"
```

`start` runs the bounded Claude authentication probe first and launches only when it succeeds; it then runs
`"$BIN" start --phase 1 --agent claude --mode auto --yes-ship "$EV/project"` in unit `p49-a$N-start` and the watcher
in `p49-a$N-surv`. On `auth probe failed` nothing launched and the attempt is unused; read
`49-RUNBOOK.md` section 4 for the credential checks (names only, never values).

Liveness is the supervisor socket, not a pid: `"$BIN" status "$EV/project"` prints
`supervisor: <socket> — ALIVE (owner: …, launch: stream|capture)` for a live stage.

## 5. Answer the Define and Code gates

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

The Code approval records the auto-decide set; the resume that follows runs under the capture-shape supervisor. Do not
pre-write responses or answer any other gate as though it were expected.

## 6. Wait for the end

```bash
"$BIN" status "$EV/project"
"$BIN" evidence --phase 1 --json --root "$EV/project"
tail -30 "$EV/project/.devflow/events.jsonl"
```

Completion is a `workflow_shipped` event. Each `stage_launched` event carries `supervisor_pid`, `socket_path`,
`launch_id` and `launch` (`stream` or `capture`); the attempt judge reads those fields.

## 7. Collect and judge

After the run ended, stop the observer and release the start unit exactly as `49-RUNBOOK.md` section 7 describes,
with `p49-a$N-*` unit names. Then collect, add the leftover sidecars, and judge:

```bash
bash "$T49/collect-attempt.sh" --attempt "$N" --home "$P50"
bash "$T50/check-supervisor-evidence.sh" collect-sidecars "$EV" --project "$EV/project" --bin "$BIN"
bash "$T50/check-supervisor-evidence.sh" --attempt "$EV"
bash "$T50/check-supervisor-evidence.sh" --attempt-gate "$P50"
bash "$T50/check-supervisor-evidence.sh" --attempt-gate "$P50" --current-source
```

The `--current-source` form closes the checkpoint for an attempt after a crate fix: it refuses an attempt whose paired
fault record was not taken from this checkout's committed crate tree, such as a binary built before the fix
(`reason=… from the current crate source`).

For an intentionally aborted stopped run, pass the actual reason to the collector:
`--stop-note "aborted: describe the observed failure"`.

`collect-sidecars` finding no sidecar is the normal result after a run to Ship: `finish()` removes each finished
launch's socket and sidecar. `--attempt` runs Phase 49's positive-arm check and then requires, on every collected
`stage_launched` event, a socket path of at most 107 bytes, a `launch` label and a `launch_id` the socket file name
ends with (`-<launch_id>.sock`); both `stream` and `capture` must appear and no event may carry `monitor_pid`.
`--attempt-gate` prints `ATTEMPT_GATE verdict=shipped|cap_reached|not_ready`; `shipped` also prints the fault directory
it matched on this attempt's binary (`ATTEMPT_GATE fault=<dir> bin_sha256=<hex>`).

If the attempt shipped but the gate says `not_ready` for a collection reason, follow `49-RUNBOOK.md` section 7's
archive-and-recollect procedure (with `--home "$P50"`); never rewrite collected files.

## 8. Close the checkpoint or stop the attempt

For attempt 1 this was the 50-19 checkpoint, closed on the plain `--attempt-gate`. From attempt 2, reply `done`
to the 50-28 checkpoint only after `--attempt-gate "$P50" --current-source` printed `verdict=shipped` or
`verdict=cap_reached`. On `verdict=not_ready`, do not reply. If the attempt stopped below the cap, record it with
`--stop-note`, end the execute-phase session, confirm the fix vehicle, fix, and resume with `/gsd-execute-phase 50` for
the next attempt.

## If a gate you did not expect opens

Every gate beyond Define and Code is a deviation to record in `$EV/deviations.txt` before answering it, exactly as in
`49-RUNBOOK.md` ("If a gate you did not expect opens"). Never fabricate a response file.

## If the run dies without reaching Ship

Confirm no `workflow_shipped`, no live waiter and no surviving supervisor or agent for this project. Phase 50 answers
liveness from the socket the state records, never from a pid in state (`State::monitor_pid` no longer exists):

```bash
"$BIN" status "$EV/project"
"$BIN" doctor --json "$EV/project" > "$EV/death-doctor.json"
ls -l "$HOME/.cache/devflow/sock/"
```

- ALIVE: the run is not dead. Wait, or end it with `"$BIN" stop --phase 1 "$EV/project"`.
- STALE: the supervisor died and its socket is left. `"$BIN" stop --phase 1 "$EV/project"` runs the backstop: it ends
  the agent group only when its leader is the recorded process, and removes the socket and sidecar.
- GONE: nothing is listening. Check the sidecars that name this project for leftover processes before collecting:
  read `supervisor_pid`, `supervisor_start_time`, `agent_pgid` and `agent_start_time` from each, and inspect a pid with
  `ps -ww -o pid,pgid,sid,lstart,args -p <pid>`. List an agent group's members from `/proc/<pid>/stat` field 5; do not
  use `ps -g`, which selects sessions on procps-ng. Signal nothing by pid alone or by name.

Only after confirming every run process is gone, no waiter remains and no shipped event exists, collect the dead run
with the Phase 49 collector's `--allow-state --stop-note "died: …"` (`49-RUNBOOK.md`, "If the run dies without reaching
Ship"), then run `collect-sidecars`, `--attempt` and `--attempt-gate` as in section 7.

## Fix vehicle for a blocking defect

Confirm the vehicle at the stop. A payload problem is fixed in `scripts/scratch-dogfood-repo.sh` on
`feature/phase-50`, with a fresh payload next time. A contained DevFlow defect is fixed in this phase (D-18) with its
own failing test first and the normal hooks; a fix-branch `crates/` commit while the phase worktree exists uses
`DEVFLOW_ALLOW_OFF_WORKTREE=1`, which is not permission to bypass hooks. A cross-crate fix or a design decision becomes
a decimal phase through `/gsd-phase --insert 50`. For a fix built from another checkout, follow `49-RUNBOOK.md`'s
"Fix vehicle" (`prepare --repo "$FIX" --scaffold "$W/scripts/scratch-dogfood-repo.sh"`, with `--home "$P50"`), and run
the fault arm on the binary that `prepare` pinned.

## Publishing the evidence

Evidence enters the repository only through `"$T49/sanitize-evidence.py"` (`sanitize <src> <dest>`, then `scan
<dest>`), after verification. 50-20 owned attempt 1's publication and the publish check
(`check-supervisor-evidence.sh --published`, added by 50-25); 50-29 owns attempt 2's, with `--identity-scan` and
`--published --attempt <N>`.

## Cleanup (operator only, after Phase 50 verification)

Do not delete local or GitHub records until Phase 50 verification has passed and the sanitized evidence is committed.
Repository deletion destroys its branch and PR history permanently.

```bash
systemctl --user stop "p49-a$N-start" "p49-a$N-surv"
systemctl --user reset-failed "p49-a$N-start" "p49-a$N-surv"
REPO=$(jq -er '.nameWithOwner | select(type == "string" and length > 0)' "$EV/prelaunch/repo.json")
gh auth refresh -s delete_repo
gh repo view "$REPO" --json nameWithOwner,visibility
gh repo delete "$REPO" --yes
```

Remove only this exact attempt directory after confirming `N`, the absolute `EV` path and its `attempt.json`, never by
wildcard. Fault directories hold only records and can be removed the same way once published.

## What an attempt does not establish

- One attempt is one sample, a weak bound on reliability, not a guarantee. A shipped verdict shows the supervisor
  carried this payload to Ship once, on this binary.
- The capture shape is reached only through the Claude checkpoint resume. Parity for the non-Claude adapters, the
  shape's real cohort, rests on V-08's table-driven test, not on this run.
- How often a supervisor goes STALE in the field stays unmeasured: the fault arm makes STALE by `kill -9`, it does
  not observe it.
- The fault arm's `stop-races-completion` case is one sample of a race; whichever side won is recorded
  (`first_agent_exit`, `advance_ran`), and the other side is covered by the 50-13 and 50-23 fixtures, not by the run.
- The publish/unlink window is not reachable live; its record points at its fixture test.
- Socket supervision is unverified on macOS: every live run and fault arm here runs on Linux.
