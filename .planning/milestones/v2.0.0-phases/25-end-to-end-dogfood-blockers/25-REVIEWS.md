---
phase: 25
reviewers: [codex]
reviewed_at: 2026-07-28T13:10:00Z
plans_reviewed:
  - 25-11-PLAN.md
  - 25-12-PLAN.md
  - 25-13-PLAN.md
scope_note: >
  Scoped to the three gap-closure plans of round 2. Plans 25-01..25-09 are executed and
  merged; 25-10 is superseded by 25-13. Reviewing them would have diluted the signal and
  cost ~300KB of prompt budget for no decision value.
prompt_substitution: >
  25-CI-OBSERVATION.md (7KB, the authoritative live-gap record) was supplied in place of
  25-RESEARCH.md (72KB, almost entirely about the already-closed units 25a-25d).
single_reviewer: true
codex_version: codex-cli 0.145.0
codex_model: CLI default (review.models.codex unset)
---

# Cross-AI Plan Review — Phase 25 (gap-closure round 2)

## Codex Review

**Summary**
Plans 25-11 and 25-12 are mostly well-grounded and address the real `fork()`→`execve()` `/proc/cmdline` race. Plan 25-13 has a blocking bookkeeping flaw: it requires the remote branch to remain at the observed SHA, but later pushes evidence commits that necessarily move the remote to a new SHA. Until that is fixed, the closure evidence criteria cannot be satisfied as written.

**Strengths**
- 25-11's vulnerable-site hypothesis matches the code. The observed failing test spawns `sh` and immediately reads `discover_stray_devflow_processes()` at `commands.rs:3670` and `commands.rs:3679`. Additional real vulnerable sites exist at `agent.rs:634`, `commands.rs:4794`, and `reap_strays_e2e.rs:90`.
- The barrier is conceptually justified. The repo already documents that `/proc/<pid>/cmdline` reports parent argv during the fork/exec window at `agent.rs:175`, while `agent_running` only proves liveness, not exec visibility, at `agent.rs:36`.
- 25-12 puts the production mitigation at the right boundary: `doctor` only reads findings via `commands.rs:2998`, while `gate_sweep --reap-strays` turns the census into signalling at `commands.rs:1149` and `commands.rs:1298`.
- 25-13 correctly identifies the evidence-shape asymmetry. The pre-push hook runs `scripts/check-in-container.sh all` at `scripts/hooks/pre-push:129`; that uses 2-core pinning by default at `check-in-container.sh:57`; `all` is fmt → clippy → test at `check.sh:56`. CI's `Test` job only runs `scripts/check.sh test` at `.github/workflows/ci.yml:53`.

**Concerns**
- **HIGH — 25-13's final remote-ref criterion is internally impossible.** Task 1 records `HEAD_SHA`, pushes it, and requires `origin/feature/phase-25` to equal it (`25-13-PLAN.md:333`). Task 1 then commits `25-CI-TRIALS.md` locally and explicitly does not push it (`:379`). Task 2 later pushes the artifact and `STATE.md` commits, creating a new SHA (`:581`). But final verification still requires `origin/feature/phase-25` to equal the artifact `head_sha` (`:851`). Those cannot both be true after the evidence push.
- **MEDIUM — 25-11 likely exposes test plumbing as stable public API unnecessarily.** The plan adds public `wait_for_exec_visibility` in `agent.rs` (`25-11-PLAN.md:301`). This repo already has a test-only cross-crate helper surface via `test_support` at `lib.rs:76`, enabled by the `test-support` feature at `devflow-core/Cargo.toml:12` and consumed by CLI tests at `devflow-cli/Cargo.toml:20`. The function is useful, but public API is stickier than needed.
- **MEDIUM — 25-12's age floor is a pragmatic heuristic, not a proof.** The 2-second `STRAY_MIN_AGE` is reasonable for avoiding mid-exec false positives, but it intentionally delays reaping any true stray younger than the floor. The plan acknowledges this at `25-12-PLAN.md:148`, but the summary/acceptance should preserve that as delayed cleanup, not "cannot signal a process inside the window" in an absolute sense.
- **LOW — committing RED states is unnecessary risk.** 25-11 and 25-12 both instruct committing failing tests before implementation (`25-11-PLAN.md:288`, `25-12-PLAN.md:240`). Since the branch is later pushed as a large stack, this worsens bisectability without adding much value.

**Suggestions**
- Split 25-13's SHA fields: `tested_head_sha` for the 11 observations, and `artifact_head_sha` / `evidence_commit_sha` for the final pushed docs state. Do not require `origin/feature/phase-25` to equal `tested_head_sha` after pushing artifact commits.
- Prefer putting `wait_for_exec_visibility` under `devflow_core::test_support` unless there is a real non-test consumer.
- In 25-12 docs, phrase `STRAY_MIN_AGE` as "refuses to reap candidates younger than the floor; rerun after the floor elapses" rather than implying perfect classification.
- Squash or avoid RED commits before the final push; keep the TDD evidence in summaries/logs instead.

**Risk Assessment**
**HIGH until 25-13 is corrected.** The code-facing fixes in 25-11/25-12 are mostly sound, but the observation plan's final remote-state acceptance is contradictory and can fail despite a good fix, or tempt the executor to rewrite evidence semantics mid-run. Once that bookkeeping is fixed, residual risk drops to MEDIUM due to the public API and age-floor tradeoffs.

---

## Orchestrator Verification of Findings

Single reviewer — no cross-AI consensus is available, so each finding was independently
checked against source rather than counted by agreement.

| Finding | Verdict | Evidence |
|---|---|---|
| HIGH — 25-13 remote-ref contradiction | **CONFIRMED** | Read `25-13-PLAN.md` at all four cited lines. Task 1 Step 2 requires `origin/feature/phase-25 == HEAD_SHA`; Task 2 mandates a push of the artifact + STATE commits ("Only now, with all five trials `completed`, push the artifact and STATE commits"), which necessarily advances origin past the trials' `head_sha`; `<verification>` item 2 then requires origin to print "the artifact's `head_sha`". Unsatisfiable as written. The plan-checker missed this — it verified each task internally but not the cross-task SHA invariant. |
| MEDIUM — `wait_for_exec_visibility` should be `test_support` | **CONFIRMED, and cleaner than codex realised** | `crates/devflow-core/src/lib.rs:76-79` gates `pub mod test_support` behind `#[cfg(any(test, feature = "test-support"))]`; `devflow-core/Cargo.toml:12` defines the feature; `devflow-cli/Cargo.toml:20` **already** enables it as a dev-dependency. So `test_support` satisfies 25-11's own must_haves truth (callable from all three consumer layers) with no new plumbing, and there is direct precedent — `test_support::git_command`, added by 999.37 for exactly this cross-crate-test-helper reason. |
| MEDIUM — age-floor phrasing overstates | **PARTLY CONFIRMED** | `25-12-PLAN.md:148` does disclose the tradeoff, so this is not a hidden defect. But the `must_haves` truth at `:18` reads "cannot send a signal to a process that is young enough to be inside its own `fork()`->`execve()` window" — and the verifier reads must_haves truths literally. Worth tightening to name the delayed-cleanup cost, since a true stray younger than 2s is also refused. |
| LOW — avoid RED commits | **REJECTED — argues against established project convention** | Codex did not check the repo's history. Committing a failing test before its fix is this project's deliberate, repeated practice, including three times in this very phase: `70ee8e3` "test(25-08): add failing test for CR-01", `4821b76` "test(25-08): add RED real-worktree regression test for CR-02", `6e34287` "test(25-09): add two release-anchor topology fixtures for CR-03" — all merged. The RED commit *is* the evidence that the test fails without the fix, which is precisely what this phase was burned for lacking. Do not act on this. |

### What to act on

1. **HIGH, blocking** — 25-13's SHA invariant. Codex's own fix (split `tested_head_sha` from
   `evidence_commit_sha`) is correct and minimal.
2. **MEDIUM** — move `wait_for_exec_visibility` into `devflow_core::test_support`.
3. **MEDIUM** — tighten 25-12's `must_haves` truth at `:18` to name the delayed-cleanup cost.
4. **LOW** — no action; see rejection above.

### Caveat on this review's weight

One reviewer, so nothing here carries cross-AI corroboration. Codex did read the repo — every
`file:line` citation it gave checked out against source, including the four in 25-13 that
establish the HIGH finding — so the findings are source-grounded rather than impressionistic.
But a single model's blind spots are uncorrected: it did not check project history before
filing the RED-commit concern, and it did not evaluate whether the barrier itself can produce
a false positive (review-prompt question 2), which remains unexamined by any reviewer.
