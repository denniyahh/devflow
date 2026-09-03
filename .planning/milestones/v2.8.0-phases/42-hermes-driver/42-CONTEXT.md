# Phase 42: Hermes Driver - Context

**Gathered:** 2026-08-21
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 42 delivers the modular **Hermes Driver** (`AgentKind::Hermes`), enabling `devflow start --agent hermes` to launch Hermes in oneshot headless mode (`hermes -z "<prompt>" --yolo --accept-hooks`) with honest completion detection and conformance suite enrollment (HRMS-01, HRMS-02, HRMS-03).

Crucially, Phase 42 is executed as a **supervised DevFlow dogfood run driven by Antigravity** (`devflow start --agent antigravity --phase 42 --mode supervise`), satisfying **ANTG-04** to measure real event cadence, verify the `--print-timeout 60m` quiet-gap override, and unlock unattended `--mode auto` in preflight.

</domain>

<decisions>
## Implementation Decisions

### Hermes Driver Core & Launch
- **D-01: Headless oneshot launch with `--accept-hooks`.** — **Reversibility:** costly — affects CLI spawn contract.
  `HermesDriver::build_command` returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])`.
  `HermesDriver::environment` sets `("HERMES_ACCEPT_HOOKS".into(), "1".into())`.
  Rationale: `--yolo` bypasses command approvals; `--accept-hooks` prevents interactive TTY prompts on unseen shell hooks configured in `~/.hermes/config.yaml`.
- **D-02: Prompt rendering uses `render_claude_style`.** — **Reversibility:** reversible.
  `HermesDriver::render_prompt` delegates to `crate::prompt::render_claude_style(intent)`, providing standard `/gsd-*` commands and the `DEVFLOW_RESULT` completion contract.
- **D-03: Process-exit completion parsing with `DEVFLOW_RESULT` contract.** — **Reversibility:** costly.
  `HermesDriver` uses the standard process-exit completion transport: stdout is scanned for `DEVFLOW_RESULT` JSON markers via `parse_marker_lines`. A marker-less run never advances a commit-gated stage (`Plan`, `Code`).
- **D-04: Dynamic subagent capability discovery.** — **Reversibility:** reversible.
  `HermesDriver::capabilities` probes `hermes tools list` for `enabled.*delegation` via a helper `hermes_subagent_dispatch_available()`, setting `subagent_dispatch: true` when enabled and `false` otherwise (mirroring the `pi_subagent_dispatch_available` pattern).

### Registration, Conformance & Health
- **D-05: Full `AgentKind` registration.** — **Reversibility:** one-way — public enum in `devflow-core`.
  Add `AgentKind::Hermes`, wire `FromStr` / `Display`, `driver_for` mapping (`Box::new(HermesDriver)`), and `agent_program` (`"hermes"`).
- **D-06: Doctor presence check & conformance enrollment.** — **Reversibility:** costly.
  Add `hermes` to `devflow doctor` checks (`doctor_checks()` in `commands.rs`). Enroll `HermesDriver` in `every_driver_passes_the_conformance_suite` (5 → 6 drivers in `crates/devflow-core/src/agents/mod.rs`).

### Antigravity Dogfooding Execution (ANTG-04)
- **D-07: Phase 42 driven via supervised Antigravity run.** — **Reversibility:** one-way — milestone gating & requirement fulfillment.
  The implementation of Phase 42 is executed through `devflow start --agent antigravity --phase 42 --mode supervise`.
  During the run:
  1. Measure the real quiet-gap event cadence and compare against the 120s idle timeout floor (`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`).
  2. Confirm `--print-timeout 60m` survives long tool operations (closing the 41-UAT deferred negative control).
  3. Completing the supervised run satisfies **ANTG-04** and unlocks `--mode auto` for Antigravity in `preflight.rs:974` (C2 preflight gate).

### Operator's Discretion
- Exact layout of unit and integration test fixtures for Hermes driver.
- Minor doctor version string parsing nuances (`hermes --version` / `hermes -V`).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope & Requirements
- `.planning/REQUIREMENTS.md` § "Hermes" (HRMS-01..03) & "Antigravity Dogfood + Cadence" (ANTG-04)
- `.planning/ROADMAP.md` § "Phase 42: Hermes Driver"

### Prior Driver & Dogfood Precedents
- `crates/devflow-core/src/agents/pi.rs` — non-stream process-exit driver reference & dynamic subagent capability probe
- `crates/devflow-core/src/agents/antigravity.rs` — Antigravity driver transport & `render_claude_style` usage
- `crates/devflow-core/src/agents/mod.rs` — `AgentDriver` trait, `driver_for`, and conformance suite
- `crates/devflow-cli/src/preflight.rs` — `agent_program` and `unattended_launch_shape_condition` (C2 gate for ANTG-04)
- `.planning/phases/40-pi-dogfood/40-02-PLAN.md` — Supervised dogfood plan structure reference

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `render_claude_style` in `crates/devflow-core/src/prompt.rs`
- `parse_marker_lines` in `crates/devflow-core/src/agent_result.rs`
- `contract_checks` in `crates/devflow-core/src/agents/mod.rs`

### Established Patterns
- Modular driver pattern (`AgentDriver` impl in `crates/devflow-core/src/agents/hermes.rs`)
- Capability detection via CLI probe (like `pi_subagent_dispatch_available`)
- Doctor check enumeration in `crates/devflow-cli/src/commands.rs`

### Integration Points
- `crates/devflow-core/src/state.rs` (`AgentKind::Hermes`)
- `crates/devflow-core/src/agents/mod.rs` (`driver_for`, `conformance_suite`)
- `crates/devflow-cli/src/preflight.rs` (`agent_program`, C2 auto-mode gate)
- `crates/devflow-cli/src/commands.rs` (`doctor_checks`)

</code_context>

<specifics>
## Specific Ideas

- Ensure `hermes -z "<prompt>" --yolo --accept-hooks` executes cleanly in a headless subshell.
- Phase 42 plan structure: 42-01 (Hermes driver implementation & unit/conformance tests) and 42-02 (supervised Antigravity dogfood run & cadence verification).

</specifics>

<deferred>
## Deferred Ideas

- None — discussion stayed strictly within Phase 42 scope.

</deferred>

---

*Phase: 42-Hermes Driver*
*Context gathered: 2026-08-21*
