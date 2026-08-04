# Superseded phase — Phase 26 (Release-Cut Automation)

Phase 26 was **not shipped**. 9 plans executed, verification 11/11, 763 tests
passing — but two independent review passes found Critical defects in the
release executor (7, then 5 after a fix round: 1 closed / 5 partially-closed /
1 regressed), none of which any test ever caught. Rather than a third fix
round, the operator closed the phase **PARTIAL** on 2026-07-30 and re-opened
its goal as backlog item **999.25**, to be re-attempted as its own future
phase rather than patched further in place.

**Code preserved, not deleted** (operator decision, 2026-07-31): the
executor's ~75 commits remain on `feature/phase-26`, unmerged and untouched.
Treat that branch as a **starting point with known Critical defects**, not a
near-complete implementation — its five open Criticals are one *lifecycle*
defect (no terminal state for the non-success outcome, two code paths owning
one `v{version}` namespace, a printed remediation that re-arms the failure it
reports), not five independent bugs. 999.25's own scoping explicitly treats
this code as reference material only, not something to rebase from.

**What's here:** the pre-execution planning docs that made it onto `develop`
before the phase closed — `26-CONTEXT.md`, `26-DISCUSSION-LOG.md`, and the two
backlog dossiers (`999.25-BACKLOG-DOSSIER.md`, `999.5-BACKLOG-DOSSIER.md`).
The actual execution artifacts (`PLAN.md`/`SUMMARY.md`/`26-REVIEW.md` for the
9 executed plans) exist only on `feature/phase-26` and were never merged, so
they are not present here or anywhere on `develop`.

**Live successor:** `ROADMAP.md`'s **999.25** backlog entry is where this work
continues, gated on 999.39 (closed, Phase 27). See that entry for the current
disposition. Do not treat anything in this directory as an implementation to
resume — it is provenance for why Phase 26 didn't ship.

**Moved here** 2026-08-04, during the retroactive v1.0/v2.0.0 milestone
archival — Phase 26 doesn't belong in a "shipped" milestone archive, but it's
also not active work, so it follows this project's existing `superseded/`
convention (see `../23-end-to-end-dogfood/README.md` for the precedent).
