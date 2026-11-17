# DevFlow Operations Reference

The accurate, operator-facing reference for running DevFlow v2 day to day.
This page describes the CLI **as built** (verified against `main.rs` by the
`--help` snapshot test); the README/ARCHITECTURE rewrite lands in Phase 15b.

## The pipeline

Each phase runs a linear stage chain, driven end-to-end by one runner per run,
which runs as a systemd user unit ([Run shape](#run-shape)):

```
Plan → Code → Validate
```

- **Plan / Code** launch a headless coding agent with the stage's
  GSD command.
- **Validate** passes only when the agent reports an explicit
  `verdict: pass` **and** GSD's own reader, `gsd query verification status`,
  reports `passed` for the phase ([How Validate decides](#how-validate-decides)).
  Any other answer routes by GSD's status: gaps loop back to Code (after
  `MAX_CONSECUTIVE_FAILURES` = 3 failures a gate is forced), and a reader
  that cannot answer stops at a gate. Validate is the last stage that runs: a
  pass ends the run as finished and leaves the worktree and branch in place
  ([How a run ends](#how-a-run-ends)).
- Modes: `--mode auto` (finishes on a passing Validate without a gate;
  only never-silent failure gates and the failure caps stop it) or
  `--mode supervise` (also gates every Validate; approving it finishes the
  run).
- Every run gets an isolated git worktree under `.worktrees/phase-NN/` by
  default (`--no-worktree` opts out).

## Commands

| Command | Purpose |
|---|---|
| `devflow start --phase N --agent claude\|codex\|opencode --mode auto\|supervise [--force] [--no-worktree] [--dry-run] [--until plan\|code\|validate] [--fallback-agent <agent>]` | Begin a phase: launches the run as the phase's systemd user unit and returns once the unit is ready ([Run shape](#run-shape)); the unit's runner owns the agent and auto-advances. Over an existing state it says first that it replaces it, naming which: a finished run, a stopped run (with its stop reason), or a state file that does not load (`--dry-run` shows this too). `--until <stage>` runs through `<stage>` then halts cleanly (no orphaned monitor, no `doctor` false positive) instead of continuing; `--until validate` halts nothing, prints a note saying so and records an `until_noop` event: the run ends as one without `--until` does ([How a run ends](#how-a-run-ends)). `--until define` and `--until ship` are rejected by clap as invalid values: there is no Define or Ship stage. Refuses before scaffolding anything if phase N is not reachable from `develop` (see Preflight refusals below). `--fallback-agent <agent>` names one agent that takes over if the Code stage is rate-limited: one hop per run, at Code only (a rate limit at any other stage waits at the stage's gate), relaunching Code in the same worktree on the same branch. It is saved in `state-NN.json`, and refused when it equals `--agent`, when its binary is missing or its driver unhealthy, and in `--mode auto` unless it is `claude` or `antigravity`. That launch check is best effort: the full preflight runs again at the handoff, a refusal there leaves the run waiting at the Code gate, and a relaunch that fails after the handoff stops at a Code gate (see [Rate-limit failover](#rate-limit-failover)). Without the flag, `start` uses `devflow.toml`'s `fallback_agent` key and prints a note naming it; the flag wins the value, and no environment variable selects a fallback |
| `devflow resume --phase N [--agent <agent>] [--fallback-agent <agent>]` | Resume a rate-limited or infra-paused phase from its saved stage (loads `state-NN.json`, does not recreate the branch/worktree or reset to Plan) and launches it as a new unit, returning once the unit is ready, as `start` does ([Run shape](#run-shape)) — the command a rate-limit gate names for after the reset. Also clears a `--until`-stopped phase's stop marker, so the phase advances past its old stop point instead of re-stopping immediately. A run that finished (its Validate passed) is refused, naming `devflow start`, and nothing is launched. It reruns the stage, so a pending gate is asked again: it sets any unused answer aside and reports it, and reports an answer that was claimed with no record that it was applied (see [Answering gates](#answering-gates)). `--agent <agent>` hands the run to another agent (an `agent_handoff` event with reason `resume --agent`); naming the run's saved, unused fallback is refused unless `--fallback-agent` names another. `--fallback-agent <agent>` sets the fallback or re-arms a spent one, checked as on `start`; like any resume it is accepted only on a run that is not live. Without it the saved fallback is kept as it is, spent or not: `resume` does not re-read `devflow.toml` |
| `devflow status` | Every phase with a state: the header lists the active runs and names each other phase's kind (finished, stopped, worktree missing). Per phase: `run: active`, `run: stopped`, `run: finished`, or `run: not active` when its recorded worktree no longer exists (the worktree line is marked `(missing)`); a stopped or finished run's `stop reason:` verbatim; `fallback: armed` (configured, not used), `spent` (already handed off) or `none`; stage, gate state, elapsed, last action. Each phase's first line is its run's liveness answer from the runner lock (`live`, `live (stopping)`, `finished`, `stopped`, `crashed`, …) with the verb that applies; see [Run liveness](#run-liveness). Also shows a held state write lock with its holder, and lists set-aside gate answers |
| `devflow logs [-f] [--phase N] [--stderr]` | Print/follow a phase's captured agent output |
| `devflow history [N]` | Show chronological events with retained capture and review evidence for a phase |
| `devflow gate list` | Gates awaiting a response |
| `devflow gate approve <phase> [--stage S] [--note ...] [--gate-id ID]` | Approve a gate — the workflow advances |
| `devflow gate reject <phase> --note ... [--stage S] [--gate-id ID]` | Reject — the stage is fixed and relaunched; the run stays alive, whatever the note says |
| `devflow gate abort <phase> [--note ...] [--stage S] [--gate-id ID]` | Abort — the only answer that ends the run |
| `devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH]` | Answer or report aged, unattended gates across every registered root (or one `--root`) — bounds an abandoned run's lifetime without `kill(1)`. On-demand only; nothing schedules it. Never writes an approval |
| `devflow stop --phase N [PROJECT] [--root PATH]` | End a phase's run by stopping its user unit ([Run shape](#run-shape)) with `systemctl --user stop`, which returns once the unit is down: the manager sends SIGTERM to every process in the unit, the runner and the agent alike, and SIGKILL to any still there after 10 s. The runner records `stopped` and its stop reason; a run waiting at a gate has that gate closed and stays resumable. `stop` then reports how the run ended, from the state: **stopped**, quoting the stop reason and naming `devflow resume --phase N`; **already finished**; **stopped during launch**, when the runner had recorded nothing yet, naming `devflow start --phase N` (these three exit 0); **crashed**, when the runner ended without recording a stop, naming `devflow resume --phase N` and `devflow discard --phase N`; or a state that does not load, naming `devflow discard --phase N` (these two exit non-zero). With no live run it says there is nothing to stop and exits 0. When no runner is live but another DevFlow verb holds the phase (such as a `start` or `resume` before its unit is launched, or a `discard`), it stops nothing, names that verb's pid and exits non-zero: run it again once that verb returns. `stop` signals no pid itself, answers no gate and writes no state; the runner records its own stop. `--root` overrides the positional project root. `stop` adds no exit codes |
| `devflow cleanup [--force]` | Remove phase worktrees + their feature branches, then delete branches merged into the base. **Keeps a worktree, and says why:** a stopped or finished run (the line quotes the run's recorded stop reason; a stopped run is offered `devflow resume --phase N` or `--force`, a finished run only `--force`, since `resume` refuses it). A live run (its runner lock is held, its unit is still stopping, or that cannot be read; see [Run liveness](#run-liveness)) is refused with a non-zero exit even with `--force`, naming the runner and `devflow stop --phase N`. `--force` also removes the run's `.devflow/state-NN.json` once its worktree is gone; another phase's state is untouched. `--force` never deletes a feature branch with commits not merged into the base: it keeps the branch and prints one line naming it, its unmerged commit count and `git branch -D feature/phase-NN` to delete it yourself. A merged branch that a kept worktree (or any checkout) still has checked out is never deleted |
| `devflow discard --phase N [PROJECT] [--delete-unmerged-branch BRANCH]` | Remove one phase's worktree (`.worktrees/phase-NN`, with everything in it: uncommitted and ignored files too), its run state (`.devflow/state-NN.json`) and its feature branch (`feature/phase-NN`). It first lists what it will remove: the worktree with its count of uncommitted changes, the state file, and the branch with whether it is merged into the base (the base the run recorded, else the project's). Each of the three is listed and removed only if it exists, so it also clears a leftover branch or worktree that has no state, and a state file that does not load. It also lists and removes the debris a killed run leaves (D-06): the phase's files under `.devflow/gates/` (gate requests, answers, claims, unheld waiter registrations and write temps), each stage's through its gate close, which sets an unclaimed answer aside rather than deleting it; and the phase's `*.hold-tmp` temps whose creator has exited, through the orphan sweep, which then also removes any other orphan temp in `.devflow/` and `.devflow/gates/` and names each. A temp whose creator still runs is kept and named. So a phase with nothing but gate files or temps is discarded too. Nothing else under `.devflow/` is removed (never `.holder-NN`, `.runner-NN` or a set-aside answer), and no other phase or branch is touched. **Refuses, removing nothing:** a live run, whatever the state names: its runner lock is held, or whether it is cannot be read (a unit still stopping is waited out first; see [Run liveness](#run-liveness)), each naming `devflow stop --phase N`. A gate of the phase whose waiter registration is held, or cannot be probed: something may still wait on it. A phase another foreground DevFlow process holds. A branch with commits not merged into the base, unless `--delete-unmerged-branch` names it: the value must be exactly `feature/phase-NN`, and a different name is refused. A branch whose merge state cannot be read. A branch checked out in the primary checkout (`start --no-worktree`): it names `git switch <base>` and never switches itself. A branch checked out in any other worktree than the phase's own. A directory at `.worktrees/phase-NN` that git does not list as a worktree. A phase worktree that is not on `feature/phase-NN` (another branch, or a detached HEAD, whose commits no branch may keep). A phase worktree whose status cannot be read (after its directory was deleted by hand, run `git worktree prune` first). A phase with no state, worktree, branch, gate file or hold temp. It holds the phase's foreground holder from its first check to its last removal, so a `start` cannot begin in between. A removal that fails prints a `warning:` line and the exit is non-zero |
| `devflow doctor [--json]` | Environment audit (agents installed, versions, RUST_LOG) plus per-phase reconciliation, including each phase's runner liveness (see [Run liveness](#run-liveness)); `--json` emits one object with the keys `environment`, `reconciliation`, `planning_doc_staleness`, `stray_processes` and `set_aside_gate_answers` |

(`devflow runner` is internal and registered as the hidden `__runner`
subcommand: it is the command of a phase's user unit, which `devflow start`
and `devflow resume` launch through `systemd-run` ([Run shape](#run-shape)).
It takes the phase's runner lock, reports ready to systemd, and runs every
stage of the phase in its own process, each stage's agent its child, waiting
at each gate until it is answered from any shell. Never run it by hand.)

## Run shape

Every run is a systemd user unit named from the project and the phase,
`devflow-<digest>-NN.service`, where `<digest>` is 16 hex digits derived from
the canonical project root. One project's phase therefore always names the same
unit, and systemd refuses a second active one. `devflow start` and `devflow
resume` launch it with `systemd-run --user` and return once the unit's runner
holds the phase's run lock and has reported ready. The unit runs the hidden
`__runner` subcommand in the project root: it runs the stages, waits at gates
and advances. It receives the caller's environment variables by name, so no
value appears on a command line. A launch that fails names the unit and
systemd-run's error; `start` then leaves no state behind, so it can be run
again, and `resume` marks the run stopped again. On success both print the
unit and how to watch it:

```
started phase 15 in supervise mode as user service devflow-<digest>-15.service
  status:     devflow status
  watch live: devflow logs --phase 15 -f
```

- **Watch** with `devflow logs --phase N -f` (the agent's captured output) and
  `devflow status`, from any shell. `systemctl --user status <unit>` shows the
  unit itself.
- **Answer gates** with `devflow gate approve` or `devflow gate reject` (or
  `gate abort`) from any shell; the runner picks the answer up ([Answering
  gates](#answering-gates)).
- **Stop** with `devflow stop --phase N [PROJECT]`, which stops the unit (see
  its row under [Commands](#commands)); `devflow resume --phase N` continues a
  stopped run as a new unit.
- **Lingering.** Without lingering, the user manager ends at logout and every
  run under it ends with it. `start` and `resume` warn when lingering is off
  (`warning: lingering is off for this user, so the run ends at logout`),
  naming `loginctl enable-linger`, and launch anyway.
- **No user manager, no launch.** On a host where no systemd user manager
  answers (none runs for the user, or this shell cannot reach it because
  `XDG_RUNTIME_DIR` is unset or not the user's own), `start` and `resume`
  refuse with systemctl's error and write nothing. There is no fallback launch
  without a unit.

## Run liveness

Whether a phase's run is live has one answer: its runner lock,
`.devflow/.runner-NN`, and then its unit. The runner holds the lock for its
whole life, and the kernel lets go of it when the runner exits, however it
ends. `devflow status`, `doctor`, `stop`, `cleanup`, `discard`, `start` and
`resume` read the lock first: a held lock is a live runner, whatever the state
file says. With the lock free they read the phase's unit
(`devflow-<digest>-NN.service`, see [Run shape](#run-shape)): a unit that is
neither `inactive` nor `failed` is a runner still exiting. When no systemd
user manager runs for the user, the lock alone answers and the verb says so on
stderr (`checked by its runner lock only`). A lock that cannot be probed, or a
unit whose state cannot be read, is an error, never read as free or inactive.
No verb reads the agent pid file or a supervisor socket to decide whether a run
is live.

`devflow status` names each phase's run with one word and the verb that
applies (`phase N: <word> — …`):

| Answer | Meaning | Verb |
|---|---|---|
| `live` | The runner lock is held. The line gives the runner's pid when its record reads, and the stage; a run waiting at a gate says so | At a gate, `devflow gate approve N` or `devflow gate reject N --note <reason>`; otherwise watch it with `devflow logs --phase N -f`. End it with `devflow stop --phase N` |
| `live (stopping)` | The lock is free and the unit is not yet inactive: the runner is exiting | Wait for it to stop, or `devflow stop --phase N` |
| `finished` | The run finished (its Validate passed) | Close the phase out, then remove the run with `devflow discard --phase N` |
| `stopped` | The run recorded a stop | `devflow resume --phase N` (`--agent <other>` while its recorded delivery canary refuses the agent), or `devflow discard --phase N` |
| `crashed` | No runner, and the run neither finished nor stopped, so nothing advances it; the line says when a gate was pending | `devflow resume --phase N`, or `devflow discard --phase N` |
| `idle` | The state's `devflow start` never prepared the run | The `devflow start --phase N …` line it prints |
| `debris` | No state, and gate files remain with nothing waiting on them | `devflow discard --phase N` |
| `unloadable` | The state file does not load | `devflow discard --phase N` |
| `error` | Whether a run is live could not be read; the line gives why | Fix what it names; nothing is guessed |

How the other verbs use it:

- **`devflow start` / `devflow resume`** refuse while the runner lock is held,
  naming `devflow stop --phase N`, and when the answer cannot be read. A unit
  still stopping is waited out, for up to 15 s, under the phase's holder, and
  then read again. They also refuse while another foreground DevFlow process
  holds `.devflow/.holder-NN`.
- **`devflow cleanup`** refuses, with a non-zero exit and even with `--force`,
  a worktree whose phase's runner lock is held (naming the runner's pid when it
  reads), whose unit is still stopping, or whose answer cannot be read, naming
  `devflow stop --phase N`. It does not wait.
- **`devflow discard`** refuses the same, removing nothing, after waiting out a
  stopping unit as `start` does.
- **`devflow doctor`** reports a phase whose answer could not be read
  (repair `devflow status`, which prints the error; the finding names no path)
  and a phase no runner runs that is neither stopped nor finished (repair
  `devflow resume --phase N`, or the `devflow start` line a run its `start`
  never prepared needs). A live or stopping runner, and a stopped or finished
  run, are not findings; a launch reservation the state names while no runner
  runs is a note. No repair names `resume` while the runner may be live.
  `doctor --json` gives each finding a `runner` object: `liveness` (`live`,
  `stopping`, `not_live` or `error`), the runner's `pid` when its record reads,
  and the reservation's launch id as `pending`. It also warns about a held
  state write lock, naming its holder, and lists set-aside gate answers.
- **`devflow stop`** stops the unit; see its row under [Commands](#commands).

**Where the cache lives.** A launch registers its project under
`registry::cache_dir()`: the cache directory is `DEVFLOW_CACHE_DIR` when set, then `$XDG_CACHE_HOME/devflow`,
then `~/.cache/devflow`. A launch with none of `DEVFLOW_CACHE_DIR`,
`XDG_CACHE_HOME` and `HOME` set fails (`cannot resolve DevFlow's cache
directory for this launch`).

**Signals.** `SIGTERM` or `SIGINT` sent to the runner ends a running agent,
launches no next stage, and records `stopped`. A runner waiting at a gate
closes the gate and records `stopped`, so the run stays resumable. `devflow
stop` sends them through the unit's manager.

**Platform.** A run needs a systemd user manager ([Run shape](#run-shape)).
DevFlow is unverified on macOS: no macOS host or CI has run it.

**Upgrading from 2.x.** Install a 3.x or later binary only while no 2.x run is
live; it makes no promise about a 2.x state.json, which may fail to load.

## Preflight refusals

Checks `devflow start` performs, in order, before it scaffolds anything (a
worktree, a feature branch, or `.devflow/` state):

| Check | Condition | Operator action |
|---|---|---|
| Agent binary present | The agent's executable (`claude`/`codex`/`opencode`) is not found on `PATH` | Install the agent, or run `devflow doctor` |
| Phase reachable from `develop` (23f) | Phase N's `### Phase N:` ROADMAP heading or its `.planning/phases/NN-*/` directory is missing from `develop` — skipped entirely if `develop` has no `.planning/ROADMAP.md` at all | Merge the branch carrying the phase's planning artifacts into `develop`, then re-run |
| Codex headless Plan | `--agent codex` and phase N has no `-CONTEXT.md` on `develop` — Codex's `exec` mode cannot run the discuss-phase interview headlessly, so its Plan stage needs CONTEXT.md to exist | Run `/gsd-discuss-phase N` interactively first (any agent), or use `--agent claude` |

## Answering gates

When a gate fires you'll get the notify hook (below) and `devflow status`
shows `gate: pending`. Answer from any terminal:

```bash
devflow gate list
devflow gate approve 15 --note "lgtm"
devflow gate reject 15 --note "tests are thin, tighten coverage"   # fix and relaunch
devflow gate abort 15 --note "wrong direction"                     # ends the run
```

There are three answers. `gate approve` advances. `gate reject` always means
fix and relaunch: the run stays alive. `gate abort` is the only answer that
ends the run. A note's wording never changes which one it is, so a rejection
that reads "do not abort, fix the test" relaunches the stage. An answer that
both approves and aborts is refused when it is written.

### What each answer runs

Every gate's context opens with one sentence that names what each answer runs
on that gate, or that it is not offered, and that abort ends the run. The cause
follows it, so the notify hook, the pending-gate banner and `gate list` show
the way on first. `gate approve`, `gate reject` and `gate abort` print the same
route (a `route:` line) once the answer is written. The sentence and the print
come from the routers the supervisor dispatches through, read over the gate
file's kind and the run's recorded delivery canary, never from the gate's
text. An answer the supervisor would refuse is refused by the command before
anything is written: it exits non-zero and names the answers that apply.

**Verification gates** are the gates opened over a GSD status or a failed GSD
read: `clean_pass`, `gaps_found`, `reverify_needed`, `human_needed`,
`disagreement_over_passed`, `disagreement_over_human_needed`,
`ambiguous_verdict`, `failure_cap`, `reader_failure` and
`no_gap_closure_plan`. Approve reads GSD's verification status again when the
supervisor picks the answer up, and routes on that fresh read, never on the
status the gate was opened over:

| Fresh GSD status | Validate pass on the current commit, over a clean tree | Auto mode, after Validate | Approve |
|---|---|---|---|
| `passed` | yes | finish | finish |
| `passed` | no | gate | run Validate again |
| `gaps_found` | any | the gap-closure loop | the gap-closure loop |
| `stale` or `missing` | any | the re-verify Code pass | the re-verify Code pass |
| `human_needed` | any | gate | read again; the gate reopens with the fresh facts |
| no answer (a reader failure) | any | gate | read again; the gate reopens with the fresh facts |

In supervise mode every Validate ends at a gate. So does an ambiguous verdict
in auto mode, over every status in the table: it counts no Validate failure, so
the gap-closure loop or the re-verify pass would have nothing to bound it. An
approve follows the
Approve column whether the run is in auto or supervise mode, a `failure_cap`
approve included: that one restarts the count first. `gate approve` prints the
route the table gives over a read it makes itself, and says that the
supervisor reads again at pickup; a status that changes between the two reads
can route differently from the print.

A finish needs a fresh `passed` and a Validate pass recorded on the current
commit. A commit made during a gate wait sends approve back to Validate, and so
does an uncommitted change: a pass counts only on a clean tree, `.planning/`
included, and the gate names the dirty paths. A Validate that ran over a dirty
tree records no pass, so cleaning the tree afterwards does not revive one: the
finish needs another Validate over the clean tree. Approving an
`ambiguous_verdict` gate no longer advances the run: an ambiguous verdict
records no Validate pass, so approve over `passed` runs Validate again, and a
Validate that stays ambiguous can finish only through a later clean pass.

GSD's own `/gsd-plan-phase`, run without `--auto` as DevFlow's Plan stage runs
it, writes `workflow._auto_chain_active` into `.planning/config.json` and does
not commit it. So in a project whose committed config lacks that key, the
first Validate gate names `.planning/config.json` as a dirty path; commit the
file once, and later Validates read a clean tree (GSD's later writes of the
same value leave it unchanged). A `devflow resume` runs DevFlow's existing
chain-flag repair before it relaunches, and when the flag is the file's only
difference from the branch's last commit the repair commits the file itself,
with a commit message and notice that say a killed run leaked the flag, which
is not so here (backlog 999.219, not fixed in Phase 52). `devflow start
--no-worktree` on a checkout that still holds that uncommitted write (from an
earlier run or a hand-run `/gsd-plan-phase`) commits it the same way when git
carries the write across start's branch checkout and the flag is the file's
only difference from develop; otherwise the checkout refuses and start fails
first, or the repair declines with a warning and the file stays dirty. That run
launches in the project checkout, and start makes no dirty-tree check before
the repair. `devflow start` runs the repair before its own Plan writes the key,
so a worktree run that is not resumed, or a `--no-worktree` run whose checkout
held no such write at start, needs the one manual commit. DevFlow makes no
other write for it and exempts no path.

Reject with a note runs `/gsd-quick` with the note, then Validate, on every
verification gate. A reject whose note is empty or only whitespace is refused.
Abort ends the run.

Accepted limits, fixed upstream: a `gaps_found` report gone stale runs the
gap-closure loop, whose execute-phase re-verifies (open-gsd/gsd-core#6078); and
a quick fix to a file the report does not cover can leave GSD's `passed`
unchanged (open-gsd/gsd-core#6096), so the finish still needs a fresh Validate
pass on that commit.

**Status-less gates** read no GSD status, and none of them can finish a run:

| Gate kind | Approve | Reject with a note | Abort |
|---|---|---|---|
| `stage_failure` | retries the stage | relaunches the stage with the note | ends the run |
| `rate_limit` | relaunches the stage now on the same agent | relaunches the stage with the note | ends the run |
| `stage_failure` on a run whose recorded delivery canary refuses its agent | stops the run; the way on is `devflow resume --phase N --agent <other>` | the same, and the note is kept for that resume | ends the run |
| `preflight` | relaunches the stage with the failed check overridden | not offered: refused | ends the run |
| `checkpoint_declaration` | records the checkpoints as seen and relaunches Code | not offered: refused | ends the run |
| a gate file with no `gate_kind` (a 3.x or hand-written file) | refused | refused | ends the run |

`--stage` is only needed when one phase somehow has several open gates.
`--gate-id <ID>` binds the answer to the gate `gate list` or `gate show`
displayed, and a gate opened since is refused; without it the command answers
the gate open now.

Every gate opened gets a fresh `gate_id`, named in its request file
`.devflow/gates/NN-<stage>.json`. An answer belongs to that one gate. `gate
approve`, `gate reject` and `gate abort` write it to
`.devflow/gates/NN-{stage}.{gate_id}.response.json`. Before they publish, they
move an answer in that gate's own slot that does not bind to it aside, so a
refused answer never blocks a valid one, and they refuse to overwrite an
unconsumed answer that does bind. With no registered waiter they write nothing
and name the repair.

To answer by hand, copy `gate_id` from the gate file and publish the same path
with the same body, renaming it into place so no reader sees it half written:

```json
{"gate_id": "<copied from the gate file>", "approved": true, "note": "lgtm"}
```

`approved` is `true` or `false`, and `note` is optional. Add `"abort": true`
(with `"approved": false`) to end the run; without the field the answer is not
an abort. Otherwise a rejection loops back to Code. A body with both
`"approved": true` and `"abort": true` is read as an abort, never as an
approval. `devflow gate list`
stops listing a gate once an answer bound to it is in its slot. A
process waiting at the gate reads its answer slot with a backoff (1s, 2s, … capped
at 60s, so a long wait costs few reads, and a stop is still seen within about a
second), claims the answer with `.devflow/gates/NN-{stage}.{gate_id}.ack.json`,
and moves on. The first process to publish that claim consumes the answer.

**Answers that do not bind.** An answer with a missing or other id, an
unparsable one, a legacy unkeyed `NN-<stage>.response.json`, or one left unused
when its gate closes is never consumed or deleted. It is set aside as
`NN-{stage}.{gate_id|noid}.{secs}.{pid}.{seq}.quarantined.json` and logged as
`gate_answer_quarantined` with its reason. `status` and `doctor` list it. A set-aside file persists until removed by hand:
nothing in DevFlow deletes it, so remove it once read.

**A late answer.** `DEVFLOW_GATE_TIMEOUT_SECS` ends the wait, not the gate, and
a run that timed out has ended at that gate. `devflow resume --phase N` reruns
the stage and asks again. It reports any unused answer it set aside, and any
answer that "was claimed, with no record that it was applied" (logged
`resume_found_unapplied_answer`; the event's name means that record is missing,
and the closed gate's claim is logged `gate_claimed_answer_removed`). The
answer's consumer may have ended before acting on it, or it may have acted with
the gate left open, so the line says only what is known, and the rerun asks
again either way.

## Rate-limit failover

A rate-limited stage gets one of two answers.

- **Failover.** The run has an unused fallback agent (`--fallback-agent` on
  `devflow start` or `devflow resume`, or `devflow.toml`'s `fallback_agent` key
  at `devflow start`) and the rate limit hit the Code stage. The supervisor
  hands the run to the fallback and relaunches Code in the same worktree, on the
  same branch, with the tree as it is: nothing is reset, stashed or cleaned. The
  hop happens once per run, because it marks the fallback used in
  `state-NN.json`; `devflow resume --phase N --fallback-agent <agent>` re-arms it
  on a run that is not live. A hop does not count toward the infra ceiling.
- **Rate-limit gate.** Otherwise the run waits at the stage's never-silent
  gate: no fallback configured, the fallback already used, a rate limit at any
  stage but Code, or a handoff refused before it happened. The full preflight
  runs again at the handoff, because the check at launch is best effort, so a
  fallback can still be refused there (its binary gone, say). The gate shows in
  `devflow gate list` and `devflow gate show`, and fires the notify command.
  It is its own kind of gate (`gate_kind: rate_limit`). Its context leads with
  the routers' way-on sentence, then `devflow resume --phase N` (with
  `--agent <agent>` when the run is not on `claude`), then the reset time when
  the denial carried one (`the limit resets at <time>`, otherwise `the denial
  carried no reset time`), then the cause. `devflow gate approve N` relaunches
  the stage now, on the same agent (`relaunch_now`); `devflow gate reject N
  --note <text>` relaunches it with the note in the stage's prompt
  (`relaunch_with_note`; an empty note is refused); `devflow gate abort N` ends
  the run. The run's recorded delivery canary does not change these answers.
  When the denial named a reset time, the gate file records `not_before` (the
  reset plus 60 seconds), and `devflow gate sweep` leaves the gate alone until
  then, however old it is. Waiting for quota does not count toward the infra
  ceiling, so it cannot abort the run.

  **The one automatic relaunch.** When the denial named a reset time, the
  waiting supervisor relaunches the stage by itself once `not_before` has
  passed with the gate still unanswered: it answers the gate as
  `devflow-rate-limit-timer` (the answer's `responded_by`), and the stage
  relaunches on the same agent, as an approve would. The wake is the reset plus
  60 seconds whatever the gate timeout. The gate keeps waiting past
  `DEVFLOW_GATE_TIMEOUT_SECS` when the reset is later, and the relaunch still
  happens at the wake. This happens at most once per run: the supervisor
  records `rate_limit_auto_relaunch_used: true` in `state-NN.json` before it
  writes the answer. If an operator answered first, the supervisor takes that
  answer and the flag goes back to false. A gate's answer is claimed once, so
  the timer and an operator answering the same gate launch the stage once.

  - A second denial after the automatic relaunch opens the same kind of gate.
    The gate records its `not_before`, but no timer is armed, and it waits for
    an operator.
  - A denial with no reset time is an ordinary blocking gate. Nothing is
    guessed and nothing relaunches by itself.
  - An operator's answer (`gate approve` relaunches now, `gate reject --note`
    relaunches with the note in the stage's prompt, `gate abort` ends the run)
    does not use up the run's one automatic relaunch. A later denial with a
    reset time still gets it.
  - A resume, a stop and resume, or a supervisor takeover after the automatic
    relaunch was used does not arm it again.
  - A crash after the supervisor recorded the flag leaves no automatic
    relaunch: nothing re-arms the timer or writes its answer again. Run the
    `devflow resume --phase N` command the gate names. `devflow resume` is
    refused while the waiting runner holds its runner lock, so it is the way
    on once that runner has ended. It closes the phase's gates and relaunches the
    stage itself.

  The timer's write is recorded as a `rate_limit_timer_fired` event (below).

Only top-level events decide a rate limit: a subagent's rate-limit event or
`result` in the capture does not. A stage that went silent with no quota
denial in its capture ends as an idle timeout, which takes neither answer. Its
failure names the last top-level `result` event's `terminal_reason`, `origin`
and `errors` when the capture carries them, as in
`idle timeout (last result: terminal_reason aborted_tools; origin task-notification; …)`.
When the capture ends in a torn final line, the clause starts with
`torn final line after the last parsed event`, and the verdict stays an idle
timeout. For a Claude capture the torn line may have been a quota denial; the
classifier reads only Claude's denials, so for other agents it says nothing
about one either way. A torn final line outranks a quota denial parsed before
it, because the torn line may be a later event that replaced the denial: the
stage is never classified as rate-limited, so no failover and no rate-limit
gate follow. At an idle timeout it is recorded as one, flagged torn, and stops at a
gate, when the torn line reached the capture before the monitor's idle check.
Known residual (backlog 999.207): the capture keeps whole lines only, so a
partial line the agent was writing when it was killed reaches the capture only
at end of output, after that check. If the capture held a complete quota denial
at the check, the monitor logs the stage as rate-limited (resumable), writes no
idle-timeout record and kills the agent; the verdict then reads the torn line
and the gate shows an indeterminate failure (`stream capture ends in an
unparseable line`), not an idle timeout. The outcome is still a gate, with no
failover and no rate-limit gate.

The failover events in `events.jsonl`:

| Event | When | Keys |
|---|---|---|
| `agent_handoff` | The run changed agent | `stage`, `from_agent`, `to_agent`, `reason` (`rate_limited` for a failover, `resume --agent` for `devflow resume --agent`), `tree_dirty` (the tree handed over had uncommitted changes), `commit_count` (commits on the phase branch). Each of the last two is `null` when it could not be measured |
| `agent_handoff_refused` | The fallback was refused before the handoff; the rate-limit gate follows | `stage`, `from_agent`, `to_agent`, `reason` |
| `rate_limit_timer_fired` | The rate-limit gate's reset plus margin passed with the gate unanswered, and the supervisor's timer tried its one answer | `stage`, `gate_id`, `outcome` (`answered`; `operator_answered_first`, when an operator's answer was taken instead and the automatic relaunch stays unused; `failed`, when the write failed, the automatic relaunch is spent and the gate waits for an operator) |
| `agent_handoff_relaunch_failed` | The handoff happened, but Code could not be relaunched under the fallback | `stage`, `from_agent`, `to_agent`, `reason` (the launch error) |
| `gate_relaunch_refused` | A retry or loop-back answer to a stage's failure gate was not relaunched, because the delivery canary recorded for the run refuses its agent; the run is marked stopped | `stage`, `agent`, `reason` (the recorded refusal) |

`agent_handoff_relaunch_failed` is followed by the never-silent Code gate
(`gate_fired`), or by an abort when the run is at the infra ceiling. When the
fallback's delivery canary refused, a resume would only fail again: the
refusal stays recorded for the run and refuses every later launch. And after a shutdown that could not be confirmed, a
successor may still be running. The gate names the cause as
`the handoff to <agent> could not relaunch the code stage: <error>`. When the
recorded canary refuses the agent, the gate context (and so the notify hook)
leads with the routers' sentence: approve and reject both stop the run, and the
way on is `devflow resume --phase N --agent <other>`. The cause follows it.

The way on when the fallback's canary refused:

- Answering the gate with a retry (`devflow gate approve` or `devflow gate
  reject`) does not relaunch Code, because the recorded refusal would
  refuse it again. The supervisor marks the run stopped, with a stop reason
  naming the refusal and `devflow resume --phase N --agent <other>`, writes a
  `gate_relaunch_refused` event, and exits. The same applies to any stage's
  failure gate whose run has a refusing canary recorded for its agent.
- `devflow stop --phase N` answers the gate with an abort, which ends the run
  and removes its state.
- To continue on another agent, answer the gate with a retry, wait until
  `devflow status` shows the run not live (its hint then reads
  `devflow resume --phase N --agent <other>`), then run
  `devflow resume --phase N --agent <agent>` naming an agent other than the
  run's current one. The handoff clears the recorded canary, so that agent
  runs its own. A plain `devflow resume --phase N` is refused by the same
  recorded canary before it changes anything, so the stop reason that names
  the way on is kept.

## How Validate decides

A Validate verdict has two halves: the validate agent's own `verdict`, and
the status GSD's reader reports for the phase. DevFlow never parses
`VERIFICATION.md` itself. After every Validate it runs
`gsd query verification status <phase dir>` through GSD's `gsd_run`
launcher, found under the run's `HOME` (`.claude/gsd-core/bin/`, then
`.codex/gsd-core/bin/`, then `.pi/agent/gsd-core/bin/`). The read runs in the
phase's worktree, where the Validate agent wrote its report (the main checkout
when the run uses `--no-worktree`), and is killed after 60 seconds.

| GSD status | After an agent pass | After an agent fail |
|---|---|---|
| `passed` | auto: the run finishes; supervise: the Validate gate, whose approval finishes the run | a gate: the agent and GSD disagree; approving runs Validate again |
| `gaps_found` | auto: a gap-closure pass: Plan runs `/gsd-plan-phase N --gaps`, then Code runs `/gsd-execute-phase N --gaps-only`, then Validate runs again; supervise: a `gaps_found` gate, whose approval runs that gap-closure pass and never finishes the run, and whose abort (`devflow gate abort`) ends the run | the same |
| `stale` or `missing` | auto: back to Code for the re-verify pass (below); supervise: a `reverify_needed` gate, and nothing launches until it is answered | the same |
| `human_needed` | a gate in both modes; approving reads GSD again and reopens the gate while it still says `human_needed` | a gate: the agent and GSD disagree |
| no answer | a `[never-silent]` gate naming the failure: no launcher, a non-zero exit, a timeout, output that is not JSON, a status outside GSD's set, no phase directory, and the like. It is never a pass and never a silent retry | the same |

A gap-closure pass keeps going until a Validate reads `passed`. A `devflow
stop`, a resume or a crash in the middle of it keeps it: a resumed Plan still
runs `--gaps` and a resumed Code still runs `--gaps-only`. After its Plan
pass DevFlow checks that the pass wrote a plan with `gap_closure: true` (read
through GSD's `frontmatter get`) whose file is newer than the phase's
`VERIFICATION.md`. An earlier round's gap plan does not count. Without one the
run stops at a Plan gate of kind `no_gap_closure_plan`; if the check cannot
read the plans it stops at a `[never-silent]` `reader_failure` gate. Approving
either gate reruns the gaps Plan pass, and `devflow gate abort` ends the run.

The re-verify pass. A `stale` or `missing` status says the phase's report is
out of date or was never written, not that the code has gaps. DevFlow
relaunches Code with `/gsd-execute-phase N`, telling the agent to resume at the
phase gates and re-run the verifier, and the prompt quotes GSD's status, route
and next action as GSD gave them. DevFlow does not read GSD's status when that
pass ends: Validate runs after it as after any Code pass, and that Validate's
own read decides. A report that is still `stale` takes the re-verify pass
again, and the run finishes only over `passed`. A `devflow stop` keeps the
pass: a stopped re-verify pass resumes as the re-verify pass, and one taken in
the middle of a gap-closure pass ends that pass, so it resumes as the re-verify
pass and not as `--gaps-only`.

The re-verify prompt cannot make an agent run the verifier (gsd-core#6096). What
bounds an agent that skips it is the count: every Validate whose status is
`stale`, `missing` or `gaps_found` counts one Validate failure, whether the
agent passed or failed and in either mode, and a Validate counts at most once
even when the agent also failed. An ambiguous verdict counts nothing and stops
at a gate instead. Three such rounds whose commits change nothing outside
`.planning/` reach the consecutive-failure gate instead of looping.

DevFlow does not check a `gaps_found` report's own digest. A `gaps_found`
report whose covered files changed after it was written still runs the
gap-closure pass, whose execute-phase re-verifies at its end. The upstream fix
is open-gsd/gsd-core#6078. Re-running `/gsd-execute-phase N` by hand refreshes
the report.

An agent pass over any status but `passed` counts as a Validate failure, so the
consecutive-failure and per-phase caps still bound the loop; at a cap the run
stops at a gate whatever the status says. A gaps Plan pass that wrote no
gap-closure plan counts one failure the same way. When that brings the phase
to its per-phase ceiling, its `no_gap_closure_plan` gate says so, and an answer
that continues the run restarts the per-phase count (a
`phase_failure_budget_reset` event); `devflow gate abort` does not. An ambiguous verdict (the external
verification probe succeeded but the agent reported gaps or no verdict)
records no Validate pass and counts no Validate failure: it stops at an
`ambiguous_verdict` gate at once (a `reader_failure` gate over a failed read),
over every GSD status and in either mode, and never takes the gap-closure loop
or the re-verify pass on its own. A gate context quotes GSD's status as GSD printed it
and says `passed` only when GSD reported `passed`, apart from the way-on
sentence that leads every verification gate, whose finish rule names `passed`.

What a run records:

- Each Validate read is one `gsd_verification` event: the agent's verdict and
  GSD's status, or the failure's short label. GSD's message can name absolute
  paths, so it goes only to the gate context and stderr, never to an event.
- Every gate file records which gate it is in `gate_kind` (for example
  `clean_pass`, `gaps_found`, `reverify_needed`, `reader_failure`,
  `failure_cap`), and the `gate_fired` event carries the same value. Gates
  outside Validate's routes record one too: `stage_failure` for a failed stage
  at any stage, `rate_limit` for a rate limit no fallback takes, `preflight`
  for a failed preflight check and `checkpoint_declaration` for an unapproved
  human-only checkpoint.
  A gate file written by a 3.x binary has no `gate_kind` and still loads.
- An agent pass is recorded in the state file's `validate_pass_head`, the
  commit the Validate ran on; any other verdict clears it. A commit that cannot
  be read stops at the `[never-silent]` gate.
- `devflow start` records the GSD version that answers the run's reads in one
  `gsd_runtime` event.

## How a run ends

A run records how it ended with one `workflow_finished` event in
`.devflow/events.jsonl`, followed by one `notify_fired` event carrying the
same `reason`. The notify command runs once, between the two (see
[Notify hook](#notify-hook-never-miss-a-gate)).

| `workflow_finished` payload | When | State afterwards |
|---|---|---|
| `{"reason":"finished","shipped":false}` | Validate passed: on the pass in auto mode, on the approved Validate gate in supervise mode | kept, with `stopped: true`, `finished: true` and a stop reason naming the finish |
| `{"reason":"stopped_at","stage":"<stage>"}` | `--until plan` or `--until code` halted the run after that stage | kept, with `stopped: true`, `finished: false` |

Validate is the last stage, so `--until validate` halts nothing: a run whose
Validate passes ends as `finished`, exactly like a run without `--until`, and
a `gaps_found` verdict runs the gap-closure loop (see
[How Validate decides](#how-validate-decides)) as it would without `--until`.
It never reports `stopped_at` for Validate. `devflow start --until validate`
says so in one printed note and records one `until_noop` event with
`{"until":"validate"}`; `--until plan` and `--until code` do neither.

A `devflow stop` taken while the run advances toward its finish (for example
just after the Validate gate was approved) loses to a finish that is
permitted: when GSD's fresh status is `passed` and the Validate pass is on the
current commit over a clean tree, the run is recorded finished, one
`workflow_finished` is recorded, and `stop` prints that the run already
finished and that nothing was stopped (the supervisor answers
`already_finished`, never `handed_off`). A stop that lands where no finish is
permitted (the approve runs Validate again, the gap loop or the re-verify
pass) is honoured at the next advance safe point as before, and the run ends
stopped and unfinished.

Finishing never merges, pushes, tags, deletes or resets anything and runs no
hook: the worktree and the `feature/phase-NN` branch are left as Validate left
them. The phase is removed from the machine-global gate registry. `cleanup`
without `--force` keeps a finished run's worktree and names its stop reason, `cleanup --force` removes the worktree and the run's state but keeps an unmerged `feature/phase-NN` branch, and `devflow resume`
refuses a finished run and launches nothing: start a new run with
`devflow start`. While the finished run's branch has commits not merged into
the base, `devflow start --force` refuses before removing anything: close the
branch out first (merge it into the base), or delete it yourself with the
command the refusal prints.

## Notify hook (never miss a gate)

Set `DEVFLOW_GATE_NOTIFY_CMD` to any shell command; it runs on every gate
with metadata in env vars — never interpolated into the command. It also
runs once when a run ends ([How a run ends](#how-a-run-ends)): for a
finish `DEVFLOW_GATE_STAGE` is `validate`, for an `--until` halt it is the
stage the run stopped after, and `DEVFLOW_GATE_CONTEXT` names the end.

```bash
# ntfy.sh example
export DEVFLOW_GATE_NOTIFY_CMD='curl -s -d "devflow gate: phase $DEVFLOW_GATE_PHASE $DEVFLOW_GATE_STAGE — $DEVFLOW_GATE_CONTEXT" ntfy.sh/my-topic'
```

Env provided to the hook: `DEVFLOW_GATE_PHASE`, `DEVFLOW_GATE_STAGE`,
`DEVFLOW_GATE_CONTEXT`, `DEVFLOW_NON_SILENT_GATE` (`1` when the gate exists
only because a stage failed unexpectedly).

`DEVFLOW_GATE_CONTEXT` holds the first 450 characters of the gate's context,
the same start the pending-gate banner and `devflow gate list` show; a longer
context ends in `… [truncated; full output in .devflow/]`. Every context leads
with the way-on sentence, so the start names each answer's route. The gate
file stores a failure's reason whole (up to 4096 characters), and `devflow
gate show <phase>` prints all of it.

The command is saved in the run's state file (`gate_notify_cmd` in
`.devflow/state-NN.json`), so gates fired by the run's unit use it
without your shell's environment. `devflow start` and `resume`
update the saved command from the variable: unset keeps it, a new value
replaces it, and an empty value (`DEVFLOW_GATE_NOTIFY_CMD= devflow resume …`)
turns notification off for the run.

`resume` refuses while a run is live (an agent running or a gate waiting), so
it changes the hook only for a stopped run. To change the hook of a live run,
edit `gate_notify_cmd` in the state file. An edit made while a gate is waiting
is kept when the gate is answered. An edit made while a stage is still running
is kept too: the gate reloads the state file when it opens and fires the edited
command. The coding agent never receives the variable.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `DEVFLOW_GATE_NOTIFY_CMD` | unset | Shell command fired when a gate is written, and once when a run ends |
| `DEVFLOW_GATE_TIMEOUT_SECS` | 259200 (3d) | How long a monitor waits at a gate, holding its gate registration rather than a lock. Not a deadline for answering: the gate request and phase state are files that never expire, and timing out is a clean resumable stop. Past it the run has ended at that gate: `devflow resume` reruns the stage and asks again, setting any unused answer aside |
| `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` | 259200 (3d) | Age threshold `devflow gate sweep` uses to decide a gate is abandoned. Held **equal to** `DEVFLOW_GATE_TIMEOUT_SECS`, never shorter: `gate sweep` writes an abort response, so a threshold below the poll timeout lets a sweep reap gates a live monitor is still polling — turning a clean resumable timeout into an `abort()` that clears state, machine-wide across every registered root. An unparsable value or explicit `0` falls back to the default rather than reaping every gate on the machine |
| `DEVFLOW_CACHE_DIR` | unset (falls back to `$XDG_CACHE_HOME/devflow`, then `$HOME/.cache/devflow`) | Test/override hook for the machine-global registry directory (`devflow gate list --all-roots`) |
| `DEVFLOW_CAPTURE_RETENTION` | 5 | Capture generations retained per phase; overrides `devflow.toml` |
| `DEVFLOW_EXTERNAL_VERIFY_ENABLED` | true | Enable PLAN-declared external post-condition probes; overrides `devflow.toml`. Accepts `true` or `false`; an invalid value prints a `warning:` line at `start` and `resume` and falls back to `devflow.toml` or the default |
| `DEVFLOW_BASE_BRANCH` | `develop` | The branch phase worktrees fork **from** and phase work merges back **into** (D-01, `45-CONTEXT.md`); overrides `devflow.toml`'s `base_branch` key. Set it when your `.planning/` artifacts live on a branch other than `develop` — a worktree forked from `develop` does not carry a `.planning/config.json` that exists only on a planning branch, and the unattended preflight then refuses every launch (999.110/AUTO-01). **An explicitly supplied value that is `main`, blank, or begins with `-` is a hard error, not a silent fallback to `develop`**, and the value must name an existing local branch — a remote-tracking name, a `refs/heads/` path, `HEAD` or a commit SHA is refused. A resolved base other than `develop` is announced at `devflow start`, naming the source |
| `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` | 120 | The per-agent idle timeout for the pipe-owning Antigravity monitor (phase 41, D-08): how long the monitor waits with NO output on the child's `stream-json` stream before recording an `idle_timeout` verdict and terminating it. Same 120s floor and clamp semantics as `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS`. The default is a DECIDED starting point, not a silent inheritance of the Claude-measured floor — it is to be revisited after the first real Antigravity cadence measurement. Claude's variable is never read for Antigravity and vice versa |
| `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` | 120 | How long the pipe-owning Claude monitor waits with NO output on the child's stream before recording an `idle_timeout` verdict and terminating it. Clamped to a 120s floor: a lower value is raised and the clamp is printed loudly at `devflow start`. The floor is 4x the CLI's measured `tool_progress` keepalive interval (a fixed 30.00s, measured 2026-08-03 over five workload-controlled trials on CLI 2.1.220), so it survives three consecutive missed keepalives; the earlier 30s floor sat exactly ON the keepalive interval and would have killed healthy stages running any tool call longer than ~30s, including `cargo test --workspace`. Every stream line resets the window, so a healthy long-running stage is never bounded by wall clock |
| `RUST_LOG` | `off` | Tracing verbosity (stderr). Unset or empty prints no tracing lines; set a level such as `info` or `debug` to see them |
| `DEVFLOW_LOG_FORMAT` | plain | `json` for machine-readable log lines |
| `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` | 90 | Test-only: bounds `gate_sweep_e2e.rs`'s patience with a spawned `devflow __runner` child so CI cannot hang indefinitely; not read by any production code path |

## `.devflow/` file inventory

| File | What it is |
|---|---|
| `state-NN.json` | A phase's persisted stage-machine state (per phase since 14a) |
| `.state-NN.lock` | The phase's short state write lock: held for one state write, usually milliseconds and never across a gate wait, and never removed |
| `.gates-NN.lock` | The per-phase gate lock: held for a few file operations, never removed |
| `.holder-NN` | D-17 foreground holder: held by a foreground verb for its lifetime (a `start` or `resume` until its unit is launched, a `discard`), recording its pid and start time. `stop` refuses while it is held and no runner is live, and never signals it; never removed by `discard` |
| `supervisor-<launch_id>.live` | Launch reservation: its lock is taken before the pending record is saved and held by the spawned supervisor for life. No DevFlow verb removes an abandoned one (D-06) |
| `*.hold-tmp` | A hold being taken, here or under `gates/`. An orphan is removed by `devflow discard --phase N` when phase N has one |
| `phase-NN-stdout` / `phase-NN-stderr.log` | Agent capture files (what `logs` tails) |
| `phase-NN-exit` / `phase-NN-agent-pid` | Exit code + PID the supervisor records |
| `gates/NN-<stage>.json` | Gate request, naming its `gate_id` |
| `gates/NN-{stage}.{gate_id}.response.json` | The answer to that one gate |
| `gates/NN-{stage}.{gate_id}.ack.json` | The claim: the first process to publish it consumes the answer |
| `gates/NN-{stage}.{gate_id}.waiter` | The waiter's registration: its held lock says something waits on that gate. An abandoned one is removed by the stage's next gate close, and so by `devflow discard --phase N` |
| `gates/NN-{stage}.{gate_id\|noid}.{secs}.{pid}.{seq}.quarantined.json` | An answer set aside. It persists until removed by hand |
| `events.jsonl` | Append-only event log (schema v1, one JSON object per line, phase id on every line) — tail it from any tool |
| `history/phase-NN/` | Bounded archive of prior stage stdout/exit captures |

Everything under `.devflow/` and `.worktrees/` is runtime state
(git-ignored); `devflow discard --phase N` is the sanctioned reset of one phase.

## A typical dogfood session

```bash
devflow start --phase 15 --agent claude --mode supervise .
devflow status                       # any time, from any terminal
devflow logs -f --phase 15           # watch the agent work
# ...notify hook pings you at the Validate gate...
devflow gate list
devflow gate approve 15 --note "reviewed the validation"
devflow status                       # finished — worktree and branch kept
```

To end a run early, `devflow stop --phase 15` stops its unit, and
`devflow resume --phase 15` later continues it from its saved stage.

When something wedges: `devflow status` to see each phase's state and the verb
that repairs it, then `devflow resume --phase N` to continue the run, or
`devflow discard --phase N` to remove it (its worktree, state, branch, gate
files and hold temps; exits non-zero if anything could not be removed), then
re-`start`. `discard` refuses a run that may still be live, and names
`devflow stop --phase N` first.
