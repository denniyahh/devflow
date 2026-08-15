---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
plan: 01
subsystem: agent-result-classification
tags: [999.77, 999.87, HARDEN-01, HARDEN-07, breaking-change, test-harness]
status: complete

requires:
  - "devflow_core::agent_result::phase_commit_count"
  - "devflow_core::mode::consecutive_failures_made_progress"
provides:
  - "phase_commit_count -> Option<u32> (breaking, D-08)"
  - "evaluate_layer3 unmeasurable-count arm (behaviour change, no signature change)"
  - "NoGitPath RAII guard (devflow-cli only)"
affects:
  - "crates/devflow-cli/src/pipeline_outcomes.rs"
  - "crates/devflow-core/src/agent_result.rs"

tech-stack:
  added: []
  patterns:
    - "RAII PATH guard mirroring NeutralPath (devflow-cli)"
    - "unspawnable working directory as a hermetic substitute for an absent binary"
    - "opposite-result assertion in the same suite run"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-core/src/test_support.rs
    - .planning/phases/35-loop-termination-and-baseline-correctness-999-77-999-78-999-/35-VALIDATION.md

decisions:
  - "phase_commit_count returns Option<u32>; unparseable stdout maps to None, branch-absent to Some(0)"
  - "evaluate_layer3 re-pointed at phase_commit_count; unmeasurable count is Unknown with commits absent"
  - "devflow-core's NoGitPath removed on measurement — a process-global PATH guard is not viable in that test binary"
  - "criterion 6's layer-level and cascade-level tests live in devflow-cli, calling the same pub functions"

metrics:
  duration: "64m"
  completed: 2026-08-07

actuals:
  tokens: 13568
  tasks: 3
  commits: 3
---

# Phase 35 Plan 01: Loop-Termination and Baseline Correctness Summary

A transient `git` fault can no longer forge a false-zero commit count: `phase_commit_count`
returns `Option<u32>`, and all three consumers — `handle_validate_outcome`, `evaluate_layer2`
and `evaluate_layer3` — now distinguish "could not count" from "counted zero".

## What Changed

**999.77 (criterion 1).** `handle_validate_outcome`'s baseline write is now inside the `Some`
arm only. A cycle whose count could not be measured is treated as not-progress and leaves
`state.last_validate_failure_commit_count` byte-identical, so the next real measurement compares
against the last real observation. Previously one unmeasurable cycle wrote `Some(0)`, the next
real count exceeded it, that read as forward progress, and the streak reset to 1 — one free
extension of the `MAX_CONSECUTIVE_FAILURES` ceiling per transient fault.

**999.87 (criterion 6).** `evaluate_layer2` returns `Ok(None)` on an unmeasurable count and falls
through to Layer 3 (D-09). **`evaluate_layer3`'s own inline `rev-list --count` was deleted and
re-pointed at `phase_commit_count` (F-4)** — it carried an independent copy of the same lossy
collapse and classified the resulting zero as `Failed`, so fixing only Layer 2 would have
relocated the misclassification one layer down rather than removing it. An unmeasurable count at
Layer 3 is now `Unknown` with `commits: None`.

**Doc comments (a phase deliverable, not cleanup).** `phase_commit_count` no longer claims every
consumer treats the three zero-causes alike; `handle_validate_outcome` no longer promises a
gating direction that a single transient fault defeats; `evaluate_layer3` documents its third
case and the shared counter. `idle_timeout_result` was left untouched (999.85, explicitly out of
scope) — verified: 0 diff lines touch it.

## Negative Controls — every one performed, none asserted from reading the fix

**NC-1 — the harness blocks `git`.** `devflow-cli`: `test result: ok. 1 passed; 0 failed; 279
filtered out`. Its controls are inside the test (pre-guard `Ok`, post-drop `Ok`, `PATH`
byte-identical after drop). Its own mutation control: with `install()` made a no-op, the test
reported `FAILED. 0 passed; 1 failed` with `git version 2.55.0` in the panic message — so a guard
that silently did nothing cannot produce a green result.

**NC-2 — `None` preserves the baseline.** Restored the unconditional write with the old lossy
zero mapping (`.unwrap_or(0)`), re-ran the two-cycle test:

```
assertion `left == right` failed: a cycle whose commit count could NOT be measured must
leave the baseline byte-identical to the last real observation
  left: Some(0)
 right: Some(1)
test result: FAILED. 0 passed; 1 failed; 280 filtered out
```

Restored from a checksummed backup (md5 verified identical), re-ran: `47 passed; 0 failed`.

**NC-3 — the sequence discriminates; a single cycle would not.** RED output of the two-cycle test
against unmodified production code, with the baseline assertion temporarily relaxed so the run
reached the streak assertion:

```
looping back to Code (validate failures: 2)
looping back to Code (validate failures: 1)

assertion `left != right` failed: the streak must never be reset to 1 by this sequence
  left: 1
 right: 1
test result: FAILED. 0 passed; 1 failed; 280 filtered out
```

The buggy code's own stdout shows the streak going 2 → 1 across the two cycles. A single-cycle
variant stops after `validate failures: 2`, which is identical under the buggy and the fixed code
— that is precisely why it is a proxy.

**NC-4 — unrunnable is not measured-zero.** The pair passes in the same run:
`phase_commit_count_reports_zero_without_a_branch` asserts `Some(0)` (git ran, branch genuinely
absent) and `phase_commit_count_reports_none_when_git_cannot_run` asserts `None`. The split is on
"did the command run", not on "was the answer zero".

**NC-11 — the criterion-6 case is the unrunnable one.** `evaluate_layer2_exit_zero_no_commits_is_failed`
passes (`1 passed; 556 filtered out`) and its body is **byte-identical to HEAD** — verified by
extracting the function from both trees and diffing (18 lines extracted, diff exit 0; the
non-empty extraction is itself the control against a vacuous both-empty comparison).

**NC-12 — the defect was removed, not relocated. This is the measurement that matters for F-4.**
With `evaluate_layer2`'s `Ok(None)` fix left in place and **only** `evaluate_layer3`'s
could-not-measure arm reverted to a zero treatment:

```
evaluate_agent_result_with_unrunnable_git_does_not_report_failed ... FAILED
  assertion `left != right` failed: END TO END: exit 0 + Stage::Code + an unrunnable git
  must not report Failed
  left: Failed
 right: Failed

evaluate_layer2_unrunnable_git_falls_through_to_layer3 ... ok  (1 passed)
```

**The asymmetry is the evidence:** under the same mutation the cascade test fails while the
layer-level test passes. That is a direct demonstration that a `evaluate_layer2`-only unit test
is the proxy which hid the Layer 3 defect through planning. Restored from a checksummed backup
(md5 verified identical).

**Layer-2 arm's own control.** Reverting `evaluate_layer2`'s `None` arm to `.unwrap_or(0)` makes
the layer-level test fail with exactly the 999.87 symptom:
`Some(AgentResult { status: Failed, reason: Some("no commits found on feature/phase-04 (agent exit code was 0)"), commits: Some(0), decided_by_layer: Some(2) })`.

**`evaluate_layer3` region check.** The plan's command
(`awk '/^pub fn evaluate_layer3/,/^\/\/\/ Layer 0/' … | grep -c 'unwrap_or(0)'`):
**pre-change `1`, post-change `0`.** Note the check matches comment prose as well as code — my
first post-change run printed `1` because an explanatory comment contained the literal token; I
reworded the comment so the check measures code. A stronger, comment-stripped check
(`rev-list|.output()|git_command|unwrap_or(0)` on non-comment lines) gives **pre-change `4`,
post-change `0`.**

## Stated Limits — what this green run does NOT establish

**F-5 (a): no dispatch-level change.** `AgentStatus::Failed` and `AgentStatus::Unknown` map
identically to `Action::GateReview` (`outcome_policy.rs:53-54`, a deliberately defended identical
mapping). This work changes the recorded classification, the `commits` field and the
operator-facing reason string; it does **not** change what the run does next. No test here
asserts on `Action`, `decide_action`, or any gating consequence — confirmed by inspection, and
such an assertion would pass against the buggy code too.

> **Correction (2026-08-07, 35-REVIEW CR-01/IN-03).** This limit was **false as 35-01 originally
> shipped**, and is recorded here rather than silently rewritten. The claim held for the Layer 3
> edit but not for the Layer 2 one: that edit placed the unmeasurable-count guard *above* the
> exit-code classification, so an unmeasurable count also turned `Success → Unknown`
> (`Advance → GateReview`) and `ResourceKilled → Unknown` (`GateInfra → GateReview`) — the second
> of which violates `pipeline_launch.rs`'s explicit prohibition on routing infra faults through
> `handle_validate_outcome`. Commit `cf462ec` narrowed the guard to
> `commit_gated && exit_code == 0 && commits.is_none()`, after which the limit as written is true
> again: the sole remaining fall-through was `Failed → GateReview` before this phase and is
> `Unknown → GateReview` after it. **Why it went unnoticed:** the very inspection this paragraph
> cites is what missed it, and no test paired an unmeasurable count with a non-zero exit. Two
> tests now do (`evaluate_layer2_unrunnable_git_still_classifies_exit_137_as_resource_killed`,
> `evaluate_layer2_unrunnable_git_keeps_success_for_a_non_commit_gated_stage`).

**D-09's stated "accepted cost" is corrected, without reopening the decision.** D-09 predicted
that falling through to Layer 3 means "the run continues rather than gating". It does not: Layer
3's `Failed` and its post-fix `Unknown` both gate for review. The decision's *action* (`Ok(None)`)
is unchanged and remains correct; only its predicted consequence was wrong.

**F-5 (b) / C5: `NoGitPath` is PATH-based.** It works because every `git` child in this workspace
is constructed PATH-resolved (`git.rs:72` `git_command` → `:87` `hermetic_command` →
`Command::new("git")`). A future refactor to an absolute `git` path would disarm the guard
silently while every dependent test kept passing. A known structural property of the harness, not
a live defect. The two `devflow-core` tests that use an unspawnable working directory instead are
immune to this.

**HARDEN-01 unclassified, carried forward.** The run boundary stays open: `State::new` zeroes both
`consecutive_failures` and the baseline on every `devflow start --force`, and nothing here shows
the streak surviving a restart. (35-04's never-reset per-phase total is a different counter.)

**Frequency is unmeasured.** Nothing establishes how often Layer 2 or Layer 3 is the deciding
layer in production. The code paths were read and driven; the frequency was not measured.

**Stability bounds are weak bounds.** 10 consecutive clean runs per crate is evidence of
direction, not a reliability guarantee. It supports "the removal fixed the flake I introduced";
it does not support "this suite has no races".

## Deviations from Plan

### [Rule 3 - Blocking] Every `-p devflow --lib` command in the PLAN is unrunnable

- **Found during:** Task 1, before any code was written.
- **Issue:** `devflow` is a **binary-only** crate (`crates/devflow-cli/src/main.rs`, no
  `src/lib.rs`). `cargo test -p devflow --lib …` exits non-zero with
  `error: no library targets found in package 'devflow'`. Every `<automated>` verify and
  acceptance command in the PLAN that targets the `devflow` package used `--lib`.
- **Fix:** used `cargo test -p devflow --bin devflow …` throughout. Confirmed working
  (`Executable unittests src/main.rs`). Corrected in `35-VALIDATION.md`'s Quick-run row.
- **Note:** this failure is loud, not silent — it does not produce a false green. But it is the
  companion to the trap CLAUDE.md already records (`--exact` on a name matching nothing exits 0),
  and both were live in this plan's commands.

### [Rule 1 - Bug] `devflow-core`'s `NoGitPath` guard removed after measurement

- **Found during:** Task 2, first full run of `agent_result::` tests.
- **Issue:** a process-global `PATH` guard is **not viable in `devflow-core`'s test binary**.
  That crate shells out to `git` from eight modules that run as parallel threads in one process,
  and — decisively — its tests reach `git` by calling **production code** that spawns it, not only
  through fixture helpers. So no fixture-level lock can cover them.
- **Measured, with a control:**

  | arm | result |
  |---|---|
  | guard used by 3 regression tests | 1-5 unrelated failures per run |
  | + this module's `git()` helper takes the lock | still 1 failure in 8 runs (`evaluate_layer2_exit_zero_no_commits_is_failed`, whose `git` call happens *inside* `evaluate_layer2`) |
  | guard used by its own NC-1 sanity test only | 1 failure in 8 runs |
  | NC-1 `#[ignore]`d — **the control** | **0 failures in 10 runs** |
  | guard removed entirely | **0 failures in 10 runs** |

  The asymmetry between the last two arms and the one above them is what identifies the guard
  itself as the cause rather than a pre-existing flake.
- **Fix:** `NoGitPath`, `PATH_MUTEX`, `path_lock` and the NC-1 test were removed from
  `devflow-core`. A ~40-line comment stands in their place recording the measurement, the reason,
  and the two sanctioned alternatives — so the next author needing a failing `git` does not
  rebuild it.
- **Consequence for the plan's contract, stated plainly:** this **deviates from an explicit
  artifact requirement**. The plan's `artifacts` block requires `crates/devflow-core/src/test_support.rs`
  to contain `NoGitPath`, and truth #7 requires an NC-1 sanity control "in each crate". Neither
  holds now. The F-2 decision that motivated a per-crate guard was written to give `devflow-core`'s
  criterion-6 tests a guard they could use; those tests moved to `devflow-cli` (below), so the
  need it served no longer exists. **This is flagged for the operator rather than treated as
  settled** — see "Still open".

### [Rule 3 - Blocking] Criterion 6's two tests live in `devflow-cli`, not beside their subject

- **Found during:** Task 3.
- **Issue:** `evaluate_layer2` reads its exit file from `project_root`, so the unspawnable-root
  route used elsewhere would make the *exit read* fail and return `Ok(None)` for the wrong reason
  — a test that passes against the unfixed code. The root must **exist** while `git` is
  unresolvable, which only a `PATH` guard delivers; and per the deviation above, that guard is
  unsafe in `devflow-core`'s binary.
- **Fix:** `evaluate_layer2_unrunnable_git_falls_through_to_layer3`,
  `evaluate_agent_result_with_unrunnable_git_does_not_report_failed` and its companion control
  live in `devflow-cli/src/pipeline_outcomes.rs`, whose binary routes every `PATH` mutation
  through the single `ENV_MUTEX` its `git`-touching tests already hold. All three functions under
  test are `pub`, so the assertions are unchanged — only the binary differs. Measured stable:
  **10/10 clean runs, 284 tests.**
- **F-2 as actually executed:** one `NoGitPath` in `devflow-cli` (used by four tests), none in
  `devflow-core`. The plan's structural anti-race argument still holds, by a stronger route: there
  is now exactly one `PATH` guard in the workspace, under exactly one mutex.

### [Rule 1 - Bug] The plan's `evaluate_layer3` region check matches comment prose

- **Found during:** Task 2 acceptance checks.
- **Issue:** `grep -c 'unwrap_or(0)'` over the function's text counts comments. My explanatory
  comment described the deleted code using the literal token, so the check printed `1`
  post-change — a false negative for the fix.
- **Fix:** reworded the comment, and added a comment-stripped check as the real measurement
  (pre `4`, post `0`). Both results recorded above rather than only the flattering one.

### [Rule 2 - Missing] F-1b's fallback taken for the two Layer 3 / count tests

`phase_commit_count_reports_none_when_git_cannot_run` and
`evaluate_layer3_unmeasurable_count_is_unknown_not_failed` drive the `Err` arm through an
unspawnable working directory (`hermetic_command` sets `cmd.current_dir(dir)`) rather than
`NoGitPath`. F-1b authorises this explicitly on observed flakes, and flakes were observed. The
route reaches the identical arm — `phase_commit_count` sees only `Err` and cannot distinguish the
causes — with no environment mutation, and it is immune to the C5 PATH-resolution fragility.

## Verification

| Check | Result |
|---|---|
| `scripts/check.sh all` | **OK** — fmt clean, clippy clean under `-D warnings`, **917 passed / 0 failed** across 22 binaries |
| `cargo test -p devflow-core --lib` | 556 passed, 0 failed — **10/10 clean runs** |
| `cargo test -p devflow --bin devflow` | 284 passed, 0 failed — **10/10 clean runs** |
| `cargo test … agent_result::tests::evaluate_layer3` | 3 passed, 0 failed (new test + both unedited controls) |
| `rg 'treats all three the same way'` | no matches |
| `rg 'the gate stays reachable'` | no matches |
| `rg 'idle_timeout_result'` | still matches; 0 diff lines touch it |
| CR-01 note in `handle_validate_outcome` | unedited (0 removed lines matching `CR-01`) |
| Layer 3 control bodies vs HEAD | byte-identical (12 and 17 lines extracted, diff exit 0) |

**Measured runtime** (recorded in `35-VALIDATION.md`): `scripts/check.sh all` **76.4s**;
`cargo test -p devflow-core --lib` **5.1-5.9s** (n=10); `cargo test -p devflow --bin devflow`
**12.2-14.3s** (n=10). All warm/incremental — they exclude a cold build, and the `check.sh`
figure is n=1.

## For 35-06 to collect

`evaluate_layer3` **changed behaviour without changing its signature**. It is a `pub` item of the
published `devflow-core` crate and belongs in the changelog's enumeration as a *behaviour change*,
distinct from the signature breaks. The signature break in this plan is
`phase_commit_count: u32 -> Option<u32>`.

## Known Stubs

None. No hardcoded empty values, placeholder text, TODO/FIXME markers, or unwired components were
introduced. No tests are `#[ignore]`d (the one temporary `#[ignore]` was a negative control and
was removed with the code it measured).

## Threat Flags

None. No new network endpoint, auth path, file-access pattern or trust-boundary schema change.
T-35-01, T-35-02, T-35-02b and T-35-03 are mitigated as planned; T-35-03's mitigation changed
shape (one guard in one crate under one mutex, rather than two structurally-separated guards),
and is stronger for it.

## Still open — needs the operator's word, not my assumption

1. **RATIFIED 2026-08-07 (operator, during 35-verify-work).** The `devflow-core` `NoGitPath`
   removal contradicted an explicit plan artifact and truth #7. Taken on measurement (1-in-8
   flake with the guard, 0-in-10 without it), and the operator ratified the removal rather than
   reverting to either rejected alternative — restoring it with its NC-1 sanity test (reintroduces
   the measured flake) or with NC-1 `#[ignore]`d (a skipped test, itself a defect on this
   project's broken-windows ledger). One `PATH` guard now exists in the workspace, in
   `devflow-cli`, under one shared mutex; nothing downstream depends on `devflow-core` having its
   own.
2. **RESOLVED 2026-08-07 (35-verify-work investigation) — see the disposition below.**
   **The tracer feedback gate was run as the autonomous variant, not the interactive one.**
   `workflow.auto_advance` is absent from `.planning/config.json` (so the executor spec's
   detection reads "not auto"), while `workflow.auto_mode` is `true` and I was spawned into a
   worktree with no channel to receive a checkpoint answer. I ran the gate's substantive check —
   re-running the tracer's `<verify>` end-to-end, `47 passed; 0 failed` — rather than emitting a
   checkpoint that could not be answered. Whether `auto_mode` should satisfy the spec's
   `auto_advance` check is a config question I should not settle by inference.


   **RESOLVED 2026-08-07 (35-verify-work investigation).** The question as posed — *should
   `workflow.auto_mode` satisfy the spec's `auto_advance` check?* — has no answer, because
   **`workflow.auto_mode` is read by nothing.** It is absent from gsd-core's config schema, its
   defaults manifest, and `references/planning-config.md`; the only `auto_mode` occurrences in that
   repo are the unrelated XML tags `<auto_mode>` and `<auto_mode_detection>`. It was written into
   `.planning/config.json` by the project's `GSD init` commit and appears to be a rename that
   shipped without a migration. Filed upstream as **G-04** in `.planning/UPSTREAM-GSD-ISSUES.md`.

   **What the executors did was therefore correct, for a reason neither could see.** Running the
   gate's substantive check was the right call — but not because `auto_mode` implied autonomy.
   Auto-mode was genuinely inactive: `check auto-mode` reports
   `{"active": false, "source": "none", "auto_chain_active": false, "auto_advance": false}` on this
   project. There was no channel and no flag, and the improvisation was the only thing available.

   **Why simply setting `auto_advance: true` is NOT the fix**, and this is the part that took the
   investigation: that flag conflates checkpoint-bypass with stage-chaining
   (`plan-phase.md:1563` launches execute-phase itself when it is set), so enabling it would make
   DevFlow's Plan stage run the Code stage and corrupt the very commit-attribution this phase's
   accounting depends on. Upstream causes filed as **G-01**, **G-02**, **G-03**; DevFlow's own fix
   is scoped as **Phase 35.1** (999.93), which also adds the preflight that would have made this
   condition loud instead of leaving it to two executors to rediscover.

## Self-Check: PASSED

Files verified present on disk: `crates/devflow-core/src/agent_result.rs`,
`crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-cli/src/test_support.rs`,
`crates/devflow-core/src/test_support.rs`, and this SUMMARY.

Commits verified in `git log`: `cdbfcfd`, `eea4766`, `bda73bc`.
