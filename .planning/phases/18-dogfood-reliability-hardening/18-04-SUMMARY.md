---
phase: 18-dogfood-reliability-hardening
plan: 04
subsystem: cli
tags: [rust, cli, state-machine, safety-gate, retry-loop, mode]

# Dependency graph
requires:
  - phase: 18-dogfood-reliability-hardening
    provides: "18-01/18-02/18-03's established main.rs test conventions (ENV_MUTEX-guarded PATH neutralization via agent_free_git_only_path_dir, pre-seeded gate responses) reused verbatim rather than reinvented"
provides:
  - "mode.rs pure predicate transition_resets_consecutive_failures(from, to) — false only for (Code, Validate)"
  - "transition() consults the predicate instead of unconditionally zeroing consecutive_failures; infra_failures reset is untouched and still unconditional"
  - "handle_validate_outcome increments consecutive_failures via saturating_add, closing the overflow-wrap reintroduction risk now that the counter genuinely accumulates"
  - "MAX_INFRA_FAILURES' doc comment corrected (WR-11) to stop describing a shared reset condition with consecutive_failures"
affects: [18-05-layer0-validate-verdict-fix, mode, transition, handle_validate_outcome]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "pure Stage-pair predicate in mode.rs (transition_resets_consecutive_failures), sibling to should_gate/should_auto_loop, consulted by a main.rs caller rather than inlined as a local conditional — same devflow-core/devflow-cli split those two already use"
    - "RED-then-GREEN via real transition()/handle_validate_outcome calls (not a synthetic mock), with a pre-seeded gate response re-written on every loop iteration to survive prepare_loop_back_to_code's own Gates::cleanup, following transition_resets_infra_failures' established ENV_MUTEX + PATH-neutralization approach"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/mode.rs
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "transition_resets_consecutive_failures lives as a free function in mode.rs (not a Mode method) per the plan's locked Open-Question-1 resolution — the reset rule is Stage-pair-shaped, not mode-dependent"
  - "The reset predicate's asymmetry (Code->Validate skips, everything else fires) is intentionally NOT shared with infra_failures, whose unconditional reset stays exactly as 17-06 left it — the frozen regression test transition_resets_infra_failures passes byte-for-byte unchanged"
  - "Ceiling test forces state.stage = Stage::Code before every transition() call in its loop, rather than relying on incidental side effects of handle_validate_outcome's internal branching, so every loop iteration deterministically exercises the exact (Code, Validate) hop under test regardless of whether that cycle takes the ordinary loop-back path or the final gate-triggering path"

requirements-completed: [18d]

coverage:
  - id: D1
    description: "consecutive_failures accumulates across repeated Code-succeeds/Validate-fails cycles and reaches MAX_CONSECUTIVE_FAILURES, making the Auto-mode forced gate reachable for the loop it bounds"
    requirement: "18d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::consecutive_failures_reaches_ceiling_across_cycles"
        status: pass
    human_judgment: false
  - id: D2
    description: "transition() still resets infra_failures unconditionally — the pre-existing transition_resets_infra_failures test passes unedited, proving 18d did not widen or narrow the infra counter's reset scope"
    requirement: "18d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::transition_resets_infra_failures"
        status: pass
    human_judgment: false
  - id: D3
    description: "consecutive_failures reaching exactly MAX_CONSECUTIVE_FAILURES (3) forces the Validate gate; a value of 2 does not (boundary edge)"
    requirement: "18d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/mode.rs#tests::auto_does_not_gate_validate_until_failure_threshold"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::consecutive_failures_reaches_ceiling_across_cycles"
        status: pass
    human_judgment: false
  - id: D4
    description: "consecutive_failures increments with saturating_add and cannot wrap u32 in a long-running stuck loop (precision edge)"
    requirement: "18d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::consecutive_failures_increment_saturates"
        status: pass
    human_judgment: false
  - id: D5
    description: "A repeated Code->Validate transition leaves consecutive_failures unchanged rather than zeroing it (idempotency edge)"
    requirement: "18d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::repeated_code_to_validate_transition_is_idempotent_on_the_counter"
        status: pass
    human_judgment: false
  - id: D6
    description: "Two concurrently-active phases' consecutive_failures counters are independent — advancing one does not reset the other (concurrency edge)"
    requirement: "18d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::consecutive_failures_are_independent_across_phases"
        status: pass
    human_judgment: false

duration: ~35min
completed: 2026-07-21
status: complete
---

# Phase 18 Plan 04: Code↔Validate Safety-Gate Reachability Summary

**`transition()`'s `consecutive_failures` reset is now conditional on transition identity via a new `mode.rs` predicate (`transition_resets_consecutive_failures`), making `MAX_CONSECUTIVE_FAILURES` reachable for the Code↔Validate loop it bounds, while `infra_failures`' unconditional reset — proven by an unedited frozen regression test — stays exactly as 17-06 left it.**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-07-21T03:58:54Z
- **Completed:** 2026-07-21T04:15:46Z
- **Tasks:** 2
- **Files modified:** 2 (`crates/devflow-core/src/mode.rs`, `crates/devflow-cli/src/main.rs`; +279/-13 net across both commits)

## Accomplishments

- `transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool` — a pure, sibling-to-`should_gate`/`should_auto_loop` predicate in `mode.rs`: `false` only for `(Code, Validate)` (the mid-cycle hop that previously defeated the counter), `true` for every other transition. Doc comment records the asymmetry and explicitly why it does NOT extend to `infra_failures`.
- `MAX_INFRA_FAILURES`' doc comment corrected (closes 17-REVIEW.md WR-11): no longer describes a reset condition shared with `consecutive_failures`; now cross-references the new predicate for the counter whose reset is conditional.
- `transition()` now consults the predicate instead of unconditionally zeroing `consecutive_failures`; `state.infra_failures = 0;` is untouched, still unconditional, still the very next line — not reordered, not merged into a shared helper.
- `handle_validate_outcome` increments `consecutive_failures` via `saturating_add(1)` instead of `+=`, closing the overflow-wrap risk that only becomes real once the counter genuinely accumulates.
- RED-then-GREEN proven live (not just by inspection): `consecutive_failures_reaches_ceiling_across_cycles` failed with `left: 0, right: 3` against the unfixed `transition()`, and `consecutive_failures_increment_saturates` panicked with `attempt to add with overflow` against the unfixed `+=` — both pass after the fix.
- 6 new unit tests total: 2 exhaustive predicate tests in `mode.rs` (`consecutive_reset_skips_the_code_to_validate_hop`, `consecutive_reset_fires_on_every_other_transition` — the latter enumerates transitions explicitly rather than negating the former, so a future `Stage` variant can't silently slip through untested); 4 in `main.rs` covering the ceiling (+ infra-untouched assertion), saturation, idempotency, and cross-phase independence.
- `transition_resets_infra_failures` (the frozen regression guard) passes byte-for-byte unchanged.

## Task Commits

1. **Task 1: Add the pure reset predicate to mode.rs and correct the stale doc comment** - `37b74ac` (feat)
2. **Task 2: Make the reset conditional in transition() and prove the ceiling is reachable** - `3036927` (feat)

**Plan metadata:** (this commit, once created below)

## Files Created/Modified

- `crates/devflow-core/src/mode.rs` - `transition_resets_consecutive_failures` pure predicate placed after `MAX_INFRA_FAILURES`/before `Mode`; `MAX_INFRA_FAILURES` doc comment corrected; 2 new tests
- `crates/devflow-cli/src/main.rs` - `transition()`'s reset made conditional + doc comment rewritten; `handle_validate_outcome`'s increment switched to `saturating_add`; 4 new tests

## Decisions Made

- `transition_resets_consecutive_failures` is a free function, not a `Mode` method — matches the plan's locked Open Question 1 resolution: the reset rule is a property of the `(Stage, Stage)` pair, not the active `Mode`, so forcing it onto `Mode` would imply a dependence that doesn't exist.
- Did not add a `consecutive_failures_below_ceiling_does_not_gate` test in `main.rs` even though the plan's `must_haves.truths` names the boundary edge (2 vs. 3) — that exact edge is already proven by the pre-existing, unmodified `mode.rs` test `auto_does_not_gate_validate_until_failure_threshold` (`!should_gate(Validate, 2)`, `should_gate(Validate, MAX_CONSECUTIVE_FAILURES)`), and is additionally exercised end-to-end by `consecutive_failures_reaches_ceiling_across_cycles`'s final assertion. Adding a third, main.rs-local duplicate would be redundant coverage not named in the plan's `artifacts_this_phase_produces` table.
- `consecutive_failures_reaches_ceiling_across_cycles` forces `state.stage = Stage::Code` before every `transition()` call in its loop rather than trusting `handle_validate_outcome`'s internal branching to leave `state.stage` at `Code` — see Deviations for why the naive version (relying on `prepare_loop_back_to_code`'s side effect alone) breaks specifically on the gate-triggering final cycle.
- Per the plan's explicit instruction, `crates/devflow-core/src/state.rs`'s `consecutive_failures`/`infra_failures` doc comments (which still describe the OLD shared reset condition — "Reset to 0 on every successful stage transition, alongside `consecutive_failures`") were left untouched; that file is outside this plan's `files_modified`. **Follow-up needed:** a future small doc-only pass on `state.rs` lines ~29-43 should correct this staleness now that the two counters' reset conditions have diverged, matching `mode.rs`'s already-corrected `MAX_INFRA_FAILURES` doc comment.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test's pre-seeded gate response was deleted by an earlier loop-back cycle, hanging the test on the real 7-day gate timeout**
- **Found during:** Task 2 verification (`consecutive_failures_reaches_ceiling_across_cycles` hung indefinitely on first run)
- **Issue:** The plan's described test shape (loop `MAX_CONSECUTIVE_FAILURES` times calling `handle_validate_outcome` then `transition`) pre-seeds a single gate response file before the loop starts, intended to let the final, gate-triggering cycle's `run_gate` resolve immediately. But `prepare_loop_back_to_code` — which runs on every ordinary (non-final) loop-back cycle once `state.stage` is `Validate` — calls `Gates::cleanup(project_root, state.phase, state.stage)`, which deletes the response/gate/ack files for that stage. Since `state.stage` is `Validate` by the second loop iteration, that cycle's cleanup silently deletes the pre-seeded response before the third (ceiling-reaching) cycle's `run_gate` ever reads it, so `Gates::poll_response` blocks on the real (default 7-day) timeout with no response file present. Confirmed live: the test hung after printing `gate written: .devflow/gates/81-validate.json — awaiting response` and nothing further; killing the stuck process and re-running with `--nocapture` isolated the exact print-ordering that pinpointed the cleanup call as the cause.
- **Fix:** Re-write the response file at the top of every loop iteration (not once before the loop), so it always exists by the time any cycle's `run_gate` might read it, regardless of whether an earlier cycle's `Gates::cleanup` deleted a prior copy.
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** `cargo test -p devflow consecutive_failures_reaches_ceiling_across_cycles -- --test-threads=1` → `1 passed` in 0.01s (was hanging past 60s before the fix); confirmed with `--nocapture` that the loop-back/gate/abort print sequence now completes and returns.
- **Committed in:** `3036927` (Task 2 commit — the test was authored and fixed within the same commit, never landed in its broken form)

---

**Total deviations:** 1 auto-fixed (Rule 1 bug in test authorship, caught during the plan's own mandated verification step before any commit). No production-code deviations — `transition()` and `handle_validate_outcome` were implemented exactly per the plan's `<action>` block.
**Impact on plan:** No functional behavior changed beyond what the plan specified. The deviation is a test-authoring correctness fix required to make the plan's own RED-then-GREEN requirement actually completable rather than hang forever; it does not touch what is being asserted, only how the test's fixture (a pre-seeded gate response) is kept alive across the loop.

## Issues Encountered

Beyond the test-hang deviation above: the carry-forward correction from 18-01/18-02/18-03 (`cargo test -p devflow --lib` hard-errors on this binary-only crate) was reconfirmed — additionally, `--exact` combined with a bare test name (no `tests::` prefix) matches 0 tests in this crate's binary because the harness's exact-match compares against the fully-qualified `tests::<name>` path, not the bare name; using the bare name WITHOUT `--exact` (substring filter) is what actually selects the intended test, matching what 18-01/18-02/18-03 already established.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 18d (Code↔Validate safety-gate reachability) is complete. The Auto-mode forced gate at `MAX_CONSECUTIVE_FAILURES` is now reachable for the exact loop it was designed to bound.
- `cargo test --workspace` is green at 411 passed / 0 failed (up from 18-03's 405-test baseline: +6 — 2 in `devflow-core`'s `mode::tests`, 4 in `devflow-cli`'s `devflow` unittests binary). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both exit 0.
- 18-05 (18e, Layer 0/Validate verdict fix) depends on this plan per 18-RESEARCH.md's Pitfall 1 — 18e's disagreement/no-verdict handling needs a genuinely reachable ceiling to bound the loop it introduces at Validate; that ceiling is now real, not just correctly evaluating pass/fail while still running forever. No blockers.
- Follow-up (not blocking, noted above): `crates/devflow-core/src/state.rs`'s `consecutive_failures`/`infra_failures` doc comments still describe the pre-18d shared reset condition — a small doc-only correction pass would bring them in line with `mode.rs`'s already-corrected wording.

---
*Phase: 18-dogfood-reliability-hardening*
*Completed: 2026-07-21*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/mode.rs
- FOUND: crates/devflow-cli/src/main.rs
- FOUND: .planning/phases/18-dogfood-reliability-hardening/18-04-SUMMARY.md
- FOUND: 37b74ac (Task 1 commit)
- FOUND: 3036927 (Task 2 commit)
- FOUND: ae29b66 (summary commit)
