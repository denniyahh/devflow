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
