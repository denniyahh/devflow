---
phase: 23-end-to-end-dogfood
plan: 15
subsystem: testing
tags: [devflow, dogfood, acceptance-test, gate, evidence-oracle, staleness, gap-closure]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood (23-14)
    provides: "develop fast-forwarded to origin/develop, fresh recovery ref, operator PROCEED authorization against 0dad20d"
provides:
  - "Second real-run acceptance record (23-ACCEPTANCE-RUN-2.md), judged and closed: run-incomplete/valid record, ACCEPTANCE FAILED"
  - "First real-run exercise of the self-dogfood staleness hard block (D-18 Stale/Block path), source-verified as divergent-lineage rather than linear staleness"
  - "Operator-directed next step: a new gap-closure plan 23-16, retrying with a develop-built binary, with 23-14's preconditions re-run against that new binary"
affects: [23-16-gap-closure, release-preflight, self-dogfood-staleness-guard]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Two-outcome acceptance record: RUN RECORD VALID and ACCEPTANCE PASSED judged and declared separately, never blended"
    - "Binary provenance for self-dogfood guards must be built from an ancestor of the target ref (develop), not from the long-lived working branch, or the divergent-lineage Stale path trips regardless of content"

key-files:
  created:
    - .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN-2.md
  modified: []

key-decisions:
  - "Operator verdict: record valid (run-incomplete declaration matches the quoted evidence exactly)."
  - "Operator verdict: acceptance failed (zero workflow_shipped events for phase 24; devflow evidence --phase 24 --require-shipped unchanged at exit 1; the run never reached Define)."
  - "Operator next step: retry with a develop-built binary (a new plan, 23-16), re-running 23-14's preconditions because the binary itself changes."
  - "Recovery-ref disposition: both origin refs (recovery/pre-23-11-acceptance-e0f87c2, recovery/pre-23-15-acceptance-0dad20d) retained untouched on origin; the deleted local copy of the pre-23-11 ref is deliberately not restored per 23-FINDINGS.md SS B2a; the now-unused pre-23-15 ref is retained (not deleted) for reuse by the 23-16 retry."

patterns-established:
  - "A run-incomplete record that names its stop cause precisely and is source-verified is a successful plan execution and a failed acceptance run — the two outcomes are judged and recorded independently."

requirements-completed: [23-acceptance]

coverage:
  - id: D1
    description: "Second acceptance attempt run and recorded end-to-end (launch, freshness re-check, event excerpts, absent-events list, poll series, shipped-oracle verdict, post-run hygiene, git evidence vs. prediction)"
    requirement: "23-acceptance"
    verification:
      - kind: manual_procedural
        ref: ".planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN-2.md"
        status: pass
    human_judgment: false
  - id: D2
    description: "Operator judgment on record validity and acceptance outcome, plus the next-step decision and recovery-ref disposition"
    requirement: "23-acceptance"
    verification: []
    human_judgment: true
    rationale: "The plan's own design requires a human to judge record validity and acceptance separately, and to choose the retry strategy — this cannot be automated by construction."

duration: 17min
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 15: Second Acceptance Run — Blocked by Self-Dogfood Staleness, Operator Judged Summary

**Second real acceptance attempt (`devflow start --phase 24 --agent claude --mode auto --yes-ship`) was blocked at launch by the self-dogfood staleness hard block (D-18) before Define ever ran — a valid, source-verified `run-incomplete` record and a failed acceptance, with the operator directing a 23-16 retry using a `develop`-built binary.**

## Performance

- **Duration:** 17 min (this continuation session: Task 1 commit `30de49d` through Task 3 commit `4efe46a`)
- **Started:** 2026-07-26T22:17:02Z
- **Completed:** 2026-07-26T22:34:16Z
- **Tasks:** 3 (1 tracer + 1 auto + 1 checkpoint:human-verify, all complete)
- **Files modified:** 1 (`23-ACCEPTANCE-RUN-2.md`, created then extended across three commits)

## Accomplishments

- Ran the second real end-to-end acceptance attempt on this repository (Phase 24 target), with the 23f reachability guard now merged and Phase 23 fast-forwarded into `develop`. The reachability guard correctly allowed the launch — the exact condition that killed the first attempt (`23-ACCEPTANCE-RUN.md`) did not recur.
- The run was blocked synchronously, inside the foreground `devflow start` process, by the self-dogfood staleness hard block (D-18, `StalenessOutcome::Block`) — the first time in this project's real-run history that branch has fired, closing the coverage gap `23-FINDINGS.md` §B3 named against the first attempt.
- Source-verified (against `crates/devflow-cli/src/staleness.rs`) that the block fired because the running binary's embedded commit (`0c9dcfe`, built from `feature/phase-23`) and `origin/develop`'s tip (`0dad20d`) are mutually non-ancestors — genuine divergence, not linear staleness — confirmed independently twice: once in Task 1's original record, and again independently at Task 3 (`git merge-base --is-ancestor` both directions, exit 1 each way).
- Computed the shipped-oracle verdict as a pre-run/post-run delta: `devflow evidence --phase 24 --require-shipped` exited `1` both before and after (no change), and no `workflow_shipped` event exists for phase 24 in `.devflow/events.jsonl` — both facts agree: **ACCEPTANCE FAILED**.
- Proved post-run hygiene: zero open gates for phase 24, zero resident phase-24 processes, `feature/phase-24` branch and worktree registration removed by `devflow cleanup` (with the now-thrice-recorded local `recovery/pre-23-11-...` deletion side effect disclosed, not silently repaired).
- Compared the post-run tree against the operator's 23-14 prediction: no merge into `develop`, no version bump (`Cargo.toml` still `1.8.1`), no changelog commit — none of the mechanisms that would have produced either the operator's predicted `1.8.2` or the orchestrator's `~1.11.339` counter-finding ever ran, because the block fired before Define.
- Obtained the operator's two separate verdicts (`record: valid`, `acceptance: failed`), the chosen next step (retry via a new plan 23-16 with a `develop`-built binary, re-running 23-14's preconditions), and the recovery-ref disposition (both `origin` refs retained; local pre-23-11 copy deliberately not restored; the unused pre-23-15 ref retained for the 23-16 retry).
- Folded in two corrections identified during orchestrator re-verification: (a) `.worktrees/` still exists as an empty, inert directory shell — the earlier "worktree cleaned up" characterization overclaimed by omission; left in place, not deleted, since removing it exceeds this plan's observational-only boundary; (b) the resident phase-12 orphan-process/gate population is larger than the Task 2 snapshot captured (grew from 5 to 6 rows/pairs during the session, including `/tmp/.tmpqZmoON` which was absent from an even earlier orchestrator snapshot) — named as the known 23-FINDINGS §A1/§A3 noise class actively accruing, not phase-24 residue.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end acceptance run, launch + observation** - `30de49d` (docs)
2. **Task 2: Shipped oracle, post-run hygiene, git evidence vs prediction** - `28cdda1` (docs)
3. **Task 3: Operator judgment on the acceptance run** - `4efe46a` (docs)

**Plan metadata:** (this commit)

_Note: This plan produces no source changes — every task is a `docs` commit against the acceptance-run record._

## Files Created/Modified
- `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN-2.md` - The full second-attempt acceptance record: launch + freshness re-check, guard-at-launch record, verbatim event excerpts, absent-events list, poll series, stop-cause analysis, shipped-oracle delta, post-run hygiene, git evidence vs. prediction, gate-chain results, source-grounded staleness root-cause analysis, redaction, and the operator's Task 3 verdicts with corrections folded in.

## Decisions Made
- Operator judged the **record** as `valid` — a run-incomplete declaration that precisely names its stop cause with a source-verified root cause is a successful plan execution, independent of the failed acceptance.
- Operator judged the **acceptance** as `failed` — forced by the two-piece evidence contract (no `workflow_shipped` event; `--require-shipped` unchanged at exit 1); the run never reached Define.
- Operator chose the next step: a new gap-closure plan **23-16**, retrying with a binary built from a `develop` checkout (not the working branch), since any binary built from `feature/phase-23` will always trip the divergent-lineage staleness path against `develop`-based targets. 23-14's preconditions (freshness re-check, binary hash, `origin/develop` SHA verification) must be re-run because the binary itself changes.
- Recovery-ref disposition: default (undirected) applied — both `origin` refs untouched; local pre-23-11 copy not restored (per `23-FINDINGS.md` §B2a); the now-unused pre-23-15 ref retained on `origin` for reuse by 23-16 rather than deleted.

## Deviations from Plan

None - plan executed exactly as written across all three tasks. The two orchestrator corrections folded into Task 3 (`.worktrees/` directory-shell overclaim; larger-than-snapshot orphan process/gate population) are documentation corrections to the record's own prose, not deviations in execution — no code, test, or additional artifact changes were made beyond the plan's own `23-ACCEPTANCE-RUN-2.md` output.

## Issues Encountered

The acceptance run itself failed — this is the plan's own documented possible outcome (Task 3's "if the verdict is `failed`" branch), not an execution problem. The root cause (a binary built from a long-lived working branch cannot be an ancestor of `develop`'s tip, so the divergent-lineage staleness path fires regardless of content) is fully source-verified and recorded in `23-ACCEPTANCE-RUN-2.md` §10, and the operator's chosen remedy (a `develop`-built binary, via 23-16) directly addresses it.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

**Phase 23's behavioural acceptance criterion — "one phase driven Define→completed Ship, unattended, with Claude" — remains UNMET after two attempts.** The phase's own goal has not been achieved: attempt 1 failed on target unreachability (fixed by 23-12/23-13/23-14); attempt 2 failed on binary provenance (embedded commit not an ancestor of `origin/develop`'s tip). Both failures were structural, source-verified, and each closed one specific coverage gap without yet producing the acceptance itself.

The operator has authorized the next step: **a new gap-closure plan 23-16**, which must build `devflow` from a `develop` checkout (not `feature/phase-23`) and re-run all of 23-14's preconditions (freshness re-check, binary hash recording, `origin/develop` SHA verification, recovery-ref rehearsal) against that new binary before attempting a third launch. STATE.md is updated to reflect this plainly — the phase is not complete, and Plan 15 being "done" reflects a judged, valid, failed acceptance record, not phase closure.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*

## Self-Check: PASSED
- FOUND: `.planning/phases/23-end-to-end-dogfood/23-15-SUMMARY.md`
- FOUND: `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN-2.md`
- FOUND commit: `30de49d`
- FOUND commit: `28cdda1`
- FOUND commit: `4efe46a`
