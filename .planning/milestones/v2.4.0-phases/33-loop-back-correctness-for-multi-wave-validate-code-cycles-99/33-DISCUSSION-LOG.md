# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-04
**Phase:** 33-loop-back-correctness-for-multi-wave-validate-code-cycles-999-65-999-66
**Areas discussed:** Reset signal (999.66), Gaps signal (999.65), Fix scope

---

## Reset signal (999.66)

| Option | Description | Selected |
|--------|-------------|----------|
| Code commit set changed | Record HEAD/commit set at the moment Validate fails; reset only if Code produced new commits since then. Matches the roadmap's own candidate direction. Requires a new persisted State field. | |
| Validate's finding content changed | Hash/compare Validate's reported findings/reason text across loop-backs; reset only if findings differ from the immediately preceding failure. More precise but couples to Validate's free-text output shape. | |
| Not sure — research and recommend | Let the phase researcher lay out concrete options against the actual code paths and recommend one before planning locks it in. | ✓ |

**User's choice:** Not sure — research and recommend.
**Notes:** Deferred to `gsd-phase-researcher` per CONTEXT.md D-03. Hard constraint carried forward regardless of which option research recommends: do not default to "reset on every loop-back" — that reintroduces 18d's original unreachable-ceiling bug in a different form.

---

## Gaps signal (999.65)

| Option | Description | Selected |
|--------|-------------|----------|
| File existence only | No `{N}-VERIFICATION.md` → plain `/gsd-execute-phase {N}`. File exists → `--gaps-only`, unchanged. Matches the phase's success criteria literally, no VERIFICATION.md content parsing needed. | ✓ |
| File existence + finding-type parsing | Also parse VERIFICATION.md's findings to confirm they're gap-shaped. More robust against a stale/partial file, but adds a content-format dependency the loop-back code doesn't have today. | |

**User's choice:** File existence only (recommended option).
**Notes:** None — direct selection of the recommended option.

---

## Fix scope

| Option | Description | Selected |
|--------|-------------|----------|
| Only the 3 Validate-outcome sites | `handle_validate_outcome`'s 3 `FixType::GapsOnly` call sites get the new mid-arc-vs-gaps check. `handle_ship_outcome`'s Ship→Code loop-back site is left untouched — by the time Ship runs, the phase is by definition not mid-arc. | ✓ |
| All 4 sites, for one consistent decision path | Route every GapsOnly-selecting call site through the same check, even though Ship's case is expected to always resolve to gaps-only in practice. | |

**User's choice:** Only the 3 Validate-outcome sites (recommended option).
**Notes:** Ship→Code loop-back left as a deferred idea in case a future dogfood run surfaces a real defect there.

---

## Claude's Discretion

- Exact shape of any new `State` field(s) D-03's eventual heuristic requires (naming, type, serde defaulting for older persisted state).
- Whether `handle_validate_outcome`'s `FixType` enum needs a new variant versus branching upstream of the `loop_back_to_code` call while keeping `FixType::GapsOnly`/`AuditFix` as-is.

## Deferred Ideas

- 999.73 / 999.74 (stream-json coverage, Validate trust boundary) — already split into Phase 34 by an existing ROADMAP.md decision, not re-discussed here.
- Whether `handle_ship_outcome`'s Ship→Code loop-back (`pipeline_outcomes.rs:321`) has its own latent defect — out of scope for Phase 33 (see Fix scope above).
