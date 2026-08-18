# 37-04 Summary — Conformance suite + interactivity + docs; AgentAdapter deferred

**Plan:** 37-04 (wave 4) — `test_contract()`, `DriverHealth`/`InteractivityMode`, docs, and the
conditional `AgentAdapter` removal.
**Status:** complete — the removal was deferred (see below), the rest landed.

## What landed
- **`test_contract()`** — every driver now returns the shared conformance suite
  (`contract_checks()` in `agents/mod.rs`): non-empty name, completion contract rendered at all five
  stages, and a non-empty `build_command` program. `every_driver_passes_the_conformance_suite`
  asserts all four drivers pass. This is the extensibility proof (CONTEXT D-02/D-07) — a future
  Antigravity/Hermes driver plugs in by passing it.
- **`InteractivityMode`** (`HeadlessSafe` / `RequiresExistingArtifact` / `RequiresTypedSubagents` /
  `InteractiveOnly`) + `interactivity_mode(stage)` on the driver. `CodexDriver` declares Define/Plan →
  `RequiresExistingArtifact` (the 13-06 headless-interview finding); `codex_define_and_plan_require_an_existing_artifact`
  pins it. **`DriverHealth`** (`BinaryAbsent` / `NotHeadlessCapable` / `HeadlessCapable`) +
  `health_classification()`.
- **Docs de-Claude-ification** — `README.md`, `docs/architecture/agent-model.md`,
  `docs/guides/adding-agent.md` (rewritten around the `AgentDriver` contract), `ARCHITECTURE.md`: no
  remaining "same/identical prompt for all agents" claim; prompt rendering is documented as
  driver-owned.

## Deferred — `AgentAdapter` removal (CONTEXT D-11, conditional)

All four agents implement `AgentDriver`, but the legacy `AgentAdapter` trait + `DriverShim` +
the four adapter structs were **left in place** per D-11 ("remove only if required for Pi; otherwise
defer — whatever's easiest"). Pi runs through the shim, so removal was not required. Recorded as an
explicit follow-up: **`999.106`** in ROADMAP (enumerating `canary.rs:40`, `test_support.rs:205/244`,
`preflight.rs:1266`, `pipeline_launch.rs:190/204`), which also folds in the `InteractivityMode`
*consumption* (the hardcoded `agent == Codex` checks in `commands.rs:289` / `preflight.rs` still gate
Define/Plan — wiring them through `interactivity_mode` needs the `&dyn AgentAdapter` → `&dyn AgentDriver`
signature migration).

## Verification
- `cargo test -p devflow-core --lib`: **628 passed, 0 failed** (added the conformance + interactivity tests).
- `cargo test -p devflow --bin devflow`: **322 passed, 0 failed**.
- `cargo clippy -p devflow-core -p devflow --all-targets -- -D warnings`: clean; `cargo fmt --check`: clean.
