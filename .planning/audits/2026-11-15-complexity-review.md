# Complexity review, 2026-11-15

Asked for by the operator after Phase 52.1 plan 07 (`devflow discard`) kept turning up unlisted cases. Measured on
`feature/phase-52.1` at `561f8b9`. Line counts exclude blank and comment lines; "test" is every line in a `tests/`
file, a `#[cfg(test)]` module or `test_support.rs`.

## Measurements

| What | Value |
|---|---|
| Production code | 27,822 lines |
| Test and test-support code | 114,594 lines (4.1 to 1) |
| Commands / flags / `DEVFLOW_` names in production code | 13 / 68 / 34 |
| Agent adapters | 6 (claude 58, codex 79, hermes 80, pi 99, antigravity 32, opencode 439 lines) |
| Background process and liveness (`monitor`, `supervisor*`, `liveness`, `recover`, `registry`) | 4,686 lines, 17% |
| File gates (`gates`, `pipeline_gate`) | 2,398 lines, 9% |
| Documentation lint (`doc_check`) | 2,485 lines, 9% |
| Places in `commands.rs` that read a liveness signal directly | 22 |
| Callers of the one combined check, `phase_run_liveness` | 1 (`discard`) |

Liveness is described by four enums that multiply: `PhaseLiveness` (8 values), `PhaseRunLiveness` (6), `Liveness`
(3), `Held` (3), on top of `Existing` (3) for the state file. The signals behind them: the agent pid file, the
supervisor record in state, the socket and its sidecar, the per-launch lock, the write lock. Plan 07 named 18
refusals for `discard`; the command now has 35 tests and a third gap of the same kind is open.

Root cause: the runner is a detached process, and every other command is a separate process that infers from files
whether it is alive. Each fix so far added a signal beside the old ones (pid file, then socket, then sidecar, then
launch lock) instead of replacing them.

## Recommendations, by impact

1. **One answer to "is this run alive".** `liveness.rs` already has a lock the supervisor holds for life, which the
   kernel releases when the process dies. Make that the only liveness test; keep the socket for sending `stop`, not
   for deciding liveness; delete the pid-file and socket-probe paths as liveness sources. Every command calls one
   function. Not verified: that every launch path takes the lock before anything else can observe the run, and how
   a forked child holding a copy of the lock (the 999.221 mechanism) is excluded.
   The larger version of the same idea: run the stage in the foreground of a terminal pane and drop the detached
   supervisor. That removes most of the 4,686 lines and the need for `recover`, but the v4 direction lists the
   detached runner as kept, so it is the operator's call.
2. **Cut what is not the product.** Agent adapters the operator does not drive (opencode alone is 439 lines and
   owned one of this phase's flakes), the documentation lint inside the published crate, and switches nothing sets.
   Low design risk; each cut also removes its tests. Not verified: which adapters are in real use.
3. **Tests as tables, after 1.** A decision over several signals gets one table of every combination and one test
   that walks it, replacing a test per imagined scenario. This is what stops the unlisted-case drip, and it is the
   only route to shrinking the 4-to-1 test mass.

Not established: the size of any saving; whether the same shape explains the project's fix rate (345 `fix` commits
against 393 `feat` on develop); anything about the file gates beyond their size.

## Reassessment of the foreground option (same day, at the operator's request)

The operator leans to running the stage in the foreground and dropping the detached supervisor. Findings.

### What the code shows

- The stage logic is one function, `monitor::run_supervised_stage`. The detached `__monitor` process only calls
  it (`pipeline_launch::run_monitor`) and then advances the stage machine. Six unit tests already call it in the
  test process, so supervising an agent does not need a separate process.
- What exists only because the launcher and the supervisor are different processes, and each stage hands over to a
  new supervisor: the reservation and publish wait, the promotion watch (`Recorded` / `Superseded` /
  `OrphanedPending`), the hand-off fence, the socket, its sidecar and probe, the stale and unprobeable states, the
  backstop, `recover`, and every liveness arm in `stop`, `cleanup`, `discard`, `status` and `doctor`.
- Size, by a keyword scan of function bodies (an upper bound on what is touched, not a count of what would be
  deleted): 9,426 of 27,822 production lines (33%) and 17,675 of 85,671 test lines (20%) sit in functions that
  deal with the background runner. 15 of 35 integration test files start a real background run.

### Experiments (scratch directory, stand-in scripts, nothing of DevFlow's run)

1. A stand-in runner that starts a child in its own session, run as a transient user service
   (`systemd-run --user`, `KillMode=control-group`): `is-active` answered "active"; its output was in the journal;
   after `kill -9` of the runner the service read "inactive" and the child was gone. Control: the same runner
   started as a plain process and killed the same way left the child running. The host has lingering enabled, so a
   user service outlives logout. tmux and herdr are installed.
2. A lifetime lock: with the descriptor closed on exec, the lock read "free" as soon as the holder was killed. With
   the descriptor inherited by a child, it read "held" until the child ended. So a lock alone is a sound "alive"
   test only when no child inherits it; DevFlow passes it across an exec on purpose today.

### What is lost

1. `start` returning at once with the run carrying on by itself. The run lives in a terminal pane or a user
   service started through a wrapper.
2. Starting a long run straight from an agent's shell tool, which has a time limit; it needs the wrapper.
3. `recover`. After a crash the next step is `resume`.
4. `stop` from another terminal with its "stopped while advancing" bookkeeping. It becomes an interrupt or a
   service stop.
5. Diagnosis of half-dead runners in `status` and `doctor`; those states stop existing.
6. A run that survives a terminal closing, unless it was started in tmux, herdr or as a service.
7. Part of this phase's work: plan 07's `discard` and its two follow-ups guard states that would no longer exist.

Kept: the stage chain, output capture and the verdict, idle timeout, failover, the delivery canary, gates and
notify, worktrees, events, `resume`, `status`.

### Not established

- That the real stage runs correctly in the foreground through Plan, Code and Validate: nothing of DevFlow's was
  run. An interrupt has to end the agent's process group; a crashed plain process leaves the agent orphaned
  (experiment 1's control), so either the service's control group or an in-process guard is required.
- How gates are answered while the runner occupies the terminal.
- The effort. The integration tests that assume a detached start are the bulk of it.
- No change can be shown to guarantee quality in advance. What changes is that the "alive" question has few enough
  states to list and test in full.
