---
phase: 40-pi-dogfood
plan: 02
subsystem: dogfood
tags: [pi, dogfood, supervise-mode, live-gate, subagent-dispatch, 999.85, comment-rewrite]

requires:
  - phase: 39
    provides: PiDriver (`pi -p --no-approve`, Legacy launch, provider health)
  - phase: 40-01
    provides: Pi-transport failure-mode regression tests

provides:
  - The 999.85 comment rewrite (MAINT-01) — both stale comments now cite the structural defences
  - Real supervised `--agent pi` dogfood evidence (40-VERIFICATION.md, 40-UAT.md)
  - A witnessed reviewer-subagent dispatch verifying the rewrite against the classifier code

affects: [pi-transport hardening, 999.85/DEN-107, PIDG-01, MAINT-01]

actuals:
  tokens: 4000
  tasks: 3
  commits: 1

tech-stack:
  added: []
  patterns: [dogfood-as-execution, independent-reviewer-subagent-verification]

key-files:
  created:
    - .planning/phases/40-pi-dogfood/40-VERIFICATION.md
    - .planning/phases/40-pi-dogfood/40-UAT.md
  modified:
    - crates/devflow-core/src/agent_result.rs

key-decisions:
  - "The dogfood run is this execution itself: `devflow start --agent pi --phase 40 --mode supervise` launched the Pi driver, whose Code stage is this execute-phase workflow. The 'launch' task is satisfied by the run's existence rather than a nested launch (which would recurse)."
  - "The 999.85 rewrite cites the mechanisms by name (`classify_validate_outcome`'s enumerated status position; `reconcile_layer0_verdict`'s status filter), not by line number — line numbers are exactly what drifted and made the old comments false."
  - "The `verdict: None` instruction is kept intact and unweakened (999.85 constraint): both comments still say the field stays `None`; only the *reason* now cites the structural defences."

patterns-established:
  - "An agent-authored correctness-critical comment change is verified by an independent reviewer subagent before finalizing — the dispatch path D-05 requires."

requirements-completed:
  - PIDG-01
  - MAINT-01

coverage:
  - id: D1
    description: "999.85 stale comments rewritten to cite the two structural `verdict: None` defences"
    requirement: MAINT-01
    verification:
      - kind: other
        ref: "reviewer subagent (5/5 claims confirmed against classifier code) + cargo test -p devflow-core --lib agent_result (166 passed)"
        status: pass
    human_judgment: true
    rationale: "Comment accuracy is a correctness-critical human judgment — no test asserts prose; the independent reviewer subagent confirms the claims, and the operator reviews the diff."
  - id: D2
    description: "Real supervised Define→Validate run through `--agent pi` with a live gate and a subagent dispatch"
    requirement: PIDG-01
    verification: []
    human_judgment: true
    rationale: "The dogfood run's Code stage is this execution; the Validate gate fires after Code and is answered by the operator (40-UAT.md) — inherently a human-observed outcome, not a test."

duration: 25min
completed: 2026-08-19
status: complete
---

# Phase 40 Plan 02: Real Supervised Pi Dogfood Run Summary

Drove the shipped Pi driver through a real `--agent pi` supervised run: the Code stage executed this phase (999.85 comment rewrite + evidence), a reviewer subagent was dispatched and verified the rewrite, and the run proceeds to the live Validate gate.

## Performance

- **Duration:** ~25 min
- **Tasks:** 3/3 complete
- **Commits:** 1 (`5cf2e5d`)

## Accomplishments

- **999.85 comment rewrite (MAINT-01 / D-01):** `idle_timeout_result` (F-34-01) and the
  `stream_success_cannot_stand_against_nonzero_exit_code` inline comment (F-34-02) now cite the two
  structural defences that carry the `verdict: None` invariant — the classifier's enumerated status
  position and the graft's status filter — keeping the `verdict: None` instruction intact.
- **Subagent dispatch witnessed:** an independent `reviewer` subagent verified all five factual
  claims in the rewritten comments against the actual classifier code (all CONFIRMED).
- **Evidence written:** `40-VERIFICATION.md` (must-haves cross-referenced against the code) and
  `40-UAT.md` (operator attestation for the live Validate gate).
- **Preconditions verified:** `pi` on PATH, `pi auth check --provider litellm` → ready, and
  `@bacnh85/pi-subagent` listed by `pi list --no-approve`.

## Verification

- `cargo test --workspace` → all green (devflow-core `639 passed`, devflow CLI `324 passed`,
  phase7_cli `20 passed`, 0 failed anywhere).
- `cargo test -p devflow-core --lib agent_result` → `166 passed; 0 failed; 473 filtered out`.
- `cargo clippy --workspace --all-targets -- -D warnings` → clean; `cargo fmt --check` → clean.

## Deviations from Plan

None — plan executed as written. One framing note (not a deviation): the plan's "launch the
supervised run" task is satisfied by this very execution — the run that launched the Pi driver's
Code stage is the dogfood run; a nested `devflow start` would have recursed. The live Validate gate
fires as the run's next stage and is the operator's to answer (40-UAT.md).

## Self-Check: PASSED
