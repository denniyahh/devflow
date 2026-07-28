---
phase: 25-end-to-end-dogfood-blockers
plan: 18
subsystem: testing
tags: [rust, monitor-reap, unwind-safety, process-leak, test-infrastructure]

# Dependency graph
requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: "25-17's ReapMonitorOnDrop::after_launch RAII guard in test_support.rs"
provides:
  - "run_preflight_advance_gate_launches_agent_exactly_once and run_preflight_loopback_gate_launches_agent_exactly_once converted to the unwind-safe guard, each additionally proving they spawned a real monitor"
  - "A third, previously-unnamed leak site (run_preflight_advance_skips_recheck_on_idempotently_failing_check) discovered and fixed with the same guard"
  - "A re-derived, source-verified enumeration of every test-mod call site that can reach monitor::spawn_monitor across preflight.rs, staleness.rs, and pipeline_launch.rs"
  - ".planning/WINDOWS.md items 1 and 3 closed (fixed); item 4 (this plan's own finding) opened and closed in the same plan; item 2 left waived and untouched"
affects: [25-end-to-end-dogfood-blockers, /gsd-ship gate, any future test that drives run_preflight/launch_stage/launch_stage_inner]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Re-derive an enumeration by searching for what REACHES a spawn (recursive call chains), not just direct call sites — a call-site-only search structurally cannot see spawns reached through a function's own internal recursion (run_preflight's Advance/LoopBack arms)"
    - "Empirically verify a 'provably cannot spawn' claim with a temporary, uncommitted probe (eprintln! + reap) rather than trusting source-reading alone when the claim is load-bearing for closing a defect ledger"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/preflight.rs
    - .planning/WINDOWS.md

key-decisions:
  - "Fixed a third leak site discovered during the plan's own mandated verification-step-6 re-derivation, beyond the two tests declared in scope, rather than silently reporting it and leaving it open — the fix is the identical narrow, test-only guard-binding pattern already established, so extending it costs nothing in risk and leaving a freshly-confirmed live leak unfixed while I held the exact mechanism to close it would be irresponsible"
  - "Recorded the third finding as a new WINDOWS.md entry (item 4), opened and immediately marked fixed in the same plan, rather than silently folding it into item 1's description — item 1's description names two specific tests and is now historically accurate; the third test is a distinct discovery with its own provenance"
  - "Verified two borderline test-mod call sites empirically (temporary eprintln!+reap probe, reverted before any commit) rather than relying on source-reading alone: run_preflight_advance_skips_recheck_on_idempotently_failing_check was confirmed to spawn a real monitor (pid captured); run_preflight_loopback_bounds_recursion was confirmed NOT to spawn (monitor_pid stayed None — the retry ceiling aborts before any launch)"

requirements-completed: [G-25-1]

coverage:
  - id: D1
    description: "run_preflight_advance_gate_launches_agent_exactly_once binds ReapMonitorOnDrop::after_launch ahead of every unwrap/assertion and asserts state.monitor_pid.is_some()"
    requirement: "G-25-1"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::run_preflight_advance_gate_launches_agent_exactly_once"
        status: pass
    human_judgment: false
  - id: D2
    description: "run_preflight_loopback_gate_launches_agent_exactly_once converted the same way, structurally parallel to D1"
    requirement: "G-25-1"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::run_preflight_loopback_gate_launches_agent_exactly_once"
        status: pass
    human_judgment: false
  - id: D3
    description: "Third, previously-unnamed leak site (run_preflight_advance_skips_recheck_on_idempotently_failing_check) discovered via verification-step-6 re-derivation, empirically confirmed to spawn a real monitor, and fixed with the same guard"
    requirement: "G-25-1"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::run_preflight_advance_skips_recheck_on_idempotently_failing_check"
        status: pass
    human_judgment: false
  - id: D4
    description: "Full re-derived enumeration of every test-mod site that can reach monitor::spawn_monitor across preflight.rs/staleness.rs/pipeline_launch.rs, with a per-site verdict (guarded, or provably cannot spawn)"
    requirement: "G-25-1"
    verification:
      - kind: other
        ref: "manual source read + rg -n 'launch_stage\\(|launch_stage_inner\\(|run_preflight\\(' crates/devflow-cli/src restricted to mod tests bodies, plus two empirical probes"
        status: pass
    human_judgment: false
  - id: D5
    description: ".planning/WINDOWS.md items 1 and 3 closed fixed; item 4 (this plan's own finding) opened and closed fixed; item 2 left waived and byte-identical; counters consistent across frontmatter/table/JSON mirror"
    requirement: "G-25-1"
    verification:
      - kind: other
        ref: "gsd-tools windows status: open_count 0, waived_count 1, fixed_count 3, total_count 4"
        status: pass
    human_judgment: false

# Metrics
duration: 18min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 18: Reap the Recursion-Reached Monitor Spawns in preflight.rs Summary

**Converted both preflight.rs recursion tests to the unwind-safe `ReapMonitorOnDrop` guard, then discovered and fixed a THIRD live leak site the plan's own re-derived enumeration surfaced — closing WINDOWS.md items 1 and 3 as planned plus a new item 4 for the third finding, all in the same plan.**

## Performance

- **Duration:** 18 min
- **Started:** 2026-07-28T19:14:00Z (approx)
- **Completed:** 2026-07-28T19:32:39Z
- **Tasks:** 3 planned + 1 unplanned (deviation, documented below)
- **Files modified:** 2 (`crates/devflow-cli/src/preflight.rs`, `.planning/WINDOWS.md`)

## Accomplishments
- `run_preflight_advance_gate_launches_agent_exactly_once` (guard bound at `preflight.rs:1587`, first panic site it now precedes is `preflight.unwrap()` at `:1601`) and `run_preflight_loopback_gate_launches_agent_exactly_once` (guard at `:1677`, precedes `:1691`) both converted to `ReapMonitorOnDrop::after_launch(&state)`, each additionally asserting `state.monitor_pid.is_some()` (`:1616` and `:1706`) — turning WR-05's premise from an inference into a runtime-verified fact
- Neither test lost, weakened, or reordered an existing assertion; the `launch_stage`/`launch_stage_inner` calls still run inside the stubbed-`PATH` window under `ENV_MUTEX`; only the unwrapping moved below the PATH-restore block (deliberate — narrows the mutated-PATH window on the error path)
- **Unplanned finding, fixed in this plan:** verification step 6's re-derived enumeration (searching for what *reaches* `monitor::spawn_monitor` rather than direct call sites) surfaced a third test, `run_preflight_advance_skips_recheck_on_idempotently_failing_check` (`:1747`), that reaches the same unconditional `launch_stage_inner` call inside `run_preflight`'s `Advance` arm and, with a working `codex`+`sh` stub on `PATH` (`agent_free_dir_with_agent_stub`), genuinely spawns a real detached monitor wrapper. Confirmed empirically with a temporary `eprintln!`+reap probe (reverted before any commit; pid `745043` observed and reaped in the probe run). Converted with the identical guard (bound at `:1793`, precedes the `matches!(result, Ok(false))` assertion at `:1808`; new `state.monitor_pid.is_some()` assertion at `:1821`)
- Two other borderline sites checked and confirmed SAFE, one empirically: `run_preflight_loopback_bounds_recursion` (`:1820` pre-fix numbering) reaches the retry ceiling on its recursive real-adapter re-check before any launch — empirically confirmed `monitor_pid` stays `None` via the same probe technique — and `preflight_retries_reset_on_pass` never calls `launch_stage`/`launch_stage_inner` at all, only `run_preflight` on a passing check. `pipeline_launch.rs:606`'s `launch_stage_inner_clears_monitor_pid_on_early_failure` uses an agent-free PATH (`ensure_agent_binary` fails before any spawn) — also provably safe.
- Whole workspace test suite: 696 passed, 0 failed (unchanged count — this plan adds assertions to existing tests, no new tests). `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` both clean. One transient failure (`agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`, a known load-sensitive exec-visibility test, DEN-78/999.53) observed on one run under this machine's heavy concurrent-worktree load; confirmed to pass both in isolation and on a clean re-run of the full suite — not caused by this plan's changes (different crate, `devflow-core`, unrelated file).
- `.planning/WINDOWS.md`: items 1 and 3 closed `fixed`; item 4 (this plan's own finding) opened and closed `fixed` in the same plan; item 2 left `waived` and byte-identical. Final counters: `open_count: 0, waived_count: 1, fixed_count: 3, total_count: 4`.

## Re-derived Enumeration (verification step 6)

Search: `rg -n 'launch_stage\(|launch_stage_inner\(|run_preflight\(' crates/devflow-cli/src`, restricted to `#[cfg(test)] mod tests` bodies, per-site verdict:

| File:line | Test | Reaches spawn? | Verdict |
|---|---|---|---|
| `preflight.rs:1313` | `run_preflight_major_bump_gates_and_never_ships_unattended` | No | Response is `abort` → `GateAction::Abort`, never launches |
| `preflight.rs:1371` | `run_preflight_major_bump_gate_not_auto_approved_by_yes_ship` | No | No gate response ever written → times out `Err` before Advance/LoopBack |
| `preflight.rs:1480` | `run_preflight_failing_check_gates_and_never_reaches_spawn_monitor` | No | Response is `abort` → `Abort` arm (test's own name confirms) |
| `preflight.rs:1518` | `run_preflight_adapter_hook_override_fires` | No | Response is `abort` → `Abort` arm |
| `preflight.rs:1571` (was `:1571`, now `:1587` region) | `run_preflight_advance_gate_launches_agent_exactly_once` | **Yes** | **Guarded (Task 1, this plan)** |
| `preflight.rs:1659` region | `run_preflight_loopback_gate_launches_agent_exactly_once` | **Yes** | **Guarded (Task 2, this plan)** |
| `preflight.rs:1779` region | `run_preflight_advance_skips_recheck_on_idempotently_failing_check` | **Yes — newly discovered** | **Guarded (unplanned Task 4, this plan)** |
| `preflight.rs:1853` region | `run_preflight_loopback_bounds_recursion` | No (empirically confirmed) | Recursive real-adapter re-check hits the retry ceiling and aborts before any launch; `monitor_pid` stays `None` |
| `preflight.rs:1904` region | `preflight_retries_reset_on_pass` | No | Only calls `run_preflight`, never `launch_stage`/`launch_stage_inner` |
| `staleness.rs:767` | `mid_run_stage_transition_does_not_readjudicate_staleness` | Yes | Already guarded (25-17) |
| `pipeline_launch.rs:414` region | `launch_stage_persists_monitor_pid_for_reload` | Yes | Already guarded (25-17) |
| `pipeline_launch.rs:606` | `launch_stage_inner_clears_monitor_pid_on_early_failure` | No | Agent-free PATH; `ensure_agent_binary` fails before any spawn |
| `pipeline_outcomes.rs:362,370`, `pipeline_gate.rs:110,122`, `commands.rs:302` | (production code, not test-mod) | N/A | Outside test scope — these are the real call sites the tests above exercise |

**Explicit statement on a fourth path:** no path to `monitor::spawn_monitor` exists in this codebase's test suite beyond the three functions named in the grep (`launch_stage`, `launch_stage_inner`, `run_preflight`'s own recursion) — every test-mod call site above was individually classified, and the one path this plan's original scope missed (`run_preflight_advance_skips_recheck_on_idempotently_failing_check`) reaches the spawn through the identical `Advance` arm mechanism already named, not a new mechanism. The known residual (per the plan's own must-have truth) remains: this enumeration covers the suite's own manufacture of new leaks; it does not clean up wrappers already accumulated on a developer's machine from previous runs — that is `devflow gate sweep --reap-strays`, which this plan does not run.

## Task Commits

Each task was committed atomically:

1. **Task 1: Convert the Advance-arm preflight test to the guard and verify it really spawns** - `ef1d123` (test)
2. **Task 2: Convert the LoopBack-arm preflight test the same way** - `3bd6d66` (test)
3. **Task 3: Close WINDOWS.md items 1 and 3, and gate the whole workspace** - `37442a5` (docs)
4. **Unplanned Task 4 (deviation): Reap monitor in a third, previously-unnamed preflight test** - `b39b000` (test)

## Files Created/Modified
- `crates/devflow-cli/src/preflight.rs` - Converted three tests (two planned, one discovered) to bind `ReapMonitorOnDrop::after_launch(&state)` ahead of every panicking checkpoint, each asserting `state.monitor_pid.is_some()`; no production code touched
- `.planning/WINDOWS.md` - Items 1 and 3 marked `fixed`; item 4 appended, then marked `fixed` in the same plan; item 2 untouched (`waived`); counters `open_count: 0, waived_count: 1, fixed_count: 3, total_count: 4`

## Decisions Made
- Fixed the third discovered leak site in this same plan rather than deferring it, since the fix is the identical zero-risk, test-only guard-binding pattern already validated by Tasks 1 and 2, and leaving a freshly-confirmed live leak unfixed while holding the exact mechanism to close it contradicts the whole point of this gap-closure round
- Recorded the third finding as a distinct WINDOWS.md entry (item 4) rather than silently amending item 1's description, since item 1's description accurately names only the two originally-scoped tests and rewriting it would erase the historical record of what item 1 actually was
- Verified two ambiguous call sites empirically with a temporary, uncommitted `eprintln!`+reap probe rather than trusting source-reading alone, since the whole objective of this plan is closing a ledger that gates `/gsd-ship` — an incorrect "provably safe" classification would have been exactly the kind of hidden residual this round exists to prevent

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Reaped a third, previously-unnamed monitor leak in `preflight.rs`**
- **Found during:** Task 3's mandated verification-step-6 re-derivation (searching for every path that reaches `monitor::spawn_monitor`, not just direct call sites)
- **Issue:** `run_preflight_advance_skips_recheck_on_idempotently_failing_check` reaches the same unconditional `launch_stage_inner` call (via `run_preflight`'s `Advance` arm) as the two tests this plan was scoped to fix, and with a working `codex`+`sh` stub on `PATH` genuinely spawns a real detached monitor wrapper — confirmed empirically (pid captured, unreaped) before any fix
- **Fix:** Bound `ReapMonitorOnDrop::after_launch(&state)` immediately after the test's final `&mut state` use, ahead of every panicking checkpoint, identical to Tasks 1/2's pattern; added a `state.monitor_pid.is_some()` assertion
- **Files modified:** `crates/devflow-cli/src/preflight.rs`
- **Verification:** Test passes in isolation; full workspace suite re-run clean (696 passed, 0 failed) after the fix; `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean
- **Committed in:** `b39b000`

**2. [Rule 2 - Missing Critical] Recorded and closed the new finding in `.planning/WINDOWS.md`**
- **Found during:** Same as above
- **Issue:** A newly-discovered, already-fixed defect needed a ledger entry so the discovery is not silently lost, per this project's broken-windows-ledger discipline
- **Fix:** `gsd-tools windows append` (item 4, `deviation` kind) followed by `gsd-tools windows fixed 4`
- **Files modified:** `.planning/WINDOWS.md`
- **Verification:** `gsd-tools windows status` confirms `open_count: 0, waived_count: 1, fixed_count: 3, total_count: 4`; table/frontmatter/JSON-mirror counts all consistent
- **Committed in:** `b39b000`

---

**Total deviations:** 2 auto-fixed (both Rule 2 - missing critical functionality, same underlying finding)
**Impact on plan:** Closes a live process leak the plan's original scope did not name, using the exact mechanism the plan already established. No scope creep into production code; test-only, matching the plan's own boundary. `/gsd-ship`'s ledger gate ends at `open_count: 0`, the same end state the plan targeted, now additionally covering a defect the plan didn't know about when written.

## Issues Encountered
One transient, unrelated test failure during a full-workspace run: `agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape` (in `devflow-core`, an exec-visibility timing test known to be sensitive to concurrent system load — DEN-78/999.53) failed once under this machine's heavy concurrent-worktree load, then passed both in isolation and on an immediate full-suite re-run. Not caused by this plan's changes (different crate, unrelated file); not investigated further per the scope boundary (pre-existing, out-of-scope flake).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
G-25-1 is closed: both originally-scoped preflight tests reap their spawned monitors on every exit path, and a third, previously-unnamed leak surfaced by this plan's own re-derived enumeration is also closed. `.planning/WINDOWS.md` is at `open_count: 0` (waived: 1, fixed: 3, total: 4), so `/gsd-ship`'s ledger gate no longer blocks on any open defect from this round. The residual noted in the plan (already-accumulated orphaned wrappers on developer machines from prior runs) remains explicitly out of scope — that cleanup is `devflow gate sweep --reap-strays`, run by a human, not by this plan.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*
