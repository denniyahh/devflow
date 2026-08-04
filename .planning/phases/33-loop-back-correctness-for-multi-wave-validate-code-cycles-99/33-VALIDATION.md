---
phase: 33
slug: loop-back-correctness-for-multi-wave-validate-code-cycles-99
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-04
---

# Phase 33 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust's built-in `cargo test`, inline `#[cfg(test)] mod tests` colocated with each modified source file — no separate test directory or config file for this workspace's Rust crates |
| **Config file** | none — the workspace `Cargo.toml` plus each crate's own `Cargo.toml`; test behavior is governed by `scripts/check.sh`, not a pytest/jest-style test-config file |
| **Quick run command** | `cargo test -p devflow --lib pipeline_outcomes::` (and `mode::` / `state::` as needed) — package name is `devflow`, not `devflow-cli` |
| **Full suite command** | `scripts/check.sh all` (fmt + clippy `-D warnings` + `cargo test --workspace --no-fail-fast`) |
| **Estimated runtime** | Not benchmarked by research — crate-scoped `cargo test` slices are expected to complete in low tens of seconds; full-suite runtime is whatever `scripts/check.sh all` already takes today |

---

## Sampling Rate

- **After every task commit:** Run the relevant crate-scoped quick command above (e.g. `cargo test -p devflow --lib pipeline_outcomes::`)
- **After every plan wave:** Run `cargo test --workspace --no-fail-fast`
- **Before `/gsd-verify-work`:** `scripts/check.sh all` must be green
- **Max feedback latency:** 60 seconds (target, not a benchmark — these are fast Rust unit-test slices, not integration tests)

---

## Per-Task Verification Map

Task IDs reconciled against the real plan set on 2026-08-04 (`33-01`, `33-02`, `33-03`). Every
automated command below asserts on a literal `1 passed` / `test result: ok` match rather than an
exit code — this repo's CLAUDE.md records that `cargo test --exact` exits 0 when the filter matches
nothing, and the CLI package is `devflow`, not `devflow-cli`.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (999.65, criterion 1) | T-33-01 | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues plain `/gsd-execute-phase {N}`, not `--gaps-only` | unit | `cargo test -p devflow --lib pipeline_outcomes::tests::mid_arc_loop_back_issues_plain_execute_command -- --exact 2>&1 \| rg "1 passed"` | ❌ Wave 0 — new test | ⬜ pending |
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (999.65, criterion 2) | T-33-01 | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists) still issues `--gaps-only` — the negative control for the row above | unit | `cargo test -p devflow --lib pipeline_outcomes::tests::genuine_gaps_loop_back_still_issues_gaps_only -- --exact 2>&1 \| rg "1 passed"` | ❌ Wave 0 — new test | ⬜ pending |
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (D-01 probe) | T-33-01 | `phase_verification_exists` answers correctly for absent tree, present dir without artifact, and present artifact | unit | `cargo test -p devflow-core --lib agent_result::tests::phase_verification_exists_finds_the_artifact_by_prefix -- --exact 2>&1 \| rg "1 passed"` | ❌ Wave 0 — new test | ⬜ pending |
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (command rendering) | T-33-02 | `FixType::FullExecute` renders the unflagged execute command and is distinguishable from `GapsOnly` | unit | `cargo test -p devflow-core --lib prompt::tests::fix_prompts_select_the_right_command -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — extended | ⬜ pending |
| 33-01-T2 | 33-01 | 1 | DOGFOOD-01 (999.65, gated arms) | T-33-01 | The Ambiguous-gate and consecutive-failure-gate loop-back arms agree with the ungated tail | unit | `cargo test -p devflow --lib pipeline_outcomes::tests::ambiguous_gate_loop_back_respects_the_mid_arc_check -- --exact 2>&1 \| rg "1 passed"` and `...::failure_gate_loop_back_respects_the_mid_arc_check -- --exact` | ❌ Wave 0 — new tests | ⬜ pending |
| 33-01-T2 | 33-01 | 1 | DOGFOOD-01 (D-02 scope boundary) | T-33-01 | The out-of-scope Ship loop-back still issues `--gaps-only` under the same precondition that flips the Validate arms | unit | `cargo test -p devflow --lib pipeline_outcomes::tests::ship_loop_back_still_issues_gaps_only_when_verification_absent -- --exact 2>&1 \| rg "1 passed"` | ❌ Wave 0 — new test | ⬜ pending |
| 33-02-T1 | 33-02 | 1 | DOGFOOD-02 (backward-compat) | T-33-05 | `last_validate_failure_commit_count` round-trips, and absent-from-JSON reads as `None` rather than a present zero | unit | `cargo test -p devflow-core --lib state::tests::last_validate_failure_commit_count_round_trips_through_serde -- --exact` and `...::last_validate_failure_commit_count_absent_from_json_defaults_to_none -- --exact`, each `2>&1 \| rg "1 passed"` | ❌ Wave 0 — new tests | ⬜ pending |
| 33-02-T2 | 33-02 | 1 | DOGFOOD-02 (reset predicate) | T-33-06 | `consecutive_failures_made_progress` reports progress only for no-prior-record or a strictly higher count | unit | `cargo test -p devflow-core --lib mode::tests::made_progress_treats_no_prior_record_as_progress -- --exact` and `...::made_progress_requires_a_strictly_higher_count -- --exact`, each `2>&1 \| rg "1 passed"` | ❌ Wave 0 — new tests | ⬜ pending |
| 33-03-T1 | 33-03 | 2 | DOGFOOD-02 (count extraction) | T-33-07 | `phase_commit_count` returns 0 for a missing branch; extracting it from `evaluate_layer2` changes no Layer-2 behavior | unit | `cargo test -p devflow-core --lib agent_result::tests::phase_commit_count_reports_zero_without_a_branch -- --exact 2>&1 \| rg "1 passed"` and `cargo test -p devflow-core --lib agent_result:: 2>&1 \| rg "test result: ok"` | ❌ Wave 0 — new test | ⬜ pending |
| 33-03-T2 | 33-03 | 2 | DOGFOOD-02 (999.66, criterion 4 — no-repo path) | T-33-07 | Same unresolved problem with no commits at all still reaches the ceiling and gates | unit | `cargo test -p devflow --lib pipeline_outcomes::tests::consecutive_failures_reaches_ceiling_across_cycles -- --exact 2>&1 \| rg "1 passed"` — existing guard, must pass with a byte-unchanged body | ✅ Exists — regression guard | ⬜ pending |
| 33-03-T3 | 33-03 | 2 | DOGFOOD-02 (999.66, criterion 3) | T-33-06 | 3+ healthy wave transitions with new commits landing between cycles do not false-gate | unit/integration | `cargo test -p devflow --lib pipeline_outcomes::tests::healthy_multi_wave_progress_does_not_reach_the_ceiling -- --exact 2>&1 \| rg "1 passed"` | ❌ Wave 0 — new test | ⬜ pending |
| 33-03-T3 | 33-03 | 2 | DOGFOOD-02 (999.66, criterion 4 — live-branch path) | T-33-06 | Repeated failure on an existing branch with a static commit count still reaches the ceiling — the negative control for the row above, and a path the pre-existing guard never reaches | unit/integration | `cargo test -p devflow --lib pipeline_outcomes::tests::repeated_failure_without_new_commits_still_reaches_the_ceiling -- --exact 2>&1 \| rg "1 passed"` | ❌ Wave 0 — new test | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### What this map does not cover

Recorded here rather than discovered later. The commit-count forward-progress signal cannot
distinguish a commit that addressed Validate's finding from one that did not (threat T-33-06,
33-RESEARCH.md § D-03 Recommendation, Assumptions Log A1). Criterion 4's guarantee therefore holds
for loops producing **no** commits, which is what both criterion-4 rows above assert, and does not
hold for a stuck agent that commits something trivial each cycle. No test in this map covers that
case because none can — it is an accepted limitation of the chosen signal, not a coverage gap to
close, and it is written into source in two places so a future strengthening pass finds it stated.

### Flagged assumption carried from planning

The deterministic edge-probe engine returned `unclassified`/`unresolved` for both DOGFOOD-01 and
DOGFOOD-02 (`applicable: 2, resolved: 0`). Its heuristics have no coverage for internal
state-machine correctness. Edge coverage for this phase is therefore derived from CONTEXT.md's
D-01/D-02/D-03 and 33-RESEARCH.md's test map, as reflected in the rows above — a "0 resolved edge
cases" report for phase 33 is that known non-classification, not a planning omission.

---

## Wave 0 Requirements

- [ ] `test_support::commit_on_feature_branch(root, phase, label)` — lands one real commit on `feature/phase-NN`, callable repeatedly within a single test. Owned by **33-03-T3**. Mirrors `devflow-core`'s test-module-private `init_repo_with_feature_commits` (`monitor.rs:2385`), which cannot be imported across crates. Must use create-or-`checkout`, never `checkout -B`, which would reset an existing branch to HEAD and discard commits an earlier call in the same test made.
- [ ] The thirteen new unit tests named in the Per-Task Verification Map above — 33-01 contributes six, 33-02 four, 33-03 three (the helper's own zero-branch test plus the matched multi-wave pair). One further existing test, `prompt::fix_prompts_select_the_right_command`, is extended rather than added.
- [ ] No new test framework install needed — `cargo test` is already the workspace's only test runner, and this phase adds no `Cargo.toml` dependency.

---

## Manual-Only Verifications

*None — all phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
