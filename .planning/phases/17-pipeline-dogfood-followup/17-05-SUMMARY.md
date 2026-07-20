---
phase: 17-pipeline-dogfood-followup
plan: 05
subsystem: infra
tags: [rust, cli, preflight, gate, git, build-provenance, self-dogfood]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "Plan 02's build.rs (DEVFLOW_BUILD_COMMIT/DIRTY/TIMESTAMP env vars) and Plan 04's exhaustive outcome_policy::decide_action dispatch + run_gate/GateAction/fire_gate_notify machinery"
provides:
  - "AgentAdapter::preflight default trait method (adapter hook surface for reviewer-set enforcement)"
  - "run_preflight: generic interactivity + Ship-scoped gh-auth checks, called from launch_stage before spawn_monitor, gated via run_gate (never a hard exit)"
  - "workflow_started event payload with version/commit/dirty/build_timestamp/exe_path"
  - "self-dogfood build-staleness gate: hard-blocks a Stale build only against DevFlow's own workspace, warns everywhere else"
affects: [18-hermes-support]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "AgentAdapter default-method trait hook (mirrors extra_env) for opt-in adapter-specific preflight checks"
    - "Pure decision-table functions (staleness_outcome, gh_auth_check_applies) extracted alongside I/O-performing gate functions for direct unit-testability"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "preflight_interactivity_check is scoped to AgentKind::Codex (not every agent) — a blanket Auto+Define+no-CONTEXT.md check broke three passing integration tests (start_defaults_to_worktree, start_no_worktree_uses_feature_branch, start_worktree_mode_ignores_main_checkout_divergence) because Claude/OpenCode complete Define headlessly today; only Codex's exec mode genuinely cannot answer the discuss-phase interview"
  - "launch_stage's signature changed from &State to &mut State so run_preflight/enforce_build_staleness can drive run_gate (which mutates state.gate_pending and persists it) — all call sites updated"

requirements-completed: [17c, 17d]

coverage:
  - id: D1
    description: "AgentAdapter::preflight default method (Ok(())) plus adjacency-boundary test via a TEST-ONLY adapter"
    requirement: "17c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/mod.rs#default_preflight_is_ok_for_built_in_adapters"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/mod.rs#reviewer_set_adapter_hook_adjacency_boundary"
        status: pass
    human_judgment: false
  - id: D2
    description: "run_preflight composes generic universal checks + adapter hook, gated via run_gate before spawn_monitor"
    requirement: "17c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#run_preflight_failing_check_gates_and_never_reaches_spawn_monitor"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#run_preflight_adapter_hook_override_fires"
        status: pass
    human_judgment: false
  - id: D3
    description: "gh-auth credential probe scoped to Stage::Ship only, fail-soft when gh absent"
    requirement: "17c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#gh_auth_check_applies_only_to_ship_stage"
        status: pass
    human_judgment: false
  - id: D4
    description: "workflow_started payload carries version/commit/dirty/build_timestamp/exe_path"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#workflow_started_payload_carries_build_provenance"
        status: pass
    human_judgment: false
  - id: D5
    description: "Self-dogfood staleness gate: blocks Stale on DevFlow's own workspace (with notify+event before erroring), warns on ordinary projects and Indeterminate results"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#enforce_build_staleness_warns_for_ordinary_project_with_stale_commit"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#enforce_build_staleness_never_blocks_on_indeterminate"
        status: pass
    human_judgment: false

duration: 45min
completed: 2026-07-19
status: complete
---

# Phase 17 Plan 05: Preflight Readiness Gate + Build Provenance Summary

**AgentAdapter::preflight adapter hook + run_preflight (interactivity/gh-auth) gate stage launches before spawn_monitor, and workflow_started/launch_stage now carry build provenance with a self-dogfood staleness hard-block**

## Performance

- **Duration:** 45 min
- **Completed:** 2026-07-19
- **Tasks:** 2 completed
- **Files modified:** 2

## Accomplishments

- `AgentAdapter` gained a `preflight(&self, state) -> Result<(), String>` default method (Ok(())), mirroring `extra_env`'s empty-default shape — the trait surface Phase 18's Hermes adapter will implement for reviewer-set enforcement
- `run_preflight` in `main.rs` composes the generic universal checks (Codex-Define interactivity, Ship-scoped `gh auth status`) with the adapter hook, called from `launch_stage` **before** `monitor::spawn_monitor` — a failing check routes through `run_gate`/`fire_gate_notify` (named preflight gate + notify), never a hard exit or panic
- `workflow_started` now carries `version`/`commit`/`dirty`/`build_timestamp`/`exe_path` (factored into a testable `workflow_started_payload` helper)
- A self-dogfood build-staleness gate (`Staleness` enum, `embedded_commit_is_stale` ancestry check, `tracked_source_newer_than_build` mtime arm, `combined_staleness` OR composition, `is_self_dogfood_workspace` Cargo.toml member-path scan, `enforce_build_staleness` the gate itself) blocks a confirmed-Stale build **only** when the target project IS DevFlow's own workspace — firing notify + an event before the blocking error — and only warns everywhere else (ordinary projects, or an Indeterminate result on any project)

## Task Commits

Each task was committed atomically:

1. **Task 1 (core half): AgentAdapter::preflight default method** - `3225fd1` (feat)
2. **Task 1 (main.rs half) + Task 2: preflight gate + build provenance/staleness gate** - `a0f49cb` (feat)

_Note: Task 1's implementation spans both `agents/mod.rs` (committed separately, cleanly self-contained) and `main.rs` (the wiring/call site). Task 1's `main.rs` half and Task 2 were committed together because both gate the exact same `launch_stage` call site immediately before `monitor::spawn_monitor` — the two features' code is physically interleaved in that function and in the new-function block between `ensure_agent_binary` and `launch_stage`, making a clean per-task split of the `main.rs` diff impractical without a risky manual reconstruction. Both tasks' behavior was independently verified (targeted `cargo test -p devflow preflight`, `cargo test -p devflow-core agents::`, and the full workspace suite) before either commit landed._

**Plan metadata:** (this commit) `docs(17-05): complete preflight-readiness-and-build-provenance plan`

## Files Created/Modified

- `crates/devflow-core/src/agents/mod.rs` - `AgentAdapter::preflight` default method + adjacency-boundary tests
- `crates/devflow-cli/src/main.rs` - `run_preflight`/`generic_preflight_checks`/`preflight_interactivity_check`/`gh_auth_check_applies`/`preflight_gh_auth_check`; `workflow_started_payload`; `Staleness`/`StalenessOutcome`/`embedded_commit_is_stale`/`tracked_source_newer_than_build`/`combined_staleness`/`is_self_dogfood_workspace`/`enforce_build_staleness`; `launch_stage` signature changed to `&mut State` and now calls both gates before `spawn_monitor`; 21 new unit tests

## Decisions Made

- **Scoped the new interactivity check to `AgentKind::Codex`, not every adapter (Rule 1 auto-fix).** The plan's action text reads as generalizing the existing Codex-only `phase_artifact_on_develop` check to "every adapter." Implementing it literally that way (any agent, Auto mode, Define stage, no CONTEXT.md) caused three previously-passing integration tests to hang on a real 7-day gate poll (`start_defaults_to_worktree`, `start_no_worktree_uses_feature_branch`, `start_worktree_mode_ignores_main_checkout_divergence`) — those tests run `--agent claude --mode auto` with no pre-existing CONTEXT.md and expect success, because Claude's headless Define genuinely completes the discuss-phase interview non-interactively (verified live, 13-06); only Codex's `exec` mode has no route to answer it. Keeping the check scoped to `state.agent == AgentKind::Codex` (still living in the generic layer as a plain function, not a per-adapter trait override — satisfying D-13's "generic vs adapter hook" architectural split) preserves both the new never-silent gate coverage for non-`start()` launch paths (`resume`, gate retries, loop-backs — none of which run the existing pre-state Codex check) and the pre-existing, product-verified Claude/OpenCode behavior.
- **`launch_stage(state: &State, ...)` changed to `launch_stage(state: &mut State, ...)`.** Required because `run_preflight` and `enforce_build_staleness` both call `run_gate`, which mutates `state.gate_pending` and persists state. All five call sites (`start`, `resume`, `handle_stage_failure`'s two retry branches, `transition`, `loop_back_to_code`) already held a `&mut State` binding except `start`/`resume`, which were trivially adjusted (`let mut state`).
- **Task boundary commits landed as agents/mod.rs (Task 1 core) + a single combined main.rs commit (Task 1 wiring + Task 2)**, documented above under Task Commits — both features gate the identical `launch_stage` call site and their new-function blocks are physically interleaved in the diff.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Scoped the generic interactivity preflight check to Codex only instead of every adapter**
- **Found during:** Task 1, after implementing `preflight_interactivity_check` as written (any agent) and running the full workspace test suite
- **Issue:** A blanket `state.mode == Auto && state.stage == Define && !phase_artifact_on_develop(...)` check (applying to every agent, not just Codex) caused three real, previously-passing `phase7_cli.rs` integration tests to hang indefinitely on a real gate poll — they exercise `--agent claude --mode auto` with no pre-existing CONTEXT.md and expect `start` to succeed, which is the actual, verified product behavior (Claude completes Define headlessly; only Codex cannot)
- **Fix:** Added `state.agent == AgentKind::Codex` to the check's condition, matching the exact real-world justification already coded into the existing (unmigrated) pre-state Codex check in `start()`
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** `cargo test -p devflow --test phase7_cli` (all 11 tests, including the three previously-hanging ones) and the full workspace suite (352 tests) both green after the fix
- **Committed in:** `a0f49cb` (part of the main.rs task commit — caught and fixed before either main.rs-touching commit landed, so no separate fix-up commit was needed)

---

**Total deviations:** 1 auto-fixed (Rule 1)
**Impact on plan:** The fix narrows the interactivity check's scope from "every agent" to "Codex only" relative to a literal reading of the plan's action text, but preserves every acceptance criterion the plan actually specifies (the check still lives in the generic layer, still reuses the same `phase_artifact_on_develop` predicate, still routes through the gate — none of the plan's ACs mention non-Codex agents). No scope creep; this is a correctness fix that prevents a real regression against already-shipped, tested behavior.

## Issues Encountered

None beyond the deviation above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `AgentAdapter::preflight` is ready for Phase 18's Hermes adapter to implement for real reviewer-set enforcement — no further trait changes needed.
- `workflow_started`'s provenance fields and the staleness-gate machinery are available for Phase 18d's deferred `devflow doctor` state/event reconciliation work.
- No blockers.

## Self-Check: PASSED

- `crates/devflow-core/src/agents/mod.rs` — FOUND
- `crates/devflow-cli/src/main.rs` — FOUND
- Commit `3225fd1` — FOUND in `git log --oneline`
- Commit `a0f49cb` — FOUND in `git log --oneline`

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-19*
