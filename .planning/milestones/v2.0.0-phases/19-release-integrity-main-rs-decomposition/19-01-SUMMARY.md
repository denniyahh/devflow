---
phase: 19-release-integrity-main-rs-decomposition
plan: 01
subsystem: infra
tags: [rust, filesystem, gitignore, devflow-core, workflow]

requires: []
provides:
  - "workflow::ensure_devflow_dir(dir) — the single chokepoint every .devflow/-creating call site now routes through"
  - "Regression coverage proving all 7 sites (incl. the sequentagent/parallel path) produce .devflow/.gitignore"
  - "Downstream-repo reproduction test closing the 17-REVIEW.md WR-01 scenario"
affects: [19-02, 19-03]

tech-stack:
  added: []
  patterns:
    - "Path-component walk (not ancestors()-tail string match) to resolve a marker directory correctly for both absolute and relative inputs"
    - "create_new(true) + AlreadyExists->Ok(()) as the idempotent, race-safe 'write once, never clobber' file-creation idiom"

key-files:
  created:
    - crates/devflow-core/tests/devflow_dir_gitignore.rs
  modified:
    - crates/devflow-core/src/workflow.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/events.rs
    - crates/devflow-core/src/ship.rs
    - crates/devflow-core/src/lock.rs

key-decisions:
  - "ensure_devflow_dir returns std::io::Result<()>, not a crate-specific error enum, so ? converts at all 7 call sites (6 different error enums) with zero signature churn"
  - "Marker resolution walks dir.components() (not ancestors()) so a relative .devflow-leaf path resolves correctly without depending on process cwd"
  - "Deleted .devflow/.gitignore is never recreated — documented tradeoff, not a bug: recreating would violate 'never overwrite a file a user/tool may own'"

patterns-established:
  - "ensure_devflow_dir(dir) vs. devflow_dir(project_root): the pure-accessor / side-effecting-constructor pair, documented explicitly to prevent future confusion (Codex review)"

requirements-completed: [19a]

coverage:
  - id: D1
    description: "workflow::ensure_devflow_dir creates a directory and writes a self-ignoring .devflow/.gitignore, idempotently and without clobbering foreign content"
    requirement: "19a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#workflow::tests::ensure_devflow_dir_writes_star_gitignore"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#workflow::tests::ensure_devflow_dir_is_idempotent_and_preserves_existing_gitignore"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#workflow::tests::ensure_devflow_dir_preserves_foreign_gitignore_content"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#workflow::tests::ensure_devflow_dir_on_nested_subpath_marks_the_devflow_ancestor"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#workflow::tests::ensure_devflow_dir_on_relative_devflow_leaf_path_marks_it"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#workflow::tests::ensure_devflow_dir_without_a_devflow_ancestor_only_creates_dirs"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#workflow::tests::ensure_devflow_dir_concurrent_calls_both_succeed"
        status: pass
    human_judgment: false
  - id: D2
    description: "All 7 production .devflow/-creating call sites (workflow, gates, monitor, agent_result, events, ship, lock), including the sequentagent/parallel path, route through ensure_devflow_dir"
    requirement: "19a"
    verification:
      - kind: integration
        ref: "crates/devflow-core/tests/devflow_dir_gitignore.rs#all_seven_devflow_constructors_produce_the_gitignore"
        status: pass
    human_judgment: false
  - id: D3
    description: "A downstream repo with no .devflow root-.gitignore pattern commits zero .devflow/ paths after git add . && git commit"
    requirement: "19a"
    verification:
      - kind: integration
        ref: "crates/devflow-core/tests/devflow_dir_gitignore.rs#git_add_all_no_longer_sweeps_devflow_into_a_commit"
        status: pass
    human_judgment: false

duration: 55min
completed: 2026-07-21
status: complete
---

# Phase 19 Plan 01: `.devflow/` artifact hygiene (19a) Summary

**New `workflow::ensure_devflow_dir` chokepoint converts all 7 production `.devflow/`-creating sites (including the never-`save_state` sequentagent/parallel path) to self-write a `.devflow/.gitignore` containing `*`, closing 19a-WR-01/WR-03 with regression + downstream-repo-reproduction coverage.**

## Performance

- **Duration:** 55 min
- **Started:** 2026-07-21T23:47:00Z
- **Completed:** 2026-07-22T00:42:35Z
- **Tasks:** 3
- **Files modified:** 7 (+ 1 new test file)

## Accomplishments

- `workflow::ensure_devflow_dir(dir: &Path) -> std::io::Result<()>` — creates `dir`, then locates the shallowest `.devflow` path *component* (not a string-matched ancestor) and writes `<marker>/.gitignore` containing `*` via `create_new(true)`, mapping a lost race (`AlreadyExists`) to `Ok(())` so a foreign or already-written `.gitignore` is never clobbered.
- All 7 production `create_dir_all` sites converted: `workflow.rs` (`write_state_atomic`), `gates.rs` (`write_atomic`), `monitor.rs` (capture dir before the detached spawn), `agent_result.rs` (`archive_phase_files`'s `history_dir`), `events.rs` (fail-soft let-chain, shape unchanged), `ship.rs` (`write_cron_instructions`), `lock.rs` (`acquire_path`).
- New integration test proves all 7 sites — driven through their real public API, not `ensure_devflow_dir` directly — produce the `.gitignore`, explicitly naming the two sequentagent/parallel-path sites (`spawn_monitor_no_advance`, `archive_phase_files`) that a `save_state` chokepoint would have missed.
- New reproduction test closes the literal `17-REVIEW.md` scenario: a scratch git repo whose root `.gitignore` has no `.devflow` pattern commits zero `.devflow/` paths after `git add . && git commit`, plus the empty-input edge (a `.devflow/` containing only `.gitignore` stages nothing).
- `workflow::devflow_dir()` is provably still pure — untouched, single `join` expression, no `fs::` call.

## Task Commits

1. **Task 1: Add `workflow::ensure_devflow_dir` with its own unit tests** - `3281810` (feat)
2. **Task 2: Convert all 7 production `.devflow/` constructors** - `85aaecf` (fix)
3. **Task 3: Coverage + downstream-repo reproduction tests** - `0b2604f` (test)

**Plan metadata:** *(this commit)* (docs: complete plan)

## Files Created/Modified

- `crates/devflow-core/src/workflow.rs` — new `ensure_devflow_dir` + private `find_devflow_marker` component-walk helper, placed immediately after `devflow_dir`; `write_state_atomic` converted; 7 new unit tests
- `crates/devflow-core/src/gates.rs` — `write_atomic` converted
- `crates/devflow-core/src/monitor.rs` — capture-directory creation in `spawn_monitor_inner` converted
- `crates/devflow-core/src/agent_result.rs` — `archive_phase_files_with_stamp`'s `history_dir` creation converted
- `crates/devflow-core/src/events.rs` — `emit`'s fail-soft let-chain converted (byte-identical `warn!` + early `return` shape)
- `crates/devflow-core/src/ship.rs` — `write_cron_instructions` converted
- `crates/devflow-core/src/lock.rs` — `acquire_path` converted
- `crates/devflow-core/tests/devflow_dir_gitignore.rs` — new integration test file (2 tests: `all_seven_devflow_constructors_produce_the_gitignore`, `git_add_all_no_longer_sweeps_devflow_into_a_commit`)

## Decisions Made

- **`ensure_devflow_dir` returns `std::io::Result<()>`**, not a crate-specific error enum — deliberate, per plan: the 7 call sites span 6 different error enums (`WorkflowError`, `GateError`, `MonitorError`, `ResultError`, `ShipError`, `LockError`), each already carrying an `Io(#[from] std::io::Error)` variant, so `?` converts everywhere with zero signature churn.
- **Marker resolution walks `dir.components()`**, not `dir.ancestors()` string-matching — this is what makes a relative `.devflow`-leaf input (e.g. `Path::new(".devflow/captures")` or the bare `Path::new(".devflow")`) resolve to the right marker directory without hitting an empty final ancestor.
- **The relative leaf-path test (`ensure_devflow_dir_on_relative_devflow_leaf_path_marks_it`) exercises the private `find_devflow_marker` helper directly**, per the plan's stated preference, rather than mutating and restoring the test process's cwd — cwd is process-global and shared across concurrently-running test threads in the same binary, so asserting on the pure marker-resolution logic proves the same contract without that risk.
- **Task 3's monitor-cleanup requirement needed an explicit `libc::waitpid` reap**, not just a liveness poll: `spawn_monitor_no_advance` returns only a detached child's `u32` pid (no `Child` handle), but the *test binary itself* is still that process's OS-level parent (it invoked `Command::spawn()` internally), so the exited shell sits as a zombie — which still answers `kill(pid, 0)` successfully — until explicitly reaped. Discovered empirically: the first version of the test (polling liveness alone, no reap) hung/failed at the 5s timeout; adding the `waitpid` call before the liveness poll fixed it.
- **The reproduction test's first commit needed a real non-`.devflow` file change** (`README.md`) alongside the `.devflow/` sentinel write — discovered empirically: with only `.devflow/` content changed, `git add .` staged nothing (fully ignored) and `git commit` failed with "nothing to commit," which is actually the *correct* proof of the fix but broke the test's literal `git commit` step. Added the unrelated real change so the commit succeeds while `.devflow/` still contributes nothing to it.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Zombie monitor process leaking past the coverage test**
- **Found during:** Task 3 (`all_seven_devflow_constructors_produce_the_gitignore`)
- **Issue:** The plan's monitor-cleanup instruction said to poll the returned pid via `kill -0`/`/proc` until it disappeared. `spawn_monitor_no_advance` doesn't return a `Child` handle, so nothing in the test explicitly reaps the process — but the test binary IS its OS parent, meaning the exited shell becomes a zombie (still answers `kill(pid, 0)` == alive) that a pure liveness poll would never see disappear within any timeout.
- **Fix:** Added an explicit `unsafe { libc::waitpid(pid as libc::pid_t, &mut status, 0) }` reap before the liveness poll in `wait_for_pid_to_die`. `libc` was already a direct dependency of `devflow-core` (used in `agent.rs`), so no new dependency was introduced.
- **Files modified:** `crates/devflow-core/tests/devflow_dir_gitignore.rs`
- **Verification:** `cargo test -p devflow-core --test devflow_dir_gitignore` passes; the monitor-cleanup test no longer times out and asserts `!agent::agent_running(pid)` before returning.
- **Committed in:** `0b2604f` (Task 3 commit)

**2. [Rule 1 - Bug] Reproduction test's `git commit` failed on an all-ignored diff**
- **Found during:** Task 3 (`git_add_all_no_longer_sweeps_devflow_into_a_commit`)
- **Issue:** With only `.devflow/` content added, `git add .` staged nothing (everything ignored) and the subsequent `git commit -q -m "routine commit"` exited non-zero ("nothing to commit"), failing the test — even though this is actually the desired outcome for the *file*, it made the literal "run a commit and check its contents" test step non-representative of a routine mixed commit.
- **Fix:** Added a real, non-`.devflow` file write (`README.md` append) alongside the `.devflow/` sentinel before `git add . && git commit`, so the commit legitimately succeeds and its absence of `.devflow/` paths is a meaningful assertion rather than a vacuous one.
- **Files modified:** `crates/devflow-core/tests/devflow_dir_gitignore.rs`
- **Verification:** `cargo test -p devflow-core --test devflow_dir_gitignore` passes; `git log -1 --name-only` output contains `README.md` and no `.devflow/` path.
- **Committed in:** `0b2604f` (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 1 — bugs found while writing Task 3's own verification, not in the production fix itself)
**Impact on plan:** Both fixes are test-only corrections needed to make Task 3's tests actually pass and mean what they claim; no production code or scope changed as a result.

## Issues Encountered

None beyond the two auto-fixed test issues above.

### RED-first evidence (Task 3 acceptance criterion)

Before Task 2's conversions, the coverage test would have failed. Reproduced live by temporarily reverting `lock.rs`'s converted site back to a bare `fs::create_dir_all(parent)?;` (restoring immediately after) and re-running:

```
thread 'all_seven_devflow_constructors_produce_the_gitignore' panicked at
crates/devflow-core/tests/devflow_dir_gitignore.rs:177:5:
the following .devflow/ constructor(s) did not produce a .devflow/.gitignore: ["lock::acquire (lock.rs)"]
```

The failure names exactly the reverted site. Restored and re-verified green (`cargo test -p devflow-core --test devflow_dir_gitignore` → 2 passed, 0 failed).

### Manual confirmation: `run_agent_blocking` (`main.rs:2417`)

Read directly at HEAD. Its complete list of calls that touch `.devflow/`:

1. `agent_result::archive_phase_files(project_root, workdir, phase, capture_retention(project_root))` (`main.rs:2423`) — always called; now routes through `ensure_devflow_dir` via `history_dir`'s creation.
2. `events::emit(project_root, phase, "capture_archived", …)` (`main.rs:2434`) — called **conditionally**, only when `archive_phase_files` returned `Some(stamp)` (i.e. there was something to archive). This is a *third* `.devflow/`-touching call inside `run_agent_blocking` not named in the plan's two-call framing, but it is covered by the same conversion since `events.rs`'s `emit` is one of the 7 sites converted in Task 2.
3. `monitor::spawn_monitor_no_advance(&state, program, &args, &adapter.extra_env())` (`main.rs:2461-2463`) — always called; routes through `ensure_devflow_dir` via the capture-directory creation in `spawn_monitor_inner`.

Confirmed: `run_agent_blocking` never calls `workflow::save_state` anywhere in its body — the `state` it constructs (`main.rs:2456`) is explicitly commented as "Synthetic, never-persisted state." This is exactly why D-14's original `save_state`-chokepoint proposal was rejected in favor of converting the constructors themselves — all three of this function's `.devflow/`-touching calls are covered by construction, not by any single call-graph chokepoint.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 19a (`.devflow/` artifact hygiene) is closed: every production path that creates `.devflow/` self-protects against a downstream user's `git add .`, independent of that user's own root `.gitignore`.
- `workflow::ensure_devflow_dir` is now available as the crate's one sanctioned `.devflow/`-directory constructor for any future call site (e.g. new 19c–19f split modules should call it, not a raw `create_dir_all`, if they ever construct a fresh `.devflow/` path directly — though the split plans are expected to move existing call sites, not add new ones).
- No blockers for 19-02 (`commit_path` empty commits) or the `main.rs` split plans — this plan touched only `devflow-core`, zero overlap with `main.rs`.

---
*Phase: 19-release-integrity-main-rs-decomposition*
*Completed: 2026-07-21*

## Self-Check: PASSED

All 8 claimed files verified present on disk (7 modified `devflow-core` source
files + 1 new integration test file). All 4 claimed commit hashes
(`3281810`, `85aaecf`, `0b2604f`, `3505246`) verified present in `git log`.
