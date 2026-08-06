---
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
plan: 06
subsystem: test-harness
tags: [env-mutex, poison-tolerance, stream-json, widening, dogfood-03, criterion-1]
status: complete

requires:
  - "plan 34-02 — the canary test rebuilt on the legacy opt-out; the idiom this plan generalizes"
  - "plan 34-04 — its pipeline_launch.rs changes are in the tree this plan re-measured"
provides:
  - "test_support::env_lock() — poison-tolerant ENV_MUTEX acquisition at all 57 call sites"
  - "six tests whose launch-path premise is stated explicitly rather than inherited from STREAM_JSON_STAGES membership"
  - "the widened-suite measurement for --bin devflow (0 failed) with its negative control"
  - "deferred-items.md — the integration-suite widening gap that 34-05 still faces"
affects:
  - "plan 34-05 — its precondition is PARTLY discharged: the binary suite survives widening, the integration suite does not"

tech-stack:
  added: []
  patterns:
    - "poison-tolerance justified by a CONDITION (RAII guards restore state during unwind) recorded as a condition, so removing a guard is visibly a change to this function's premise"
    - "reproducing a suspected flake's mechanism directly, rather than attributing it from run counts that cannot carry the conclusion"

key-files:
  created:
    - .planning/phases/34-stream-json-coverage-and-the-validate-trust-boundary-999-73-/deferred-items.md
  modified:
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/staleness.rs

decisions:
  - "All six genuine widened-run failures got the explicit legacy opt-out, not an injected canary outcome — every one has a subject orthogonal to the launch path (pid persistence, resume/cap semantics, launch COUNT)"
  - "The 5 phase7_cli integration failures under widening were RECORDED, not fixed: outside the plan's declared files, and one of them (parallel_creates_two_worktrees_and_spawns_two_monitors) may have the canary's own guarantee as its subject, making the obvious opt-out repair a coverage deletion"
  - "The hostile-GIT_DIR flake was characterized by reproducing its mechanism, not by attributing it from run counts (0/5 vs 2/7 is Fisher p ~ 0.47 and carries nothing)"

metrics:
  duration: "~45 min"
  completed: 2026-08-05

actuals:
  tokens: 9200
  tasks: 3
  commits: 3
---

# Phase 34 Plan 06: Making the Suite Survive 34-05's Widening — Summary

`ENV_MUTEX` acquisition is poison-tolerant at all 57 call sites, so a failure count is now a
count of failures rather than of downstream victims; six tests that carried an incidental
stage-membership premise now state their premise explicitly; and the five-stage widening is
measured green on `cargo test -p devflow --bin devflow` alongside a control proving that
reading can still go red.

**Read the next paragraph before treating 34-05 as unblocked.** The `--bin devflow` suite this
plan's acceptance is written against does **not compile the integration tests**. `scripts/check.sh
all` does, and under the same widening five `phase7_cli` tests fail. This plan's stated purpose —
that 34-05 spend its budget on evidence rather than on an unbudgeted repair — is therefore only
**partly** discharged. Details in "The finding that qualifies this plan's purpose" below and in
`deferred-items.md`.

## The composition finding: 6 genuine failures, not 1, not 14, not 42

Three different numbers have been reported for the same widening, and the SUMMARY owes an account
of why they differ:

| Source | Reported | What it was actually counting |
|---|---|---|
| Plan 34-02 | 14 failed | `pipeline_launch::` only, single-threaded, before 34-04 touched the file |
| This plan's orchestrator | 42 failed (41 poison + 1 genuine) | whole binary, at a later HEAD, with the cascade included |
| **This plan, measured** | **6 failed, 0 PoisonError** | whole binary, cascade removed by task 1 |

The plan predicted the legible run would show **1** genuine failure. It showed **6**. The plan
anticipated this possibility explicitly — "poison masks as well as amplifies: a test that died on
`PoisonError` never reached its own assertions" — and that is what happened: four of the six had
never previously run far enough to fail on their own account. Working from the run's actual output
rather than the plan's predicted list was load-bearing, not pedantry.

## Task 1 — the paired measurement

This is the evidence the task rests on, and it was run in both directions with the induced fault
held constant (a single `assert!(false, "induced")` immediately after the lock acquisition in
`pipeline_outcomes::checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout`).

| Tree | Total failed | of which `PoisonError` |
|---|---|---|
| **Before** the accessor (`.lock().unwrap()`) | **25** | **24** |
| **After** the accessor (`env_lock()`) | **1** | **0** |

The two counts differ, so the change does what the task claims rather than merely compiling. The
induced assertion was removed afterwards; `rg -n 'induced' crates/devflow-cli/src/` returns exactly
one match, in the accessor's own doc comment where the measurement is recorded for future readers.

### The sweep, counted two ways

A count that comes back clean cannot be trusted if the pattern might match nothing, so both
patterns are reported and both demonstrably match something:

| Pattern | Result |
|---|---|
| `rg -c 'ENV_MUTEX\.lock\(\)\.unwrap\(\)' crates/devflow-cli/src/` | **2**, both in `test_support.rs` |
| `rg -c 'env_lock\(\)' crates/devflow-cli/src/` | 23 + 15 + 8 + 6 + 5 call sites = **57**, plus **1** definition |

The two remaining old-form matches are **doc-comment prose, not call sites**: `NeutralPath`'s
existing cascade paragraph (which the plan instructed me to cross-reference rather than duplicate)
and the new accessor's own "do not call this directly" warning. Both are load-bearing text; neither
is code. See Deviation 1 — the plan's "58 sites" figure counted one of them as a call site.

The unwidened suite is **279 passed; 0 failed, `EXIT=0`** — unchanged, as a refactor must be.
`cargo fmt --all --check` exit 0, `cargo clippy -p devflow --all-targets -- -D warnings` exit 0.

## Task 2 — six tests, each classified before being repaired

All six failed through one mechanism: their stage joins the stream path under widening, so
`canary_gate` invokes the real `ClaudeCanaryLauncher`, which cannot confirm background-task
notification delivery in a test environment and refuses.

The plan warns that applying the opt-out to a test whose subject **is** the stream path silently
deletes coverage. Each was therefore classified before being touched:

| Test | Subject | Repair | Why that one |
|---|---|---|---|
| `pipeline_launch::launch_stage_persists_monitor_pid_for_reload` | monitor pid persisted and survives reload | explicit opt-out | subject is state persistence; the launch path is incidental |
| `pipeline_launch::resume_clears_stop_marker_and_advances_past_stop_point` | resume clears `stopped`/`stop_reason`/`stop_until` | explicit opt-out | subject is resume's marker semantics |
| `pipeline_launch::resume_preserves_unfired_until_cap` | an unfired `--until` cap survives resume | explicit opt-out | subject is the cap, not the relaunch mechanism |
| `pipeline_launch::resume_without_a_cap_is_unchanged` | the no-cap resume path is undisturbed | explicit opt-out | same |
| `preflight::run_preflight_advance_gate_launches_agent_exactly_once` | launch **count** through the Advance arm (CR-01) | explicit opt-out | subject is how many launches, not which kind |
| `preflight::run_preflight_loopback_gate_launches_agent_exactly_once` | launch **count** through the LoopBack arm | explicit opt-out | same |

**No test received an injected canary outcome, because none of the six has the stream path as its
subject.** The tests that do — `canary_gate_only_applies_to_the_stream_launch_path` and
`canary_gate_still_fires_for_a_widened_stage_without_the_opt_out` — were built on that idiom by
34-02 and passed under widening untouched.

For the three `resume_*` tests the opt-out is set **before** `workflow::save_state`, not on the
local binding: `resume()` loads its own `State` from disk, and `apply_legacy_launch_opt_out` ORs the
persisted value (`pipeline_launch.rs:312`), so it survives the reload. Setting it on the local
binding would have been a no-op that still turned the test green under the narrow constant — a
silent false pass.

### No assertion was deleted or weakened

The task-2 diff is **57 insertions, 0 deletions**:

```
 crates/devflow-cli/src/pipeline_launch.rs | 41 +++++++++++++++++++++++++++++++
 crates/devflow-cli/src/preflight.rs       | 16 ++++++++++++
 2 files changed, 57 insertions(+)
```

`git diff -U0 | grep '^-' | grep -v '^---'` returns **nothing**, so no line was removed at all —
which is a stronger statement than "no assertion was weakened" and settles it mechanically rather
than by inspection. Exactly **6** added lines match `legacy_claude_launch = true`, one per repaired
test; the remaining 51 are comments and doc comments recording that the premise was moved
deliberately.

Post-repair, unwidened: **279 passed; 0 failed, `EXIT=0`**, constant reads `&[Stage::Code]`.

## Task 3 — the four readings

| # | Reading | Result | Exit code (captured directly, not through a pipe) |
|---|---|---|---|
| 1 | widened / clean, `--bin devflow` | `test result: ok. 279 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` | **0** |
| 2 | widened / `scripts/check.sh all` | **`phase7_cli`: 12 passed; 5 failed** | **101** |
| 3 | widened / induced fault, `--bin devflow` | `test result: FAILED. 277 passed; 2 failed` | **101** |
| 4 | post-revert tree state | `git status --porcelain` empty; `&[Stage::Code]` at `:470` | — |

**Reading 3 is 2 failed, not the 1 the plan's acceptance criterion names.** Reporting it as 1 would
have been rounding toward the expected answer. It decomposes exactly:

- `pipeline_launch::tests::resume_without_a_cap_is_unchanged` — the induced `NC-8 control`. This is
  the control firing: the harness still detects breakage under widening, so reading 1's `0 failed`
  is a measurement and not a constant.
- `staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir` — an
  independent pre-existing flake, characterized below.

`PoisonError` count in reading 3 was **0** with a real failure present — a second, independent
confirmation of task 1 beyond the paired measurement.

**The `concurrent_ship_advances_finish_both_phases_independently` flake the plan asked about did not
occur** in either `check.sh all` run, so those runs confirm nothing either way about it. Recording
the absence rather than claiming a confirmation I did not observe.

## The finding that qualifies this plan's purpose

Reading 2 is the one that matters most, and it is not a flake.

| State | `phase7_cli` | `check.sh all` exit |
|---|---|---|
| `&[Stage::Code]` (as committed) | **17 passed; 0 failed** | **0** |
| widened to all five stages | **12 passed; 5 failed** | **101** |

Same tree, same commit, only the constant differs — so the widening causes them. The unwidened row
is the negative control; without it, "5 failed" would be equally consistent with an
already-red suite.

The five all fail with the same `background-task notification delivery is ABSENT` refusal, at
`Stage::Define`: `parallel_creates_two_worktrees_and_spawns_two_monitors`,
`start_defaults_to_worktree`, `start_no_worktree_uses_feature_branch`,
`start_until_plan_halts_cleanly`, `start_worktree_mode_ignores_main_checkout_divergence`.

**Recorded, not fixed**, for two reasons — both stated rather than assumed:

1. `crates/devflow-cli/tests/phase7_cli.rs` is outside this plan's `files_modified` and outside
   task 2's `<files>`; it is reached only by a command none of the plan's acceptance criteria run.
2. The obvious repair (setting `DEVFLOW_CLAUDE_LEGACY_LAUNCH=true` on the spawned `devflow`
   process) is plausibly right for four of the five, but
   **`parallel_creates_two_worktrees_and_spawns_two_monitors` is the multi-plan wave test, and the
   delivery canary exists precisely to guarantee that wave does not silently orphan concurrent
   work.** Pinning it to the legacy path could delete exactly the coverage the canary was added to
   provide — the "repaired by the wrong mechanism" failure this plan was written to avoid. That
   judgment belongs to whoever owns the canary's contract.

## The hostile-GIT_DIR flake: mechanism, not attribution

`staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir` failed
3 times across ~12 full-suite runs, at both widened and unwidened states, and passes in isolation
(`1 passed`, 278 filtered out — a real match, not the `--exact`-matches-nothing trap, which this
run hit once and which CLAUDE.md already documents).

**I did not attribute it from run counts.** Those were taken (0 failures in 5 baseline runs vs 2 in
7 with the change) and they **do not distinguish the two trees** — Fisher exact p ≈ 0.47. Reporting
"0/5 vs 2/7" as though it implicated the change would have been reading noise as signal.

Instead the mechanism was reproduced directly. The test spawns a child copy of the test binary,
which inherits the parent's `PATH` at spawn time while the outer half holds no lock; a concurrent
`NeutralPath` region replaces `PATH` with a tempdir and deletes it on drop.

| Child `PATH` | Result |
|---|---|
| normal | `1 passed` |
| a directory that does not exist | **`FAILED`, `left: Indeterminate, right: Stale`, `staleness.rs:1039`** |

The second row reproduces the observed failure's message and line number exactly; the first is the
control showing the probe is not simply broken. **Not attributable to `env_lock`**: the test never
acquires `ENV_MUTEX`, and the two tests that deliberately panic do so inside `catch_unwind` without
holding the lock, so they never poison it.

## What these results do NOT establish

- **They do not establish that the suite survives widening.** They establish that the **binary**
  suite does. `check.sh all` is red under the same widening, and that is the single most important
  qualification in this document.
- **They do not establish that any stage SHOULD be widened.** A green suite under widening says
  nothing about whether `Stage::Define` or `Stage::Validate` belongs on the stream path. That rests
  on per-stage production capture evidence, which is plan 34-05's deliverable and is untouched here.
- **`0 failed` under widening is one observation per reading, not a rate.** Reading 1 was taken
  once; given a demonstrated ~25% flake in the same suite, a single green run is a weak bound.
- **The paired measurement's "25" is not a fixed number.** Cascade size depends on thread
  scheduling — how many `ENV_MUTEX` tests had yet to run when the poison landed. The load-bearing
  claim is that the two counts differ by an order of magnitude and that the post-change count is
  exactly 1, not that "25" would recur.
- **Task 1 is not proven to fix the ROADMAP-1472 cascade**, because that test did not fire during
  this plan. The de-amplification is proven by the induced fault, on a different trigger.
- **`279 passed` is regression surface**, not evidence about this plan. Only the six repaired tests
  and the accessor bear on it.

## Deviations from Plan

### 1. [Measurement correction] 57 `ENV_MUTEX` call sites, not 58

- **Found during:** task 1, inventorying before the sweep.
- **Issue:** the plan states "There are 58 sites: 57 of the form `let _guard = ENV_MUTEX.lock().unwrap();`
  and 1 bare", with `test_support.rs` contributing 1. `test_support.rs` has **no call site** — its
  single pre-edit match is prose inside `NeutralPath`'s doc comment (`turning every subsequent
  ENV_MUTEX.lock().unwrap() into a PoisonError panic`). `rg -c` counts matching lines, and a
  doc-comment line was counted as code.
- **Consequence for the acceptance criterion:** "reports no matches in any file" is **not
  achievable** without deleting the very paragraph the plan told me to cross-reference. The sweep
  covered all 57 real sites; 2 prose mentions remain by design. A future regression check should use
  `rg -n 'ENV_MUTEX\.lock\(\)' crates/devflow-cli/src/ | rg -v '///'`.
- **Fix:** none needed; measurement recorded rather than the plan's number repeated.

### 2. [Prediction correction] The legible widened run named 6 genuine failures, not 1

Recorded in full above. The plan's objective states the arithmetic "42 failed − 41 poison = 1 root"
as exact; measured at this HEAD with the cascade removed it is 6. Four had been masked, not merely
amplified. The plan's own instruction not to assume the list is exactly one test is what caught it.

### 3. [Scope boundary — recorded, not fixed] 5 `phase7_cli` integration failures under widening

See "The finding that qualifies this plan's purpose" and `deferred-items.md`. No source change made,
no fix attempted.

### 4. [Scope boundary — recorded, not fixed] The hostile-GIT_DIR flake

Pre-existing; mechanism reproduced; not attributable to this plan's change. See above.

### 5. [Tooling] The WINDOWS.md ledger append was refused by a pre-existing integrity error

`gsd-tools windows append` refused both entries with:

```
Error: Ledger counts disagree with entries: frontmatter open/waived/fixed/total=1/1/4/6
but entries yield 0/1/5/6.
```

The inconsistency is pre-existing and in a shared cross-phase artifact. I did **not** hand-repair it:
this is a worktree agent, the orchestrator owns shared-file writes, and CLAUDE.md prefers GSD
commands over hand-editing `.planning/`. Both findings are instead recorded in the phase's
`deferred-items.md`, which is committed. **The ledger still needs the counts reconciled and these two
entries added.**

### 6. [Verification habit] `--exact` with a bare test name matched nothing

`cargo test ... embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir -- --exact`
returned `0 passed; 0 failed; 279 filtered out` and **exit 0**. The module path is required:
`staleness::tests::...` gives `1 passed; 278 filtered out`. CLAUDE.md already records this trap; it
is noted because it occurred live and a less careful read would have taken "exit 0" as a pass.

The plan's `--bin devflow` selector was correct throughout, as the executor briefing said — no
`--lib` correction was needed in this plan.

## Known Stubs

None. No placeholder values, no `TODO`/`FIXME` added, no skipped or ignored tests, no assertions
removed. Both induced assertions (`"induced"`, `"NC-8 control"`) were removed and their absence
verified by `rg`; neither appears in any commit. The temporary five-stage widening was applied twice
and reverted twice, verified by `git status --porcelain` (empty) and by reading the constant back —
`STREAM_JSON_STAGES` is `&[Stage::Code]` in every one of this plan's three commits.

The one substantive incompleteness is **not a stub** but the integration-suite gap recorded above.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern or schema change. Every change in this
plan is inside `#[cfg(test)]` code or test fixtures; no production code path is touched, and
`STREAM_JSON_STAGES` — the only production-visible constant in scope — is byte-identical to its
pre-plan value.

One threat-adjacent note: `env_lock()` deliberately suppresses a safety signal (mutex poisoning). Its
soundness is **conditional** on `NeutralPath` and `ReapMonitorOnDrop` restoring process state during
unwind, and the doc comment states that as a condition — naming what a future author would silently
break by replacing either guard with a trailing statement.

## Rollback

`git revert --no-commit c3def74^..d5bb97d` and commit once.

**Ordering constraint.** Do not revert this plan while plan 34-05's widening is in place: task 2's
six opt-outs are what keep those tests green under widening, and reverting them under a widened
constant turns the binary suite red — demonstrated directly by this plan's own widened run. If only
34-06 must be withdrawn, narrow `STREAM_JSON_STAGES` back to `&[Stage::Code]` in the same operation.

Task 1's accessor is **independently revertible** in principle, but reverting it restores the
cascade, and any subsequent failure count in this crate becomes uninterpretable again.

## Self-Check: PASSED

- `.planning/phases/34-.../34-06-SUMMARY.md` — being written now; committed in this step.
- `.planning/phases/34-.../deferred-items.md` — FOUND on disk and in commit `d5bb97d`.
- `c3def74`, `d1f1ff0`, `d5bb97d` — all FOUND in `git log c95c96a..HEAD`.
- All six modified source files — FOUND, modified and committed; `git status --porcelain` empty
  before this commit.
- `STREAM_JSON_STAGES` — confirmed `&[Stage::Code]` at `pipeline_launch.rs:470`; neither temporary
  widening appears in any commit.
- Both induced assertions — confirmed absent from the tree and from all three commits.
- STATE.md and ROADMAP.md deliberately NOT modified — worktree mode; the orchestrator owns those
  writes after the wave completes.
