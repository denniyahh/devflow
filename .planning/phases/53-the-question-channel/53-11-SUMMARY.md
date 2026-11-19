---
phase: 53-the-question-channel
plan: 11
subsystem: tests, doc_check
tags: [e2e, checkpoint, ask, removed-surface]
status: complete
requires:
  - phase: 53-06
    provides: the deleted checkpoint relaunch items this plan pins
  - phase: 53-10
    provides: the deleted checkpoint re-scan machinery this plan pins
provides:
  - a runner-driven e2e proving a blocking-human checkpoint is asked and the Code stage finishes in one launch, with the D-13 fallback as its control
  - removed-surface tokens that fail the suite if the checkpoint relaunch surface returns
affects: [53-VALIDATION M2/M3, 54]
tech-stack:
  added: []
  patterns: []
key-files:
  created:
    - crates/devflow-cli/tests/ask_checkpoint_e2e.rs
  modified:
    - crates/devflow-core/src/doc_check.rs
key-decisions:
  - "All ten planned tokens kept: none had a legitimate occurrence left under crates/, tools/, docs/ or the top-level docs, so the count assertion is 29 + 10 = 39."
  - "Planted lines for the ten new tokens come from the ec413ef origin/develop tree (the pre-deletion tree for 53-03, 53-06 and 53-10); each was picked to hold no second token (MAX_CHECKPOINT_RESUMES uses its definition line, not a line that also names checkpoint_resumes)."
requirements-completed: [ASK-05, ASK-03]
duration: about 15 min
completed: 2026-11-19
commits: 2
plan_head_before: cf72ed0f0330e121d7b53f8a582a7e32b29efbc8
plan_head_after: 6240d8484bf1b4a463815c4b31f5c1fa3337f041
actuals:
  tokens: 5300
  tasks: 2
  commits: 2
---

# Phase 53 Plan 11: Same-session checkpoint e2e and removed-surface pins Summary

A Code-stage codex stub meeting the Phase 49 fixture's blocking-human budget decision asks through `devflow_ask`, gets `budget-120` from `devflow gate answer`, commits it and finishes Code in one launch; the unasked fallback reaches the Code failure gate; ten removed-surface tokens pin the deleted relaunch surface.

## Tasks

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | A Phase-49-shaped checkpoint continues in the same session | 9d3a967 | crates/devflow-cli/tests/ask_checkpoint_e2e.rs |
| 2 | Pin the deleted relaunch surface in the removed-surface scan | 6240d84 | crates/devflow-core/src/doc_check.rs |

## What the tests prove

- `a_blocking_human_checkpoint_continues_in_the_same_session`: runner exit 0; HEAD's subject is the stub's commit and `HEAD:budget.txt` is `budget-120`; one Code `stage_launched`; one Code `question_asked` of kind `blocking-human`; one `question_answered` for that question with option `budget-120` and policy `operator_via_gate`; `devflow history 1 <root>` shows the question text, all three option ids, `budget-120 "120 seconds" (recommended)` and `option budget-120 by ... via operator_via_gate`.
- `a_checkpoint_reported_without_asking_reaches_the_failure_gate`: the Code gate is `stage_failure` with context naming `could not ask through devflow_ask`; one Code `stage_launched` at the gate and after `gate abort`; no `question_asked`; `devflow history` renders the run and no line starts with any of `render_timeline`'s seven question-row prefixes.
- Negative controls run on this head (each mutation reverted, file byte-identical after): the operator answering `budget-30` failed at the `budget.txt` assertion; the stub asking with kind `decision` failed at the `kind blocking-human` assertion; the no-question-row loop applied to the asked run's history failed naming the `  question ...` row. For Task 2, a `"checkpoint_auto_decided"` literal appended to the new e2e file failed `no_removed_surface_literal_remains` naming that path and line.

## Verification

- `cargo nextest run -p devflow --test ask_checkpoint_e2e --no-tests=fail -E 'test(=...)'` for each of the two tests: 1 passed each.
- `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::no_removed_surface_literal_remains)'` and `...removed_surface_findings_are_sorted_by_path_then_line`: 1 passed each.
- `scripts/check.sh all`: exit 0, `all OK`; its log shows `ask_checkpoint_e2e` 2 passed and `doc_check::no_removed_surface_literal_remains ... ok`. The known flaky monitor test did not fail in this run.
- `cargo clippy -p devflow --all-targets -- -D warnings` clean; `check.sh all` covers the workspace clippy.

## Probe row ASK-05 "unclassified"

Carried as a flagged assumption per the plan: its edges are the two Task 1 tests above and 53-09's abort and backstop tests; this summary supplies the Task 1 evidence for the verifier.

## TDD Gate Compliance

No RED commit: the behaviour under test was built by 53-07, 53-08, 53-09 and 53-03, so both tests passed on their first run (the "unexpected GREEN" case in tdd.md, where the feature already exists). The failing direction was shown instead by the four mutation controls above, each failing on its intended assertion text. Both tasks are test-only and committed as `test(...)`.

## Deviations from Plan

None in behaviour. One commit-message fix: Task 1's first subject was 76 characters (commit-msg hook warning, over the 72-character rule), so it was amended before anything else ran on top of it.

## Self-Check: PASSED

- FOUND: crates/devflow-cli/tests/ask_checkpoint_e2e.rs
- FOUND: crates/devflow-core/src/doc_check.rs (39-token count, 39 planted lines)
- FOUND: 9d3a967, 6240d84 (ancestors of HEAD)
