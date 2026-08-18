# Phase 38: Driver Contract Completion (999.106 + 999.107) — Plan

**Planned:** 2026-08-17
**Source:** `38-CONTEXT.md` (D-01..D-05)
**Package under test:** `devflow` (binary-only) + `devflow-core` (lib)

## Objective

Delete the legacy `AgentAdapter` surface and finish the `AgentDriver` migration, wire the
driver-driven `InteractivityMode` gate, and fix the two Codex-parser defects — zero regression
on Claude (the baseline).

## Waves

### Wave 1 — driver-native dispatch seam (unblocks everything else)
- **T-38-01** Add `pub fn driver_for(kind: AgentKind) -> Box<dyn AgentDriver>` in
  `agents/mod.rs`, mirroring `adapter_for`'s match over the four `*Driver` unit structs.
  Keep `adapter_for` in place until every caller has migrated (no dead code until Wave 3).

### Wave 2 — migrate the call sites (five named + two ClaudeAgent inherent methods)
- **T-38-02** `canary.rs` — `use crate::agents::{AgentAdapter, ClaudeAgent}` →
  `crate::agents::ClaudeDriver`; `ClaudeAgent.exec_command(…)` → `ClaudeDriver.build_command(…)`.
- **T-38-03** `test_support.rs` — `AlwaysFailAdapter`/`FailOnceAdapter` re-implement
  `AgentDriver` (`preflight` → `health`, `exec_command` → `build_command`, `render_prompt`
  kept, `name` kept; `completion_signal_detected` dropped since `AgentDriver` has no such
  method — the driver surface uses `parse_completion`).
- **T-38-04** `preflight.rs` — `run_preflight(…, adapter: &dyn AgentAdapter)` →
  `&dyn AgentDriver` with `adapter.preflight(state)` → `driver.health(state)`; and
  `agent_program()`: `adapter_for(agent).exec_command(…).0` → `driver_for(agent).build_command(…).0`.
- **T-38-05** `pipeline_launch.rs` — `launch_stage_inner` + `resolve_launch_shape`:
  `adapter_for` → `driver_for`, `&dyn AgentAdapter` → `&dyn AgentDriver`, `exec_command` →
  `build_command`. Relocate `ClaudeAgent::exec_command_single_document` →
  `ClaudeDriver::exec_command_single_document` (`:208` call) and
  `ClaudeAgent::exec_resume_command` → `ClaudeDriver::exec_resume_command` (`:1048` call),
  byte-for-byte argv.

### Wave 3 — InteractivityMode consumption (999.106 fold-in)
- **T-38-06** `commands.rs:289` pre-start leg + `preflight.rs:613`
  `preflight_interactivity_check` → both gate on
  `driver_for(agent).interactivity_mode(stage)` (refuse `RequiresExistingArtifact` /
  `RequiresTypedSubagents` / `InteractiveOnly` where the artifact is absent), covering Define
  **and** Plan per D-03.

### Wave 4 — delete the legacy surface (999.106)
- **T-38-07** Remove `AgentAdapter` trait, `DriverShim`, `adapter_for`, and the four legacy
  `*Agent` structs (`claude.rs` `ClaudeAgent`, `codex.rs` `CodexAgent`, `opencode.rs`
  `OpenCodeAgent`, `pi.rs` `PiAgent`) once no caller references them. Update any remaining
  `pub use` / tests that referenced the deleted items (including reason-string greps per the
  repo's own lesson: a symbol search misses tests that reference a deleted item through
  strings).

### Wave 5 — Codex-parser defects (999.107)
- **T-38-08** `agent_result.rs` — terminal `turn.failed` must take precedence over an earlier
  `agent_message` success marker; add the negative test `success marker + turn.failed` → not-Success.
- **T-38-09** `codex.rs` — harden `writable_roots` serialization against non-UTF-8 and
  control/newline-containing paths; add a hostile-path fixture.

## Acceptance

- `cargo test -p devflow --bin devflow` — a real `N passed` (assert on non-zero `passed`, not
  exit code alone; the repo's `--exact`-empty-match footgun).
- `cargo test -p devflow-core --lib` — green.
- `grep` over `crates/**/*.rs` for `AgentAdapter` / `DriverShim` / `adapter_for` /
  `ClaudeAgent` / `CodexAgent` / `OpenCodeAgent` / `PiAgent` returns only comments/history, no
  code references (strip comments before counting).
- Claude launch path byte-identical: existing tests in `agents/mod.rs`
  (`drivers_reproduce_legacy_adapter_behavior`, `claude_launches_headless_stream_json_without_positional_prompt`,
  `codex_wraps_prompt_in_exec_and_json`) still pass after the trait deletion.
