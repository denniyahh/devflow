---
phase: 25-end-to-end-dogfood-blockers
plan: 02
subsystem: infra
tags: [process-management, libc, procfs, devflow-core, agent-lifecycle]

requires: []
provides:
  - "terminate_and_verify: bounded TERM -> KILL escalation with verified death"
  - "discover_stray_devflow_processes + StrayProcess/StrayLayer: registry-independent two-layer process census"
  - "looks_like_devflow_process deprecated, retained, its test de-raced"
affects: [25-07]

tech-stack:
  added: []
  patterns:
    - "Verified-fact liveness: never report a signal outcome without re-checking agent_running afterward"
    - "(pid, starttime) identity pair for any process reference that will be acted on later, never a bare pid or a cmdline inference"
    - "Structural, positional argv matchers (never a scan of all argv elements, never a prefix match)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent.rs

key-decisions:
  - "SIGKILL escalation re-polls agent_running for up to 1s after the kill(2) call rather than checking once immediately -- SIGKILL is uncatchable but not synchronous, and a single immediate check raced the kernel and produced a false FAILED result during RED/GREEN development"
  - "The wrapper-script marker and binary/subcommand literals are copied verbatim into agent.rs as private consts (not exported from monitor.rs) because this plan's files_modified scope is agent.rs only"
  - "The Layer-2 rejection test uses CommandExt::arg0 to present argv[0]=='devflow' without needing a real devflow binary, keeping the fixture self-contained and side-effect-free"
  - "The retargeted looks_like_devflow_process_is_false_for_a_non_devflow_process test asserts is_same_process directly rather than spawning a child, eliminating the execve race by construction (999.47)"

requirements-completed: ["25d", "25e"]

coverage:
  - id: D1
    description: "terminate_and_verify -- bounded TERM->KILL escalation that proves death rather than assuming it (D-17)"
    requirement: "25d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::terminate_and_verify_returns_true_immediately_for_a_dead_pid"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::terminate_and_verify_clears_a_normal_child_before_the_wait_elapses"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::terminate_and_verify_rejects_pid_zero_and_out_of_range_without_signalling"
        status: pass
    human_judgment: false
  - id: D2
    description: "discover_stray_devflow_processes -- registry-independent two-layer census (both process layers discoverable with no registry/lock/state file, uid-scoped, no parentage filter)"
    requirement: "25d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::discover_stray_devflow_processes_rejects_devflow_named_argv0_with_wrong_argv1"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::discover_stray_devflow_processes_excludes_an_unrelated_process"
        status: pass
    human_judgment: false
  - id: D3
    description: "looks_like_devflow_process deprecated (retained, body unchanged) and its unit test retargeted to the (pid, starttime) identity guard, eliminating the execve race (25e / D-13)"
    requirement: "25e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process"
        status: pass
    human_judgment: true
    rationale: "25e's flake closure cannot be confirmed by one local green run (999.47's mechanism only reproduces reliably inside the pinned CI container per 25-VALIDATION.md's '25e exception'); a human/CI-on-branch observation across several pushes is required before the flake can be declared closed."

duration: 55min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 02: Process Termination + Stray-Process Discovery Summary

**Bounded TERM->KILL escalation with verified death, registry-independent two-layer process census, and deprecation of the unsound `looks_like_devflow_process` predicate -- all in `crates/devflow-core/src/agent.rs`, zero new dependencies.**

## Performance

- **Duration:** ~55 min
- **Completed:** 2026-07-28T00:33:46Z
- **Tasks:** 3 (all `type="auto" tdd="true"` except Task 3)
- **Files modified:** 1 (`crates/devflow-core/src/agent.rs`)

## Accomplishments

- `terminate_and_verify(pid, wait, poll)`: sends one `SIGTERM`, polls `agent_running` until `wait` elapses, escalates to `SIGKILL` on timeout, re-polls after the kill (SIGKILL is uncatchable but not synchronous), and returns a verified liveness fact -- never an assumption. Clears a TERM-ignoring child (`trap '' TERM; sleep 30`) that a bare `SIGTERM` cannot touch.
- `discover_stray_devflow_processes() -> Vec<StrayProcess>`: a pure, read-only, uid-scoped census of `/proc` matching two narrow structural shapes (monitor wrapper: `sh -c <script containing the literal marker>`; advance child: `argv[0]` basename == binary name AND `argv[1]` == `advance`). No parentage filter (orphans reparent to the user's service manager, not init). Records `process_start_time` per candidate for later `is_same_process` re-confirmation, closing the discovery-to-signal TOCTOU window.
- `looks_like_devflow_process` marked `#[deprecated]` (body and export unchanged, per D-13), and its flaky unit test retargeted to assert the `(pid, starttime)` identity guard directly -- no `spawn()`, no `execve` race.

## Task Commits

1. **Task 1: terminate_and_verify** - `d69de84` (feat)
2. **Task 2: Registry-independent two-layer stray discovery** - `ef6322d` (feat)
3. **Task 3: Deprecate the unsound predicate and de-race its unit test** - `cfcbdf8` (fix)

**Plan metadata:** (this commit, docs)

_No TDD test/feat/refactor split commits -- RED and GREEN were combined per task since each task's action and its tests were authored together and verified before the single commit, matching this repo's established per-task commit granularity rather than a three-commit-per-task cadence._

## Files Created/Modified

- `crates/devflow-core/src/agent.rs` - Added `terminate_and_verify` (+2 constants), `StrayProcess`/`StrayLayer`/`discover_stray_devflow_processes` (+3 private helpers/consts), deprecated `looks_like_devflow_process` and retargeted its unit test. 8 new tests total; pre-change `agent::` suite was 10 passed, post-change is 18 passed, 0 failed.

## Decisions Made

- **Post-`SIGKILL` re-poll, not a single check.** The RED run for the TERM-ignoring-child test failed intermittently with a single immediate `agent_running` check right after `libc::kill(pid, SIGKILL)` -- SIGKILL is uncatchable but its delivery is not synchronous with the syscall returning. Fixed by polling again (bounded to 1s) after escalation, same as the pre-escalation wait. Documented inline as the reason this isn't a single check.
- **Wrapper-script marker copied verbatim, not exported.** The plan's `files_modified` scope is `agent.rs` only, so `MONITOR_WRAPPER_MARKER = "trap cleanup TERM INT"` is a private const copied byte-for-byte from `monitor.rs`'s literal script text rather than a shared export from `monitor.rs` (which this plan does not touch). Recorded here for future readers to confirm the two have not drifted (see "Verbatim marker" below).
- **Layer-2 rejection fixture uses `CommandExt::arg0`.** Rather than requiring a real `devflow` binary on `PATH`, the test spawns `sleep` with `argv[0]` overridden to `"devflow"` via `std::os::unix::process::CommandExt::arg0`, giving a self-contained, side-effect-free fixture for "basename matches but argv[1] doesn't."
- **Retargeted test asserts `is_same_process` directly.** Per D-13/25e, the flaky test now compares the current process's own recorded start time (must match) against a deliberately perturbed value (must not match) -- no child spawn, so no `execve` race is possible by construction.

## Verbatim wrapper-script marker (for drift confirmation)

Copied from `crates/devflow-core/src/monitor.rs`'s `script` format! literal (the trap-installation line):

```
trap cleanup TERM INT
```

This exact string is `agent.rs`'s `MONITOR_WRAPPER_MARKER` constant. If `monitor.rs`'s script text ever changes, this constant (and this note) must be updated in the same commit -- there is no shared export enforcing this mechanically, since `monitor.rs` is out of this plan's scope.

## Pre-change / post-change `agent::` test counts

- **Pre-change:** `cargo test --package devflow-core --lib agent::` -> `10 passed; 0 failed`
- **Post-change:** `cargo test --package devflow-core --lib agent::` -> `18 passed; 0 failed` (8 new: 4 for `terminate_and_verify`, 4 for `discover_stray_devflow_processes`; the 3 `looks_like_devflow_process` tests are retained at the same count, one retargeted in place)
- Full `devflow-core` lib suite: `376 passed; 0 failed` (all packages, not just `agent::`)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `terminate_and_verify`'s post-`SIGKILL` check raced the kernel**
- **Found during:** Task 1, RED/GREEN verification of `terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child`
- **Issue:** The initial implementation issued `libc::kill(pid, SIGKILL)` and immediately returned `!agent_running(pid)` once, with no re-poll. This intermittently reported a just-killed process as still alive, because `SIGKILL` delivery is asynchronous relative to the syscall returning.
- **Fix:** Added a second bounded poll loop (1s ceiling, same `poll` granularity) after the `SIGKILL` call, identical in shape to the pre-escalation `SIGTERM` wait.
- **Files modified:** `crates/devflow-core/src/agent.rs`
- **Verification:** `terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child` passes reliably across repeated runs; the acceptance-criteria source assertion (`agent_running` appears >= 2 times in the function body) still holds.
- **Committed in:** `d69de84` (Task 1 commit -- fixed before commit, not a separate commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Necessary for correctness of the escalation's core contract ("verified fact, never an assumption"). No scope creep.

## Issues Encountered

- **Package-scoped clippy verify command has a pre-existing, unrelated feature-gating gap.** The plan's own `<verify>` for Task 3 runs `cargo clippy --package devflow-core --all-targets -- -D warnings`, which fails with `E0433: cannot find test_support in devflow_core` -- this is unrelated to any change in this plan (reproduces identically after Task 1 alone, before Task 2/3 existed). Root cause: `devflow-core`'s `tests/monitor_e2e.rs` and `tests/devflow_dir_gitignore.rs` reference `devflow_core::test_support`, which is gated behind `#[cfg(any(test, feature = "test-support"))]` and is not enabled by a package-scoped `--all-targets` invocation on its own crate (only `test`, not the crate's own integration-test targets, satisfies that `cfg`). Verified clean via `cargo clippy --package devflow-core --all-targets --features test-support -- -D warnings` (0 warnings), and via the canonical project-wide gate `cargo clippy --workspace --all-targets -- -D warnings` for feature unification -- **except** for the known cross-crate gap below.
- **Known, expected cross-crate gap: `cargo clippy --workspace --all-targets -- -D warnings` currently fails.** `crates/devflow-cli/src/commands.rs:3308` calls the now-deprecated `agent::looks_like_devflow_process` from its own 999.47/DEN-72 flaky-test instrumentation, with no `#[allow(deprecated)]`. This plan's scope is `agent.rs` only ("Do not touch `crates/devflow-cli` -- its instrumentation call site and its own flaky test are plan 25-07's scope," per the plan's Task 3 action text), so this call site is deliberately left as-is. **This means the full-workspace clippy gate will not pass until plan 25-07 lands** (it owns that call site and its test per the plan's own design split). This is a known, planned, cross-plan sequencing gap -- not a defect in this plan -- but it should be visible to whoever runs a workspace-wide gate before 25-07 merges.
- **25e's flake closure is NOT established by this plan's local run.** Per 25-VALIDATION.md's "25e exception," 999.47's flake mechanism only reproduces reliably inside the pinned CI container; local probes never observed it (0/3000 in the project's own prior investigation). The retargeted test removing the `execve` race by construction is a strong structural fix, but confirmation that the flake is actually closed requires CI-on-branch stability across several pushes -- not claimed here as already proven.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `terminate_and_verify` and `discover_stray_devflow_processes`/`StrayProcess`/`StrayLayer` are ready for plan 25-07 to surface on the CLI (per this plan's objective: "This plan is the library half. Plan 25-07 surfaces both primitives on the CLI and retargets the second flaky test").
- **Blocker for a clean workspace-wide gate:** plan 25-07 must add `#[allow(deprecated)]` (or migrate off the deprecated call) at `crates/devflow-cli/src/commands.rs:3308` before `cargo clippy --workspace --all-targets -- -D warnings` will pass again. Until then, `devflow-core` alone is clean but the workspace is not.
- 25e (the flaky-test closure requirement) still needs CI-on-branch confirmation across several pushes before it can be declared resolved -- this plan only removes the `execve` race by construction in `devflow-core`'s own test; the CLI's own DEN-72 flaky test (`commands.rs`, the `stop()` guard test) is untouched and is 25-07's stated scope.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*
