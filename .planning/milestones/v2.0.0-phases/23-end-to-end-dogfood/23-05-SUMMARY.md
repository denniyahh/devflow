---
phase: 23-end-to-end-dogfood
plan: 05
subsystem: infra
tags: [rust, cli, process-lifecycle, gates, signals]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood (23-04)
    provides: "devflow_core::gates::Gates::reap — the rejection-only gate response primitive this plan's primary stop path reuses so stop and sweep cannot drift apart"
provides:
  - "devflow stop --phase N [--root PATH] — the missing primitive: ends a running phase by answering its open gate (no signal) or, when no gate is open, signalling the process recorded in the per-phase lock file after confirming liveness and devflow identity"
  - "devflow_core::agent::terminate(pid) -> bool — the crate's one SIGTERM call, guarded against pid 0 and pid > i32::MAX exactly like agent_running"
  - "devflow_core::agent::looks_like_devflow_process(pid) -> bool — fail-closed /proc/<pid>/cmdline identity check"
  - "state.stopped / state.stop_reason, now also written by devflow stop (no schema change) — composes with cleanup's existing stopped && !force refusal"
affects: [23-11 (acceptance run) — the 54-process orphan class 23-ORPHAN-FORENSICS.md documented now has an operator-facing remedy that is not kill(1)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Primary-path-then-fallback command shape: stop_via_gate (writes a file, no signal) attempted first; stop_via_lock (SIGTERM after a two-part liveness+identity check) only runs when the primary path finds nothing — mirrors 23-04's Gates::reap-is-the-only-write-path shape, but for a command that has a real signalling escape hatch."
    - "Race-tolerant error classification at a call site that already scanned once: GateError::AlreadyResponded is success (someone else won the race), GateError::NoOpenGate is 'fall through to the next path,' any other GateError is a real failure — same shape as 23-04's gate_sweep error handling, reused rather than reinvented."

key-files:
  created:
    - crates/devflow-cli/tests/stop_e2e.rs
  modified:
    - crates/devflow-core/src/agent.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - OPERATIONS.md

key-decisions:
  - "commands::stop is spawned as the compiled binary from stop_e2e.rs, never called in-process — devflow-cli has no lib.rs (established fact from 23-04, re-applied here rather than re-litigated), so pub(crate) items are structurally unreachable from tests/. All stop_e2e.rs assertions drive Command::new(devflow_bin())."
  - "Task 1 shipped stop_via_gate as a no-op when no gate is open (no fallback yet, no error tolerance yet); Task 2 added stop_via_lock and changed stop_via_gate's return type to bool so stop() can decide whether to fall through; Task 3 added the GateError/WorkflowError tolerance. This is a genuine three-stage incremental build, not just a documentation split — each task's own <verify> command (scoped to stop_e2e.rs alone, or a name-filtered run) only exercises the tests that specific task introduced, and each stage compiles and passes standalone before the next task's diff lands on top of it."
  - "Command::Stop's fields are named `phase: u32` (long flag, matching the plan's literal signature) and `root: Option<PathBuf>` (long flag, matching GateCmd::Sweep's own --root convention) rather than the positional `project: PathBuf` shape every other subcommand uses — the plan explicitly specified `Option<PathBuf>`, and Sweep's already-established --root flag is the closest existing precedent for an Option-typed root arg."

requirements-completed: [23c]

coverage:
  - id: D1
    description: "devflow stop --phase N ends a gated phase cleanly by writing one rejection file (Gates::reap, attributed devflow-stop) — the target process unwinds through its own abort() path and releases its lock; no signal is ever sent on this path"
    requirement: "23c"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs — stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent (real, separate devflow advance child; measured wall-clock for all 9 tests in the file: 1.18s)"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs — source assertions: exactly one `Command::Stop {` definition, a dispatch arm to commands::stop, `Gates::reap` used inside stop_via_gate, literal `devflow-stop` attribution"
        status: pass
    human_judgment: false
  - id: D2
    description: "When no gate is open, stop signals the process recorded in .devflow/lock-{phase:02} (lock::holder) — never state.monitor_pid, which the generated monitor script's trap only ever captures for the agent, not the trailing advance"
    requirement: "23c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs — stop_never_treats_monitor_pid_as_a_signalling_target; source assertion rg -n 'monitor_pid' shows no occurrence inside commands::stop's body"
        status: pass
    human_judgment: false
  - id: D3
    description: "The signalling fallback refuses a live pid whose /proc/<pid>/cmdline does not identify it as devflow-owned (T-23-52, PID reuse in a stale lock) — fail-closed: errors and signals nothing rather than guessing"
    requirement: "23c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs — looks_like_devflow_process_is_true_for_the_current_process, looks_like_devflow_process_is_false_for_a_non_devflow_process, looks_like_devflow_process_is_false_when_proc_cannot_be_read; crates/devflow-cli/src/commands.rs — stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check"
        status: pass
    human_judgment: false
  - id: D4
    description: "agent::terminate applies agent_running's existing pid-0 and above-i32::MAX guards before ever calling libc::kill — signalling either would be catastrophic (own process group / every signalable process) rather than merely wrong"
    requirement: "23c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs — terminate_rejects_pid_zero, terminate_rejects_pid_above_i32_max, terminate_signals_a_live_child_and_it_exits (cargo test -p devflow-core agent:: — 9 passed)"
        status: pass
    human_judgment: false
  - id: D5
    description: "stop is idempotent and race-tolerant: a second run against an already-answered gate, a hand-written response, or a phase with no persisted state at all all succeed rather than error"
    requirement: "23c"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs — stop_is_idempotent_against_an_already_answered_gate, stop_against_a_hand_written_response_is_a_success_no_op, stop_against_a_root_with_no_state_is_a_success"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs — stop_is_a_success_no_op_with_no_gate_and_no_lock, stop_is_a_success_no_op_when_the_lock_names_a_dead_pid"
        status: pass
    human_judgment: false
  - id: D6
    description: "cleanup --force keeps refusing to touch a live/stopped phase without --force; stop is the verb that makes a phase not-live, and the two compose in that order — commands::cleanup's own body is provably untouched by this plan"
    requirement: "23c"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs — stop_then_cleanup_composes_refuse_then_force"
        status: pass
      - kind: other
        ref: "git diff --stat across all three task commits shows no change to commands::cleanup's function body"
        status: pass
    human_judgment: false
  - id: D7
    description: "The full workspace gate — cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --check — passes as a single chain after all three tasks land"
    requirement: "23c"
    verification:
      - kind: other
        ref: "cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check (exit 0)"
        status: pass
    human_judgment: false

# Metrics
duration: 13min
completed: 2026-07-25
status: complete
---

# Phase 23 Plan 05: `devflow stop` — Ending a Phase Without `kill(1)` Summary

**`devflow stop --phase N` — writes a gate rejection when one is open (the target unwinds through its own `abort()`, no signal sent) or SIGTERMs the confirmed-live, confirmed-devflow process recorded in the per-phase lock file when it isn't, never `state.monitor_pid`, and composes cleanly with `cleanup`'s unweakened fail-closed refusal.**

## Performance

- **Duration:** 13 min (Task 1 commit `22:45:20` → Task 3 commit `22:58:41`)
- **Started:** 2026-07-25T22:45:20-04:00 (Task 1 commit)
- **Completed:** 2026-07-25T22:58:41-04:00 (Task 3 commit)
- **Tasks:** 3 (Task 1 tracer, Task 2 TDD, Task 3 auto)
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments

- `devflow stop --phase N [--root PATH]`: a new `Command::Stop` variant, dispatched to `commands::stop`, which is the missing primitive `23-ORPHAN-FORENSICS.md` names as the reason 54 processes accumulated with no remedy but `kill(1)`.
- Primary path (`stop_via_gate`): finds `phase`'s open gate via `Gates::list_open`, answers it with `Gates::reap` (plan 23-04's rejection-only primitive), attributed `devflow-stop`, with a note whose lowercase form contains `abort` so `GateAction::from_response` resolves to `Abort` rather than looping back to Code. Writes one file; sends no signal. Race-tolerant: `GateError::AlreadyResponded` is treated as success, `GateError::NoOpenGate` falls through to the lock fallback.
- Fallback path (`stop_via_lock`, runs only when no gate is open): reads `lock::holder` (never `state.monitor_pid` — T-23-51, the monitor shell's `trap` closure only ever captures the agent's pid, so `monitor_pid` names a long-dead process by the time `advance` is the shell's foreground child). Confirms the recorded pid is alive (`agent::agent_running`) **and** identifies as devflow (`agent::looks_like_devflow_process`, a fail-closed `/proc/<pid>/cmdline` scan) before calling the new `agent::terminate`, which applies `agent_running`'s existing pid-0/above-`i32::MAX` guards — signalling either would be catastrophic here (own process group / every signalable process), not merely wrong.
- `persist_stopped_state`: unconditionally marks `state.stopped = true` and appends to (never overwrites) `state.stop_reason`, leaving `stop_until` untouched. Tolerates a missing state file as success — a phase never started, or already cleared by a completed `abort()`, is already stopped.
- `crates/devflow-cli/tests/stop_e2e.rs` (9 tests, new file): the strongest — a real, separate `devflow advance` child parked on a Code gate, ended by `devflow stop` writing a rejection, exiting cleanly, releasing its lock, and leaving `workflow_aborted` in the events file — reuses plan 23-04's bounded-wait fixture shape (`try_wait` loop, `DEVFLOW_E2E_CHILD_TIMEOUT_SECS`, reap on both paths) verbatim so the two real-child fixtures in this workspace share one reliability profile. Measured wall-clock for all 9 tests together: **1.18s** (comfortably alongside 23-04's ~1.1s for its 4).
- 9 new unit tests across `devflow-core/src/agent.rs` (terminate + identity-check guards) and `devflow-cli/src/commands.rs` (lock-fallback edge cases).

## Task Commits

1. **Task 1: End-to-end "stop a gated phase" — the no-signal path, wired through** - `a4ad504` (feat)
2. **Task 2: The signalling fallback — target the lock holder, verify its identity, never the monitor** (TDD, combined RED+GREEN in one commit per plan 23-04's precedent — the RED-phase tests referenced the not-yet-existing `agent::terminate`/`agent::looks_like_devflow_process` and failed to compile, a genuine RED, before the implementation landed) - `dec4583` (feat)
3. **Task 3: Idempotency, race tolerance, and proof that cleanup's refusal still stands** - `a397877` (test)

**Plan metadata:** (this SUMMARY's commit, made by the parallel-worktree orchestrator after merge — not created by this executor per the worktree protocol)

## Files Created/Modified

- `crates/devflow-core/src/agent.rs` - `terminate(pid) -> bool`, `looks_like_devflow_process(pid) -> bool`, 6 new unit tests
- `crates/devflow-cli/src/commands.rs` - `stop`, `stop_via_gate`, `stop_via_lock`, `persist_stopped_state`; 7 new unit tests
- `crates/devflow-cli/src/main.rs` - `Command::Stop { phase, root }`, dispatched to `commands::stop`
- `crates/devflow-cli/tests/stop_e2e.rs` - New file: 9 end-to-end tests, the strongest of which spawns a real `devflow advance` child process
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` - Regenerated for the new `stop` command (Rule 3 auto-fix, required by `help_snapshot.rs`)
- `OPERATIONS.md` - Documented `devflow stop` (Rule 2 auto-fix, required by `doc_check::source_devflow_env_vars_and_subcommands_are_documented`)

## Decisions Made

- **Three-stage incremental build, not a single combined diff:** Task 1 shipped `stop_via_gate` as a standalone no-op when no gate is open (no fallback, no error tolerance). Task 2 added `stop_via_lock` and changed `stop_via_gate`'s return type to `bool` so `stop()` could decide whether to fall through. Task 3 added the `GateError`/`WorkflowError` tolerance. Each task's own file state compiles and passes its own `<verify>` command standalone before the next task's diff lands — matching the plan's task boundaries exactly rather than writing the final version once and splitting the diff after the fact.
- **`commands::stop` is driven via the compiled binary from `stop_e2e.rs`, never called in-process:** `devflow-cli` has no `lib.rs` (established by 23-04, re-applied here), so `pub(crate)` items are structurally unreachable from `tests/`. All `stop_e2e.rs` assertions spawn `Command::new(devflow_bin())`, matching `gate_sweep_e2e.rs`'s precedent.
- **`Command::Stop`'s CLI shape:** `#[arg(long)] phase: u32` and `#[arg(long)] root: Option<PathBuf>`, matching the plan's literal signature and `GateCmd::Sweep`'s own `--root` convention, rather than the positional `project: PathBuf` every other subcommand uses (which is a required, not `Option`, field).
- **State-persistence tests each set up an explicit `State` unless the missing-state path is specifically what they test:** Task 2's lock-fallback tests (`stop_is_a_success_no_op_when_the_lock_names_a_dead_pid`, `stop_refuses_to_signal_...`, `stop_never_treats_monitor_pid_...`) each write an explicit state file so they exercise only the lock-fallback behavior under test, not Task 3's separate missing-state tolerance — kept the two concerns from being conflated in a single assertion.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `help_snapshot.rs`'s committed CLI snapshot drifted after adding `Command::Stop`**
- **Found during:** Task 3, running the full workspace gate
- **Issue:** `crates/devflow-cli/tests/help_snapshot.rs` asserts `devflow --help`'s output byte-matches a committed snapshot file; adding a new top-level subcommand necessarily changes that output.
- **Fix:** Regenerated via the test's own documented command: `cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt`.
- **Files modified:** crates/devflow-cli/tests/snapshots/devflow-help.txt
- **Verification:** `cargo test -p devflow --test help_snapshot` passes; full `cargo test --workspace` green
- **Committed in:** `a397877` (Task 3)

**2. [Rule 2 - Missing Critical] Documented `devflow stop` in OPERATIONS.md**
- **Found during:** Task 1, then re-verified failing at Task 3's full workspace gate
- **Issue:** `crates/devflow-core/src/doc_check.rs`'s `source_devflow_env_vars_and_subcommands_are_documented` test enumerates every `Command` enum variant and asserts `devflow {variant-name}` appears in scoped operator docs (README.md, ARCHITECTURE.md, CONTRIBUTING.md, OPERATIONS.md). `Command::Stop` was not documented anywhere.
- **Fix:** Added a `devflow stop --phase N [--root PATH]` row to OPERATIONS.md's commands table (Task 1's commit).
- **Files modified:** OPERATIONS.md
- **Verification:** `cargo test -p devflow-core --features test-support doc_check::source_devflow_env_vars_and_subcommands_are_documented` passes
- **Committed in:** `a4ad504` (Task 1)

**3. [Rule 3 - Blocking] The OPERATIONS.md row's own prose accidentally matched `doc_check`'s devflow-subcommand-token scanner**
- **Found during:** Task 3, running the full workspace gate (`doc_referenced_identifiers_exist_in_source` failed with `documented command \`devflow process\` does not exist in the CLI enum surface`)
- **Issue:** `doc_check.rs`'s `documented_subcommands` scans docs for the literal substring `devflow ` (lowercase, trailing space) and treats the next lowercase word as an asserted-to-exist CLI subcommand. The Task 1 doc row's phrase "identifies it as a devflow process" was read as documenting a nonexistent `devflow process` subcommand — a self-referential false positive on the row's own explanatory prose, the same class of collision 23-04's SUMMARY recorded for its own negative greps.
- **Fix:** Reworded to "identifies it as belonging to DevFlow" (capitalized project name, no longer contains the literal `devflow ` + lowercase-word pattern).
- **Files modified:** OPERATIONS.md
- **Verification:** `cargo test -p devflow-core --features test-support doc_check::doc_referenced_identifiers_exist_in_source` passes; full `cargo test --workspace` green
- **Committed in:** `a397877` (Task 3)

---

**Total deviations:** 3 auto-fixed (1 blocking committed-snapshot drift, 1 missing critical doc requirement, 1 blocking self-referential doc-scanner collision)
**Impact on plan:** All three were necessary to make the plan's own verification gates pass; none changed the plan's design, added out-of-scope functionality, or weakened any threat-model mitigation.

## Issues Encountered

- **Acceptance-criteria grep imprecision (not fixed, documented here):** Task 1's acceptance criterion `rg -n 'Stop \{' crates/devflow-cli/src/main.rs matches exactly one line` is not literally satisfiable together with its own second half (`a dispatch arm calling commands::stop exists`) — Rust match-arm syntax for a struct variant necessarily writes `Command::Stop { ... }`, which also contains the substring `Stop {`, so the grep matches twice (the enum definition and the dispatch arm) for any correctly-implemented struct-variant command. Verified this is not specific to `Stop`: the pre-existing `Ship` variant's own `rg -n 'Ship \{'` also matches twice in this same file (its definition and its dispatch arm), confirming the codebase's established pattern for every struct-variant command already produces 2 matches, not 1. Both parts of the intended criterion (a single variant definition; a dispatch arm exists) are satisfied; the literal match-count phrasing is not achievable without contradicting Rust's own match syntax. No code change made — this is a plan-authoring artifact, not a functional gap.
- **`cargo test -p devflow-core agent::` requires `--features test-support`** to compile the workspace's integration test files (a pre-existing, previously-documented quirk from 23-03/23-04's SUMMARYs — `crates/devflow-core/tests/*.rs` reference the feature-gated `devflow_core::test_support` module). Every scoped verify command in this plan's execution used `--features test-support`; the plan's own literal Task 2 `<verify>` line (`cargo test -p devflow-core agent::`, no `--features`) fails to compile for the same pre-existing reason 23-04 already recorded — `cargo test --workspace` was used as the authoritative full-suite gate at every task boundary and is unaffected.

## Known Stubs

None.

## Threat Flags

None — every new surface (`devflow stop`, `agent::terminate`, `agent::looks_like_devflow_process`, the `.devflow/lock-{phase:02}` → `libc::kill` boundary) is already covered by this plan's own `<threat_model>` (T-23-51 through T-23-57, T-23-SC), all `mitigate`d and verified: `rg -n 'monitor_pid'` shows no occurrence inside `commands::stop`'s body; `terminate(0)`/`terminate(u32::MAX)` both return `false` without signalling; `looks_like_devflow_process` fails closed on every unreadable-`/proc` case; `rg -c 'libc'` against `crates/devflow-cli/Cargo.toml` returns 0 (the signalling helper stayed in `devflow-core`); `git diff --stat` across every task commit shows no change to `commands::cleanup`'s body.

## Manual Smoke Verification

Per the plan's `<verification>` section: `devflow stop --phase 99 --root <empty-tempdir>` printed `stop: no lock held for phase 99 — nothing is running \`advance()\`` followed by `stop: no persisted state for phase 99 — already stopped`, and exited 0 — confirming a phase-99-on-a-project-with-no-phase-99 no-op is clean and silent-failure-free.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `devflow stop` is a complete, independently-usable remedy for the orphan class `23-ORPHAN-FORENSICS.md` documented — an operator can now end a running phase, whether it is parked at a gate or stuck mid-`advance`, without `ps`/`kill` archaeology, and `cleanup --force`'s fail-closed refusal to touch a live phase is provably unweakened.
- No blockers. Full workspace suite green at every task boundary — final count 368 tests in `devflow-core` (`--features test-support`), plus the `devflow` package's bin + 9 integration test files, 0 failed; `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` all green as a single chain.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-25*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/agent.rs
- FOUND: crates/devflow-cli/src/commands.rs
- FOUND: crates/devflow-cli/src/main.rs
- FOUND: crates/devflow-cli/tests/stop_e2e.rs
- FOUND: crates/devflow-cli/tests/snapshots/devflow-help.txt
- FOUND: OPERATIONS.md
- FOUND: .planning/phases/23-end-to-end-dogfood/23-05-SUMMARY.md
- FOUND: commit a4ad504 (Task 1 feat)
- FOUND: commit dec4583 (Task 2 feat)
- FOUND: commit a397877 (Task 3 test)
