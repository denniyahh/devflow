# Requirements: Loop-Termination and Release Hardening

**Defined:** 2026-08-06
**Core Value:** A developer should be able to run `devflow start --phase N` and walk away —
DevFlow must reliably drive the agent through the full pipeline and never silently corrupt its
own state or lose a human's gate decision, even under a mid-run crash or kill.

**Note:** all six requirements below are pre-existing, already-diagnosed internal defects or
measurement gaps — not new user-facing capability. No domain research was run for this milestone
(see PROJECT.md's "Current Milestone" section for why). Fix directions are already sketched in
each backlog entry in `.planning/ROADMAP.md`. Scope agreed with the operator in conversation before
this milestone was declared, confirmed rather than re-derived during questioning.

## v1 Requirements

Requirements for this milestone. Each maps to a roadmap phase.

### Loop-Termination Hardening

- [ ] **HARDEN-01**: Operator can trust `consecutive_failures` reflects real repeated failure, not
  a single transient `git` hiccup — the counter's baseline is not silently overwritten by a
  measurement failure (999.77)

- [ ] **HARDEN-02**: Operator can trust an unattended Code↔Validate loop has a bound independent
  of trivial per-cycle commits (GSD commands routinely commit `.planning/` artifacts even when
  nothing source-level changed), and the Supervise-mode gate message reports a real cumulative
  total rather than a streak length that resets misleadingly low (999.78)

- [ ] **HARDEN-03**: Operator can `--force` re-run a phase without inheriting the previous run's
  stale `VERIFICATION.md`, which today causes the loop-back to dispatch `--gaps-only` against a
  mid-arc phase and gate unresolvably (999.79)

- [ ] **HARDEN-04**: Operator can trust the worktree-mode `GateReview` checkpoint auto-decide path
  (999.76) is covered by a regression test that would catch a future regression at the call site,
  not just correct by construction with no test driving it (999.84)

- [ ] **HARDEN-07**: Operator can trust that a transient `git` failure does not make a *successful*
  agent run read as failed — "could not count" is distinguished from "counted zero" at **both**
  consumers of the commit count, not only at the `consecutive_failures` baseline (999.87). Added
  2026-08-06 by operator decision, after the 999.77 fix was found to force `evaluate_layer2`'s call
  site open at compile time; see ROADMAP Phase 35 criterion 6.

### Release Hardening

- [ ] **HARDEN-05**: Operator can trust `release --check`'s tag-signing preflight reflects whether
  signing will actually work — via a real `ssh-keygen -Y sign` probe on a throwaway payload,
  rather than a fingerprint-matching predictor that has now false-negatived live during two
  separate release cuts with the correct key present (999.86)

## v2 Requirements

### Drain Gate Concurrency Measurement

- [ ] **HARDEN-06**: Operator can trust the drain gate's guarantee — the safety net the v2.4.0
  stream-json widening depends on — has actually been measured against real sub-agent
  concurrency. Split into its own phase deliberately: this is investigation-shaped work (design
  the right experiment, same family as backlog 999.71's precedent) rather than a quick patch, and
  bundling it with HARDEN-01..05 would slow those down waiting on a harness (999.83)

## Out of Scope

| Item | Reason |
|---|---|
| Every other open `999.x` backlog item | Not required to close the loop-termination and release-preflight gaps specifically — this milestone is scoped to the six items agreed with the operator, not a general backlog sweep. |
| 999.85 (two comments justifying themselves by mechanisms v2.4.0 deleted) | Low severity, no functional risk — the guarded instruction (`verdict: None`) is still correct and enforced; only the stated rationale is stale. Deferred to a future pass. |
| 999.67 (`parse_devflow_result` lets an agent plant its own Layer-0 provenance) | Bounded blast radius by the entry's own analysis — it can flip a Validate `Failed` to `Ambiguous`, which still gates; it cannot manufacture a pass. Does not threaten completion the way the six selected items do. |
| 999.61 (four indirect git-reaching spawn edges) | Environment-redirection concern in operator-configured commands (`gates.rs`/`verify.rs`), not a completion blocker. Already downgraded from High once the one high-consequence site (agent spawn) closed in Phase 27. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| HARDEN-01 | Phase 35 | Pending |
| HARDEN-02 | Phase 35 | Pending |
| HARDEN-03 | Phase 35 | Pending |
| HARDEN-04 | Phase 35 | Pending |
| HARDEN-05 | Phase 35 | Pending |
| HARDEN-07 | Phase 35 | Pending |
| HARDEN-06 | Phase 36 | Pending |

**Coverage:**

- v1 requirements: 6 total (HARDEN-01..05, HARDEN-07)
- v2 requirements: 1 total (HARDEN-06)
- Mapped to phases: 7
- Unmapped: 0 ✓

---
*Requirements defined: 2026-08-06*
*Last updated: 2026-08-06 after initial definition*
