---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
verified: 2026-08-05T12:16:49Z
status: passed
score: 4/4 ROADMAP success criteria verified; 26/26 must-haves verified (20 carried from the prior
  pass + 6 new from 33-06's must_haves.truths)
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: passed
  previous_score: "4/4 ROADMAP success criteria; 20/20 plan-level truths (33-01..33-05)"
  gaps_closed: []
  gaps_remaining: []
  regressions: []
gaps: []
deferred:
  - truth: "WR-03 (33-REVIEW.md, carried across three plans, not closed by 33-06): a transient git failure records Some(0) as the baseline and hands the next successful measurement a free counter reset"
    addressed_in: "Backlog 999.77 / DEN-99. Not addressed in this milestone's roadmap. 33-06's own prohibitions explicitly forbid touching this."
    evidence: "33-06-PLAN.md prohibitions list: 'Do NOT fix WR-03 / the transient-git counter reset. Filed as backlog 999.77 / DEN-99.' Confirmed untouched: handle_validate_outcome's counter branch (pipeline_outcomes.rs:334-358) consumes the new evidence_root binding only via the surrounding hoist, its own logic unchanged."
  - truth: "WR-06 (33-REVIEW.md, carried, not closed): 3 of 4 tests seeding consecutive_failures directly still lack PATH-neutralization"
    addressed_in: "Not addressed in this milestone's roadmap. 33-06's own success criteria list it as deliberately untouched."
    evidence: "33-06-SUMMARY.md 'Findings deliberately NOT closed' table: 'WR-06 (spawn hardening on 3 of 4 sites) — open, carried'."
  - truth: "Ten pre-existing non-RAII PATH regions (WR-05's other half) remain trailing-statement restores, not the new NeutralPath RAII guard"
    addressed_in: "Not addressed in this milestone's roadmap; 33-06 explicitly scoped itself to only the two regions 33-05 added."
    evidence: "33-06-PLAN.md prohibitions: 'Do NOT retrofit NeutralPath to the ten PRE-EXISTING PATH regions... Widening to all twelve is a bigger mechanical sweep... belongs in its own plan.' Confirmed by rg: NeutralPath::install appears at exactly 3 sites (pipeline_outcomes.rs:1632, 1688, 1752), all inside the two 33-05-added regions (one became two after the IN-06 split)."
  - truth: "WR-05's ENV_MUTEX poison-recovery half (unwrap_or_else(PoisonError::into_inner)) is not implemented"
    addressed_in: "Not addressed in this milestone's roadmap; explicitly out of 33-06's scope."
    evidence: "33-06-SUMMARY.md: 'the ENV_MUTEX poisoning half of WR-05 ... is out of this plan's scope and remains open.'"
  - truth: "New CR-01-class defect at evaluate_layer0 (agent_result.rs:2041-2042) reads external_verify_commands from project_root, one call site over from the fixed defect"
    addressed_in: "Backlog 999.76 / DEN-98, filed with a test-rewrite scope. Predates the phase-33 merge base; not touched by any of 33-06's 3 files."
    evidence: "33-06-PLAN.md prohibitions: 'Do NOT fix CR-01 in agent_result.rs:2041-2042 (evaluate_layer0). It is filed as backlog 999.76 / DEN-98... You are editing the same FILE — stay out of that function.' Confirmed the plan's 18-line diff to agent_result.rs touches only phase_review_path (:2549) and phase_verification_exists (:2588-2596), nowhere near :2041-2042."
  - truth: "WR-01, WR-02, WR-04, IN-01 through IN-05 (33-REVIEW.md, carried across all plans)"
    addressed_in: "Not addressed in this milestone's roadmap."
    evidence: "33-06-SUMMARY.md 'Findings deliberately NOT closed' table lists all as 'open, carried'."
behavior_unverified_items: []
---

# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles Verification Report

**Phase Goal:** A 3+ wave unattended `devflow start` phase can complete its Code↔Validate loop
without gating on an impossible `--gaps-only` command or a false "3 consecutive failures" ceiling.

**Verified:** 2026-08-05
**Status:** passed
**Re-verification:** Yes — fourth pass, after plan 33-06 (commits `3aff2c7`, `670be34`, `6b0d3c3`,
merged at `a203ab1`). The prior pass (2026-08-05T05:00:00Z) already found `passed` 4/4 with `gaps: []`.
33-06 is a refactor + test-hygiene plan closing five findings from the *third* code-review pass
(WR-07, WR-08, WR-05's two new instances, IN-06, IN-07) — all findings about phase 33's OWN new code,
none of the four ROADMAP success criteria. This pass does not re-trust that framing: it independently
re-derives the production-behavior-unchanged claim from current source and re-runs the behavioral
evidence live rather than accepting 33-06-SUMMARY's counts.

## Independent confirmation that 33-06 changed nothing observable about the four ROADMAP criteria

Read directly against HEAD, not inherited from any SUMMARY:

- **WR-07 closed, correctly scoped.** `phase_verification_exists` and `phase_review_path`
  (`agent_result.rs:2588`, `:2549`) both now take `evidence_root: &Path`. `phase_commit_count`
  (`:1841`) is confirmed still `project_root` — the prohibition's own negative control, re-run live
  in this pass (`rg -n "fn phase_verification_exists|fn phase_review_path|fn phase_commit_count"`
  shows all three lines in one command, so the unchanged one is a real hit, not an absence).
- **WR-08 closed, and the hoisted binding reaches all three arms.** `handle_validate_outcome` binds
  `let evidence_root: PathBuf = state.worktree_path.clone().unwrap_or_else(|| project_root.to_path_buf());`
  once, before the match. `rg -n "select_loop_back_fix\("` shows exactly one definition (`:261`) plus
  three call sites (`:335`, `:387`, `:400`), and reading each in context confirms all three pass
  `&evidence_root` — none reverted to the old `state.worktree_path.as_deref().unwrap_or(project_root)`
  inline form or to bare `project_root`. `rg -c "worktree_path.as_deref\(\).unwrap_or\(project_root\)"
  crates/devflow-cli/src/pipeline_outcomes.rs` returns 0 (re-run live, matching the plan's must-have
  verbatim), with the required positive control (`rg -c --no-heading` over `crates/`) still returning
  4 hits elsewhere (`staleness.rs:1`, `preflight.rs:2`, `agent_result.rs:1`) — confirming the 0 is
  real, not a broken pattern. `phase_commit_count(project_root` is confirmed still exactly 1 hit in
  this file (`:347`) — the prohibited-change negative control.
- **The `git diff -U0 5d439a80..HEAD -- pipeline_outcomes.rs` byte-identity claim holds.** Read
  directly in this pass. The three call-site diffs are pure comment-and-call-shape changes (replacing
  a repeated inline `state.worktree_path.as_deref().unwrap_or(project_root)` + a per-arm comment with
  `&evidence_root` + a one-line pointer comment) — no logic changed. The IN-06 split's two
  `assert_eq!` blocks (`last["fix"], "FullExecute"` for scenario A at phase 94, `last_b["fix"],
  "FullExecute"` for scenario B at phase 95) are present verbatim in the post-split source with their
  original messages word-for-word — confirmed by reading both new test bodies directly, not by
  trusting the diff summary alone. Nothing was rewritten; the diff is textually a cut-and-paste plus
  new doc comments.
- **IN-07 fixtures now say `Stage::Validate`.** All three `Stage::Code` → `Stage::Validate` diff
  hunks (`:1607`, near the split tests) read `state.stage = Stage::Validate` at HEAD; confirmed no
  assertion changed as a result, consistent with `select_loop_back_fix` never reading `state.stage`.
- **`NeutralPath` RAII guard is real, not just declared.** `test_support.rs` has `pub(crate) fn
  install()` and `impl Drop for NeutralPath`. Used at exactly 3 call sites in `pipeline_outcomes.rs`
  (`:1632`, `:1688`, `:1752`) — all inside the two 33-05-added regions (one became two after the split),
  confirming the plan's explicit "only these two regions" scope, not a wider retrofit.
- **File-scope check.** `git diff --stat 5d439a80..a203ab1 -- crates/` touches exactly the three
  files the plan declares (`agent_result.rs`, `pipeline_outcomes.rs`, `test_support.rs`) — no
  unexpected file was swept in by the merge.

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues plain `/gsd-execute-phase {N}`, not `--gaps-only` | ✓ VERIFIED | `worktree_mode_mid_arc_loop_back_issues_plain_execute` (now scenario-A-only after the IN-06 split) and its new sibling `worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator` (scenario B) both re-run live in this pass by exact name together: `2 passed; 0 failed; 270 filtered out`. Non-worktree companion `mid_arc_loop_back_issues_plain_execute_command` also re-run live in this pass: `1 passed`. **Negative control performed in this pass:** the identical `--exact` command against a fabricated name returns `0 passed; 0 failed; 272 filtered out` — the total filtered-out count (272, matching 33-06's reconciled bin total) confirms the two positive names matched real tests. |
| 2 | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists) still issues `--gaps-only` | ✓ VERIFIED | `worktree_mode_genuine_gaps_loop_back_issues_gaps_only` re-run live in this pass (grouped with the criteria-3/4 tests below): `ok`. Non-worktree companion `genuine_gaps_loop_back_still_issues_gaps_only` re-run live: `ok`. Both preserved byte-unchanged by 33-06 per its prohibition ("must NOT be deleted, renamed, merged, or have any assertion removed or loosened") — confirmed by direct read; neither appears in the `git diff -U0 5d439a80..HEAD` hunks at all. |
| 3 | 3+ wave phase making genuine forward progress does not false-gate at wave 3 | ✓ VERIFIED | `healthy_multi_wave_progress_does_not_reach_the_ceiling` re-run live: `ok`. `phase_commit_count` (the signal this depends on) confirmed still reads `project_root` — deliberately outside 33-06's scope (its own prohibition list names this function by name and forbids renaming it). |
| 4 | `consecutive_failures` still gates when Validate finds the SAME unresolved problem again | ✓ VERIFIED | `repeated_failure_without_new_commits_still_reaches_the_ceiling` and `consecutive_failures_reaches_ceiling_across_cycles` both re-run live: `ok` each. `handle_validate_outcome`'s counter branch (`:334-358`) confirmed untouched apart from now reading the hoisted `evidence_root` binding, which that branch does not use at all — it consumes `project_root` and `state.last_validate_failure_commit_count` only. |

**Score:** 4/4 ROADMAP success criteria verified — unchanged from the prior pass. **What this pass
adds beyond re-confirming the prior pass's finding:** independent proof that 33-06's refactor did not
regress any of the four criteria, via a live re-run of all 8 relevant named tests (6 grouped +
2 individually) in this pass, plus direct source reading of the diff itself rather than trusting
33-06-SUMMARY's before/after tables. **What this does NOT establish** (carried forward from the prior
pass, still true, not narrowed or widened by 33-06): every test exercising these four criteria drives
a tempdir with `PATH` neutralized, and the worktree-mode tests build their "worktree" with
`std::fs::create_dir_all`, not a linked `git worktree`. `phase_verification_exists` is filesystem-only
and never calls `git`, so the plain-directory stand-in faithfully exercises the exact code path under
test — but nothing here proves an actual multi-wave unattended run completes end-to-end against a real
spawned agent. That end-to-end claim is explicitly the next phase's job, not phase 33's, per the prior
pass's own conclusion (unchanged by 33-06, which touches none of the wiring that produces
`state.worktree_path` in a real run).

### 33-06 Must-Haves (from its own PLAN frontmatter, re-derived from source)

| # | Truth (abbreviated) | Status | Evidence |
|---|---|---|---|
| 1 | `phase_verification_exists`/`phase_review_path` name their root `evidence_root`, doc comment states the worktree-tracking rationale | ✓ VERIFIED | Confirmed by direct read of `agent_result.rs:2549`, `:2588`, and the doc block at `:2567-2586` naming the `.planning/`-is-tracked rationale and explicitly distinguishing this root from `phase_commit_count`'s. |
| 2 | Evidence root resolved ONCE as an owned `PathBuf`, all three arms use it; `rg -c` of the old triplicated idiom inside `handle_validate_outcome` returns 0 | ✓ VERIFIED | `let evidence_root: PathBuf = ...` at `:307-310` (pre-match); all three `select_loop_back_fix(&evidence_root, ...)` call sites confirmed by direct read; `rg -c` re-run live returns 0 with the required positive control confirming the pattern still matches elsewhere. |
| 3 | `NeutralPath` RAII guard exists, used by both regions 33-05 added | ✓ VERIFIED | `test_support.rs` has `install()`/`impl Drop`; 3 call sites in `pipeline_outcomes.rs`, all inside the two originally-added regions (one became two post-split). |
| 4 | `worktree_mode_mid_arc_loop_back_issues_plain_execute`'s two scenarios are two separate `#[test]` fns | ✓ VERIFIED | Confirmed by direct read: `worktree_mode_mid_arc_loop_back_issues_plain_execute` (scenario A, phase 94) and `worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator` (scenario B, phase 95) are two distinct `#[test]` functions; both re-run live independently in this pass, each producing its own pass/fail. |
| 5 | Every test driving `handle_validate_outcome` sets `Stage::Validate` | ✓ VERIFIED | All three sites in the touched tests confirmed `Stage::Validate` by direct read (`:1607`, and the two split tests' fixtures); no assertion changed as a result, per the plan's own STOP-if-changed condition, which did not fire. |
| 6 | Behavior unchanged: full suite at 271/547 baseline +1 net new (272/547) | ✓ VERIFIED (relying on orchestrator measurement, not re-run in this pass) | Orchestrator's post-merge `cargo test --workspace --no-fail-fast` at `a203ab1`: exit 0, 272 bin / 547 core, zero failures. This verification pass did NOT re-run the full workspace suite (constraint: run it at most once per verification pass; the orchestrator's run is more recent — post-merge — than any run this pass could produce). This pass independently re-ran 8 of the specific named tests live and got consistent counts (270 filtered out for 2-test group + itself = 272; 266 filtered out for the 6-test group + itself = 272), which is consistent with, but does not independently re-establish, the orchestrator's full-suite green. |

**Score:** 6/6 of 33-06's own must-haves verified.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `agent_result.rs::phase_verification_exists` / `phase_review_path` | `evidence_root`-named callee, contract doc comment | ✓ VERIFIED | Confirmed live at `:2549`, `:2588-2596`. |
| `pipeline_outcomes.rs::handle_validate_outcome` | single owned `evidence_root: PathBuf` binding, all 3 arms consume it | ✓ VERIFIED | Confirmed live, binding at `:307-310`, 3 call sites at `:335`, `:387`, `:400`. |
| `test_support.rs::NeutralPath` | RAII PATH guard, `Drop`-restored | ✓ VERIFIED | `install()` + `impl Drop for NeutralPath` present. |
| Split test pair (`worktree_mode_mid_arc_loop_back_issues_plain_execute` / `worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator`) | independently reportable scenario tests | ✓ VERIFIED | Both re-run live in this pass, `2 passed` grouped and confirmed each individually resolves to a real symbol via the negative-control filtered-out count. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `pipeline_outcomes.rs` (`handle_validate_outcome`) | `agent_result.rs` (`phase_verification_exists`) | `select_loop_back_fix(&evidence_root, ...)` from all 3 arms | ✓ WIRED, ✓ CORRECT ROOT | Confirmed all three arms pass the hoisted binding, none reverted. |
| `pipeline_outcomes.rs` | `agent_result.rs` (`phase_commit_count`) | still `project_root`, deliberately unchanged | ✓ WIRED, ✓ CORRECT ROOT (unchanged) | Confirmed at `:347`, exactly 1 hit. |
| `test_support.rs::NeutralPath` | `pipeline_outcomes.rs` tests | `NeutralPath::install()` in the two (now three, post-split) regions | ✓ WIRED | Confirmed 3 call sites. |

### Prohibitions (must-NOT checks, 33-06's own list)

| Prohibition | Status | Evidence |
|---|---|---|
| No production behavior may change; every existing assertion byte-identical | ✓ HOLDS | `git diff -U0` read directly: the only non-comment/non-signature changes to existing tests are `Stage::Code`→`Stage::Validate` (IN-07, sanctioned) and the trailing-PATH-restore→`NeutralPath::install()` block (WR-05, sanctioned) and the IN-06 split (sanctioned, verbatim-moved assertions confirmed by direct read of both post-split bodies). No `assert_eq!` expected-value or message text changed anywhere in the diff. |
| `phase_commit_count`'s root must NOT be changed / renamed | ✓ HOLDS | Confirmed live, still `project_root` at `agent_result.rs:1841` and `pipeline_outcomes.rs:347`. |
| Do NOT fix CR-01 in `evaluate_layer0` (`agent_result.rs:2041-2042`) | ✓ HOLDS | The 18-line diff to `agent_result.rs` touches only `:2549` and `:2567-2596` — nowhere near `:2041-2042`. Confirmed by reading the diff hunks directly. |
| Do NOT fix WR-03 (transient-git counter reset) | ✓ HOLDS | `handle_validate_outcome`'s counter branch (`:334-358`) unchanged in logic, only now reads `project_root` and `state` exactly as before; the hoisted `evidence_root` binding is not consumed by this branch. |
| Do NOT retrofit `NeutralPath` to the ten pre-existing PATH regions | ✓ HOLDS | 3 call sites total, all inside the two originally-33-05-added regions. |
| The two preserved `--no-worktree` tests must not be touched | ✓ HOLDS | Neither `genuine_gaps_loop_back_still_issues_gaps_only` nor `mid_arc_loop_back_issues_plain_execute_command` appears anywhere in the `git diff -U0 5d439a80..HEAD` output for this file. |
| No test weakened, ignored, deleted, or excluded | ✓ HOLDS | No `#[ignore]` introduced; no assertion softened; confirmed by direct diff read. |
| Neither split test may spawn a real agent process | ✓ HOLDS | Both wrap their drive in `ENV_MUTEX::lock()` + `NeutralPath::install()`, confirmed by direct read. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Scenario A + B (criterion 1) | `cargo test -p devflow --bins -- --exact pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute pipeline_outcomes::tests::worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator` | `2 passed; 0 failed; 270 filtered out` | ✓ PASS |
| Fabricated name (negative control) | same pattern, fabricated name | `0 passed; 0 failed; 272 filtered out` | ✓ Confirms the 272-count baseline and that the 2 positive names above matched real tests |
| Criteria 2/3/4 grouped (6 named tests: `worktree_mode_genuine_gaps_loop_back_issues_gaps_only`, `mid_arc_loop_back_issues_plain_execute_command`, `genuine_gaps_loop_back_still_issues_gaps_only`, `healthy_multi_wave_progress_does_not_reach_the_ceiling`, `repeated_failure_without_new_commits_still_reaches_the_ceiling`, `consecutive_failures_reaches_ceiling_across_cycles`) | `cargo test -p devflow --bins -- --exact <6 names>` | `6 passed; 0 failed; 266 filtered out` | ✓ PASS |
| `rg -c` triplicated idiom inside `pipeline_outcomes.rs` | `rg -c "worktree_path.as_deref\(\).unwrap_or\(project_root\)" crates/devflow-cli/src/pipeline_outcomes.rs` | no match (exit 1, 0 count) | ✓ PASS, with positive control (4 hits across 3 other files) confirming pattern validity |
| `phase_commit_count(project_root` negative-change control | `rg -n "phase_commit_count\(project_root" crates/devflow-cli/src/pipeline_outcomes.rs` | 1 hit at `:347` | ✓ PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | direct exit-code capture | exit 0 | ✓ PASS |
| `cargo fmt --all -- --check` | direct exit-code capture | exit 0 | ✓ PASS |
| `TBD`/`FIXME`/`XXX` scan across the 3 files 33-06 modified | `rg -n "TBD\|FIXME\|XXX"` per file | no matches | ✓ PASS (no debt markers) |
| Full-workspace test suite (272 devflow bin + 547 devflow-core) | relied on the orchestrator's post-merge measurement at `a203ab1` (exit 0, zero failures, zero `PoisonError`) — NOT re-run in this pass, per the "run the full suite at most once" constraint and because a fresher post-merge run already exists | exit 0, 0 failures | ✓ PASS (relied upon, not independently re-run this pass) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DOGFOOD-01 | 33-01, 33-05, 33-06 (hardening only) | Mid-arc vs genuine-gaps loop-back dispatch | ✓ SATISFIED | ROADMAP criteria 1 and 2 both VERIFIED, unchanged by 33-06. `.planning/REQUIREMENTS.md` now correctly reads `[x]` (line 19) and "Complete" (line 51) — the doc-hygiene staleness the prior pass flagged has since been resolved. |
| DOGFOOD-02 | 33-02, 33-03 | Forward-progress-aware consecutive-failures reset | ✓ SATISFIED | ROADMAP criteria 3 and 4 hold, unaffected by 33-06 (which touches neither `mode.rs` nor `state.rs`). `.planning/REQUIREMENTS.md` reads `[x]` (line 23) and "Complete" (line 52). |

No orphaned requirements. 33-06's own `requirements:` field declares `[DOGFOOD-01]`, consistent with
its scope (all five closed findings sit inside the D-01/CR-01 evidence-root mechanism or its test
coverage).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| `.planning/STATE.md` | frontmatter (`progress.completed_plans: 5`) | Tracking metadata not yet updated to reflect 33-06 as a completed plan (total_plans: 6, completed_plans still reads 5) | ℹ️ Info | Doc-hygiene only, not a phase-goal correctness issue. Last touched by commit `5d439a8` (33-06 plan authoring), before 33-06's execution/merge (`a203ab1`, `f07090b`). `f07090b` ("update tracking after wave 4") touched only `ROADMAP.md`, not `STATE.md`. Recommend a tracking-sync pass as part of closing this phase. |
| `crates/devflow-core/src/agent_result.rs` | 2041-2042 | Carried, unchanged: `evaluate_layer0` reads `external_verify_commands` from `project_root` instead of `execution_root` — same defect class as the original CR-01, one call site over | 🛑 Blocker-class defect, PRE-EXISTING and explicitly OUT OF SCOPE (999.76/DEN-98, and 33-06's own prohibition list names this exact line range and forbids touching it) | Confirmed the plan's diff does not go near these lines. Carried to `deferred`, not a phase-33 gap. |
| Various (see `deferred`) | — | WR-01, WR-02, WR-03, WR-04, WR-06, IN-01–IN-05, WR-05's ENV_MUTEX-poison half, the ten pre-existing PATH regions | ⚠️ Warning | All carried forward, all deliberately out of 33-06's stated scope per its own PLAN prohibitions and SUMMARY's "Findings deliberately NOT closed" table — none is one of the four ROADMAP success criteria. |

No `TBD`/`FIXME`/`XXX` debt markers found in any of the 3 files 33-06 modified (re-scanned directly
in this pass).

### Human Verification Required

None required to certify the four ROADMAP success criteria. As with the prior pass, one item remains
worth the operator's awareness without blocking the phase: no test in this repository (before or
after 33-06) drives a real linked `git worktree` or a real spawned agent end-to-end — the strongest
available confirmation of the phase's literal goal prose will come from actually running a 3+ wave
unattended phase against this binary, which remains the next phase's job, not something phase 33 can
self-certify. 33-06 does not change this limitation in either direction (it hardened the unit-test
mechanism, not the worktree-fidelity gap).

### Gaps Summary

**No gaps.** 33-06 is exactly what it claims to be: a refactor + test-hygiene plan that closed five
findings about phase 33's own new code (WR-07, WR-08, two new WR-05 instances, IN-06, IN-07) without
touching any of the logic the four ROADMAP success criteria depend on. This pass independently
re-derived that claim from current source rather than trusting 33-06-SUMMARY or 33-REVIEW.md:

- The hoisted `evidence_root` binding reaches all three `handle_validate_outcome` arms correctly —
  confirmed by direct read of all three call sites, not just the `rg -c` count.
- `phase_commit_count`'s root, the CR-01/999.76 boundary, WR-03, and the ten pre-existing PATH regions
  are all confirmed genuinely untouched, not merely claimed untouched.
- The IN-06 split moved two `assert_eq!` blocks byte-identically — read directly, not inferred from
  the diff's line-count summary.
- All 8 behaviorally relevant named tests (the 4 ROADMAP-criteria pairs plus their non-worktree
  companions) re-run live in this pass with real `N passed` counts and non-zero `filtered out`,
  including a negative control confirming the counts are meaningful.

Six items remain carried forward as non-blocking `deferred` findings, none of which is a ROADMAP
success criterion and none of which 33-06's own plan was scoped to fix: WR-03, WR-06, the ten
pre-existing PATH regions, WR-05's `ENV_MUTEX`-poison half, the new `evaluate_layer0` CR-01-class
defect (999.76/DEN-98), and the carried WR-01/WR-02/WR-04/IN-01–IN-05 set. One doc-hygiene item
(`STATE.md`'s `completed_plans` count) is noted for operator awareness — not a code-correctness gap.

The known flake (`concurrent_ship_advances_finish_both_phases_independently` racing on git
`index.lock`, cascading into `ENV_MUTEX` poisoning) is a documented pre-existing defect (STATE.md,
Phase 17/17-09 GAP-2), did not reproduce in the orchestrator's post-merge run, and is unrelated to any
code this phase touches. 33-06 does add one test (271→272 bin count), which changes harness scheduling
under the default parallel runner — honestly, whether that shifts the flake's frequency either way is
unproven, not established, in either direction; nobody re-ran the pre-change commit under equivalent
load to compare.

---

_Verified: 2026-08-05_
_Verifier: Claude (gsd-verifier)_
