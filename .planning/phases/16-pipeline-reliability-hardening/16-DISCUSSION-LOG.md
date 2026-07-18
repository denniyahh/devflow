# Phase 16: Pipeline Reliability Hardening - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-17
**Phase:** 16-pipeline-reliability-hardening
**Areas discussed:** 16d/16e Review pipeline, 16c/16i Deterministic checks
**Areas offered but not selected:** 16a Verification contract, 16b/16h Capture history

---

## 16d/16e — Review Pipeline

### Q1: Where should the multi-angle Ship review logic live?

| Option | Description | Selected |
|--------|-------------|----------|
| Prompt-level flags | Ship prompt invokes review skill at deep effort; skill owns fan-out | |
| DevFlow-orchestrated | DevFlow spawns N parallel reviewers and merges REVIEW.md itself | |
| Hybrid | DevFlow prompt dictates angles; agent spawns subagents itself | |
| Adaptive hybrid, conditional | Shared prompt: angle list + "parallel if supported, else sequential focused passes" | ✓ |
| Adaptive hybrid, per-adapter | stage_prompt takes agent kind, emits explicit per-adapter variants | |

**User's choice:** Adaptive hybrid, conditional — reached after two clarifying rounds.
**Notes:** User probed (a) cons of the hybrid approach and whether subagent
support is universal (it isn't — Claude Code has first-class subagents,
Codex CLI lacks a primitive, OpenCode partial; a shared prompt degrades
silently), and (b) whether hybrid could adapt fan-out to harness capability.
Resolution: capability-conditional instruction degrades gracefully on every
adapter with zero new process management; sequential narrow passes preserve
recall better than one broad generalist pass.

### Q2: Angle list for the Ship deep review

| Option | Description | Selected |
|--------|-------------|----------|
| 4 named + generalist | Incident-derived angles + general deep pass as safety net | |
| Just the 4 named | Cheapest, most targeted; no finder for novel categories | |
| Config-extensible list | 4+generalist defaults, overridable via config | ✓ |

**User's choice:** Config-extensible list.
**Notes:** Triggered the config-mechanism question below.

### Q3: Extensibility mechanism vs the no-config-file decision

| Option | Description | Selected |
|--------|-------------|----------|
| Env var override | DEVFLOW_REVIEW_ANGLES; matches existing knob idiom | |
| CLI flag on start | --review-angles per run; explicit but forgettable | |
| Reopen the config-file decision | Introduce a real config file; deliberate reversal of Phase 11/15a | ✓ |

**User's choice:** Reopen the config-file decision.
**Notes:** Conflict with the prior decision (config.rs doc comment,
.devflow.yaml decoy removal in 15a, PROJECT.md shelving) was surfaced
explicitly before the choice was made.

### Q4: Scope of the reintroduced config file

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal: new knobs only | devflow.toml, Phase 16 knobs only; env > file > default | ✓ |
| New knobs + migrate env vars | Also fold ~10 DEVFLOW_* knobs in; bigger blast radius | |
| Full pipeline configurability | The shelved milestone-sized vision | |

**User's choice:** Minimal: new knobs only.

**16e per-wave gating:** Not discussed — user moved to next area; left to
planner's discretion.

---

## 16c/16i — Deterministic Checks

### Q1: Enforcement point

| Option | Description | Selected |
|--------|-------------|----------|
| Cargo tests in CI | #[test] fns; run locally, in CI, and inside Code stage via agent's own test run | ✓ |
| devflow hook at Code stage | Built-in hook; structured loop-back, but DevFlow-runs-only coverage | |
| Both | Tests + hook wiring; duplicated wiring | |

**User's choice:** Cargo tests in CI — after requesting a full pros/cons
comparison (no prior opinion held).
**Notes:** Key argument: the agent already runs cargo test during Code, so
tests get Code-stage enforcement for free; hook machinery is itself a
reliability risk in a hardening phase; agent-skips-tests-and-lies is 16a's
job to catch, not these checkers'.

### Q2: What the doc-claim checker verifies

| Option | Description | Selected |
|--------|-------------|----------|
| Existence + pinned claims | Generic identifier cross-ref + hand-pinned value/behavior assertions | ✓ |
| Existence-only | Zero false positives, but misses the RUST_LOG default-value incident class | |
| Existence + value extraction | Generic prose parsing; false-positive risk erodes checker trust | |

**User's choice:** Existence + pinned claims.
**Notes:** Surfaced honestly that the motivating incident (RUST_LOG
info-vs-ERROR) is semantic and invisible to existence checking.

### Q3: False-positive handling

| Option | Description | Selected |
|--------|-------------|----------|
| Scoped scan + allowlist file | Operator docs only; checked-in allowlist with per-entry reasons | ✓ |
| Inline ignore annotations | Markers next to exempted mentions; litters docs | |
| No exceptions | Purest; first false positive forces weakening under pressure | |

**User's choice:** Scoped scan + allowlist file.

### Q4: Directionality

| Option | Description | Selected |
|--------|-------------|----------|
| Bidirectional | Docs→source AND source→docs/.gitignore (16i is this direction) | ✓ |
| Docs→source only (+16i separately) | Lighter; next DEVFLOW_* var could ship undocumented | |

**User's choice:** Bidirectional.

---

## Claude's Discretion

- 16e per-wave review depth and gating semantics.
- 16a verification-contract details, 16b/16h capture retention layout and
  history surfacing, 16f/16g fixes — planner works from the scoping doc.

## Deferred Ideas

- Full pipeline configurability via devflow.toml — remains shelved.
- Migrating existing DEVFLOW_* env vars into devflow.toml — future phase.
- Promoting 16c/16i tests to a Code-stage hook — only if dogfooding shows
  agents skipping the test suite.
