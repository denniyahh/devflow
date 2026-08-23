---
phase: 18-dogfood-reliability-hardening
plan: 07
subsystem: infra
tags: [rust, cli, gates, preflight, dogfood, reliability]

# Dependency graph
requires:
  - phase: 18-dogfood-reliability-hardening (18-06)
    provides: 420/420 green baseline; worktree-aware staleness fix landed, main.rs stable to edit
provides:
  - "Approving a preflight gate (GateAction::Advance) skips run_preflight entirely on the relaunch via a launch_stage/launch_stage_inner split — the agent launches exactly once, no second gate is written"
  - "GateAction::LoopBack still re-runs the check (state may genuinely have changed), now bounded by a persisted State.preflight_retries counter against mode::MAX_PREFLIGHT_RETRIES (3)"
  - "Reaching the ceiling emits preflight_retry_ceiling_reached and aborts with a logged event instead of polling a second 7-day gate timeout"
  - "preflight_retries resets to 0 on a passing preflight and on a human Advance, persisted (not just in-memory) in both cases"
  - "Module-scope AlwaysFailAdapter test fixture (hoisted from a test-function-local AlwaysRejectAdapter)"
affects: [future preflight/gate work, any future phase touching launch_stage or run_preflight]

tech-stack:
  added: []
  patterns:
    - "launch_stage / launch_stage_inner split: the post-preflight body is a separate function so a gate arm can skip the preflight guard for a single relaunch without introducing any persisted bypass flag"
    - "Ceiling checked BEFORE writing a new gate, not after — prevents the ceiling case itself from opening a gate nobody will answer"
    - "preflight_retries follows the existing consecutive_failures/infra_failures bounded-fault-counter convention on State, but with its own non-transition() reset discipline (preflight pass + human Advance, not stage transitions)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/mode.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/log_format_env.rs

key-decisions:
  - "D-18f implemented exactly as the binding operator decision specified: Advance = skip (launch_stage_inner), LoopBack = re-check (launch_stage), bounded regardless by a persisted State.preflight_retries / mode::MAX_PREFLIGHT_RETRIES=3 ceiling checked before any new gate is written"
  - "launch_stage_inner recomputes prompt/adapter/roots/program/args from state rather than accepting them as parameters (assumption_delta's stated second option) — keeps the function callable entirely on its own from run_preflight's Advance arm without widening run_preflight's signature to thread program/args through it; the shared worktree_writable_roots helper is not duplicated, only called twice"
  - "Deviation from the plan's literal Task 3 test setup: AlwaysFailAdapter (Stage::Plan + adapter-hook failure) cannot actually reproduce a failure that survives a relaunch, because launch_stage's internal recursion always re-resolves the REAL production adapter via agents::adapter_for(state.agent), discarding whatever adapter was passed into the outer run_preflight call — confirmed both by direct experiment (see below) and by the pre-existing run_preflight_advance_gate_launches_agent_exactly_once test's own comment ('the real Claude adapter's default (Ok) preflight passes every other check'). The three new regression tests instead use preflight_interactivity_check (Codex + Auto + Define + no CONTEXT.md on develop) as the deterministic, state-based failure trigger CONTEXT.md actually attributes the wedge to — it is a pure function of state, so it fails identically on every invocation regardless of which adapter object happens to be resolved. AlwaysFailAdapter is still passed as the adapter argument in all three tests (defense in depth), it's simply not what causes the failure to persist across a relaunch."
  - "run_preflight_loopback_bounds_recursion seeds state.preflight_retries at MAX_PREFLIGHT_RETRIES - 1 plus exactly one LoopBack response, rather than literally seeding N separate loop-back responses for N organic cycles — Gates::poll_response blocks synchronously in the calling thread, so nothing could write a second response file mid-recursion inside one synchronous call stack without a racy background writer thread. Starting one retry short of the ceiling still exercises the real recursive code path (a genuine second run_preflight call via launch_stage hits the ceiling on its own) without introducing timing-dependent flakiness in a phase whose whole point is eliminating flakes (19i, WR-03)."

requirements-completed: [18f]

coverage:
  - id: D1
    description: "State.preflight_retries: u32 persisted counter (serde-default), mode::MAX_PREFLIGHT_RETRIES = 3"
    requirement: "18f"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#preflight_retries_round_trips_through_serde"
        status: pass
  - id: D2
    description: "GateAction::Advance on a preflight gate skips run_preflight entirely via launch_stage_inner — agent launches exactly once, no second gate written, counter reset"
    requirement: "18f"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::run_preflight_advance_skips_recheck_on_idempotently_failing_check"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::run_preflight_advance_gate_launches_agent_exactly_once"
        status: pass
  - id: D3
    description: "GateAction::LoopBack still re-runs the check; the recursion is bounded by the persisted retry ceiling, which aborts with a logged event instead of polling a second 7-day gate"
    requirement: "18f"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::run_preflight_loopback_bounds_recursion"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::run_preflight_loopback_gate_launches_agent_exactly_once"
        status: pass
  - id: D4
    description: "preflight_retries resets to 0 on a passing preflight, persisted to disk (not just in-memory) — the wedge this counter bounds spans separate devflow invocations"
    requirement: "18f"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::preflight_retries_reset_on_pass"
        status: pass
  - id: D5
    description: "Fail-closed terminal Ship invariant and self-dogfood staleness hard block are not weakened — launch_stage_inner still runs enforce_build_staleness and every other post-preflight gate; every pre-existing run_preflight/17-08 regression test still passes"
    requirement: "18f"
    verification:
      - kind: unit
        ref: "cargo test --workspace (424 passed, 0 failed)"
        status: pass

# Metrics
duration: 25min
completed: 2026-07-21
status: complete
---

# Phase 18 Plan 07: Preflight Gate Re-Run Wedge Fix Summary

**`launch_stage`/`launch_stage_inner` split lets an approved preflight gate launch the agent once instead of re-running the same deterministic check into a 7-day wedge, bounded regardless by a persisted `State.preflight_retries` ceiling of 3.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-07-21T05:08:00Z (approx, first task commit)
- **Completed:** 2026-07-21T05:28:00Z
- **Tasks:** 3
- **Files modified:** 4 (`state.rs`, `mode.rs`, `main.rs`, `tests/log_format_env.rs`)

## Accomplishments

- `GateAction::Advance` on a preflight gate now calls `launch_stage_inner` directly (the post-preflight body of `launch_stage`), skipping the just-adjudicated check entirely — the documented "approve, then wedge for 7 days" defect (18f) is closed.
- `GateAction::LoopBack` still re-runs the check via the full `launch_stage` path (deliberately — the operator may have fixed the condition), now bounded by a persisted `State.preflight_retries` counter against `mode::MAX_PREFLIGHT_RETRIES = 3`, checked BEFORE any new gate is written so the ceiling case itself never opens an unanswerable gate.
- Reaching the ceiling emits a `preflight_retry_ceiling_reached` event and aborts (clearing state, matching the `[never-silent]` idiom) instead of polling a second 7-day gate timeout.
- `preflight_retries` resets to 0, persisted, on both a passing preflight and a human `Advance` — pinned by a test that reloads state from disk rather than checking the in-memory value, since the wedge this counter bounds spans separate `devflow` invocations (a monitor restart reloads from disk).
- `AlwaysFailAdapter` (module-scope, no interior mutability) hoisted from a test-function-local `AlwaysRejectAdapter`.
- RED-then-GREEN proven live: manually reverted the `Advance` arm to call `launch_stage` instead of `launch_stage_inner` and reproduced the exact documented wedge (two gates written, then a bounded `"gate for stage define timed out awaiting a response"` error), then restored the fix and confirmed green.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add the persisted retry counter, its ceiling, and an unconditionally-failing adapter** - `a397d46` (feat)
2. **Task 2: Split launch_stage so Advance skips the adjudicated check, and bound the recursion** - `950a358` (feat)
3. **Task 3: Prove the wedge is gone and the recursion is bounded** - `1ca79dd` (test)

**Plan metadata:** (this commit) `docs(18-07): complete plan`

## Files Created/Modified

- `crates/devflow-core/src/state.rs` — `State.preflight_retries: u32` (`#[serde(default)]`), initialized in `State::new`, round-trip test
- `crates/devflow-core/src/mode.rs` — `pub const MAX_PREFLIGHT_RETRIES: u32 = 3`
- `crates/devflow-cli/src/main.rs` — `launch_stage_inner` extracted; `launch_stage` now resolves + guards on `run_preflight` then delegates; `run_preflight` restructured with the ceiling check, retry increment/reset, and the Advance/LoopBack asymmetry; `AlwaysFailAdapter` hoisted to module scope; three new regression tests
- `crates/devflow-cli/tests/log_format_env.rs` — Rule 3 fix: a `State` struct literal needed the new `preflight_retries` field to keep compiling

## Decisions Made

See `key-decisions` in frontmatter. Summary: implemented D-18f exactly as specified (Advance=skip, LoopBack=re-check, bounded regardless); chose to recompute `prompt`/`adapter`/`roots`/`program`/`args` in `launch_stage_inner` rather than threading them through `run_preflight`'s signature; deviated from the plan's literal Task 3 test setup because `AlwaysFailAdapter` structurally cannot survive a relaunch (recursion always re-resolves the real production adapter), using the generic `preflight_interactivity_check` as the actual deterministic trigger instead; used a single pre-positioned retry count rather than literal multi-cycle response seeding to avoid a racy background-writer test.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `log_format_env.rs`'s `State` struct literal needed the new field**
- **Found during:** Task 1 (adding `State.preflight_retries`)
- **Issue:** `crates/devflow-cli/tests/log_format_env.rs` constructs `State` via a struct literal (not `State::new`), so adding a new non-defaulted-in-literal field broke compilation (`E0063: missing field 'preflight_retries'`).
- **Fix:** Added `preflight_retries: 0,` to the literal.
- **Files modified:** `crates/devflow-cli/tests/log_format_env.rs`
- **Verification:** `cargo test --workspace` green.
- **Committed in:** `a397d46` (Task 1 commit)

**2. [Rule 1 - Test design bug] `AlwaysFailAdapter` cannot reproduce a failure that survives a relaunch — corrected the reproduction mechanism for Task 3's three new tests**
- **Found during:** Task 3 (writing the wedge-reproduction tests)
- **Issue:** The plan's Task 3 action specified using `Stage::Plan` + `AlwaysFailAdapter` (adapter-hook failure) as the failure source for all three new tests, reasoning that this isolates the adapter hook as "provably the only failure source." However, `launch_stage`'s internal recursion always resolves the adapter via `agents::adapter_for(state.agent)` — a hardcoded match over `AgentKind` — which discards whatever adapter reference was passed into the OUTER `run_preflight` call. This is not new/introduced by this plan; it's a structural property already implicitly documented by the pre-existing `run_preflight_advance_gate_launches_agent_exactly_once` test's own comment ("the real Claude adapter's default (Ok) preflight passes every other check"). Consequence: with `Stage::Plan` + `AlwaysFailAdapter`, the recursive relaunch's real Claude/Codex/OpenCode adapter has a default `Ok` preflight and generic checks pass at `Plan`, so the retry ALWAYS succeeds trivially, regardless of whether the 18f fix exists — making the literal test setup unable to distinguish fixed from unfixed code (a tautology, not a regression guard).
- **Fix:** Used `Stage::Define` + `AgentKind::Codex` + `Mode::Auto` + no `.planning/phases/<N>-*/…-CONTEXT.md` on `develop` — this deterministically fails `preflight_interactivity_check`, a pure function of `state` that CONTEXT.md itself names as one of the two production checks the wedge is actually about. Since it depends only on state (not on which adapter object happens to be resolved), it fails IDENTICALLY on every invocation, including the recursive relaunch — genuinely reproducing the documented wedge. Verified empirically: with the plan's literal `Stage::Plan`/`AlwaysFailAdapter` setup, temporarily reverting the fix produced NO observable difference (both fixed and unfixed code returned `Ok(false)` with one clean launch); with the corrected `Stage::Define`/interactivity-check setup, reverting the fix reproduced the exact documented wedge (two gates written, then a bounded gate-timeout `Err`). `AlwaysFailAdapter` is still passed as the `adapter` argument to all three tests (harmless defense-in-depth — it would also fail were it ever reached via `.and_then`'s short-circuit), keeping the fixture exercised as the plan intended, but it is not what causes the failure to persist across the relaunch.
- **Files modified:** `crates/devflow-cli/src/main.rs` (test module only — no production code affected by this deviation)
- **Verification:** Manually reverted the `Advance` arm (`launch_stage_inner` → `launch_stage`) and confirmed `run_preflight_advance_skips_recheck_on_idempotently_failing_check` fails with `Err(Message("gate for stage define timed out awaiting a response"))` after two gates are written; restored the fix and confirmed all three new tests pass in well under 30s each (0.00s–0.04s).
- **Committed in:** `1ca79dd` (Task 3 commit)

**3. [Rule 1 - Test design bug] `run_preflight_loopback_bounds_recursion` seeds one retry-short-of-ceiling instead of N literal loop-back responses**
- **Found during:** Task 3
- **Issue:** The plan's action text says to "seed loop-back responses" (plural) and "let the recursion run" through multiple organic cycles to reach the ceiling. `Gates::poll_response` blocks synchronously in the calling thread with exponential backoff; since each cycle's `Gates::cleanup` deletes the single stage-scoped response file before the next cycle's gate is even written, nothing could seed a fresh response file mid-recursion inside one synchronous call stack — a literal multi-cycle test would require a racy background thread rewriting the response file between polls.
- **Fix:** Pre-set `state.preflight_retries = mode::MAX_PREFLIGHT_RETRIES - 1` and seed exactly ONE loop-back response. The single seeded response resolves the first (outer) cycle via a genuine `run_gate`→`poll_response` round-trip; the recursive call this triggers (through the real `launch_stage`) hits the ceiling on its own, since the SAME deterministic interactivity-check failure recurs and the counter is now at the ceiling. This exercises the real ceiling-check code path via genuine recursion, deterministically and without timing dependence — appropriate for a phase whose stated purpose is eliminating exactly this class of flake (19i, WR-03).
- **Files modified:** `crates/devflow-cli/src/main.rs` (test module only)
- **Verification:** Test passes in 0.03s; asserts `Ok(false)`, cleared state (`workflow::load_state` errors), and a `preflight_retry_ceiling_reached`/`workflow_aborted` terminal event.
- **Committed in:** `1ca79dd` (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking compile fix, 2 test-design corrections preserving the plan's intent with a working mechanism)
**Impact on plan:** All three deviations were necessary for correctness — #1 was a genuine compile blocker; #2 and #3 correct a plan-specified test design that (as verified empirically) could not actually distinguish fixed from unfixed code, replacing it with a mechanism proven (via manual RED/GREEN toggling) to genuinely reproduce and then close the documented wedge. No scope creep — no production code beyond the plan's stated `state.rs`/`mode.rs`/`main.rs` was touched.

## Issues Encountered

None beyond the deviations documented above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 18 (Dogfood Reliability Hardening) is now complete — this was the final plan (7/7: 18a–18g all closed). `cargo test --workspace` is green at 424/424 (up from 420 after 18-06), `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both clean. The `launch_stage`/`launch_stage_inner` split and the `preflight_retries` counter are new surface any future plan touching `run_preflight`, `launch_stage`, or preflight gating must be aware of — in particular, any NEW caller of `launch_stage_inner` directly (bypassing `run_preflight`) would need the same T-18-28 scoping discipline this plan documents (a plain function call, no persisted bypass state).

---
*Phase: 18-dogfood-reliability-hardening*
*Completed: 2026-07-21*

## Self-Check: PASSED

All modified/created files confirmed present on disk (`state.rs`, `mode.rs`, `main.rs`, `tests/log_format_env.rs`, this SUMMARY.md). All three task commit hashes (`a397d46`, `950a358`, `1ca79dd`) confirmed present in `git log`.
