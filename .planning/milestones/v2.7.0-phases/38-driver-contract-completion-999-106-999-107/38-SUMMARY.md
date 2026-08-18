---
phase: 38-driver-contract-completion-999-106-999-107
plan: 38
subsystem: agents
tags: [agent-driver, agentadapter, migration, interactivity-mode, codex-parser]

# Dependency graph
requires:
  - phase: 37-modular-agent-driver
    provides: AgentDriver trait, four *Driver unit structs, conformance suite
  - phase: 37.1-pi-subagent-extension-spike-research
    provides: VIABLE verdict that re-scoped the dispatch arm (no drain gate)
provides:
  - AgentAdapter trait + DriverShim + adapter_for + four legacy *Agent structs deleted
  - Driver-driven InteractivityMode gate (Define/Plan) replacing the hardcoded Codex-Define check
  - Two Codex-parser defect fixes (999.107): turn.failed precedence, non-UTF-8 writable-root serialization
affects: [39-pi-end-to-end]

# Actuals (#2632)
actuals:
  tokens: 3900
  tasks: 9
  commits: 4

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "driver_for(kind) -> Box<dyn AgentDriver> as the single dispatch seam; no legacy adapter surface"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-core/src/agents/claude.rs
    - crates/devflow-core/src/agents/codex.rs
    - crates/devflow-core/src/agents/opencode.rs
    - crates/devflow-core/src/agents/pi.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-core/src/canary.rs

key-decisions:
  - "Delete the legacy AgentAdapter surface outright (999.106) rather than keeping both paths — the deprecation-date escape hatch was superseded"
  - "InteractivityMode gate is driver-driven and Define/Plan scoped; the Plan extension was reverted (PLAN.md is an output, not a precondition)"
  - "999.107 #2: refuse the launch on a non-UTF-8 writable root rather than lossily converting to U+FFFD"

patterns-established:
  - "Reason-string greps when deleting pub items (tests reference deleted symbols through strings)"

requirements-completed: []

coverage:
  - id: D1
    description: "AgentAdapter/DriverShim/adapter_for and legacy *Agent structs are gone from source (only comments/history reference them)"
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow + -p devflow-core --lib (633 + 322 green)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Call sites migrated to AgentDriver with byte-identical Claude launch argv (zero regression)"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/mod.rs#drivers_reproduce_legacy_adapter_behavior"
        status: pass
    human_judgment: false
  - id: D3
    description: "Driver-driven InteractivityMode gate (Define/Plan) replaces the hardcoded Codex-Define check"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/mod.rs#codex_define_and_plan_require_an_existing_artifact"
        status: pass
    human_judgment: false
  - id: D4
    description: "999.107 #1: terminal turn.failed takes precedence over an earlier success marker"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs (success-marker + turn.failed -> not-Success)"
        status: pass
    human_judgment: false
  - id: D5
    description: "999.107 #2: non-UTF-8 / hostile writable-root paths are refused, not lossily converted"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/codex.rs (hostile-path fixture)"
        status: pass
    human_judgment: false

# Metrics
duration: ~1d (execution + FIX-FIRST review close)
completed: 2026-08-17
status: complete
---

# Phase 38: Driver Contract Completion Summary

**Deleted the legacy `AgentAdapter` surface, migrated all call sites to `AgentDriver`, wired the driver-driven `InteractivityMode` gate, and fixed both 999.107 Codex-parser defects — Claude launch argv byte-identical, 633 + 322 tests green.**

## Performance

- **Duration:** ~1d (execution + adversarial FIX-FIRST review close)
- **Tasks:** 9 (T-38-01 … T-38-09, five waves)
- **Files modified:** 13 (+316/−444 in the refactor, plus the review-fix commit)

## Accomplishments

- Removed `AgentAdapter` trait, `DriverShim`, `adapter_for`, and the four legacy `*Agent` structs
  (`ClaudeAgent`/`CodexAgent`/`OpenCodeAgent`/`PiAgent`) — the single dispatch seam is now
  `driver_for(kind) -> Box<dyn AgentDriver>`.
- Migrated the five call sites (`canary.rs`, `test_support.rs`, `preflight.rs`, `pipeline_launch.rs`,
  and the Claude inherent methods) with `exec_command` → `build_command` byte-for-byte.
- Wired the driver-driven `InteractivityMode` gate (Define/Plan) replacing the hardcoded
  `AgentKind::Codex` Define check; the Plan extension was later reverted (PLAN.md is an output).
- Fixed 999.107 #1 (terminal `turn.failed` precedence over an earlier success marker) and #2
  (non-UTF-8 writable-root refusal instead of lossy U+FFFD conversion).

## Task Commits

1. **T-38-01..09 (driver contract + InteractivityMode + 999.107)** — `9ed0432` (refactor)
2. **FIX-FIRST review close** — `189e020` (fix)
3. **Plan + architecture-reference refresh** — `e6c280b` (docs)
4. **Roadmap completion** — `0286f4c` (docs)

## Files Created/Modified

- `crates/devflow-core/src/agents/mod.rs` — `driver_for` seam, trait deletion
- `crates/devflow-core/src/agents/{claude,codex,opencode,pi}.rs` — `*Driver` structs only
- `crates/devflow-core/src/agent_result.rs` — 999.107 #1 precedence
- `crates/devflow-core/src/agents/codex.rs` — 999.107 #2 hostile-path hardening
- `crates/devflow-cli/src/{pipeline_launch,preflight,commands,pipeline_gate,test_support}.rs` — call-site migration

## Decisions Made

- Delete the legacy surface outright (999.106) rather than keep both paths — the deprecation-date
  escape hatch was superseded.
- `InteractivityMode` is driver-driven; Define/Plan gated, with Plan reverted after review (its
  artifact is produced, not a precondition).
- 999.107 #2: refuse non-UTF-8 writable roots, never lossily convert.

## Deviations from Plan

- **The D-03 Plan gate was reverted.** The first implementation extended the gate to Plan but checked
  `develop` for a `-PLAN.md` that the Plan stage *produces* on the feature branch — a dead-end gate
  (phase-38 code review finding 1). Reverted to Define-only.
- **999.107 #2 non-UTF-8 leg was initially "tested, not fixed."** The first cut asserted the U+FFFD
  replacement as correct; the review required refusal instead (finding 2), now done in `189e020`.

## Issues Encountered

- The FIX-FIRST review (claude/codex/antigravity) found the half-wired Plan gate and the
  lossy 999.107 fix; both closed in `189e020`.

## Next Phase Readiness

- `driver_for` + `AgentDriver` are the stable seam Phase 39's Pi driver builds on; no
  `AgentAdapter` references remain in source.

---
*Phase: 38-driver-contract-completion-999-106-999-107*
*Completed: 2026-08-17*
