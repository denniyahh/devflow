---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
plan: 05
subsystem: infra
tags: [rust, worktree, loop-back, regression-test, negative-control]

requires:
  - phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
    provides: "33-01's select_loop_back_fix helper and its three handle_validate_outcome call sites (the D-01 decision point this plan corrects)"
  - phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
    provides: "33-03's mid-arc/forward-progress signal separation, and 33-04's ENV_MUTEX + agent_free_git_only_path_dir PATH-neutralization idiom for tests that reach launch_stage"
provides:
  - "select_loop_back_fix takes an `evidence_root` rather than the main checkout, so the Validate->Code loop-back decision reads the working tree the Validate agent actually wrote to"
  - "All three in-scope loop-back arms (Ambiguous gate, consecutive-failure gate, plain-Failed tail) resolve that root through the codebase's existing plain worktree-fallback idiom"
  - "worktree_mode_genuine_gaps_loop_back_issues_gaps_only — the first test in the workspace to configure state.worktree_path on a handle_validate_outcome drive"
  - "worktree_mode_mid_arc_loop_back_issues_plain_execute — the mirrored negative control, including a both-roots discriminator scenario"
affects: [dogfood, validate-code-loop, worktree-mode, phase-34]

actuals:
  tokens: 3841
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Evidence-root vs project-root separation at a decision point: probe the agent's cwd for authored artifacts, keep the main checkout for git-object reads"
    - "Paired worktree-mode regression test + mirrored negative control, each observed failing against a deliberately broken implementation"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs

key-decisions:
  - "Bound the returned FixType to a local at each call site so the shared borrow of `state` ends before loop_back_to_code takes it mutably — no .to_path_buf() allocation was needed"
  - "Used the plain worktree fallback, not hook_context_root's .exists()-filtered variant: a vanished worktree means the evidence is gone with it, and falling back would resurrect a stale or other-branch artifact as this phase's"
  - "Added scenario B (artifact in the main checkout only) beyond the verification's prescribed pair, and measured its discrimination rather than arguing it"
  - "Left REQUIREMENTS.md untouched and requirements-completed empty — commit 79916a0 reverted a premature DOGFOOD-01 checkbox on this exact requirement, and this plan's evidence is unit-level only"

patterns-established:
  - "Negative control as a hard requirement: every new decision test is run against a stubbed always-wrong implementation and the observed panic recorded verbatim"
  - "rg-count prohibitions: an untouched-by-design call site is discharged by a mechanical count, not by a claim that it was not edited"

requirements-completed: []

coverage:
  - id: D1
    description: "A {N}-VERIFICATION.md existing only inside a phase's worktree dispatches FixType::GapsOnly on the Validate->Code loop-back (ROADMAP criterion 2)"
    requirement: DOGFOOD-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::worktree_mode_genuine_gaps_loop_back_issues_gaps_only"
        status: pass
    human_judgment: false
  - id: D2
    description: "A phase with no {N}-VERIFICATION.md in its worktree dispatches FixType::FullExecute, including when a stale artifact is present in the main checkout (ROADMAP criterion 1)"
    requirement: DOGFOOD-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute"
        status: pass
    human_judgment: false
  - id: D3
    description: "DOGFOOD-01 end-to-end: a real worktree-mode `devflow start` run completes its Code<->Validate loop without a human-gate stall"
    requirement: DOGFOOD-01
    verification: []
    human_judgment: true
    rationale: "No test in this plan runs DevFlow in worktree mode against a real git repository with a real agent. The evidence here is unit-level plus 33-REVIEW.md's scratch-repo probe; the end-to-end claim belongs to a dogfood run."

duration: 14min
completed: 2026-08-05
status: complete
---

# Phase 33 Plan 05: Evidence-Root Correction for the Validate→Code Loop-Back Summary

**`select_loop_back_fix` now reads `{N}-VERIFICATION.md` from the phase's worktree instead of the main checkout, making `FixType::GapsOnly` reachable on the Validate path in worktree mode for the first time — proven by a test that failed with the inverted value against the unchanged code.**

## Performance

- **Duration:** 14 min
- **Started:** 2026-08-05T04:19:18Z
- **Completed:** 2026-08-05T04:32:48Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments

- Closed both of `33-VERIFICATION.md`'s `gaps:` entries with the one production change they share: `select_loop_back_fix`'s first parameter is now `evidence_root`, resolved at all three call sites from `state.worktree_path` with a fallback to `project_root`.
- Reproduced CR-01 inside the suite for the first time. The new worktree-mode test failed against unchanged production code with `left: String("FullExecute")` / `right: "GapsOnly"` — a green suite had been certifying an inverted decision.
- Added the mirrored negative control with two scenarios, and **observed** both of its discrimination properties against deliberately broken implementations rather than arguing either from the code path.
- Upheld the hardest prohibition mechanically: `phase_commit_count` still reads the main checkout, discharged by an `rg` count rather than by a claim.

## Task Commits

1. **Task 1: Thread the evidence root end-to-end, RED-first** — `12f12e6` (fix)
2. **Task 2: Add the mirrored negative control, and prove it discriminates** — `e9a5eb2` (test)
3. **Task 3: Confirm the fix workspace-wide** — measurement only, no source change; carried by the metadata commit below.

## Files Created/Modified

- `crates/devflow-cli/src/pipeline_outcomes.rs` — `select_loop_back_fix`'s parameter rename and probe retarget; the three call-site root resolutions; doc-comment records of why the two adjacent reads use different roots; two new tests.

---

## Task 1 — the RED, verbatim

Run against **unchanged** production code, before any part of the fix existed:

```
thread 'pipeline_outcomes::tests::worktree_mode_genuine_gaps_loop_back_issues_gaps_only' (2915650)
panicked at crates/devflow-cli/src/pipeline_outcomes.rs:1598:9:
assertion `left == right` failed: a {N}-VERIFICATION.md existing only in the phase's worktree must still dispatch GapsOnly
  left: String("FullExecute")
 right: "GapsOnly"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 269 filtered out; finished in 0.00s
```

**This was an assertion failure, explicitly not any of the alternatives that would have invalidated it:**

- **Not a compile error** — the binary built and the test ran (`running 1 test`).
- **Not the `expect` on a missing `loop_back` event** — the event *was* recorded; the drive printed `looping back to Code (validate failures: 1)` and the `expect` did not fire. The failure is `assertion left == right failed`, which is the `assert_eq!` on the event's `fix` field.
- **Not an unrelated panic** — the panic site (`:1598`) is the `assert_eq!` in this test, and the message is this test's own.

The observed value is `FullExecute` against an expected `GapsOnly` — the exact inversion CR-01 describes. This failure **is** CR-01, reproduced inside the suite.

The `269 filtered out` also independently re-measures the pre-plan bin baseline as **269**, matching the planner's figure rather than inheriting it.

### The three `rg` counts, including the prohibition discharge

| Check | Pre-fix baseline | Post-fix | Required |
|---|---|---|---|
| `select_loop_back_fix\(project_root` | 4 | **0** | 0 |
| `unwrap_or\(project_root\)` | 0 | **3** | 3 |
| `phase_commit_count\(project_root` | 1 | **1** | 1 |
| `fn select_loop_back_fix\(evidence_root: &Path` | 0 | **1** | 1 |
| `phase_verification_exists\(evidence_root` | 0 | **1** | 1 |

The stale-root check was run as `rg -c --include-zero` and its printed value tested as a string — `rg -c --include-zero` prints `0` but **exits 1** on no match, so reading its exit code would have reported a failed step at exactly the moment the criterion was satisfied.

**The `phase_commit_count` prohibition is discharged by that count of `1`, as a prohibition upheld — not as a change made.** The 999.66 commit-count read still takes the main checkout, deliberately, because git refs and the object database are shared across a repository's worktrees.

### The three changed call sites, identified per-arm from the diff

Read from `git diff -U2`, not asserted from a count (a count cannot tell three distinct arms from one arm edited three times):

| Diff hunk | Arm | Enclosing construct |
|---|---|---|
| `@@ -286,9 +316,13 @@` | **Ambiguous gate** | the `return match run_gate(...)` inside the `ValidateOutcome::Ambiguous(detail)` arm |
| `@@ -338,9 +372,13 @@` | **Consecutive-failure gate** | the `return match run_gate(...)` inside the `state.mode.should_gate(...)` block |
| `@@ -349,9 +387,13 @@` | **Plain-Failed tail** | `match result { ... ValidateResult::Failed => ... }` |

**No `.to_path_buf()` allocation was needed.** Binding the returned `FixType` to a local immediately before `loop_back_to_code` ended the shared borrow of `state` before the mutable one began, and it compiled on the first attempt — no borrow error was ever produced.

### Preserved tests: green *and* unmodified

Both `--no-worktree` tests pass, but green was explicitly treated as insufficient evidence (a weakened test is also a green test). Confirmed from the diff instead: the Task 1 commit contains **exactly 17 deleted lines**, and all 17 are accounted for by the production change — the 2-line signature plus 5 lines at each of the three call sites. Zero deletions fall inside either preserved test, inside `handle_ship_outcome`, or anywhere else.

`handle_ship_outcome` (line 411) still constructs the bare `FixType::GapsOnly` literal directly at `:426` and does not consult the helper.

### The other two arms — and the limit of that evidence

`ambiguous_gate_loop_back_respects_the_mid_arc_check` and `failure_gate_loop_back_respects_the_mid_arc_check` both pass (`1 passed; 0 failed; 269 filtered out` each).

**What that does not establish:** both run with **no worktree configured**. They confirm the other two arms still compile and behave correctly on the *fallback* path, and they establish **nothing** about worktree-mode behavior for those arms. The evidence that all three arms share the corrected root is the diff plus the single shared helper — a structural argument, not a measurement.

---

## Task 2 — the negative control, and what it was observed to catch

### Scenario table

| Test | `state.worktree_path` | Phase | `{N}-VERIFICATION.md` written | Asserted `fix` |
|---|---|---|---|---|
| `worktree_mode_genuine_gaps_loop_back_issues_gaps_only` | `Some(wt)` | 93 | **worktree only** (root has no `.planning` at all) | `GapsOnly` |
| `worktree_mode_mid_arc_loop_back_issues_plain_execute` **scenario A** | `Some(wt)` | 94 | **nowhere** | `FullExecute` |
| `worktree_mode_mid_arc_loop_back_issues_plain_execute` **scenario B** | `Some(wt)` | 95 | **bare tempdir root only**, never the worktree | `FullExecute` |
| `genuine_gaps_loop_back_still_issues_gaps_only` (preserved) | `None` | 83 | root only | `GapsOnly` |
| `mid_arc_loop_back_issues_plain_execute_command` (preserved) | `None` | 82 | nowhere | `FullExecute` |

Scenario B's artifact path is built from `root_b`, never from `worktree_b` — writing it under the worktree by mistake would silently have turned scenario B into a duplicate of the positive test with an inverted, failing assertion. Both scenarios have their own tempdir and their own `State`, so there is no bleed; one `ENV_MUTEX` guard and one PATH neutralization cover both drives.

### Part C control 1 — always-`GapsOnly` stub

`select_loop_back_fix` temporarily stubbed to return `FixType::GapsOnly` unconditionally, ignoring the probe:

```
thread 'pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute' (2933891)
panicked at crates/devflow-cli/src/pipeline_outcomes.rs:1733:9:
assertion `left == right` failed: no {N}-VERIFICATION.md in the worktree (nor anywhere else) must dispatch FullExecute
  left: String("GapsOnly")
 right: "FullExecute"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 270 filtered out; finished in 0.01s
```

Failed on **scenario A's** assertion (`:1733` is the first `assert_eq!`), reporting the observed gaps-only value against the expected `FullExecute`. The control discriminates.

### Part C control 2 — the both-roots stub. Scenario B's power was OBSERVED, not argued.

The plan permitted arguing scenario B's detection power from the code path if demonstrating it was not cheap. **It was cheap, so it was measured.** The plain-Failed tail arm was temporarily rewritten to the both-roots misreading (`probe(worktree) || probe(project_root)`):

```
thread 'pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute' (2935264)
panicked at crates/devflow-cli/src/pipeline_outcomes.rs:1748:9:
assertion `left == right` failed: a {N}-VERIFICATION.md visible only from the main checkout belongs to a different run and must NOT resurrect GapsOnly
  left: String("GapsOnly")
 right: "FullExecute"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 270 filtered out; finished in 0.01s
```

It failed at `:1748` — **scenario B's** assertion, the second one — which means scenario A's assertion passed under that stub. The full claim was then measured rather than assumed, with the both-roots stub still in place:

| Test, under the both-roots stub | Result |
|---|---|
| `worktree_mode_genuine_gaps_loop_back_issues_gaps_only` | `1 passed` — **does not catch it** |
| `worktree_mode_mid_arc_loop_back_issues_plain_execute` scenario A | passed (failure occurred later, at B) — **does not catch it** |
| `genuine_gaps_loop_back_still_issues_gaps_only` | `1 passed` — **does not catch it** |
| `mid_arc_loop_back_issues_plain_execute_command` | `1 passed` — **does not catch it** |
| `worktree_mode_mid_arc_loop_back_issues_plain_execute` scenario B | **FAILED — the only case that catches it** |

So scenario B is measurably, not rhetorically, the sole discriminator against the both-roots misreading.

### Stub reversion, confirmed mechanically

Both stubs were reverted and the reversion confirmed by count rather than by eye:

- `unwrap_or\(project_root\)` → `3`, `phase_commit_count\(project_root` → `1`, `select_loop_back_fix\(project_root` → `0`, `fn select_loop_back_fix\(evidence_root: &Path` → `1`, `phase_verification_exists\(evidence_root` → `1`
- `rg -c --include-zero 'TEMPORARY STUB'` → **`0`** (queried with `--include-zero` so an absent match prints a real `0` rather than empty output that could be misread as a clean result)
- `git diff` between the Task 1 and Task 2 commits: **103 insertions, 0 deletions** — a purely additive, test-only change, so `select_loop_back_fix`, `handle_validate_outcome` and `handle_ship_outcome` are byte-identical to Task 1's end state.

---

## Task 3 — workspace confirmation and the honest bound

### Gate and counts, each on its own captured exit code

| Measurement | Exit code | Result | Baseline | Verdict |
|---|---|---|---|---|
| `scripts/check.sh all` | **0** | `==> check.sh: all OK` | — | pass |
| `cargo test -p devflow --bin devflow` | **0** | **271 passed**; 0 failed | 269 | 269 + 2 new — exact match |
| `cargo test -p devflow-core --lib` | **0** | **547 passed**; 0 failed | 547 | unchanged, as required |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0** | clean | — | pass |
| `cargo fmt --check` | **0** | clean | — | pass |

Each exit code was captured into a variable immediately after the invocation, never read off a pipeline. `cargo fmt --check` being clean also confirms fmt did not reflow the three call sites, so the `unwrap_or(project_root)` literal stayed intact on one line each — which the count criterion depends on.

The `devflow-core` count of **547** was re-measured here rather than inherited, and it matches the figure carried from `33-VERIFICATION.md`.

### The ten named loop-back tests

All ten in `pipeline_outcomes::tests::`, run in one exact-filtered invocation, **exit code 0**:

```
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 261 filtered out; finished in 0.24s
```

| # | Test | Result |
|---|---|---|
| 1 | `worktree_mode_genuine_gaps_loop_back_issues_gaps_only` (new) | ok |
| 2 | `worktree_mode_mid_arc_loop_back_issues_plain_execute` (new) | ok |
| 3 | `genuine_gaps_loop_back_still_issues_gaps_only` | ok |
| 4 | `mid_arc_loop_back_issues_plain_execute_command` | ok |
| 5 | `ambiguous_gate_loop_back_respects_the_mid_arc_check` | ok |
| 6 | `failure_gate_loop_back_respects_the_mid_arc_check` | ok |
| 7 | `healthy_multi_wave_progress_does_not_reach_the_ceiling` | ok |
| 8 | `repeated_failure_without_new_commits_still_reaches_the_ceiling` | ok |
| 9 | `consecutive_failures_reaches_ceiling_across_cycles` | ok |
| 10 | `ship_loop_back_still_issues_gaps_only_when_verification_absent` | ok |

`10 passed + 261 filtered out = 271`, the full bin count. A name that matched nothing would have shown as a shortfall in the passed count, so this aggregate rules out the silent-no-match failure mode more strongly than ten separate runs would.

**No test was re-run after a failure to obtain green.** The only failing runs in this plan were the three deliberate ones (Task 1's RED and Task 2's two stub controls), each of which was *required* to fail.

### Discharge table against both `gaps:` entries

| `missing:` item | Status | Discharged by |
|---|---|---|
| **Gap 1a.** Rename `select_loop_back_fix`'s first parameter to `evidence_root` and compute it at each of the three call sites as `state.worktree_path.as_deref().unwrap_or(project_root)` | **Closed** | Task 1 (`12f12e6`); counts `evidence_root`=1, `unwrap_or(project_root)`=3, stale-root=0 |
| **Gap 1b.** Do **NOT** change `phase_commit_count`'s root | **Upheld — prohibition, not a change** | `rg -c 'phase_commit_count\(project_root'` = **1**, identical to the pre-fix baseline. Recorded as a prohibition held, explicitly not as work done |
| **Gap 1c.** Add a regression test setting `worktree_path`, artifact under the worktree only, asserting `GapsOnly` | **Closed** | Task 1 — `worktree_mode_genuine_gaps_loop_back_issues_gaps_only`, with an observed RED |
| **Gap 1d.** Add the mirrored negative control: same worktree setup, no artifact anywhere, asserting `FullExecute` | **Closed** | Task 2 (`e9a5eb2`) — scenario A, with an observed control failure |
| **Gap 2a.** Same fix as criterion 1; do not file as two separate fixes | **Closed** | One production change in `12f12e6` closes both criteria; no second fix was written |
| **Gap 2b.** The worktree-mode test is criterion 2's own regression coverage and must sit **alongside** `genuine_gaps_loop_back_still_issues_gaps_only`, not replace it | **Closed** | The new test is placed immediately after it in file order; the preserved test is byte-unmodified (0 deletions inside it) |

Beyond the prescribed list: **scenario B** (artifact in the main checkout only) was added and its discrimination measured.

### The honest bound — what this evidence does and does not license

**It does establish** that the loop-back decision now reads the worktree when one is configured and falls back to the main checkout when one is not, and that both directions are covered by tests observed to fail against a broken implementation (three separate observed failures, not reasoning).

**It does NOT establish that a real `devflow start` run in worktree mode now completes its Code↔Validate loop end-to-end.** No test in this plan runs DevFlow in worktree mode against a real git repository with a real agent. Every test here drives `handle_validate_outcome` directly, inside a `tempfile::tempdir()`, with `PATH` neutralized so no agent can spawn. The evidence is unit-level plus `33-REVIEW.md`'s scratch-repo probe. **The end-to-end DOGFOOD-01 claim belongs to a dogfood run, not to this plan.**

**It does NOT establish anything about the other two loop-back arms under an actual worktree.** `ambiguous_gate_...` and `failure_gate_...` both run with `worktree_path` at `None`. That all three arms share the corrected root is a structural argument from the diff and the single shared helper — not a measurement.

**Still-open deferred items, named so their absence is recorded rather than inferred:**

- **WR-01** — the forward-progress reset removes the only unconditional bound on the Code↔Validate loop (accepted design tradeoff, T-33-06).
- **WR-02** — `phase_verification_exists` has no staleness invalidation. Deliberately not folded in, despite this plan editing that read's caller: it would put an unreviewed staleness heuristic in the same commit as the fix that has to be provable. Scenario B incidentally encodes part of *why* it matters.
- **WR-03** — a transient `git` failure records `Some(0)` and hands the next cycle a free counter reset.
- **WR-06** — three further tests seed `consecutive_failures` without PATH-neutralization. Note the two tests added here **do** neutralize PATH, so the count of unprotected tests did not grow.

---

## Decisions Made

- **Local-binding form at the call sites.** Binding `FixType` to a local before `loop_back_to_code` ends the shared borrow of `state` before the mutable one begins. It compiled first try; no `.to_path_buf()` was needed and none was added.
- **Plain fallback, not the `.exists()`-filtered variant.** `hook_context_root` picks a directory to *write* into, so a vanished worktree must degrade to somewhere writable. This picks a root to *probe* for evidence, where a vanished worktree means the evidence is gone with it and falling back would resurrect a stale or other-branch artifact as this phase's. All four in-repo precedents use the plain form. The reasoning is now recorded in `select_loop_back_fix`'s doc comment.
- **`requirements-completed: []`, and REQUIREMENTS.md untouched.** See "Needs your decision" below.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 3's verify loop used bare test names, which match nothing under `--exact`**

- **Found during:** Task 1 Step 3, and again in Task 3.
- **Issue:** The plan's Task 3 `<automated>` loop iterates bare names (`for t in worktree_mode_genuine_gaps_loop_back_issues_gaps_only ...`). Under `--exact`, libtest requires the **full module path**. Observed live: `cargo test -p devflow --bin devflow genuine_gaps_loop_back_still_issues_gaps_only -- --exact` returned `test result: ok. 0 passed; 0 failed; 270 filtered out` and **exited 0** — this repo's documented false-green trap, reproduced by the plan's own verify command.
- **Fix:** Prefixed every name with `pipeline_outcomes::tests::`. All ten then matched and passed.
- **Why this was not silently green:** the plan's loop greps for a positive `1 passed` with a non-zero `filtered out`, so bare names would have printed `MISS` ten times and exited `10`. The plan fails safe here; it simply could not have succeeded as written.
- **Verification:** `10 passed; 0 failed; 261 filtered out`, exit 0.

**2. [Rule 1 - Bug] My own doc comment contaminated the `unwrap_or(project_root)` count criterion**

- **Found during:** Task 1 Step 2 acceptance checks.
- **Issue:** The first draft of `select_loop_back_fix`'s doc comment quoted the literal `state.worktree_path.as_deref().unwrap_or(project_root)`. `rg -c` then printed **4**, not the required 3 — three real call sites plus my own prose. The extra match was located (`rg -n`: lines 251, 318, 374, 389) rather than assumed benign; it was a self-inflicted false positive on a check whose entire job is to detect a missed arm.
- **Fix:** Reworded the doc comment to describe the idiom without reproducing the literal, keeping the count a clean signal for code sites only. Count returned to **3**.
- **Verification:** `rg -n 'unwrap_or\(project_root\)'` now shows exactly the three call sites.

**3. [Rule 3 - Blocking] Auto-mode key mismatch at the tracer feedback gate — flagged, not silently resolved**

- **Found during:** the tracer gate after Task 1.
- **Issue:** The executor spec reads `workflow.auto_advance` to decide whether a tracer's checkpoint is auto-approved. That key **does not exist** in this project's config (`gsd-tools query config-get workflow.auto_advance` → `Error: Key not found`). This project uses `workflow.auto_mode: true`, with `_auto_chain_active: false`. Read literally, auto mode would be inactive and the tracer gate would halt the plan after Task 1 — destroying the wave, since the orchestrator force-removes the worktree on return.
- **Fix:** Treated auto mode as active, on the basis of `workflow.auto_mode: true` plus the plan's own `autonomous: true` frontmatter, and ran the **autonomous** form of the tracer gate: re-ran the tracer's `<verify>` end-to-end against the committed state (`1 passed; 0 failed; 269 filtered out`) before touching any expansion task.
- **Surfaced rather than assumed:** this is a judgment call about a config-schema mismatch, recorded here for the operator rather than buried.

### Plan-sanctioned additions

**4. Second negative control (both-roots stub).** The plan said to measure scenario B's discrimination "if cheap" and to say plainly if it was only argued. It was cheap, so it was measured — see Task 2 Part C control 2. This upgrades scenario B's justification from a code-path argument to an observation.

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 bug) + 1 plan-sanctioned addition.
**Impact on plan:** No scope creep. Deviations 1 and 2 were defects in the plan's *verification apparatus*, not in its design — both would have produced misleading measurements. Deviation 3 is a workflow-config mismatch, not a code change.

## Issues Encountered

- **The plan's own verify command reproduced this repo's documented `--exact` false-green trap.** Worth recording: the trap is not hypothetical, it recurs, and it recurred inside a plan that explicitly warned about it. Assert on `N passed` with a non-zero `filtered out`; never on the exit code.
- **Two unexplained `test result:` lines appeared in the full bin log.** Investigated rather than waved off: they come from two pre-existing tests (`tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir`, `embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`) that re-invoke the test binary as a subprocess, so their child output lands in the parent's log. Benign, pre-existing, unrelated to this change. The authoritative line is `271 passed; 0 filtered out`.

## Needs your decision

**Should DOGFOOD-01 be marked complete?** I left `requirements-completed: []` and did **not** touch `.planning/REQUIREMENTS.md`, which still reads `[ ]` / "Gaps Found".

Reasoning: commit `79916a0` reverted a *premature* DOGFOOD-01 checkbox on this exact requirement after gaps were found, and this plan's own honest bound says the end-to-end claim is not established. Flipping it now on unit-level evidence would repeat that incident. The plan also states explicitly that no task here touches REQUIREMENTS.md.

The two ROADMAP criteria this plan targets (1 and 2) are closed at the unit level. Whether that is sufficient to close DOGFOOD-01, or whether it should wait for a worktree-mode dogfood run, is your call — not mine to assume.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- The evidence-root defect is closed and the test-coverage hole that hid it is closed with it. `FixType::GapsOnly` is reachable on the Validate path in worktree mode for the first time.
- **Recommended next:** a worktree-mode dogfood run, which is the only thing that can discharge D3 / the end-to-end DOGFOOD-01 claim.
- WR-01, WR-02, WR-03 and WR-06 remain open and are unaffected by this change.

## Self-Check

Verified in this session, not asserted:

- `crates/devflow-cli/src/pipeline_outcomes.rs` — exists, modified, committed.
- Commit `12f12e6` — present on `worktree-agent-ade090119f368baf3`.
- Commit `e9a5eb2` — present on `worktree-agent-ade090119f368baf3`.
- Every test count, exit code and panic text quoted above was read from an actual run in this session.

**Self-Check: PASSED**

---
*Phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99*
*Completed: 2026-08-05*
