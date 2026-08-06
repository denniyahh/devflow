---
phase: 33
slug: loop-back-correctness-for-multi-wave-validate-code-cycles-99
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-04
validated: 2026-08-05
---

# Phase 33 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust's built-in `cargo test`, inline `#[cfg(test)] mod tests` colocated with each modified source file — no separate test directory or config file for this workspace's Rust crates |
| **Config file** | none — the workspace `Cargo.toml` plus each crate's own `Cargo.toml`; test behavior is governed by `scripts/check.sh`, not a pytest/jest-style test-config file |
| **Quick run command** | `cargo test -p devflow --bins pipeline_outcomes::` (and `mode::` / `state::` as needed) — package name is `devflow`, not `devflow-cli` |
| **Full suite command** | `scripts/check.sh all` (fmt + clippy `-D warnings` + `cargo test --workspace --no-fail-fast`) |
| **Estimated runtime** | Not benchmarked by research — crate-scoped `cargo test` slices are expected to complete in low tens of seconds; full-suite runtime is whatever `scripts/check.sh all` already takes today |

---

## Sampling Rate

- **After every task commit:** Run the relevant crate-scoped quick command above (e.g. `cargo test -p devflow --bins pipeline_outcomes::`)
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
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (999.65, criterion 1) | T-33-01 | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues plain `/gsd-execute-phase {N}`, not `--gaps-only` | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::mid_arc_loop_back_issues_plain_execute_command -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (999.65, criterion 2) | T-33-01 | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists) still issues `--gaps-only` — the negative control for the row above | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::genuine_gaps_loop_back_still_issues_gaps_only -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (D-01 probe) | T-33-01 | `phase_verification_exists` answers correctly for absent tree, present dir without artifact, and present artifact | unit | `cargo test -p devflow-core --lib agent_result::tests::phase_verification_exists_finds_the_artifact_by_prefix -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-01-T1 | 33-01 | 1 | DOGFOOD-01 (command rendering) | T-33-02 | `FixType::FullExecute` renders the unflagged execute command and is distinguishable from `GapsOnly` | unit | `cargo test -p devflow-core --lib prompt::tests::fix_prompts_select_the_right_command -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — extended | ✅ green |
| 33-01-T2 | 33-01 | 1 | DOGFOOD-01 (999.65, gated arms) | T-33-01 | The Ambiguous-gate and consecutive-failure-gate loop-back arms agree with the ungated tail | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::ambiguous_gate_loop_back_respects_the_mid_arc_check -- --exact 2>&1 \| rg "1 passed"` and `...::failure_gate_loop_back_respects_the_mid_arc_check -- --exact` | ✅ Exists — verified via `--list`s | ✅ green |
| 33-01-T2 | 33-01 | 1 | DOGFOOD-01 (D-02 scope boundary) | T-33-01 | The out-of-scope Ship loop-back still issues `--gaps-only` under the same precondition that flips the Validate arms | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::ship_loop_back_still_issues_gaps_only_when_verification_absent -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-02-T1 | 33-02 | 1 | DOGFOOD-02 (backward-compat) | T-33-05 | `last_validate_failure_commit_count` round-trips, and absent-from-JSON reads as `None` rather than a present zero | unit | `cargo test -p devflow-core --lib state::tests::last_validate_failure_commit_count_round_trips_through_serde -- --exact` and `...::last_validate_failure_commit_count_absent_from_json_defaults_to_none -- --exact`, each `2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list`s | ✅ green |
| 33-02-T2 | 33-02 | 1 | DOGFOOD-02 (reset predicate) | T-33-06 | `consecutive_failures_made_progress` reports progress only for no-prior-record or a strictly higher count | unit | `cargo test -p devflow-core --lib mode::tests::made_progress_treats_no_prior_record_as_progress -- --exact` and `...::made_progress_requires_a_strictly_higher_count -- --exact`, each `2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list`s | ✅ green |
| 33-03-T1 | 33-03 | 2 | DOGFOOD-02 (count extraction) | T-33-07 | `phase_commit_count` returns 0 for a missing branch; extracting it from `evaluate_layer2` changes no Layer-2 behavior | unit | `cargo test -p devflow-core --lib agent_result::tests::phase_commit_count_reports_zero_without_a_branch -- --exact 2>&1 \| rg "1 passed"` and `cargo test -p devflow-core --lib agent_result:: 2>&1 \| rg "test result: ok"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-03-T2 | 33-03 | 2 | DOGFOOD-02 (999.66, criterion 4 — no-repo path) | T-33-07 | Same unresolved problem with no commits at all still reaches the ceiling and gates | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::consecutive_failures_reaches_ceiling_across_cycles -- --exact 2>&1 \| rg "1 passed"` — existing guard, must pass with a byte-unchanged body | ✅ Exists — regression guard | ✅ green |
| 33-03-T3 | 33-03 | 2 | DOGFOOD-02 (999.66, criterion 3) | T-33-06 | 3+ healthy wave transitions with new commits landing between cycles do not false-gate | unit/integration | `cargo test -p devflow --bins pipeline_outcomes::tests::healthy_multi_wave_progress_does_not_reach_the_ceiling -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-03-T3 | 33-03 | 2 | DOGFOOD-02 (999.66, criterion 4 — live-branch path) | T-33-06 | Repeated failure on an existing branch with a static commit count still reaches the ceiling — the negative control for the row above, and a path the pre-existing guard never reaches | unit/integration | `cargo test -p devflow --bins pipeline_outcomes::tests::repeated_failure_without_new_commits_still_reaches_the_ceiling -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Rows added 2026-08-05 — coverage the seeded map predates

The map above was seeded by `plan-phase` on 2026-08-04, before plans 33-04, 33-05 and 33-06
existed. Those three plans added coverage of a class the original map had none of: **worktree-mode**
loop-back decisions. Recorded here so the map reflects the phase as built, not as planned.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 33-05-T1 | 33-05 | 3 | DOGFOOD-01 (999.65, criterion 2 — worktree mode) | T-33-14 | With `{N}-VERIFICATION.md` present ONLY inside the phase's worktree, the loop-back issues `--gaps-only`. Structurally impossible to satisfy under the pre-fix code, which read the main checkout | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::worktree_mode_genuine_gaps_loop_back_issues_gaps_only -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-05-T2 | 33-05 | 3 | DOGFOOD-01 (999.65, criterion 1 — worktree mode) | T-33-14 | With no artifact anywhere, worktree mode still issues plain `/gsd-execute-phase {N}` — kills an "always `GapsOnly` when a worktree exists" implementation | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-06-T3 | 33-06 | 4 | DOGFOOD-01 (999.65, anti-OR discriminator) | T-33-14 | With the artifact in the MAIN CHECKOUT ONLY, the decision is still `FullExecute`. **The only test in the workspace that fails a "probe both roots and OR them" implementation.** Split out by 33-06 (IN-06) so a sibling failure can no longer abort before it runs | unit | `cargo test -p devflow --bins pipeline_outcomes::tests::worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator -- --exact 2>&1 \| rg "1 passed"` | ✅ Exists — verified via `--list` | ✅ green |
| 33-04-T1 | 33-04 | 3 | DOGFOOD-01/02 (test-harness integrity) | T-33-09, T-33-13 | The restored gated-arm and abort tests reach their asserted gate paths, with branch-pinning assertions and a neutralized `PATH` so no real agent can spawn during `cargo test` | unit | `cargo test -p devflow --bins pipeline_gate:: 2>&1 \| rg "test result: ok"` | ✅ Exists — verified via `--list` | ✅ green |

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

- [x] `test_support::commit_on_feature_branch(root, phase, label)` — lands one real commit on `feature/phase-NN`, callable repeatedly within a single test. Owned by **33-03-T3**. Mirrors `devflow-core`'s test-module-private `init_repo_with_feature_commits` (`monitor.rs:2385`), which cannot be imported across crates. Must use create-or-`checkout`, never `checkout -B`, which would reset an existing branch to HEAD and discard commits an earlier call in the same test made. **Delivered** (`test_support.rs:112-137`). Carries a known post-condition — it never restores the prior checkout; filed as part of 999.81 / DEN-103 (IN-04).
- [x] The thirteen new unit tests named in the Per-Task Verification Map above — 33-01 contributes six, 33-02 four, 33-03 three (the helper's own zero-branch test plus the matched multi-wave pair). One further existing test, `prompt::fix_prompts_select_the_right_command`, is extended rather than added. **All present**, confirmed by `--list` membership against a fabricated-name negative control. Three further worktree-mode tests arrived later via 33-05/33-06 and are recorded in the post-seed rows above.
- [x] No new test framework install needed — `cargo test` is already the workspace's only test runner, and this phase adds no `Cargo.toml` dependency. **Held** — no dependency was added across all six plans.

---

## Manual-Only Verifications

The seeded map claimed "None". That was true of the *unit* decisions and false of the phase goal as
worded, so it is corrected here rather than left flattering.

| # | Behavior | Why it cannot be automated today | How to discharge |
|---|----------|----------------------------------|------------------|
| M-01 | "A 3+ wave unattended `devflow start` phase completes its Code↔Validate loop" — the ROADMAP goal as literally worded | Every test in this phase drives a tempdir with `PATH` neutralized so no agent can spawn. Nothing exercises a real agent, a real multi-wave run, or the `state.worktree_path` population path in `commands.rs` | A dogfood run of a later phase against a binary built from this branch. Rebuild first — an older binary does not carry these fixes |
| M-02 | Linked-`git worktree` semantics | All three worktree-mode tests build their "worktree" with `create_dir_all` — a plain directory, not a linked worktree. Adequate for `phase_verification_exists`, which is a pure `fs` stat and never shells to git; **not** adequate for `phase_commit_count`, whose correctness rests on refs and the object database being shared across linked worktrees | Add one integration test that creates a real `git worktree add` tree. Flagged as an open question on 999.80 |
| M-03 | The three WR-06 sites' "always resolves to `Abort`" property | Content-dependent on a magic substring in a JSON note, asserted nowhere | Routed to 999.80 / DEN-102 — either harden structurally or assert the Abort resolution |

---

## Validation Audit 2026-08-05

| Metric | Count |
|--------|-------|
| Mapped requirements | 16 |
| Covered (automated, green) | 16 |
| Missing | 0 |
| Manual-only | 3 |
| Rows added post-seed (33-04/05/06 coverage) | 4 |

**Method.** Test existence established with `cargo test --list` (272 bin / 547 core listed), not with
`--exact`, which exits 0 when the filter matches nothing. A deliberately fabricated name was checked
against the same lists as a negative control and correctly reported absent. Green status inherits
the full-workspace run at merge commit `a203ab1` (272 bin / 547 core, zero failures); confirmed by
diff that only `.planning/` files have changed since, so no source is unmeasured.

**Defect found and fixed in this audit.** Every `-p devflow --lib` command in the seeded map was
broken — `devflow` is a binary crate, so that form errors with `no library targets found`. All
occurrences corrected to `--bins`. Note the failing invocation still printed `EXIT=0` when piped,
which is this repo's documented pipeline-exit-code trap; the map's own preamble warns about the
sibling `--exact` trap but was itself written with an unrunnable command form.

No gaps were MISSING, so per the workflow's Step 3 short-circuit the `gsd-nyquist-auditor` was not
spawned and no tests were generated.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags — `cargo test` is one-shot by default
- [x] Feedback latency < 60s — exact-name slices complete in hundredths of a second
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** verified 2026-08-05
