# Phase 37: Modular Agent Driver Architecture + Pi Driver - Context

**Gathered:** 2026-08-15
**Status:** Ready for planning

## Phase Boundary

Phase 37 delivers the **migration core**: the `AgentDriver` contract — `StageIntent`
de-Claude-ification plus driver-owned prompt rendering, command building, completion parsing, and
health/capability discovery — and migrates **Claude, Codex, OpenCode, and Pi's thin adapter** onto
it with **zero regression on Claude**. Pi's *end-to-end* support (JSON-mode unwrapper +
monitor/`CloseRule` integration → full Claude parity) is deferred to a `37.1` sub-phase or Phase 38.

The phase does **not** land: Pi running a full `devflow start --agent pi` to terminal completion
(deferred), 999.94 (deferred), Antigravity (future, 999.32), or Hermes (future, 999.1).

## Implementation Decisions

### Priorities & preservation
- **D-01 — Claude is the hard baseline (top priority).** All existing Claude functionality must be
  preserved through the migration. Zero regression is the acceptance bar everything else defers to.
- **D-02 — True decoupling is the north star.** The `AgentDriver` framework must move *all*
  agent-specific logic (prompt rendering, command building, completion parsing, health/capability
  discovery) out of DevFlow core, and be flexible enough to add future models quickly. This is a
  phase-level priority, not just a unit.

### Driver sequencing
- **D-03 — Pi is the second native driver.** Pi gets the full `AgentDriver` treatment and takes
  priority over any earlier Codex/Antigravity sequencing. **Supersedes 999.31 D-02**, which named
  Claude/OpenCode as the second native driver. — **Reversibility:** costly — which agent validates
  the trait shapes the conformance suite's fixtures.
- **D-04 — Pi migrates to the contract in 37, but Pi's *end-to-end* is deferred.** The JSON-mode
  unwrapper and monitor/`CloseRule` integration (the parts that make `devflow start --agent pi`
  complete a run) land in 37.1/38. Within 37 Pi stays on `-p` print mode.
- **D-05 — Codex is third, opportunistic.** Migrating Codex *is* the fix for the existing
  broken slash-command behavior (31a/31b); it rides the core migration rather than being a separate
  priority.
- **D-06 — OpenCode migrates too (thin, cheap).** Existing OpenCode support is not thrown away; it
  migrates to the contract. Lowest priority among existing agents.
- **D-07 — Antigravity-cli: no existing adapter; low-priority future model.** Recorded so it isn't
  lost (999.32). Not in 37's scope; a candidate target for the conformance suite once it lands.

### Scope & split
- **D-08 — Phase 37 = migration core; Pi end-to-end = 37.1/38.** Clean migration of existing
  functionality first; Pi parity is the follow-on. (The StageIntent de-Claude-ification is the
  shared prerequisite for both — preserving Claude/Codex/OpenCode *and* unlocking Pi — so the split
  costs nothing on the core.)
- **D-09 — 999.94 (unattended `decision` checkpoint) deferred** to 38/37.1.
- **D-10 — Conformance suite (31c) in 37 if scope allows**, else defer to 38/37.1. More important
  given D-02's extensibility priority.
- **D-11 — `AgentAdapter` removal conditional.** Remove only if the migration requires it for Pi;
  otherwise defer — whatever's easiest for the phase.

### Carried forward from 999.31 (still valid)
- **D-12 — Capabilities enumerated as-needed** (999.31 D-01): do not guess the full
  `DriverCapabilities` axis upfront; `#[non_exhaustive]` + `Default` for cheap extension.
- **D-13 — Sequence the fix before the framework** (999.31 D-03): `StageIntent` + driver-owned
  rendering ships first; fuller discovery/health/conformance does not block it.

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Migration origin & design
- `.planning/phases/999.31-agent-driver-modularization/CONTEXT.md` — the migration's origin, units
  31a–31d, and original locked decisions (D-01–D-04; note D-02 is superseded by D-03 above).
- `.planning/audits/2026-07-24-codex-compatibility-review.md` — the Codex dogfood failure root cause.

### Scattered logic to consolidate
- `crates/devflow-core/src/prompt.rs` — shared prompt rendering (bakes `/gsd-*` slash commands).
- `crates/devflow-core/src/stage.rs` — `Stage::gsd_command()` to replace with `StageIntent`.
- `crates/devflow-core/src/agent_result.rs:361-453` — Codex JSONL completion parsing.
- `crates/devflow-core/src/agents/{mod,claude,codex,opencode,pi}.rs` — the `AgentAdapter` trait +
  four adapters.
- `crates/devflow-cli/src/preflight.rs` — health checks to move under per-driver `health`.
- `crates/devflow-cli/src/pipeline_launch.rs` — Claude stream-json / `MonitorLaunch` / `CloseRule`
  wiring.

### Prior-phase context
- `.planning/phases/36-pi-agent-support/36-SPEC.md` — Pi adapter (phase 36) + the explicit deferrals
  this phase picks up (StageIntent de-Claude-ification, Pi JSON-mode unwrapper, monitor/`CloseRule`
  integration).

## Existing Code Insights

### Reusable Assets
- `AgentAdapter` trait + four adapters (`claude.rs` 288L, `pi.rs` 246L, `codex.rs` 69L,
  `opencode.rs` 27L) — argv shapes, `extra_env`, `preflight` to relocate under per-driver ownership.
- Codex JSONL completion parsing at `agent_result.rs:361-453` — the seed of `CodexDriver::parse_completion`.
- Claude stream-json launch + `MonitorLaunch`/`CloseRule` in `pipeline_launch.rs` — the seed of
  `ClaudeDriver`; the most complex, regression-sensitive piece.
- `preflight.rs` health checks — the seed of per-driver `health`.

### Established Patterns
- Adapters format the shared stage prompt into CLI flags; Claude is stdin/stream-json, Codex/OpenCode/Pi
  are positional.
- The migration is a *relocation*, not a rewrite — existing per-agent logic (argv, parsing,
  health) is moved under driver ownership, not discarded.

### Integration Points
- `crates/devflow-core/src/state.rs` — `AgentKind` enum.
- `crates/devflow-core/src/agents/mod.rs` — `adapter_for(kind)` dispatch to replace with driver
  selection.
- `crates/devflow-core/src/prompt.rs` — shared rendering to split into per-driver `render_prompt`.
- `crates/devflow-cli/src/pipeline_launch.rs` — `MonitorLaunch` routing (Claude pipe-owning vs legacy).

## Specific Ideas

No specific requirements — open to standard approaches within the driver-contract design.

## Deferred Ideas

- **Pi end-to-end** (JSON-mode unwrapper + monitor/`CloseRule` integration → full Claude parity) →
  37.1 / 38.
- **999.94** (unattended `decision` checkpoint first-option) → 38 / 37.1.
- **Antigravity-cli** (no existing adapter) → future (999.32).
- **Hermes** → future (999.1).
- **Conformance suite (31c)** — only if deferred from 37 → 38 / 37.1.

---

*Phase: 37-Modular-Agent-Driver-Architecture-+-Pi-Driver*
*Context gathered: 2026-08-15*
