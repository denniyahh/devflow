# Phase 38: Driver Contract Completion (999.106 + 999.107) - Context

**Gathered:** 2026-08-17 (headless auto-discuss; grounded in the live source, Phase 37's
CONTEXT, and the Phase-38 review evidence in `.planning/reviews/phase-38/`)
**Status:** Ready for planning

## Phase Boundary

Phase 38 finishes the `AgentDriver` migration that Phase 37 deliberately left open (37 D-11
"conditional removal"): it **deletes** the legacy `AgentAdapter` trait, the `DriverShim`
compatibility shim, and the four legacy adapter structs (`ClaudeAgent`/`CodexAgent`/
`OpenCodeAgent`/`PiAgent`), migrates every remaining call site onto `AgentDriver`, wires the
`InteractivityMode` *consumption* (a driver-driven Define/Plan gate replacing the hardcoded
`AgentKind::Codex` checks), and fixes the two pre-existing Codex-parser defects (999.107).

The phase does **not** land: Pi end-to-end (Phase 39), Antigravity (999.32), Hermes (999.1),
999.94, or any new driver. It is a *relocation + deletion*, not a rewrite — the same
relocation-not-rewrite rule Phase 37 D-02 established.

## Implementation Decisions

### Removal scope (999.106)
- **D-01 — `AgentAdapter`, `DriverShim`, and the four legacy `*Agent` structs are deleted.**
  The driver-native accessor `adapter_for(kind) -> Box<dyn AgentAdapter>` is replaced by
  `driver_for(kind) -> Box<dyn AgentDriver>`. The four `*Driver` unit structs
  (`ClaudeDriver`/`CodexDriver`/`OpenCodeDriver`/`PiDriver`) already implement the full
  `AgentDriver` contract and are the sole survivors. Claude remains the zero-regression
  baseline (carried forward from 37 D-01). — **Reversibility:** one-way — keeping both traits
  across phases is the exact drift 999.31 D-04 / 37 D-11 warned about; this is the removal
  that was deferred, not a new experiment.

### Call-site migration (the five named sites, plus two ClaudeAgent inherent methods 999.106 didn't enumerate)
- **D-02 — Migrate every `&dyn AgentAdapter` / `adapter_for` / `*Agent` use to the driver.**
  The named sites (verified 2026-08-16 in 999.106, re-verified here):
  1. `crates/devflow-core/src/canary.rs:40/285` — `use ...{AgentAdapter, ClaudeAgent}` and
     `ClaudeAgent.exec_command(…)` → `driver_for(AgentKind::Claude).build_command(…)`.
  2. `crates/devflow-cli/src/test_support.rs:205/247` — `AlwaysFailAdapter`/`FailOnceAdapter`
     test doubles re-implement `AgentDriver` (their `preflight` becomes `health`).
  3. `crates/devflow-cli/src/preflight.rs:1266` — `run_preflight(…, adapter: &dyn AgentAdapter)`
     → `&dyn AgentDriver`; the `adapter.preflight(state)` call becomes `driver.health(state)`.
  4. `crates/devflow-cli/src/preflight.rs:85` — `agent_program()`:
     `adapter_for(agent).exec_command(…).0` → `driver_for(agent).build_command(…).0`.
  5. `crates/devflow-cli/src/pipeline_launch.rs:90/194` — `launch_stage_inner` +
     `resolve_launch_shape`: `adapter_for` → `driver_for`, `&dyn AgentAdapter` → `&dyn
     AgentDriver`, `exec_command` → `build_command`.

  **Two sites 999.106 did not name but must be handled before the structs can be deleted** —
  `ClaudeAgent` carries two *inherent* methods that have no `AgentDriver` counterpart and are
  live in the launch path; both must be relocated onto `ClaudeDriver` (or a free function):
  - `ClaudeAgent::exec_command_single_document` — `pipeline_launch.rs:208`, the D-11 legacy
    opt-out launch (`DEVFLOW_CLAUDE_LEGACY_LAUNCH`). This path must survive byte-for-byte.
  - `ClaudeAgent::exec_resume_command` — `pipeline_launch.rs:1048`, the checkpoint-resume
    launch. Also must survive byte-for-byte.
  These two relocations are the regression-sensitive heart of the phase: they are the only
  places where "delete the struct" is not a pure `adapter_for`→`driver_for` substitution.

### InteractivityMode consumption
- **D-03 — Replace both hardcoded `agent == AgentKind::Codex` gates with the driver's own
  declaration.**
  - `crates/devflow-cli/src/commands.rs:289` (pre-start, hard error) and
    `crates/devflow-cli/src/preflight.rs:613` (`preflight_interactivity_check`) currently
    special-case Codex. Both become agent-agnostic: gate on
    `driver_for(agent).interactivity_mode(stage)` returning `RequiresExistingArtifact` (and,
    where the stage has no answer at all, `InteractiveOnly`/`RequiresTypedSubagents`).
  - Codex already declares Define **and Plan** → `RequiresExistingArtifact` (37). The current
    gates only cover Define. **Decision: the driver-driven gate honors the driver's full
    declaration, i.e. it extends to Plan** — that is the whole point of making the gate
    driver-driven, and it is what "Define/Plan path" in 999.106 names. A driver that declares
    HeadlessSafe for a stage is never refused.

### Codex-parser defects (999.107)
- **D-04 — Success must not beat a later terminal failure.** `agent_result.rs:764-781`
  currently returns the last `agent_message` `DEVFLOW_RESULT` marker *before* examining the
  terminal `turn.completed`/`turn.failed` event at `:784-812`. Reorder so a terminal
  `turn.failed` takes precedence over any earlier success marker; the negative test is
  `success marker + turn.failed` → not-Success (the existing coverage only tests
  `success + turn.completed`).
- **D-05 — Writable-root serialization must survive hostile paths.** `codex.rs:47-60` uses
  `root.display().to_string()` with only `\`/`"` escaping: a non-UTF-8 path becomes `�` and a
  newline/control-containing path yields malformed TOML for `sandbox_workspace_write.writable_roots`.
  Hardening + a hostile-path fixture (non-UTF-8 and newline cases) are required.

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope origin
- `.planning/ROADMAP.md` — `### Phase 999.106` (removal + call sites + InteractivityMode fold-in)
  and `### Phase 999.107` (the two parser defects), and the active-milestone `### Phase 38` goal.
- `.planning/milestones/v2.6.0-phases/37-modular-agent-driver-architecture-pi-driver-999-31-pi/37-CONTEXT.md`
  — the `AgentDriver` contract this phase completes; D-11 conditional removal; D-01/D-02.
- `.planning/reviews/phase-38/SUMMARY.md` — finding 5 (claude J): `interactivity_mode` is
  unreachable because `DriverShim` doesn't forward it and `adapter_for` returns
  `Box<dyn AgentAdapter>` — the precise gap this phase closes.

### The trait/shim surface to delete
- `crates/devflow-core/src/agents/mod.rs` — `AgentAdapter` trait, `DriverShim`, `adapter_for`,
  `AgentDriver` trait, `InteractivityMode`, `DriverHealth`, `contract_checks`.

### Per-agent legacy structs + drivers
- `crates/devflow-core/src/agents/claude.rs` — `ClaudeAgent` (legacy, incl.
  `exec_command_single_document` / `exec_resume_command`) vs `ClaudeDriver`.
- `crates/devflow-core/src/agents/{codex,opencode,pi}.rs` — the other three pairs.

### Call sites
- `crates/devflow-core/src/canary.rs` (`:40`, `:285`)
- `crates/devflow-cli/src/test_support.rs` (`:205`, `:247`)
- `crates/devflow-cli/src/preflight.rs` (`:85`, `:613`, `:1266`)
- `crates/devflow-cli/src/pipeline_launch.rs` (`:90`, `:194`, `:208`, `:1048`)
- `crates/devflow-cli/src/commands.rs` (`:289`)

### Parser defects
- `crates/devflow-core/src/agent_result.rs` — `parse_codex_event_result` (`:764-781` defect).
- `crates/devflow-core/src/agents/codex.rs` — `build_command` writable-root block (`:47-60`).

## Existing Code Insights

### Reusable Assets
- `AgentDriver` trait + all four `*Driver` impls already exist and pass `test_contract` (37).
- `DriverShim` is the *only* remaining bridge to the legacy surface — deleting it is the whole
  delete; the drivers underneath are already complete.
- `InteractivityMode` enum already exists with `HeadlessSafe` / `RequiresExistingArtifact` /
  `RequiresTypedSubagents` / `InteractiveOnly`; Codex already declares the per-stage values.

### Established Patterns
- Drivers are zero-field unit structs; `driver_for()` mirrors `adapter_for()`'s match shape.
- The migration is relocation, not rewrite (37 D-02): argv, env, health, parsing move under
  driver ownership, byte-for-byte for the launch paths.

### Integration Points / risks
- `pipeline_launch.rs` is the most regression-sensitive code in the repo (per 999.106); the
  `exec_command_single_document` and `exec_resume_command` relocations live there.
- `state.rs` `AgentKind` is unchanged; `driver_for(AgentKind)` is the single dispatch seam.
- The `commands.rs:289` pre-start leg runs before any worktree fork — a `driver_for(agent)` call
  must be cheap (it is: unit structs), and the check must stay on `develop`'s artifacts.

## Specific Ideas

No specific requirements — open to standard approaches within the driver-contract design.
The one open implementation choice is whether the two `ClaudeAgent` inherent builders become
`ClaudeDriver` methods or free functions; either is acceptable as long as the emitted argv is
byte-identical to today's.

## Deferred Ideas

- **Pi end-to-end** (JSON-mode unwrapper + `MonitorLaunch`/`CloseRule` integration) → Phase 39.
- **Antigravity-cli** (no existing adapter) → future (999.32).
- **Hermes** → future (999.1).
- **999.94** (unattended `decision` checkpoint first-option guard) → later.
- **999.105** (make adversarial cross-model review a default phase gate) → separate backlog item.

---

*Phase: 38-Driver-Contract-Completion-999.106-+-999.107*
*Context gathered: 2026-08-17 (headless auto-discuss)*
