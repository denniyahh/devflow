---
phase: 23-end-to-end-dogfood
plan: 09
subsystem: pipeline
tags: [gates, ship, cli-flag, unattended, state-machine, rust]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood
    provides: 23-07's Sequentagent removal and D-12's v2.0.0 breaking-change scoping (23-09 does not depend on 23-07's code, only shares the wave-ordering context)
provides:
  - "devflow start --yes-ship — a per-run, non-persistable pre-authorization for the Ship gate"
  - "State.yes_ship: bool (#[serde(default)]) — persisted across the CLI process boundary to the detached monitor's advance process"
  - "run_gate_with_timeout(..., auto_response: Option<&GateResponse>) — the one generalized injection point, with exactly one call site (handle_ship_outcome) ever passing Some"
  - "A named regression test proving the reopened finalization-retry gate can never inherit yes_ship's auto-approval"
  - "A config-absence test proving devflow.toml cannot set the pre-authorization"
affects: [23-10, 23-11]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Caller-supplied auto-response threaded as an Option parameter, never read from state inside the generalized function — the same 'decision belongs to the caller' shape used elsewhere in this pipeline to keep a retry path from inheriting an approval meant for a different call site"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/tests/log_format_env.rs

key-decisions:
  - "D-04/D-05/D-06 implemented as designed: --yes-ship is a per-invocation CLI flag only, persisted to State (not config), and auto-answers the Ship gate through the normal write_gate → respond → poll_response protocol rather than bypassing it."
  - "The auto_response parameter is threaded through run_gate_with_timeout as a caller-supplied Option, with exactly one call site (handle_ship_outcome) ever passing Some — trap 1 (auto-approving the reopened finalization-retry gate) is prevented by construction, not a conditional, and documented in `//`/`///` comments at both ends per the cross-AI review's MEDIUM finding."
  - "D-07's accepted risk is NOT mitigated by this plan — --yes-ship is not refused on the self-dogfood workspace, and plan 23-11's acceptance run will therefore perform a real unattended merge, version bump, and changelog commit on this repository. The mitigations (low-stakes target phase, verified recovery point) live in plan 23-10's checkpoint, not here."

patterns-established:
  - "Pattern: a pre-authorization that must never leak into a different gate call site is threaded as a per-call `Option<&T>` parameter with source comments at both the call site and the parameter declaration naming the specific regression test that would catch a future refactor collapsing the two paths."

requirements-completed: [yes-ship]

coverage:
  - id: D1
    description: "devflow start --yes-ship pre-authorizes the Ship gate; the gate still fires, is still answered through the normal protocol, and the ledger records an explicit --yes-ship attribution — an unattended Define-through-Ship run is no longer impossible by construction."
    requirement: "yes-ship"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#handle_ship_outcome_without_yes_ship_writes_gate_but_no_response"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#yes_ship_round_trips_through_serde"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#yes_ship_absent_from_json_defaults_to_false"
        status: pass
      - kind: manual_procedural
        ref: "cargo run -q -p devflow -- start --help | grep yes-ship"
        status: pass
    human_judgment: false
  - id: D2
    description: "The finalization-retry gate (reopened when a terminal hook fails after Ship's Merge succeeds) is never auto-approved even when yes_ship is set, and no devflow.toml key or environment variable can ever set the pre-authorization."
    requirement: "yes-ship"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#finalization_retry_gate_never_auto_approves_even_with_yes_ship_set"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#config_file_with_yes_ship_key_loads_but_never_sets_the_flag"
        status: pass
    human_judgment: false

# Metrics
duration: 25min
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 09: Ship-Gate Pre-Authorization (`--yes-ship`) Summary

**A per-run `devflow start --yes-ship` flag that persists to `State`, auto-answers exactly the routine Ship gate through the normal write/respond/poll protocol, and is provably unable to auto-approve the reopened finalization-retry gate or be set via config/env.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2/2
- **Files modified:** 7

## Accomplishments
- `State.yes_ship: bool` (`#[serde(default)]`) added and threaded from a new `devflow start --yes-ship` CLI flag through `commands::start` into the persisted state before the first `save_state` — surviving into the detached monitor's later `advance` process (D-04).
- `run_gate_with_timeout` gained a caller-supplied `auto_response: Option<&GateResponse>` parameter, written via `Gates::respond` between the existing `Gates::write_gate` and `Gates::poll_response` calls (the only correct window — writing earlier hits `NoOpenGate`, writing later is never observed).
- `handle_ship_outcome` is the **only** call site in the crate that ever passes `Some` — `run_gate`'s two other callers and `finish_workflow_with_gate_timeout`'s reopened retry-gate call both hardcode `None`, with `//`/`///` comments at both ends explaining why (cross-AI review MEDIUM finding, incorporated per plan).
- Named negative regression test `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` proves the reopened gate gets a request but no response, with `Merge` verified to have succeeded first (feature branch identical to `develop`) and `VersionBump` failed deterministically by pre-creating the exact tag it computes.
- `config_file_with_yes_ship_key_loads_but_never_sets_the_flag` proves a `devflow.toml` containing a `yes_ship` key loads without erroring yet never reaches a fresh `State` — `DevflowConfig` has no field of that name (D-05).
- D-07's accepted risk (no self-dogfood refusal guard) is unchanged and explicitly not mitigated here — recorded below, not implied to be handled.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end "unattended Ship"** — `5ed1f8f` (feat) — `yes_ship` field, CLI flag, `run_gate_with_timeout` parameter threading, `handle_ship_outcome` wiring, and the positive/negative behavior tests for the routine gate.
2. **Task 2: The two negative guarantees** — `a2810fe` (test) — the named finalization-retry-gate regression test and the config-absence test. No new production code; this task proves guarantees Task 1's implementation already holds.

**Plan metadata:** (this commit, docs) — completes this plan.

## Files Created/Modified
- `crates/devflow-core/src/state.rs` — new `yes_ship: bool` field (`#[serde(default)]`), `State::new` initializer, serde round-trip + absent-defaults-false tests.
- `crates/devflow-cli/src/main.rs` — new `--yes-ship` flag on `Command::Start`, threaded through the dispatch arm into `start()`.
- `crates/devflow-cli/src/commands.rs` — `start()` gains a `yes_ship: bool` parameter, sets `state.yes_ship` before the first `save_state`.
- `crates/devflow-cli/src/pipeline_gate.rs` — `run_gate_with_timeout` gains `auto_response: Option<&GateResponse>`, writes it via `Gates::respond` between `write_gate` and `poll_response`; `run_gate` and the reopened retry-gate call both pass `None`; new negative regression test.
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `handle_ship_outcome` computes the auto-response from `state.yes_ship` and calls `run_gate_with_timeout` directly (replacing the `run_gate` call); new positive/negative behavior tests and the config-absence test.
- `crates/devflow-cli/src/parallel.rs` — **not in the plan's declared `files_modified`** (deviation, see below) — updated the one other call site of `start()` to compile against the new 9-argument signature.
- `crates/devflow-cli/tests/log_format_env.rs` — **not in the plan's declared `files_modified`** (deviation, see below) — a literal `State { .. }` struct-literal test fixture needed the new field added.

## Decisions Made
- The auto-response injection point sits after the `notify_fired` event emission and before `Gates::poll_response`, per the plan's explicit ordering instruction — so the event stream (`gate_fired` → `notify_fired` → `gate_resolved`) still reads as a real gate that was really answered, not a bypass.
- `GateError::AlreadyResponded` from the auto-response write is treated as benign (first-writer-wins) — a human or 23b's stale-gate sweep may have answered first.
- `devflow parallel` (an unrelated CLI command that also calls `commands::start`) was given `false` for the new `yes_ship` parameter rather than a new flag of its own — D-05 requires the pre-authorization to be typed per invocation on `devflow start`, and `parallel` has no such flag; `false` preserves its existing gated-Ship behavior exactly.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed compile errors in files outside the plan's declared `files_modified`**
- **Found during:** Task 1, after adding `State.yes_ship` and changing `commands::start`'s signature
- **Issue:** Two files not listed in the plan's `files_modified` failed to compile: `crates/devflow-cli/src/parallel.rs` (a second call site of `start()`, in the `devflow parallel` command, missing the new 9th argument) and `crates/devflow-cli/tests/log_format_env.rs` (a literal `State { .. }` struct construction missing the new `yes_ship` field). Both are direct, mechanical consequences of Task 1's own signature/struct changes — not unrelated work.
- **Fix:** `parallel.rs`: passed `false` for the new `yes_ship` parameter with a comment explaining why (no `--yes-ship` flag exists on `devflow parallel`, and D-05 requires the pre-authorization to be typed per invocation). `log_format_env.rs`: added `yes_ship: false,` to the struct literal.
- **Files modified:** `crates/devflow-cli/src/parallel.rs`, `crates/devflow-cli/tests/log_format_env.rs`
- **Verification:** `cargo build --workspace` and `cargo test --workspace` both pass; `pairs_*`/`retry_after_from_reason_*` tests in `parallel.rs` and `help_output_matches_committed_snapshot`-adjacent `log_format_env.rs` tests unaffected.
- **Committed in:** `5ed1f8f` (Task 1 commit)

**2. [Rule 3 - Blocking] `--features test-support` needed for `-p devflow-core` tests run in isolation**
- **Found during:** Task 1, running the plan's literal verify command `cargo test -p devflow-core state::`
- **Issue:** This command fails to compile pre-existing integration tests (`tests/monitor_e2e.rs`, `tests/devflow_dir_gitignore.rs`, both untouched by this plan) with `cannot find test_support in devflow_core`. Root cause: `devflow-core`'s `test-support` feature is only enabled via `devflow-cli`'s dev-dependency declaration (`devflow-core = { workspace = true, features = ["test-support"] }`), which cargo's feature unification only picks up when the whole workspace (or at least `devflow-cli`) is part of the requested build graph — `cargo test -p devflow-core` alone never triggers that unification. This is a pre-existing condition unrelated to any change in this plan (confirmed: neither modified test file was touched, and the gap exists purely from how the two crates' `Cargo.toml`s declare the feature).
- **Fix:** Not a source fix — ran the equivalent command with `--features test-support` explicitly (`cargo test -p devflow-core --features test-support state::`) for local verification, and separately confirmed the plan's actual gating command, `cargo test --workspace`, passes cleanly with 0 failures (feature unification works correctly there).
- **Files modified:** None (verification-only workaround).
- **Verification:** `cargo test -p devflow-core --features test-support state::` → 15 passed, 0 failed. `cargo test --workspace` → all targets `test result: ok`, 0 failed.
- **Committed in:** N/A (no source change)

---

**Total deviations:** 2 auto-fixed (1 blocking/compile, 1 blocking/environment-verification workaround)
**Impact on plan:** Both were necessary to reach a compiling, fully-tested state. No scope creep — no behavior outside the plan's stated objective was added.

## TDD Gate Compliance

Task 2 (`tdd="true"`) is documented in the plan as landing regression tests against behavior Task 1 already implements — its `<action>` block describes only test additions ("Land the load-bearing negative test... Land D-05's guarantee as two assertions..."), with no new production code. Accordingly Task 2 produced a single `test(23-09): ...` commit rather than a RED-commit/GREEN-commit pair; there is no separate "implementation" commit because there is no new implementation. The negative test's fixture DID go through a genuine RED→GREEN cycle during authoring (an off-by-one in the tag-count computation initially let `VersionBump` succeed instead of collide, so the test failed for the wrong reason — `gate_path.exists()` assertion — until the fixture was corrected), but that cycle was on test-fixture logic, not production code, so it is not reflected as a separate commit. The plan's frontmatter `type` is `execute`, not `tdd`, so the plan-level RED/GREEN gate-sequence enforcement in `<tdd_execution>` does not apply to this plan.

## Issues Encountered
- The negative regression test's version-collision fixture initially computed the "expected tag" using `version::compute_version()` **before** pre-creating the blocking tag, then named the tag after that value. Since `compute_version`'s MINOR component is a live git tag count, creating that tag added 1 to the count, so `VersionBump`'s own subsequent call to `compute_version()` computed a **different**, one-higher version than the one just pre-created — no collision, `VersionBump` silently succeeded, and the test failed on the wrong assertion (`gate_path.exists()`, since the workflow shipped instead of reopening). Fixed by computing the fixed point directly: `minor = tags_before + 1` (accounting for the tag about to be created) and `patch = 0` (the new tag sits at HEAD, and `Merge` is a no-op in this fixture since the feature branch is identical to `develop`), rather than computing `compute_version()` twice around the tag creation.

## Next Phase Readiness
- The Ship gate can now be pre-authorized for an unattended run via `devflow start --phase N --agent claude --mode auto --yes-ship`, unblocking plan 23-10's checkpoint and the acceptance run.
- D-07's accepted risk is unresolved by design — plan 23-10 must encode the two operator-specified mitigations (low-stakes target phase, verified recovery point) before the acceptance run in plan 23-11 actually exercises `--yes-ship` against this repository's real `develop` branch.
- `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` is the named regression test cited from `run_gate_with_timeout`'s parameter doc comment — any future refactor that reads `state.yes_ship` inside that function instead of receiving it as a parameter will break this test loudly.

## Self-Check: PASSED

- FOUND: `.planning/phases/23-end-to-end-dogfood/23-09-SUMMARY.md`
- FOUND: commit `5ed1f8f` (Task 1)
- FOUND: commit `a2810fe` (Task 2)

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*
