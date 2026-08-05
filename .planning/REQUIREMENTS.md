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

- [ ] **DOGFOOD-03**: Operator can trust that every stage DevFlow launches through the stream-json
  path was put there on real per-stage behavioural evidence; that any stage not yet evidenced is
  visibly and deliberately still on the legacy path, never silently assumed to work; and that the
  phase moved this forward — **at least one stage is newly widened on a newly captured run, or the
  operator is explicitly told why none could be** (999.73)

  > **Reworded 2026-08-05 — a modelling correction. Amended the same day after a second review
  > pass found the first reword was a coverage relaxation described as a tightening.**
  >
  > The original text read "Define/Plan/Validate/Ship stages launch through the same reliable
  > stream-json path already proven for Code, backed by real per-stage captures, not synthetic
  > fixtures." That named four specific stages, making it an *implementation plan* rather than an
  > operator-facing guarantee — the only requirement here phrased that way. The practical
  > consequence was that a partial delivery had nowhere to land: this file models requirements as
  > checkboxes with no partial state, and Phase 34 is this milestone's last phase.
  >
  > **What the first reword got wrong, stated plainly.** It was described as "stricter, not
  > looser." That is true on the evidence axis and **false on the coverage axis**, and the second
  > characterisation was omitted. The weakest conforming delivery under the first reword was: widen
  > *zero* stages, record four "not evidenced" reasons — satisfying criteria 1, 2 and 7 vacuously,
  > since nothing widened means nothing to evidence and no collateral to fix. The requirement's
  > subject had an extension the delivery itself determined, so shrinking the set satisfied it.
  > That is not what an operator who approved the original was agreeing to.
  >
  > **Two repairs, both operator-decided 2026-08-05.** (1) The delivery floor above — the phase
  > must actually move the rollout forward or say why it cannot, so a zero-widening close is an
  > escalation rather than a silent pass. (2) The "visibly and deliberately" clause is now carried
  > in Phase 34's binding success criterion 1, not only in `34-CONTEXT.md` — which disclaims its
  > own bindingness, so the whole strictness argument previously rested on a non-binding document.
  >
  > **Known gap this requirement now owns, and Phase 34 must answer.** This clause quantifies over
  > *every* stage on the stream path, which today means `Stage::Code` — whose raw capture was
  > deleted during Phase 31's cleanup and never committed (`31-VERIFICATION.md`). No real
  > production stream capture exists in-repo. Code must therefore be re-captured, or its
  > transcription-only evidence recorded as such, before this box is ticked. Full reasoning:
  > `34-REVIEW.md`.

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
