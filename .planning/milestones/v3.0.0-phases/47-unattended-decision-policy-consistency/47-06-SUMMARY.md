---
phase: 47-unattended-decision-policy-consistency
plan: "06"
subsystem: prompt-and-result-contract
tags: [completion-protocol, decision-policy, checkpoint-gates, snapshots]
requires:
  - phase: 47-03
    provides: one gate rule and the resumed Claude prompt
provides:
  - A completion contract that permits required decision reasoning above its final result line
  - A resume-prompt guard against copied blocking-human declarations
  - Nine reviewed snapshot baselines for the corrected prompts
affects: [DECN-02, DECN-03, Phase 49]
tech-stack:
  added: []
  patterns: [prompt-to-parser contract tests, detector controls, byte-exact snapshot transforms]
key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/snapshots/
    - crates/devflow-cli/src/snapshots/
    - ARCHITECTURE.md
key-decisions:
  - "The parser is unchanged: reasoning belongs above the result line; the parser's 4000-character tail boundary remains explicit."
  - "The resumed agent names the resolved gate in prose and must not copy the checkpoint declaration line."
actuals:
  tasks: 3
  commits: 5
commits: 5
plan_head_before: bf49a77
requirements-completed: [DECN-02, DECN-03]
completed: 2026-09-12
status: complete
---

# Phase 47 Plan 06: Completion Contract and Resume-Detector Gap Summary

**Closed CR-01 with wording-only prompt changes, preserved the parser, and prevented a resumed agent's reasoning from being mistaken for a new blocking-human checkpoint.**

## Accomplishments

- `COMPLETION_PROTOCOL` now requires `DEVFLOW_RESULT` as the last line while allowing decision reasoning above it; ARCHITECTURE describes the same contract.
- Added a production-boundary test covering plain text, a single Claude envelope, and a three-turn Claude stream. It includes an over-5000-character record, a long-trailing-text negative control, and a short-trailing-text control.
- Added a detector control proving a real declaration is detected while the rendered resume prompt and free-form resolved-gate prose are not; the prompt tells the agent to describe the resolved gate in prose.
- Re-blessed exactly nine reviewed snapshots: eight have 2 added/2 deleted lines and the resume snapshot has 3/3.

## Task Commits

1. `b33406c` — red prompt-to-parser contract test.
2. `538a896` — completion-protocol and architecture wording fix.
3. `98f7e96` — red resume-detector test.
4. `509d4fb` — resume-prompt mitigation.
5. `1691b12` — reviewed snapshot re-bless.

## Verification

- Both RED tests compiled and failed at their intended assertions: missing `LAST line` wording and missing `gate-declaration line` guidance. The controls preceding those assertions passed.
- Prompt-to-parser gate passed: 1 targeted test, 771 filtered; test-before-fix ordering, prompt-region identity, production parser identity, and architecture wording all passed.
- Resume-detector gate passed: 1 targeted test, 771 filtered; 29 non-snapshot prompt tests passed; the detector, commit-order, and allowed-region controls passed.
- Bless commands each passed one test and updated 1, 6, and 2 snapshots respectively. The post-commit snapshot gate reported `tracked_snaps=9`, `byte_identical_to_expected=9`, `changed_snaps=9`, `numstat_mismatches=0`, `snap_new_files=0`, and `gate_rc=0`.
- `scripts/check.sh test` exited 0 on its first attempt after the re-bless. `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` also exited 0.

These checks prove the scoped prompt text, parser outcomes, detector boundary, and committed baselines on this checkout. They do not prove that a live model will obey the wording; Phase 49's live observation remains the evidence for that behavioral question. The parser test directly covers plain and Claude shapes only, not every adapter event format.

## Deviations from Plan

None.

## Self-Check: PASSED

- Five task commits exist in the required red-test-before-fix order.
- No `.snap.new` file remains.
- Full hardened test suite passed locally; this is not a CI or live-agent guarantee.
