# New direction brief: radical reduction, starting with a foreground-runner spike

Written 2026-11-15 for a fresh session. The operator halted Phase 52.1 and wants a new path decided before any
more coding. Read this, then `2026-11-15-complexity-review.md` beside it for the measurements. Nothing here is an
instruction to start work: the operator decides the next step.

## Where things stand

- Phase 52.1 is halted by the operator, not finished. Seven of fifteen plans are done (01 to 07), plus three
  follow-ups. The phase worktree (`.worktrees/phase-52.1`, branch `feature/phase-52.1`) is clean at `561f8b9` and
  not pushed. Its last full test run passed (2,154 tests, the follow-up executor's run; not re-run since).
- Plans 08 to 12 harden the background runner. They are on hold and would be replaced, not resumed, if the
  foreground design is adopted. Plans 13 and 14 (evidence and the live proof run) depend on them.
- One follow-up was stopped before it wrote anything: named tests for plan 04's hand-checked cases.
- A third gap of the same kind is open in the delete command and in `start`; it was found by reading and not
  fixed. It becomes moot under the foreground design.
- `.continue-here.md` and `HANDOFF.json` in `.planning/` point here.

## Why the phase was halted

Plan 07 added a command that deletes a run's state. It needed to know whether a run is alive, and unlisted cases
kept appearing: the plan named 18 refusals, the command ended with 35 tests, and a third gap surfaced after two
were fixed. The operator asked why this keeps happening across the whole project.

Finding: the runner is a detached background process, and every other command is a separate process that infers
from files whether it is alive. There are five signals (agent pid file, the supervisor record in state, the socket
and sidecar, the per-launch lock, the write lock), read directly in 22 places, described by enums that multiply.
Each past fix added a signal beside the old ones. Plans list imagined scenarios, reviews sample them, and tests
cover only what was listed, so the missing combinations surface during coding.

## The direction the operator leans to

Brutal reduction to get a working DevFlow out. Three changes, by expected impact:

1. **Run the stage in the foreground and drop the detached supervisor.** The run lives in a terminal pane or a
   user service. The "is it alive" machinery goes.
2. **Cut what is not the product:** agent adapters not in use, the documentation lint inside the published crate,
   unused switches.
3. **Test decisions by table:** one table of every combination, walked by one test, in place of a test per
   scenario.

The operator does not trust the agent's assessments at this point and wants evidence before commitment.

## Evidence so far, and its limits

Tested on this machine with stand-in scripts, not with DevFlow:

- A stand-in runner started as a transient user service answered "active", kept its output in the journal, and
  when killed hard the service went inactive and its detached child was gone. Started as a plain process and
  killed the same way, the child was left running. Lingering is enabled, so a user service outlives logout.
- A lifetime lock reads "free" when its holder is killed only if no child inherited the descriptor; with an
  inherited descriptor it read "held" until the child ended. The agent's earlier recommendation (keep the
  background runner, use the lock as the single test) was a guess and is weaker than first stated.

From reading the code, not from running it:

- The stage logic is one function that six unit tests already run inside the test process.
- About a third of production lines sit in functions that deal with the background runner. That is what is
  touched, an upper bound, not a count of what would be deleted.

Guesses, with nothing behind them yet:

- That a real stage completes correctly in the foreground.
- Any time or token cost for the conversion.
- That the same root cause explains the project's fix rate.

## What the foreground design gives up

- `start` returning at once while the run continues by itself; a long run started from an agent's shell tool needs
  a wrapper.
- `recover`; after a crash the step is `resume`.
- `stop` from another terminal in its current form; it becomes an interrupt or a service stop.
- Diagnosis of half-dead runs in `status` and `doctor`.
- A run surviving a closed terminal unless started in tmux, herdr or as a service.
- Part of plan 07 and its follow-ups, which guard states that would no longer exist.

Kept: the stage chain, output capture and verdict, idle timeout, failover, delivery canary, gates and notify,
worktrees, events, `resume`, `status`.

## Proposed next step: a spike

Purpose: replace the first guess above with evidence, at the lowest cost. Under the operator's fewest-tokens rule
the smallest version is the default; the larger one needs their word.

- **Smallest:** on a throwaway branch, make the real program run ONE stage in the foreground with a stand-in agent,
  and report whether it completes and what broke.
- **Larger:** the same, started as a user service, killed partway, then resumed.

Rules for the spike: throwaway branch cut from the phase branch, never merged; no cleanup of the old machinery;
scratch repository only (`scripts/scratch-dogfood-repo.sh`); no real agent and no installed binary; stop and
report at the first surprise instead of working round it. The output is a yes or no with what was observed, not
a design.

Open questions the spike does not answer, for the operator to weigh afterwards: how a gate is answered while the
runner holds the terminal; how an interrupt ends the agent's process group; whether Phase 52.1 is closed out with
what it has (waves 1 and 2 are independent of the runner design) or abandoned; what becomes of the v4 decision
that listed a detached runner as kept.

## Standing changes made on 2026-11-14 and 15 that a new session inherits

All in `~/.config/agents/RULES-CHANGELOG.md`. The ones that change behaviour:

- Global rules open with two rule-zero items: spend the fewest tokens and ask before any costlier path; never
  present a recommendation or claim as fact or high confidence without research or objective sources.
- Reports: impact only, detail on request; buckets are Needs my decision, Action items, Handled, FYI, Detail.
- A recommendation is not an instruction: starting work needs the operator's word.
- Project rules live in `AGENTS.md`; `CLAUDE.md` is a symlink to it.
- GSD is pinned to 1.21.0 in the operator's topgrade config until this phase is closed or abandoned; restore
  `@latest` then.
- Agent hooks belong to WSX, not DevFlow; a research brief is in `~/Github/wsx/.planning/research/`.
- Queued for after the phase and now unscheduled: the first `harness-retro`, the worktree-isolation trial, the
  codebase-map refresh, the MemPalace re-file.

## Decisions and spike results (2026-11-15, later the same day)

Operator decisions:

- The foreground runner is the direction. The detached supervisor goes.
- The hard-kill case is covered by running a long run as a user service (`systemd-run --user`), not by an
  in-process guard.

What was run, on the real program with stand-in agents, in the throwaway worktree `.worktrees/spike-foreground`
(branch `spike/foreground-runner`, uncommitted, never to be merged; five tests named `spike_*` in
`crates/devflow-cli/tests/supervisor_e2e.rs`, one run each):

- Plan, Code and Validate ran to a finished run inside one `start`, in the Claude launch shape, with no background
  process.
- A Validate gate was answered by a separate `devflow gate approve` process while `start` waited, and the run
  finished.
- SIGINT to the runner's process group ended the agent's group and `start` exited. The stop is recorded only by
  code the spike added: `record_supervisor_stop` writes nothing without a supervisor record.
- SIGKILL of the runner left the agent's group running; `status` reported it. After the test ended the orphan,
  `resume` ran to finished. `resume` with the orphan still alive was not run.
- With `PR_SET_PDEATHSIG` on the agent, SIGKILL of the runner ended the agent but not the agent's child. That is
  why the in-process guard was rejected.
- The user-service result is from the stand-in experiment recorded in the complexity review, not from DevFlow.

Not yet decided: how Phase 52.1 is closed (plans 01 to 07 kept, 08 to 14 dropped) and when the conversion phase
is opened.

### Double check before proceeding (same day)

- The user service was then run with the real program (two more `spike_*` tests, one run each): started with
  `systemd-run --user -p KillMode=control-group`, SIGKILL of the main process left no member of the agent's group
  and the unit read inactive; `systemctl --user stop` ended the agent's group and the stop was recorded.
- Correction to "drop plans 08 to 14": read by objective, only plan 08 exists solely for the detached supervisor.
  Plans 09 to 12 and 15 carry operator decisions that do not depend on the runner (D-11 remove `--force` from
  `start` and `cleanup`, D-09.1 `start` only on a new phase, D-09.5, D-24, D-27), and 13 and 14 are the phase
  goal's proof run (D-15). They need replanning against the foreground runner, not dropping.
