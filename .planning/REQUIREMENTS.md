# Requirements: Resume Unattended Dogfooding

**Defined:** 2026-08-04
**Core Value:** A developer should be able to run `devflow start --phase N` and walk away —
DevFlow must reliably drive the agent through the full pipeline and never silently corrupt its
own state or lose a human's gate decision, even under a mid-run crash or kill.

**Note:** all four requirements below are pre-existing, already-diagnosed internal defects — not
new user-facing capability. No domain research was run for this milestone (see PROJECT.md's
"Current Milestone" section for why). Fix directions are already sketched in each backlog entry
in `.planning/ROADMAP.md`.

## v1 Requirements

Requirements for this milestone. Each maps to roadmap phases.

### Dogfood Reliability

- [x] **DOGFOOD-01**: Operator can run a 3+ wave unattended phase where Validate correctly
  reports the phase mid-arc/incomplete, without the loop-back issuing an unresolvable
  `--gaps-only` command (999.65)

- [x] **DOGFOOD-02**: Operator can run a 3+ wave unattended phase in `auto` mode without a false
  "3 consecutive failures" gate firing on healthy wave-by-wave progress (999.66)

- [ ] **DOGFOOD-03**: Operator can trust that Define/Plan/Validate/Ship stages launch through the
  same reliable stream-json path already proven for Code, backed by real per-stage captures, not
  synthetic fixtures (999.73)

- [ ] **DOGFOOD-04**: Operator can trust that a Validate stage's reported outcome reflects its
  actually-derived status, not just the agent's self-reported verdict field (999.74)

## v2 Requirements

None identified — this milestone is intentionally narrow (four already-diagnosed defects, no new
scope solicited).

## Out of Scope

| Item | Reason |
|---|---|
| Every other open `999.x` backlog item | Not required to resume unattended multi-wave dogfooding specifically — this milestone is scoped to the four items directly blocking or undermining trust in that goal, not a general backlog sweep. |
| Modular Agent Driver Architecture (999.31) | Fixes a Codex-specific dogfood-breaking defect (raw `/gsd-*` slash commands rendered identically for every adapter); this milestone's dogfooding is Claude-driven. Real, but a different blocker for a different agent. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| DOGFOOD-01 | Phase 33 | Complete |
| DOGFOOD-02 | Phase 33 | Complete |
| DOGFOOD-03 | Phase 34 | Pending |
| DOGFOOD-04 | Phase 34 | Pending |

**Coverage:**

- v1 requirements: 4 total
- Mapped to phases: 4
- Unmapped: 0 ✓

---
*Requirements defined: 2026-08-04*
*Last updated: 2026-08-04 after initial definition*
