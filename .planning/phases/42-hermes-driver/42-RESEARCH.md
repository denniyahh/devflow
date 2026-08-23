# Phase 42: Hermes Driver - Research

**Researched:** 2026-08-21
**Domain:** Agent driver integration, oneshot headless execution, process-exit completion detection, Antigravity dogfooding execution
**Confidence:** HIGH

## Summary

Phase 42 integrates the Hermes Agent (`hermes` v0.20.4) as a modular DevFlow agent driver (`AgentKind::Hermes`), enabling `devflow start --agent hermes` to drive phases in headless oneshot mode (`hermes -z "<prompt>" --yolo --accept-hooks`). The driver renders prompts using `render_claude_style`, sets `HERMES_ACCEPT_HOOKS=1`, detects subagent delegation capability via `hermes tools list`, parses `DEVFLOW_RESULT` from process stdout, and enrolls in the shared conformance suite (HRMS-01, HRMS-02, HRMS-03).

Crucially, Phase 42 executes as a **supervised DevFlow dogfood run driven by Antigravity** (`devflow start --agent antigravity --phase 42 --mode supervise`), satisfying **ANTG-04** by measuring the real quiet-gap event cadence distribution against the 120s idle timeout floor (`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`), verifying that `--print-timeout 60m` survives long operations without dying (closing 41-UAT deferred negative control), and unlocking `--mode auto` for Antigravity in preflight.

**Primary recommendation:** Implement `HermesDriver` in `crates/devflow-core/src/agents/hermes.rs` following the established process-exit modular driver pattern (like `PiDriver`); register `AgentKind::Hermes` across `devflow-core` and `devflow-cli`; enroll `HermesDriver` in the conformance suite (5 → 6 drivers); add `hermes` to `devflow doctor`; test stubbed failure modes in `phase7_cli.rs`; and execute the implementation via supervised Antigravity dogfooding to satisfy ANTG-04.

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01: Headless oneshot launch with `--accept-hooks`.**
  `HermesDriver::build_command` returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])`.
  `HermesDriver::environment` sets `("HERMES_ACCEPT_HOOKS".into(), "1".into())`.
  `--yolo` bypasses command approvals; `--accept-hooks` prevents interactive TTY prompts on unseen shell hooks configured in `~/.hermes/config.yaml`. [VERIFIED: CONTEXT.md:19-22]

- **D-02: Prompt rendering uses `render_claude_style`.**
  `HermesDriver::render_prompt` delegates to `crate::prompt::render_claude_style(intent)`, providing standard `/gsd-*` commands and the `DEVFLOW_RESULT` completion contract. [VERIFIED: CONTEXT.md:23-24]

- **D-03: Process-exit completion parsing with `DEVFLOW_RESULT` contract.**
  `HermesDriver` uses the standard process-exit completion transport: stdout is scanned for `DEVFLOW_RESULT` JSON markers via `parse_marker_lines`. A marker-less run never advances a commit-gated stage (`Plan`, `Code`). [VERIFIED: CONTEXT.md:25-27]

- **D-04: Dynamic subagent capability discovery.**
  `HermesDriver::capabilities` probes `hermes tools list` for `enabled.*delegation` via a helper `hermes_subagent_dispatch_available()`, setting `subagent_dispatch: true` when enabled and `false` otherwise (mirroring the `pi_subagent_dispatch_available` pattern). [VERIFIED: CONTEXT.md:28-29]

- **D-05: Full `AgentKind` registration.**
  Add `AgentKind::Hermes`, wire `FromStr` / `Display`, `driver_for` mapping (`Box::new(HermesDriver)`), and `agent_program` (`"hermes"`). [VERIFIED: CONTEXT.md:31-32]

- **D-06: Doctor presence check & conformance enrollment.**
  Add `hermes` to `devflow doctor` checks (`doctor_checks()` in `commands.rs`). Enroll `HermesDriver` in `every_driver_passes_the_conformance_suite` (5 → 6 drivers in `crates/devflow-core/src/agents/mod.rs`), verified by `hermes_conformance_enrollment`. [VERIFIED: CONTEXT.md:33-34]

- **D-07: Phase 42 driven via supervised Antigravity run (ANTG-04).**
  The implementation of Phase 42 is executed through `devflow start --agent antigravity --phase 42 --mode supervise`.
  During the run:
  1. Measure the real quiet-gap event cadence and compare against the 120s idle timeout floor (`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`).
  2. Confirm `--print-timeout 60m` survives long tool operations (closing the 41-UAT deferred negative control).
  3. Completing the supervised run satisfies **ANTG-04** and unlocks `--mode auto` for Antigravity in `preflight.rs:974` (C2 preflight gate). [VERIFIED: CONTEXT.md:37-43]

### Operator's Discretion

- Exact layout of unit and integration test fixtures for Hermes driver.
- Minor doctor version string parsing nuances (`hermes --version` / `hermes -V`).

---

<phase_requirements>
## Phase Requirements

| Req ID | Behavior | Research Support |
|--------|----------|------------------|
| HRMS-01 | Operator can select `--agent hermes` — full `AgentKind` registration | Add `AgentKind::Hermes`, wire `FromStr`/`Display`, `driver_for`, `agent_program` |
| HRMS-02 | Hermes driver launches headless (`hermes -z "<prompt>" --yolo --accept-hooks`) and passes conformance suite | D-01 argv contract, D-02 `render_claude_style`, D-06 conformance enrollment 5→6 |
| HRMS-03 | Hermes completion is honest (process-exit + `DEVFLOW_RESULT` prompt contract); marker-less run never advances | D-03 process-exit parsing via `parse_marker_lines` + `phase7_cli.rs` stubbed-PATH regression tests |
| ANTG-04 | Antigravity dogfooded through real supervised phase run (`devflow start --agent antigravity --phase 42 --mode supervise`); quiet gaps measured; `--print-timeout 60m` confirmed; `--mode auto` unlocked in preflight | D-07 supervised run execution, cadence distribution measurement against 120s floor, C2 preflight gate unlock |

</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Agent binary discovery | CLI process startup (`main.rs`) | Preflight health checks (`ensure_agent_binary`) | Every agent launches from a CLI binary; health check detects absence before attempting a run |
| Process-exit transport | Process monitor (`monitor.rs`) | Core agent loop | Non-stream process-exit runner collects stdout, checks exit code, and parses `DEVFLOW_RESULT` |
| Completion detection | Driver / Agent result (`agent_result.rs`) | Stage gate verification | `parse_marker_lines` extracts JSON result; commit-gated stages require explicit marker |
| Prompt rendering | Driver (`HermesDriver`) | Stage intent → prompt text | `render_claude_style` produces slash-command prompts with DEVFLOW_RESULT instructions |
| Capability discovery | Driver (`capabilities()`) | CLI probe (`hermes tools list`) | Dynamic detection checks if delegation toolset is enabled in Hermes |
| Conformance validation | Shared trait methods (`test_contract`) | Driver unit tests | Every driver must pass 7 contract checks (name, prompts for all stages, program name) |
| Preflight C2 Gate | `preflight.rs` | Auto-mode guard | Unlocks Antigravity `--mode auto` once ANTG-04 is fulfilled |

## Standard Stack

### Core (No New Dependencies)

| Component | Source | Purpose | Why Standard |
|-----------|--------|---------|--------------|
| Hermes CLI | `/home/denniyahh/.local/bin/hermes` / `hermes` (v0.20.4) | Headless agent execution | Operator's installed Hermes Agent |
| Antigravity CLI | `/home/denniyahh/.local/bin/agy` (v1.1.16) | Dogfood runner for Phase 42 | Supervised dogfood harness satisfying ANTG-04 |
| Rust toolchain | Project `rust-toolchain.toml` pinned | Compilation, testing | No version bump required; all integrations are process/argv-level |

### Supporting (Process-Level Integration)

| Component | Source | Purpose | When to Use |
|-----------|--------|---------|------------|
| `AgentDriver` trait | `crates/devflow-core/src/agents/mod.rs` | Modular driver interface | Implemented by `HermesDriver` |
| `AgentKind` enum | `crates/devflow-core/src/state.rs` | Agent enumeration | Extended with `AgentKind::Hermes` |
| `parse_marker_lines` | `crates/devflow-core/src/agent_result.rs` | Marker parsing | Used for process-exit DEVFLOW_RESULT parsing |
| `doctor_checks` | `crates/devflow-cli/src/commands.rs` | Environment diagnostics | Added `hermes` presence probe |
| `unattended_launch_shape_condition` | `crates/devflow-cli/src/preflight.rs` | Auto-mode preflight gate | Updated when ANTG-04 dogfooding requirement is satisfied |

## Package Legitimacy Audit

This phase installs **no new packages**. All Hermes integrations and Antigravity dogfooding validations are process/argv-level and documentation/test fixtures.

## Architecture Patterns

### Modular Driver Pattern (`HermesDriver`)
`HermesDriver` implements `AgentDriver` in `crates/devflow-core/src/agents/hermes.rs`:
- `name()`: returns `"Hermes"`
- `render_prompt(&self, intent)`: calls `crate::prompt::render_claude_style(intent)`
- `build_command(&self, phase, prompt, extra_roots)`: returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])`
- `environment(&self)`: returns `vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]`
- `capabilities(&self)`: returns `DriverCapabilities { subagent_dispatch: hermes_subagent_dispatch_available() }`
- `health(&self, state)`: returns `Ok(())` (presence-only)

### Capability Probe Pattern
`hermes_subagent_dispatch_available()` executes `hermes tools list` and checks if output contains `enabled` and `delegation`. Fails closed (`false`) on non-zero exit or error.

### Conformance Suite Enrollment (5 → 6)
`crates/devflow-core/src/agents/mod.rs` updates `every_driver_passes_the_conformance_suite` to include `Box::new(HermesDriver)` in its 6-element array, and adds `hermes_conformance_enrollment` asserting 7 contract checks.

### Preflight Gate & Dogfooding (ANTG-04)
Once Phase 42 executes successfully under supervised Antigravity (`devflow start --agent antigravity --phase 42 --mode supervise`), Antigravity dogfooding is complete. `preflight.rs`'s `unattended_launch_shape_condition` can be updated to include Antigravity alongside Claude as dogfooded stream agents.

---

## Validation Architecture

### Quick run command
```bash
cargo test -p devflow-core --lib hermes -- --nocapture && cargo test -p devflow-core --lib hermes_conformance_enrollment -- --nocapture && cargo test -p devflow --bin devflow doctor_includes_hermes -- --nocapture && cargo test -p devflow --test phase7_cli hermes -- --nocapture
```

### Full suite command
```bash
cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && cargo test -p devflow --test phase7_cli
```

### Key Negative Controls & Invariants
1. A marker-less `hermes` run must fail closed at commit-gated stages (`Plan`, `Code`).
2. A non-zero exit code from `hermes` must gate the stage.
3. A hung `hermes` process must be detected and killed, not orphaned.
4. An unparseable `hermes tools list` output must fail closed (`subagent_dispatch: false`).
5. The `--print-timeout 60m` and quiet-gap distribution must be observed and documented during the dogfood run.
