# Phase 37: Modular Agent Driver Architecture + Pi Driver - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-15
**Phase:** 37-modular-agent-driver-architecture-pi-driver
**Areas discussed:** 999.94 scope, Pi success bar, second native driver, AgentAdapter removal, conformance suite, Codex/OpenCode preservation, Antigravity

---

## 999.94 scope

| Option | Description | Selected |
|--------|-------------|----------|
| In phase 37 | Ride along with the migration | |
| Defer to 38 / 37.1 | Keep out of 37 | ✓ |

**User's choice:** Push 999.94 to Phase 38, or possibly 37.1.
**Notes:** Orthogonal to the driver migration.

## Pi success bar

| Option | Description | Selected |
|--------|-------------|----------|
| Full Claude parity | `devflow start --agent pi` does everything Claude does for a run | ✓ (aspirational) |

**User's choice:** Aspirational goal is full Claude parity — "use Pi to do everything Claude can
currently do for a Devflow run." If too big, split the delivery.
**Notes:** Later refined to: parity is deferred to 37.1/38; 37 delivers the migration core.

## Second native driver

| Option | Description | Selected |
|--------|-------------|----------|
| Claude or OpenCode | 999.31 D-02's original answer | |
| Pi | Priority new model | ✓ |

**User's choice:** Pi is the second native driver, taking priority over any earlier
Codex/Antigravity decisions.
**Notes:** Corrected again in a later turn — see "Codex/OpenCode preservation" below. The final
shape: Claude preserved (baseline), Pi second native driver (deferred end-to-end), Codex third
(opportunistic), OpenCode migrated thin.

## AgentAdapter removal

| Option | Description | Selected |
|--------|-------------|----------|
| This phase | Remove as part of the migration | |
| Conditional | Remove if required for Pi, else defer | ✓ |

**User's choice:** Remove it if it's required to be able to use Pi; otherwise agnostic —
"whatever's easier for the phase."
**Notes:** Captured as D-11 (conditional).

## Conformance suite (31c)

| Option | Description | Selected |
|--------|-------------|----------|
| In phase 37 | Full test_contract + DriverHealth + InteractivityMode | ✓ (if scope allows) |
| Defer | 38 / 37.1 | fallback |

**User's choice:** Ideally part of the phase, but if too much work push to 38 or 37.1.
**Notes:** Elevates in importance once "true decoupling + extensibility" was named a top priority.

## Codex/OpenCode preservation (the re-ordering turn)

**User's choice (verbatim intent):**
- Priority #1: do not lose any existing Claude functionality — all of it must be preserved in the
  new framework.
- The second model to add support for is now **Pi** — over any earlier Codex/Antigravity decisions.
- The third model is **Codex**, if that helps with sequencing.
- Other top priority: the framework truly decouples all agent-specific logic from DevFlow, and is
  flexible enough to add models quickly in future.

**Notes:** Codex is mostly broken today (the slash-command issue — which is what precipitated the
migration). OpenCode is least important (no longer used). Don't throw away existing Codex/OpenCode
code if it's a useful foundation; if so, willing to defer Pi for a cleaner migration. Agent scouted
the code: the adapters are thin (27–288 lines), the real per-agent logic is scattered in
`prompt.rs`/`agent_result.rs`/`preflight.rs`/`pipeline_launch.rs` — so the existing code IS a useful
foundation (raw material to relocate), and StageIntent is the shared prerequisite for both
preserving existing agents and unlocking Pi. Resolution: defer Pi's *end-to-end* (the expensive
JSON-unwrapper + CloseRule tail), not Pi's contract migration.

## Antigravity

**User's choice:** Also want to migrate / add support for antigravity-cli (if none exists) — low
priority, just don't lose it.
**Notes:** Verified no existing Antigravity adapter in the code (only "Antigravity review" prose
references). Recorded as D-07 / Deferred → 999.32.

## the agent's Discretion

- `AgentAdapter` removal timing (D-11) — user said "whatever's easier."
- Conformance suite in/out (D-10) — user deferred to scope judgment.
- Whether 999.94/end-to-end land as a 37.1 sub-phase vs Phase 38.

## Deferred Ideas

- Pi end-to-end (JSON unwrapper + CloseRule) → 37.1 / 38.
- 999.94 → 38 / 37.1.
- Antigravity-cli → 999.32.
- Hermes → 999.1.
