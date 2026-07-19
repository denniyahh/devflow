---
phase: 17-pipeline-dogfood-followup
plan: 09
subsystem: cli
tags: [rust, gate-protocol, ship, concurrency, test-flakiness, tdd]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup (17-08)
    provides: nyquist_compliant validation baseline (GAP-1 resolved), so GAP-2 was the only open item
provides:
  - "concurrent_ship_advances_finish_both_phases_independently can never wedge cargo test --workspace — bounded to a deterministic pass or a bounded (~2s) documented-failure outcome under all timing conditions"
  - "17-VALIDATION.md GAP-2 resolved (test-level) with re-measured 25-run evidence, replacing the stale '2 of 5 (~40%)' measurement"
  - "explicit, named OUT-OF-SCOPE record of the product-level version-tag contention race for future ship/version-bump concurrency work"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Bound a racy test's poll via a short, ENV_MUTEX-guarded override of the SAME env var the production code reads for its timeout, instead of restructuring the scenario away — preserves the original acceptance criterion (shared checkout, concurrent finish_workflow) while eliminating the unbounded-wait failure mode"
    - "When a test's outcome is legitimately racy (0 or 1 of N participants may hit a documented failure path), assert on the SET of per-participant results rather than a single blanket success, with explicit, separate assertions for each accepted outcome shape"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - .planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md

key-decisions:
  - "Chose 'assert the loser's documented behavior, bounded by a short DEVFLOW_GATE_TIMEOUT_SECS' (plan's Option 1) over 'give each phase its own version lineage' (Option 2) — the test's own acceptance criterion (13-DEFERRED-CR-03) is specifically the SHARED primary checkout serialized by the coarse lock; separating version lineages per phase would require separate roots/repos, which defeats the exact thing under test"
  - "DEVFLOW_GATE_TIMEOUT_SECS overridden to 2 seconds, scoped to this test's poll only, restored immediately after under the file's existing ENV_MUTEX guard (matching the checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout / transition_resets_infra_failures precedent) — the 7-day production default (parse_gate_timeout's fallback) is untouched"
  - "Did not attempt to make the race loser retry-and-succeed (e.g. via a reactive watcher thread that answers the reopened gate) — that would add complexity without eliminating a second, equally rare collision, and would obscure rather than surface the underlying product-level contention. The test now explicitly tolerates a documented, bounded failure for the loser instead."
  - "Root-caused the collision beyond the plan's own diagnosis via temporary (reverted) debug instrumentation: the two phases' version_bump() calls were caught computing the identical version (2.0.1) within ~1.8ms of each other and both attempting git tag concurrently — direct proof the checkout lock's mutual exclusion is occasionally not actually serializing the two threads' terminal-hook execution, not merely that two phases 'happen to' target the same tag. Recorded in the GAP-2 write-up as the OUT-OF-SCOPE product-level question rather than pursued as a lock-internals fix, per the plan's explicit scope boundary."

patterns-established:
  - "For a test whose failure mode is 'occasionally, and legitimately, blocks forever,' prefer bounding the SAME timeout knob production code already reads (under existing env-mutation guard idiom) over inventing a new test-only timeout mechanism"

requirements-completed: [17b]

coverage:
  - id: D1
    description: "concurrent_ship_advances_finish_both_phases_independently can never wedge the test suite — verified under repeated external timeout, not a single run"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#concurrent_ship_advances_finish_both_phases_independently"
        status: pass
    human_judgment: false
  - id: D2
    description: "The reopened losing-phase gate can no longer poll the 7-day DEVFLOW_GATE_TIMEOUT_SECS production default inside this test's context; the 7-day default itself is untouched for real operator gates"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#concurrent_ship_advances_finish_both_phases_independently"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#parse_gate_timeout_env_override"
        status: pass
    human_judgment: false
  - id: D3
    description: "17-VALIDATION.md GAP-2 rewritten with resolution, re-measured evidence (0 hangs / 25 runs, replacing '2 of 5 (~40%)'), the feedback-latency checkbox fixed, and the product-level version-tag contention race named as an explicit out-of-scope unresolved item"
    requirement: "17b"
    verification:
      - kind: manual_procedural
        ref: ".planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md GAP-2 section + Validation Sign-Off checkbox"
        status: pass
    human_judgment: false

duration: 50min
completed: 2026-07-19
status: complete
---

# Phase 17 Plan 09: Bound the concurrent-ship gate poll so the suite cannot wedge Summary

**Bounded `concurrent_ship_advances_finish_both_phases_independently`'s reopened-gate poll to a 2-second, `ENV_MUTEX`-guarded `DEVFLOW_GATE_TIMEOUT_SECS` override (7-day production default untouched) — RED reproduced the wedge under a 120s external timeout (exit 124) and, via temporary debug instrumentation, caught both phases computing the identical version tag within 1.8ms of each other; GREEN is 25 consecutive isolated runs, 0 hangs, 9 of which actually hit the race and resolved deterministically via the new bounded loser-timeout path.**

## Performance

- **Duration:** ~50 min
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `concurrent_ship_advances_finish_both_phases_independently` can no longer hang: RED was established first (uninstrumented isolated run under a 120s external `timeout` hung, exit 124), and the fix bounds the reopened Ship gate's poll via a 2-second `DEVFLOW_GATE_TIMEOUT_SECS` override scoped to this test only, under the file's existing `ENV_MUTEX` guard — the 7-day production default is never touched
- Root-caused the race beyond the plan's initial diagnosis: temporary (reverted before commit) `eprintln!` instrumentation in `version_bump()` and `lock.rs`'s `acquire_path`/`LockGuard::drop` directly caught both phases' `version_bump()` calls computing the identical version `2.0.1` roughly 1.8ms apart and both attempting `git tag v2.0.1` concurrently — proof this is a genuine (if intermittent) failure of the checkout lock's mutual exclusion to serialize the two threads' terminal-hook execution, not merely "two phases happen to target the same tag." This is recorded as the OUT-OF-SCOPE product-level question in the updated GAP-2 write-up, per the plan's explicit scope boundary (test-level fix only)
- The test now accepts either legitimate outcome deterministically: no collision (both phases finish independently, exactly as originally written) or a bounded loser-timeout (asserted explicitly: error text contains "timed out", state left intact with `gate_pending: true`, and the Ship gate file remains on disk — the documented "awaiting human" state, never a silent vanish)
- Verified with 25 consecutive isolated runs under a 120s external `timeout`: 0 hangs, all exit 0, all identical `test result: ok` verdict; 9 of the 25 runs actually hit the race collision and resolved via the new bounded path, proving both code branches (no-collision and collision) are exercised and green
- `17-VALIDATION.md` GAP-2 rewritten: RESOLVED (test-level, `cb9359f`), re-measured 25-run evidence replaces the stale "2 of 5 (~40%)" figure, the `[⚠️]` feedback-latency sign-off checkbox flips to `[x]`, and the product-level version-tag contention race is recorded as an explicit, named OUT-OF-SCOPE unresolved item so the test-level fix does not silently bury it
- `cargo test --workspace` (362 passed / 0 failed / 0 ignored, 10 targets, this test included unfiltered — no `--skip` needed), `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` all clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Make the test incapable of wedging (RED → GREEN)** - `cb9359f` (fix)
2. **Task 2: Update GAP-2's disposition** - `647fcb3` (docs)

_No separate plan-metadata commit — this SUMMARY/STATE/ROADMAP update lands in the final metadata commit below._

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` - `concurrent_ship_advances_finish_both_phases_independently` rewritten: `DEVFLOW_GATE_TIMEOUT_SECS` overridden to `2` under `ENV_MUTEX` (restored after), `advance()` results collected per-phase instead of unconditionally unwrapped, and per-phase assertions branch on `Ok` (finished, state cleared, `workflow_finished` event) vs `Err` (bounded "timed out" error, state intact with `gate_pending: true`, Ship gate file still on disk)
- `.planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md` - GAP-2 section rewritten with RESOLVED disposition, new 25-run evidence table, and an explicit "Product-level version-tag contention — explicitly OUT OF SCOPE, unresolved" paragraph; feedback-latency sign-off checkbox fixed; a resolution addendum appended after Re-Audit #2 (historical findings left unmodified)

## Decisions Made
- **Chose "assert the loser's documented behavior, bounded by a short timeout" over "give each phase its own version lineage."** The test's own acceptance criterion (13-DEFERRED-CR-03) is specifically about the SHARED primary checkout serialized by the coarse checkout lock — giving each phase a separate version lineage would require separate git roots per phase, which defeats the exact thing under test (cross-phase clobbering on a shared checkout). Bounding the poll preserves the original architecture and acceptance criterion while eliminating the unbounded-wait failure mode.
- **Did not build a reactive watcher/retry mechanism to make the race loser also succeed.** A watcher thread that answers a reopened gate would add real complexity, still wouldn't rule out a second (much rarer) collision on retry, and — more importantly — would make the test's PASS output look identical whether or not the underlying contention manifested, hiding the very defect GAP-2 documents. Explicitly asserting the bounded-failure outcome keeps the race's existence visible in test output (9/25 collision runs are directly observable in the verification evidence) rather than papering over it.
- **DEVFLOW_GATE_TIMEOUT_SECS = 2 seconds**, not something larger like 10-30s: 2s is comfortably longer than `poll_response`'s own backoff floor (first check is immediate, first sleep is 1s) to avoid granularity-induced flakiness, while keeping each of the (currently ~36%) collision runs fast. Verified empirically across 25 runs with zero flakiness at this value.
- **Root-caused via temporary debug instrumentation, then explicitly declined to fix the lock itself.** The plan's own diagnosis attributed the race to "both phases compute the same next version" as if inherent; direct evidence (both threads inside `version_bump()` within 1.8ms, both attempting the same `git tag`) shows the checkout lock's mutual exclusion is not always actually serializing the two critical sections — a real, if rare, product-level defect distinct from "inherent version contention." Given the plan explicitly scopes any lock/version-bump concurrency fix as OUT OF SCOPE, this finding is recorded in the GAP-2 write-up's "Product-level version-tag contention" paragraph rather than acted on, so it isn't lost.

## Deviations from Plan

None — plan executed exactly as written. The debug-instrumentation root-causing described above was investigative only (added, used to confirm the mechanism, then fully reverted — `git diff` on `hooks.rs`/`lock.rs` is empty) and did not change any shipped code; it sharpened the GAP-2 write-up's product-level note beyond the plan's own diagnosis text but did not expand the plan's scope (Rule 2/3 auto-fix territory was not entered — no bug was fixed in the lock, per the plan's explicit "do not decide it here" instruction).

## Issues Encountered

Establishing a reliable RED/GREEN signal was harder than expected because the race is genuinely intermittent (measured ~33-40% across three independent audits, confirmed again here) and because the FIRST diagnostic instrumentation attempt (adding `eprintln!` debug output to `version_bump()`) shifted the timing enough to make the race temporarily unreproducible across 60+ subsequent runs — a real Heisenbug. Root cause was ultimately caught early, before the timing-sensitive instrumentation had fully "settled" the interleaving, in a single instrumented run showing the two `version_bump()` calls 1.8ms apart; that evidence was preserved in commentary and the instrumentation itself was reverted rather than left in shipped code.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 17's sole remaining open validation item (GAP-2, test-level) is now resolved. `nyquist_compliant: true` was already the phase's disposition on coverage grounds; this plan makes the full suite mechanically green and unwedgeable as well — `cargo test --workspace` no longer needs `-- --skip concurrent_ship_advances_finish_both_phases_independently` to get a deterministic, fast result.
- The product-level version-tag contention race (two phases genuinely racing to create the same git tag when their terminal hooks overlap inside the shared checkout lock's critical section) remains open and explicitly out of scope — named in `17-VALIDATION.md`'s GAP-2 section for future ship/version-bump concurrency work. It is not blocking for Phase 17 or Phase 18.

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-19*

## Self-Check: PASSED

All files (`crates/devflow-cli/src/main.rs`, `17-VALIDATION.md`, this SUMMARY) confirmed present on
disk. Both task commits (`cb9359f`, `647fcb3`) confirmed present in `git log --oneline --all`.
