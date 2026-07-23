---
phase: 20-release-correctness-operator-control
plan: 02
subsystem: cli
tags: [cli, worktree, liveness, git, testing, flaky-tests]

requires:
  - phase: 18-dogfood-reliability-hardening
    provides: "monitor_pid persistence + liveness() predicate (18b) — reused verbatim here, no second liveness predicate defined"
provides:
  - "cleanup --force liveness guard: hard-refuses removal of any worktree whose owning phase has a live agent (any monitor state) or an active monitor (Healthy/BetweenStages)"
  - "worktree->phase join keyed on State.worktree_path with dirname/branch fallback"
  - "bounded-backoff retry (remove_worktree_with_retry) absorbing the transient Directory-not-empty race on a confirmed-dead phase, with a manual-clear warning on exhaustion"
  - "durable git fixtures (core.fsyncObjectFiles/core.fsync) + shrunk 60->51 commit loop, closing both phase7_cli.rs flakes"
affects: [20-03, 20-04, 20-05, future release-cut/ship-override phases touching cleanup or worktree lifecycle]

tech-stack:
  added: []
  patterns:
    - "Liveness classification stays a single source of truth (commands.rs::liveness()) — new callers branch on it rather than re-deriving a parallel notion"
    - "Fail-closed-on-live-agent: refuse keys on agent_alive OR Healthy/BetweenStages, never on monitor state alone"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "Tightened the refuse predicate beyond a naive Healthy/BetweenStages-only guard: refuse whenever the recorded agent pid is alive under ANY monitor classification (including Unknown/monitor_pid=None and Stuck/dead-monitor), OR the monitor itself is Healthy/BetweenStages (review: Codex HIGH fail-closed)."
  - "No new cleanup override flag — cleanup --force retains its existing meaning (also remove the reference worktree); D-06 locked this."
  - "git worktree prune is never used as removal recovery — only as the existing post-loop metadata sweep, unchanged."
  - "reference_and_cleanup_worktree_cli_flow's fixture now aborts its forced Validate gate (note containing \"abort\") and waits for state to clear before invoking cleanup, instead of racing cleanup against an actively-running monitor — this is the exact race D-06 closes, so the fixture had to stop relying on it."
  - "devflow parallel object-store concern stays deferred per D-08 — no product change to parallel.rs; DEN-51/999.26 backlog item confirmed still filed and tracked."

requirements-completed: [20b]

coverage:
  - id: D1
    description: "cleanup --force hard-refuses removing a worktree whose agent is alive under Unknown liveness (monitor_pid=None)"
    requirement: "20b"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_force_refuses_on_live_agent_unknown_monitor"
        status: pass
    human_judgment: false
  - id: D2
    description: "cleanup --force hard-refuses removing a worktree whose agent is alive under Stuck liveness (dead monitor)"
    requirement: "20b"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_force_refuses_on_dead_monitor_live_agent"
        status: pass
    human_judgment: false
  - id: D3
    description: "cleanup proceeds with bounded-backoff retry for a genuinely-dead phase and is idempotent when run twice"
    requirement: "20b"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_is_idempotent_when_worktree_already_removed"
        status: pass
    human_judgment: false
  - id: D4
    description: "Both previously-flaky fixtures (worktree-removal race, object-store corruption) pass deterministically"
    requirement: "20b"
    verification:
      - kind: integration
        ref: "cargo test -p devflow --test phase7_cli (13/13 passed, 5/5 consecutive full-file runs)"
        status: pass
    human_judgment: true
    rationale: "Per VALIDATION.md/20-02-PLAN.md, local 5x green is necessary but NOT sufficient sign-off for this CI-concurrency-dependent flake class — full sign-off requires a pushed CI run, which this executor cannot trigger from an isolated worktree."

duration: 35min
completed: 2026-07-23
status: complete
---

# Phase 20 Plan 02: Cleanup Liveness Guard + Flaky Fixture Hardening Summary

**`devflow cleanup --force` now hard-refuses to remove a worktree whenever its owning phase has a live agent (any monitor state) or an active monitor, closing a real product race; both historically-flaky `phase7_cli.rs` fixtures are now deterministic.**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-07-23T08:20:00Z (approx.)
- **Completed:** 2026-07-23T08:55:00Z (approx.)
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments
- `cleanup` joins each `git worktree list` entry to its owning phase `State` via `state.worktree_path` (falling back to worktree-dirname or feature-branch matching only when absent), then hard-refuses removal whenever the recorded agent pid is alive under **any** monitor classification (Unknown/monitor_pid=None, Stuck/dead-monitor included) or the monitor itself is Healthy/BetweenStages — closing the real product race where `cleanup --force` could previously delete a live agent's worktree unconditionally.
- Genuinely-dead phases proceed through a new bounded-backoff retry (`remove_worktree_with_retry`, 3 attempts with exponential short delays) around `worktree::remove`, absorbing the transient "Directory not empty" race; retry exhaustion prints a descriptive warning naming the directory to clear manually instead of failing silently. `git worktree prune` is never used as removal recovery.
- Two new regression tests pin the tightened, fail-closed predicate: `cleanup_force_refuses_on_live_agent_unknown_monitor` (Task 1, RED-then-GREEN) and `cleanup_force_refuses_on_dead_monitor_live_agent` (Task 2) — both prove the guard keys on agent liveness, not merely on Healthy/BetweenStages classification.
- `cleanup_is_idempotent_when_worktree_already_removed` proves a second `cleanup --force` run against an already-removed worktree succeeds rather than erroring.
- Both previously-flaky fixtures (`reference_and_cleanup_worktree_cli_flow`, `start_worktree_mode_ignores_main_checkout_divergence`) are hardened: fixture git repos now set `core.fsyncObjectFiles=true`/`core.fsync=all` (durability for instance 2's object-store race, D-08 fixture-side-only), and the 60-commit divergence loop shrunk to 51 (smallest count still crossing the `behind > 50` threshold), narrowing the corruption window without touching any product code.

## Task Commits

Each task was committed atomically:

1. **Task 1: RED — cleanup --force removes a live agent's worktree (Unknown monitor)** - `ef11958` (test)
2. **Task 2: Liveness-gated removal + bounded-backoff retry in cleanup** - `599d238` (feat)
3. **Task 3: Fixture durability + idempotency for both flaky tests** - `6a582ca` (test)

_TDD gate sequence confirmed: `test(...)` (ef11958, RED) precedes `feat(...)` (599d238, GREEN); Task 3 is a follow-on `type="auto"` task (no RED/GREEN gate required) that hardens the same test file._

## Files Created/Modified
- `crates/devflow-cli/src/commands.rs` - Added `phase_from_worktree_path`, `state_for_worktree` (worktree→phase join), `remove_worktree_with_retry` (bounded-backoff retry helper), and rewrote `cleanup` to consult the liveness guard before every `worktree::remove` call.
- `crates/devflow-cli/tests/phase7_cli.rs` - Added `cleanup_force_refuses_on_live_agent_unknown_monitor`, `cleanup_force_refuses_on_dead_monitor_live_agent`, `cleanup_is_idempotent_when_worktree_already_removed`, and `wait_for_state_cleared`; hardened `init_repo` with fsync git config; shrunk the 60-commit divergence loop to 51; updated `reference_and_cleanup_worktree_cli_flow` to abort its forced Validate gate and wait for state clearance before invoking `cleanup`.

## Decisions Made
- Tightened the refuse predicate per cross-AI review (Codex HIGH): fail-closed on agent liveness alone is not sufficient reasoning to key on Healthy/BetweenStages only — a live agent under Unknown or Stuck monitor classification must also be refused. Implemented as `agent_alive || matches!(liveness, Healthy | BetweenStages)`.
- No new CLI flag introduced; `cleanup --force`'s existing "also remove the reference worktree" meaning is preserved unchanged (D-06).
- Left `git worktree prune`'s existing post-loop call untouched — it is metadata cleanup, never removal recovery (Pitfall 3).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `reference_and_cleanup_worktree_cli_flow` raced the new liveness guard**
- **Found during:** Task 2 (running the full `cleanup`-filtered test suite after implementing the guard)
- **Issue:** This fixture calls `start --mode auto` then immediately `cleanup --force` with no wait. Its fake `claude` script never produces real work, so `Validate` always loops back and forces a gate after `MAX_CONSECUTIVE_FAILURES` — at the moment `cleanup` ran, the phase's monitor was genuinely alive (`BetweenStages`, waiting on the forced gate). The new guard correctly refused removal, which is exactly the race D-06 exists to prevent — a real operator's `cleanup --force` racing an active monitor is the product bug being fixed, not a false positive in the guard.
- **Fix:** The fixture now waits for the forced Validate gate to appear, resolves it via `devflow gate reject --note "abort test teardown"` (a note containing "abort" maps to `GateAction::Abort`, per `gates.rs::GateAction::from_response`), waits for the phase's persisted state to clear (`abort()` calls `workflow::clear_state`), and only then invokes `cleanup --force` — matching what a real operator would do instead of racing an active pipeline.
- **Files modified:** `crates/devflow-cli/tests/phase7_cli.rs`
- **Verification:** `cargo test -p devflow --test phase7_cli reference_and_cleanup_worktree_cli_flow` passes; confirmed via manual reproduction script (bare binary invocation) that the pipeline reaches a stable gated state within ~50ms and clears within ~1s of the abort response landing.
- **Committed in:** `599d238` (Task 2 commit — the fix is a direct, same-file consequence of Task 2's guard, so it landed in the same commit that introduced the guard, keeping Task 2's own verify gate — `cargo test -p devflow --test phase7_cli cleanup` — green at that commit boundary).

---

**Total deviations:** 1 auto-fixed (1 bug — a fixture racing the new correctness guard, not a defect in the guard itself)
**Impact on plan:** No scope creep — the fix stayed within the same test file Task 2 was already touching, and no product code beyond the plan's stated `cleanup` change was needed.

## Issues Encountered
- Initial attempt to fix the same fixture by simply waiting for the pipeline to reach `Ship`/state-clear on its own timed out at both 10s and 30s — the fake agent's "success" self-report is contradicted by real Validate verification (no actual commits), so the pipeline loops Code→Validate indefinitely rather than ever reaching Ship on its own. Resolved by explicitly aborting the forced gate instead of waiting for organic completion (see deviation above).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `cleanup --force`'s liveness guard is a hard, unconditional dependency for any future plan that needs `cleanup` to compose safely with concurrent phases (e.g. a future release-cut executor or parallel dogfood runs) — this closes T-20-02.
- **CI sign-off still outstanding:** per `20-02-PLAN.md`'s verification section, local 5x-green (achieved: 13/13 passed on all 5 consecutive runs) is necessary but not sufficient — full sign-off for this CI-concurrency-dependent flake class requires a pushed CI run, which this isolated worktree executor cannot trigger. Flagged as `human_judgment: true` in the coverage block (D4) for the orchestrator/user to confirm post-merge.
- `devflow parallel`'s concurrent-worktree object-store race remains deferred to backlog (D-08) — DEN-51/999.26 confirmed still filed at `.planning/phases/999.26-parallel-object-store-race/`, not implemented here.

---
*Phase: 20-release-correctness-operator-control*
*Completed: 2026-07-23*
