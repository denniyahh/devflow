---
phase: 35
slug: loop-termination-and-baseline-correctness-999-77-999-78-999
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-06
validated: 2026-08-07
---

# Phase 35 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase` from `35-RESEARCH.md` § "Validation Architecture".
> The Per-Task Verification Map is deliberately unfilled at plan time — it is populated once
> PLAN.md task IDs exist.

**This phase's subject is the discipline this document enforces.** Every criterion here exists
because a test passed (or would pass) against both the buggy and the fixed code. A proxy
measurement is not a weak result in Phase 35 — it is the defect under repair. Treat every green
below as suspect until its named negative control has been run and seen to fail.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (standard Rust harness), workspace crates `devflow-core` and `devflow` |
| **Config file** | none dedicated — `scripts/check.sh` is the single definition of "green": `fmt`, `clippy --all-targets -- -D warnings`, `test` |
| **Quick run command** | `cargo test -p devflow-core --lib <module>::` or `cargo test -p devflow --bin devflow <module>::`. **Corrected during 35-01:** `-p devflow --lib` does not work — `devflow` is a binary-only crate (`crates/devflow-cli/src/main.rs`, no `src/lib.rs`), and cargo exits with `error: no library targets found in package 'devflow'`. Every `-p devflow --lib` command written into 35-01's PLAN was unrunnable as specified. |
| **Full suite command** | `scripts/check.sh all` (host) / `scripts/check-in-container.sh all` (pinned CI image) |
| **Measured runtime** | `scripts/check.sh all` **76.4s** (fmt + clippy + full workspace suite, 917 tests across 22 binaries, 0 failed). Targeted: `cargo test -p devflow-core --lib` **5.1-5.9s** (n=10), `cargo test -p devflow --bin devflow` **12.2-14.3s** (n=10). Measured 2026-08-07 during 35-01. **What these numbers do NOT cover:** all are *warm/incremental* — the workspace was already built, so they exclude a cold `cargo build` (minutes, dominated by `clap`/`tracing-subscriber`). They are one host, one run each for `check.sh`; the targeted figures are 10 runs and so carry a real spread, the `check.sh` figure is n=1 and carries none. |

**Package-name trap (CLAUDE.md).** devflow-core's package is `devflow-core`; **devflow-cli's
package is `devflow`**, not `devflow-cli` (`crates/devflow-cli/Cargo.toml:2`). `cargo test --exact
<name>` **exits 0 when the name matches nothing** — assert on a real `N passed` line with a
non-zero `filtered out` count. Never trust the exit code alone, and never trust a pipeline's exit
code (it is the last command's).

**`PATH`-mutating tests are serialized, not parallel.** Criteria 1 and 6 both install a `PATH`
guard. `test_support::env_lock()` (`crates/devflow-cli/src/test_support.rs:94`) is the mutex;
`NeutralPath` (`:327`) is the existing RAII precedent. A test that mutates `PATH` without holding
the guard corrupts unrelated tests non-deterministically — the failure will not point at the
offender.

---

## Sampling Rate

- **After every task commit:** targeted `cargo test -p <package> --lib <module>::` for the module
  touched, asserting a real `N passed` count
- **After every plan wave:** `scripts/check.sh all` (fmt + clippy + full suite)
- **Before `/gsd-verify-work`:** full suite green **and** criterion 4's performed revert
  demonstration recorded as evidence in the phase SUMMARY — `cargo test` does not reach it, so it
  needs explicit manual sign-off
- **Max feedback latency:** unmeasured — see Test Infrastructure

---

## Per-Task Verification Map

Filled by `/gsd-validate-phase` on 2026-08-07, after all 6 plans reported `status: complete`.

**Every row below was re-run live during the audit — none is transcribed from a SUMMARY's
self-reported `status: pass`.** Test *names* were confirmed against `cargo test -- --list` before
being run, and every targeted run was asserted on a real `N passed` line with a **non-zero
`filtered out`** count, because `cargo test --exact <name>` exits 0 when the name matches nothing.

| Task ID | Plan | Wave | Requirement | Secure Behavior | Test Type | Automated Command (`cargo test …`) | Status |
|---------|------|------|-------------|-----------------|-----------|------------------------------------|--------|
| T1 | 35-01 | 1 | HARDEN-01/07 · NC-1 | The absent-`git` harness genuinely disarms `git`, and restores `PATH` byte-identically on drop | unit | `-p devflow --bin devflow -- --exact test_support::tests::no_git_path_makes_git_unresolvable_and_restores_it` | ✅ green |
| T2 | 35-01 | 1 | HARDEN-01 · c1 · 999.77 | An unmeasurable count is `None`, never a forged `Some(0)`; a real branch-absent/invalid-range case still measures `Some(0)` | unit ×3 | `-p devflow-core --lib -- --exact agent_result::tests::phase_commit_count_reports_none_when_git_cannot_run agent_result::tests::phase_commit_count_reports_zero_without_a_branch agent_result::tests::phase_commit_count_reports_zero_when_the_range_is_invalid` | ✅ green |
| T2 | 35-01 | 1 | HARDEN-01 · c1 · 999.77 | Across a multi-cycle sequence a `None` does not overwrite the baseline — the streak accumulates | unit | `-p devflow --bin devflow -- --exact pipeline_outcomes::tests::validate_failure_with_unmeasurable_count_accumulates_the_streak` | ✅ green |
| T3 | 35-01 | 1 | HARDEN-07 · c6 · 999.87 | Layer 2 with exit 0 + `Stage::Code` + unrunnable `git` falls through instead of forging `Failed`; exit 137 still classifies `resource_killed`; a non-commit-gated stage keeps success | unit ×3 | `-p devflow --bin devflow -- --exact pipeline_outcomes::tests::evaluate_layer2_unrunnable_git_falls_through_to_layer3 pipeline_outcomes::tests::evaluate_layer2_unrunnable_git_still_classifies_exit_137_as_resource_killed pipeline_outcomes::tests::evaluate_layer2_unrunnable_git_keeps_success_for_a_non_commit_gated_stage` | ✅ green |
| T3 | 35-01 | 1 | HARDEN-07 · c6 · NC-12 | **The cascade** — the defect was removed, not pushed one layer down | unit | `-p devflow --bin devflow -- --exact pipeline_outcomes::tests::evaluate_agent_result_with_unrunnable_git_does_not_report_failed` | ✅ green |
| T3 | 35-01 | 1 | HARDEN-07 · c6 | Layer 3's unmeasurable arm is `Unknown` with no commit figure; its two pre-existing siblings pass byte-unchanged as opposite-result controls | unit ×3 | `-p devflow-core --lib -- --exact agent_result::tests::evaluate_layer3_unmeasurable_count_is_unknown_not_failed agent_result::tests::evaluate_layer3_falls_back_to_commit_count agent_result::tests::evaluate_layer3_zero_commits_is_failed_and_flags_human_review` | ✅ green |
| T1 | 35-02 | 1 | HARDEN-04 · c4 · 999.84 | Worktree-mode `GateReview` auto-decide reads `execution_root`, with D-05's decoy PLAN at `project_root` | integration | `-p devflow --bin devflow -- --exact pipeline_launch::tests::advance_with_worktree_declared_checkpoint_reads_the_execution_root` | ✅ green |
| T1 | 35-02 | 1 | HARDEN-04 · c4 | The predicate itself reads the execution root in worktree mode **and** still reads the project root without one — the two-direction pair | unit ×2 | `-p devflow-core --lib -- --exact verify::tests::phase_has_blocking_human_checkpoint_reads_the_execution_root_in_worktree_mode verify::tests::phase_has_blocking_human_checkpoint_still_reads_the_project_root_without_a_worktree` | ✅ green |
| T1–2 | 35-03 | 1 | HARDEN-05 · c5 · D1/D2 | `release --check` reports Viable/NotViable from a real `ssh-keygen -Y sign` probe, never from an `ssh-add -l` comparison; a key no agent holds is **Viable** (the 999.86 false negative) | unit ×2 | `-p devflow-core --lib -- --exact git::tests::ssh_signing_probe_reports_viable_with_on_disk_private_key git::tests::ssh_signing_probe_reports_not_viable_without_a_private_key` | ✅ green |
| T2 | 35-03 | 1 | HARDEN-05 · D3 · NC-10 | The probe cannot hang an unattended preflight, and the **calibrated** control proves `SSH_ASKPASS_REQUIRE` is what prevents it | unit ×2 | `-p devflow-core --lib -- --exact git::tests::ssh_signing_probe_does_not_block_on_an_encrypted_key git::tests::encrypted_key_blocks_without_the_askpass_require_env_var` | ✅ green |
| T2 | 35-03 | 1 | HARDEN-05 · D8 | **The probe is not captured by a controlling terminal's `/dev/tty` prompt** — the production `setsid` at `git.rs:1026` is load-bearing | unit (pty, 3 arms) | `-p devflow-core --lib -- --exact git::tests::the_signing_probe_is_not_captured_by_a_controlling_terminal` | ✅ green |
| T1–3 | 35-03 | 1 | HARDEN-05 · D4/D5/D7 · WR-01/WR-07 | Inline keys return `Unknown` unprobed; a timeout is `Unknown` while a rejection stays `NotViable`; no reason string leaks key or path; the probe workspace is unique, owner-only and panic-safe | unit ×5 | `-p devflow-core --lib -- --exact git::tests::inline_signing_key_returns_unknown_without_probing git::tests::a_probe_timeout_is_unknown_while_a_rejection_stays_not_viable git::tests::probe_workspace_name_is_unique_per_call git::tests::the_probe_workspace_is_owner_only_and_refuses_an_existing_path git::tests::the_probe_workspace_guard_removes_its_directory_on_unwind` | ✅ green |
| T3 | 35-03 | 1 | HARDEN-05 · c5 deletion | CLI-boundary behaviour survives the predictor's removal; no key material or path in output | integration ×10 | `-p devflow --test release_check` (whole target) | ✅ green |
| T1 | 35-04 | 2 | HARDEN-02 · c2 · 999.78 | The per-phase ceiling gates *at* the ceiling and not below it; the named predicate and `should_gate` agree on the boundary | unit ×3 | `-p devflow-core --lib -- --exact mode::tests::phase_failure_ceiling_gates_at_the_ceiling_not_below_it mode::tests::phase_failure_ceiling_predicate_agrees_with_should_gate mode::tests::phase_failure_ceiling_reached_has_the_same_boundary` | ✅ green |
| T1 | 35-04 | 2 | HARDEN-02 · c2 | `phase_validate_failures` round-trips through serde and defaults to zero when absent | unit ×2 | `-p devflow-core --lib -- --exact state::tests::phase_validate_failures_round_trips_through_serde state::tests::phase_validate_failures_absent_from_json_defaults_to_zero` | ✅ green |
| T2 | 35-04 | 2 | HARDEN-02 · c2 counter | A loop committing trivial `.planning/` artifacts every cycle **still** reaches the bound; healthy multi-wave progress does not | unit ×3 | `-p devflow --bin devflow -- --exact pipeline_outcomes::tests::phase_validate_failure_ceiling_gates_despite_trivial_commit_progress pipeline_outcomes::tests::repeated_failure_without_new_commits_still_reaches_the_ceiling pipeline_outcomes::tests::healthy_multi_wave_progress_does_not_reach_the_ceiling` | ✅ green |
| T2 | 35-04 | 2 | HARDEN-02 · c2 message · WR-03/WR-04 | The gate message reports the cumulative total (≠ the streak); the ceiling clause appears only at the ceiling; a *passing* Validate at the ceiling explains itself; the reset records what it spent | unit ×4 | `-p devflow --bin devflow -- --exact pipeline_outcomes::tests::ceiling_clause_appears_only_at_the_ceiling_even_in_supervise_mode pipeline_outcomes::tests::a_passing_validate_at_the_ceiling_explains_why_it_gated pipeline_outcomes::tests::the_ceiling_reset_records_the_total_it_spent pipeline_outcomes::tests::phase_validate_failures_increment_saturates` | ✅ green |
| T3 | 35-04 | 2 | HARDEN-02 · c2 `--force` | **The Open Risk, closed** — the total survives a forced restart and resets only on the two real events | unit ×3 | `-p devflow --bin devflow -- --exact commands::tests::phase_validate_failures_survive_a_forced_restart commands::tests::phase_validate_failures_reset_when_the_phase_completes pipeline_outcomes::tests::phase_validate_failures_reset_on_operator_approval_at_the_ceiling_gate` | ✅ green |
| T1 | 35-05 | 3 | HARDEN-03 · c3 · 999.79 | The verification fingerprint is stable, differs on content change, and is `None` when the artifact is absent | unit ×2 | `-p devflow-core --lib -- --exact agent_result::tests::phase_verification_fingerprint_differs_when_content_differs agent_result::tests::phase_verification_fingerprint_is_none_when_the_artifact_is_absent` | ✅ green |
| T2 | 35-05 | 3 | HARDEN-03 · c3 · WR-05 | `last_verification_fingerprint` round-trips and distinguishes an uncaptured baseline from an empty one | unit ×2 | `-p devflow-core --lib -- --exact state::tests::last_verification_fingerprint_round_trips_through_serde state::tests::last_verification_fingerprint_absent_from_json_defaults_to_none` | ✅ green |
| T3 | 35-05 | 3 | HARDEN-03 · c3 · NC-7 | **Two-directional**: a stale artifact dispatches `FullExecute`, a fresh one dispatches `GapsOnly`, and the four-row freshness truth table is exhaustive | unit ×3 | `-p devflow --bin devflow -- --exact pipeline_outcomes::tests::stale_verification_artifact_dispatches_full_execute pipeline_outcomes::tests::verification_written_this_run_dispatches_gaps_only pipeline_outcomes::tests::verification_freshness_truth_table_is_exhaustive` | ✅ green |
| T1–2 | 35-06 | 4 | D-08 release record | The `2.5.0` public-surface delta is accurate against source | diff review + `cargo doc` | *(editorial — see Manual-Only)* | ✅ manual |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Whole-suite baseline re-measured during this audit:** `./scripts/check.sh all` → **exit 0**,
`956 passed; 0 failed` across 22 binaries (fmt + `clippy --all-targets -- -D warnings` + full
workspace suite). The pre-audit figure was 954; the two added are D8's test and its re-entry helper.

### Requirement → verification, ahead of task IDs

> **Superseded 2026-08-07 by the Per-Task Verification Map above.** This table was written at plan
> time, so its `Exists?` column records what existed *then* (`❌ W0` = "to be built in Wave 0"). It
> is retained unedited as the record of what was predicted. Every `❌ W0` row in it now has a green
> committed test in the map above; nothing in it remains unbuilt.

| Req / Criterion | Behavior | Test Type | Command or artifact | Exists? |
|---|---|---|---|---|
| HARDEN-01 · c1 | A `None` measurement does not overwrite the persisted `consecutive_failures` baseline; the streak accumulates across failure→success-with-unchanged-count | unit (multi-cycle sequence) | new test driving `handle_validate_outcome` twice with `NoGitPath` installed for cycle 1 | ❌ W0 |
| HARDEN-01 · c1 doc | `pipeline_outcomes.rs`'s doc comment no longer promises a guarantee the code lacks | diff review | assert the phase diff edits the identified doc comment | N/A |
| HARDEN-02 · c2 counter | A loop committing trivial `.planning/` artifacts every cycle still reaches a bound | unit | new test over the never-reset per-phase total + its ceiling | ❌ W0 |
| HARDEN-02 · c2 message | Supervise-mode gate message reports the cumulative total, not the streak — and the two read as **different numbers** at the 2nd vs 5th gate | unit | assertion on the gate message string across ≥2 gates | ❌ W0 |
| HARDEN-02 · c2 `--force` | Counter's behaviour across a `--force` restart is **stated and tested**, or explicitly documented as accepted-not-tested | unit or recorded decision | depends on the option the plan picks — see Open Risk below | ❌ W0 |
| HARDEN-03 · c3 | A stale `{N}-VERIFICATION.md` dispatches `FullExecute`; a fresh one dispatches `GapsOnly` | unit (two-direction) | new test asserting **both** directions | ❌ W0 |
| HARDEN-04 · c4 | Worktree-mode `GateReview` auto-decide reads `execution_root`, not `project_root` | integration | extend `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` (`pipeline_launch.rs:2302`) with a worktree PLAN + D-05's decoy PLAN under `project_root` | ✅ base exists, extend |
| HARDEN-04 · c4 mechanical | `assert!(!phase_has_blocking_human_checkpoint(project_root, phase))` — the re-running control | integration | same test, D-06's shape | ❌ W0 |
| HARDEN-04 · c4 revert | The revert is **performed** and the new test **watched to fail** | manual demonstration | one-time; evidence recorded in SUMMARY | ❌ W0 (artifact, not a test) |
| HARDEN-05 · c5 | `release --check` reports `Viable`/`NotViable` from a real `ssh-keygen -Y sign` probe | unit | new probe tests in `git.rs` / `release_check.rs` — positive, negative, block-then-recover | ❌ W0 |
| HARDEN-05 · c5 deletion | `classify_ssh_add_status` / `SigningStatus` / `inline_key_fingerprint` removed with their tests | compile + diff review | workspace builds; `rg` finds no surviving reference | ❌ W0 |
| HARDEN-07 · c6 | `evaluate_layer2` returns `Ok(None)` for `exit_code = 0` + `Stage::Code` + unrunnable `git` | unit | new test in `agent_result.rs` with `NoGitPath` installed | ❌ W0 |
| HARDEN-07 · c6 cascade | `evaluate_agent_result` driven with the same input returns a **non-`Failed`** result — the outcome the criterion states, which the per-layer test above does not establish because `evaluate_layer3` carries its own copy of the same lossy count | unit (cascade level) | new test in `agent_result.rs`, `NoGitPath` around the `evaluate_agent_result` call only; NC-12 is its control | ❌ W0 |
| HARDEN-07 · c6 layer 3 | An unmeasurable count at Layer 3 classifies `Unknown` with no commit figure, not `Failed` with a forged zero; the two existing Layer 3 tests pass byte-unchanged as its opposite-result controls | unit | new test in `agent_result.rs` beside `evaluate_layer3_falls_back_to_commit_count` (`:6760`) and `evaluate_layer3_zero_commits_is_failed_and_flags_human_review` (`:6780`) | ❌ W0 |

---

## Mandatory Negative Controls

This repo's stated discipline: **every measurement includes a case that must produce the opposite
result. If both cases agree, the measurement is broken — not the subject.** These are not optional
and each is individually named, because an unnamed control is one nobody notices went missing.

| # | Measurement | Required opposite-result case | Source |
|---|---|---|---|
| NC-1 | The harness itself blocks `git` | A probe-level assertion that `Command::new("git")` returns `Err` **with** `NoGitPath` installed and `Ok` **without** it. Without this, a green regression test can mean "the shim never took effect". Write it **first**. | RESEARCH §A |
| NC-2 | `None` preserves the `consecutive_failures` baseline | Revert to the unconditional `Some(current)` write with the old lossy `None`→`0` mapping — the same multi-cycle test must then show the streak **reset to 1** | RESEARCH §A/§B |
| NC-3 | The 999.77 sequence discriminates | A **single-cycle** test passes against both the buggy and fixed code. It is a proxy and must not be accepted as coverage for c1 | ROADMAP 999.77 + CONTEXT.md |
| NC-4 | Unrunnable ≠ measured-zero | A `git` shim that **runs and exits non-zero** yields `Some(0)` (real observation), **not** `None`. A test built on a failing shim rather than an absent `git` exercises the wrong path entirely | A-06 + RESEARCH §A |
| NC-5 | The 999.78 ceiling actually bounds | Remove the ceiling check (or the increment) — the trivial-commit loop must no longer gate | RESEARCH §C |
| NC-6 | The gate message reports the total | Revert the format string to interpolate only `consecutive_failures` — the "total ≠ streak at the 2nd vs 5th gate" assertion must fail | RESEARCH §C |
| NC-7 | Staleness detection is two-directional | A rule marking everything stale forever passes the stale case but **must fail** the fresh case. A one-direction test cannot catch this silent permanent regression. **Two halves, both required (review pass 2, H-1):** an *automated* half — the four-row freshness truth table in `35-05` Task 3, which re-runs on every `cargo test` and fails under both an always-stale and an always-fresh stub — and the *performed mutation* below, which is one-time and now has an owner in the Manual-Only table. Before this revision NC-7 had neither: it was named here, absent from the Manual-Only table, and not a test, so an always-stale regression could ship with nothing to catch it | A-12 + RESEARCH §D + review pass 2 |
| NC-8 | `:1070` passes `execution_root` | **Perform** the revert to `project_root`, run the test, confirm it fails, revert the revert. The mechanical D-06 assertion is a re-running control and does **not** substitute for the performed revert; neither substitutes for the other | D-05/D-06 + criterion 4 |
| NC-9 | The signing probe is not vacuously true | Remove the private key file (leave only the `.pub`) — `Viable` must flip to `NotViable` | RESEARCH §F |
| NC-10 | `SSH_ASKPASS_REQUIRE=never` is what prevents the hang | Omit the env var against the **same** encrypted-key fixture — the run must visibly exceed the test's timeout budget, proving the env var and not the timeout alone (nor the fixture) is load-bearing. **The observation window must be calibrated, not assumed (review pass 2, H-3):** measure the non-blocking arm's wall-clock exit duration first and derive the window from it, at a stated multiple, with the test failing "control uncalibrated" if the relationship does not hold. An uncalibrated window shorter than the tool's own give-up time reports "blocked" for the wrong reason and passes. Record both numbers | D-01 + RESEARCH §F + review pass 2 |
| NC-11 | The 999.87 case is the unrunnable one | The existing `evaluate_layer2_exit_zero_no_commits_is_failed` (`agent_result.rs:6668`) covers ordinary `commits == 0` and correctly asserts `Failed`. Extending **it** would be a proxy — c6's discriminating case is `exit_code = 0` + `Stage::Code` + unrunnable `git` | ROADMAP 999.87 |
| NC-12 | Criterion 6's fix removed the defect rather than moving it one layer down | Leave `evaluate_layer2`'s `Ok(None)` fix in place and revert **only** `evaluate_layer3`'s could-not-measure arm to a zero treatment — the cascade test `evaluate_agent_result_with_unrunnable_git_does_not_report_failed` must then fail. A unit test on `evaluate_layer2` alone passes under that revert, which is exactly how the Layer 3 defect survived planning: `evaluate_layer3` runs its own inline count with the same lossy collapse (`agent_result.rs:1977-1986`) and classifies zero as `Failed` at `:1988`, and every path reaching Layer 2 also reaches Layer 3 | Review pass 2, A-H1 (agycli) |

**Why NC-1 is load-bearing and not ceremony.** Criteria 1 and 6 both assert on behaviour that only
occurs when `git` cannot be executed. If the guard silently fails to take effect — wrong `PATH`
ordering, a guard dropped early, an absolute-path `git` invocation — every assertion still runs and
every one of them passes for the wrong reason. NC-1 is the only thing standing between that and a
green suite over an unfixed defect.

---

## What the evidence does NOT establish

Carried as a standing obligation on the phase summary:

- **Criterion 1's test says nothing about the `Some(0)` path.** A `git` that runs and reports a
  genuinely absent branch is a different, already-correct path. Green here is not evidence about it.
- **Criterion 4 proves root-selection correctness for a plain directory standing in for a
  worktree.** D-05 deliberately uses `create_dir_all`, not a real `git worktree add` — nothing here
  establishes git-worktree-specific semantics.
- **Criterion 5's fixtures are n=1 on one host, one OpenSSH build, one key type (ed25519).**
  CONTEXT.md states this outright: the operator's measurements "fix the shape of the design; they
  are not a claim about behaviour across OpenSSH versions or key types, and the phase's own tests
  should not cite them as coverage."
- **Nothing here establishes how often Layer 2 is the deciding layer in production.** The code path
  was read and verified; frequency was never measured. The 999.87 and 999.77 backlog entries both
  record this as "Not established."
- **Criterion 6 green does not mean the run behaves differently.** `AgentStatus::Failed` and
  `AgentStatus::Unknown` map to the same `Action::GateReview` today
  (`crates/devflow-core/src/outcome_policy.rs:53-54`, a deliberate identical mapping). So the
  criterion-6 work changes the recorded classification, the commit figure and the operator-facing
  reason string — all of which are real and all of which an operator reads — and does **not** change
  what happens next in the pipeline. No test may assert a dispatch-level difference; such a test
  passes against the buggy code too.
- **`NoGitPath` is a `PATH`-based guard.** Every `git` child in this workspace is constructed
  PATH-resolved (`git.rs:72` → `:87`, `Command::new("git")`), which is what makes the guard work. A
  future refactor to an absolute `git` path would disarm it silently while every dependent test kept
  passing. Recorded as a known property of the harness, not a live defect.
- **NC-10 is a single observation of each arm** even when calibrated — a weak bound on the
  environment variable's effect, not a reliability claim about it.
- **Nothing establishes the freshness baseline's behaviour under a process kill** between the
  selector's in-memory write and its persistence in `prepare_loop_back_to_code`. The window is small
  and contains no blocking wait; its failure direction is fail-open (a later same-run check reads an
  unchanged artifact as fresh), not fail-safe. Accepted and stated in `35-05`, not covered.
- **A green suite does not establish that the 999.78 bound survives a `--force` restart** unless the
  option chosen for the Open Risk below is itself tested. `State::new` zeroes every counter
  unconditionally.

---

## Open Risk Carried Into Planning — **RESOLVED 2026-08-07**

> **Closed, and closed the way A-11 demanded: stated *and* tested, not papered over.** 35-04
> Task 3 implemented F-3 as **carry-forward** — `start()` reads persisted state for the same phase
> and copies `phase_validate_failures` only (`commands::fresh_state_carrying_phase_failures`), so
> the bound outlives the `State::new` zeroing that `--force` triggers. The reset is tied to two
> real events (phase completion, operator approval at the ceiling gate), not to process start.
>
> Verified live by the audit, not read from the SUMMARY — three green tests:
> `commands::tests::phase_validate_failures_survive_a_forced_restart`,
> `commands::tests::phase_validate_failures_reset_when_the_phase_completes`,
> `pipeline_outcomes::tests::phase_validate_failures_reset_on_operator_approval_at_the_ceiling_gate`.
>
> **What that does NOT establish:** all three drive the state helpers directly. No real `--force`
> re-run against a live repository was performed (35-05 records the same limit). The persistence
> *rule* is tested; the end-to-end restart is not.

The original entry is retained below as the record of the risk as it stood at plan time.

### Original entry (plan time)

**The 999.78 counter's lifetime is per-RUN, but the bound is specified as per-PHASE.** `State::new`
(`state.rs:263-272`) zeroes every counter and `start()` calls it unconditionally on every run,
`--force` included (CONTEXT.md A-11; independently re-confirmed by research at `commands.rs:124`).
A naive `State`-field implementation therefore resets on `--force`, and a bound that resets on
restart does not bound the unattended case D-07 exists for.

CONTEXT.md A-11's instruction is explicit: **the planner must state the counter's persistence
explicitly**, and the reset event must be a real event (phase completion / operator approval at the
ceiling gate), not "whenever a new process starts". If it cannot outlive `State`, that is a finding
to **escalate**, not to paper over.

Whichever option the plan selects, this document requires that the choice be named and either
tested or explicitly recorded as accepted-not-tested. Silence here is a validation gap.

---

## Wave 0 Requirements

> **Complete (`wave_0_complete: true`), with one recorded deviation.** Every item below landed
> except the "one guard per crate" clause, which was **deliberately reduced to devflow-cli only**.
>
> **The deviation, and why it is not a hole.** 35-01 measured that a process-global `PATH` guard is
> not viable inside devflow-core's test binary, so `NoGitPath` exists only in
> `crates/devflow-cli/src/test_support.rs:389`. devflow-core reaches the same states by a different
> mechanism — an **unspawnable working directory** — and carries its own premise assertion
> (`agent_result.rs:6912`: *"the fixture depends on this path being absent"*) so the fixture cannot
> pass for the wrong reason. The decision is documented **in source** at
> `crates/devflow-core/src/test_support.rs:142` under the heading *"Why there is no absent-`git`
> (`NoGitPath`) harness in THIS crate"* — recorded where a future reader trips over it, not only in
> a planning file. Criterion 6's layer- and cascade-level tests were moved to devflow-cli, where the
> guard does exist, and call the same `pub` functions. NC-1's sanity control is green there.
>
> Verified by the audit at both sites rather than accepted from the SUMMARY.

- [x] **One `NoGitPath` RAII guard per crate** — *deviated, see above: devflow-cli only* (empty-directory `PATH`, mirroring `NeutralPath` at
      `crates/devflow-cli/src/test_support.rs:327`, each holding its own crate's single `PATH`
      mutex). Prerequisite for **both** criterion 1's and criterion 6's tests.
      - `crates/devflow-cli/src/test_support.rs` — beside `NeutralPath`, under `env_lock()` (`:94`)
      - `crates/devflow-core/src/test_support.rs` — plus that crate's **first** `PATH` mutex,
        `#[cfg(test)]`-only
      **Why not one shared guard:** `crates/devflow-core/src/lib.rs:79` gates the module with
      `#[cfg(any(test, feature = "test-support"))]` and `tempfile` is a dev-dependency, so a guard
      shared across the crate boundary would not compile for devflow-cli. The `#[cfg(test)]` gate on
      devflow-core's copy makes it unreachable from devflow-cli, so no test binary can ever hold two
      guards under two different mutexes — the `PATH` race is prevented structurally, not by
      discipline.
- [ ] NC-1's harness-sanity control, **in each crate** — `Command::new("git")` returns `Err` with
      the guard installed and `Ok` without it, and `PATH` is byte-identical to its pre-guard value
      after the guard drops. Write **before** the regression tests; retain if cheap.
- [ ] **Flake risk to watch:** `NoGitPath` is the first guard in this workspace that makes `git`
      unresolvable process-wide, and devflow-core has had zero `PATH` mutations until now. Sibling
      `git`-shelling tests can fail spuriously inside a guarded window. Mitigation is scope
      minimisation, enforced as an acceptance criterion.
- [ ] `crates/devflow-core/src/agent_result.rs` — the 999.87 `evaluate_layer2`-unrunnable-`git`
      test; and the `Option<u32>` change to `phase_commit_count` (`:1841`) that forces both
      consumers open.
- [ ] `crates/devflow-core/src/agent_result.rs` — **the third consumer the compiler does not
      surface.** `evaluate_layer3` (`:1971`) runs its own inline count (`:1977`-`:1986`) with the
      identical lossy collapse and classifies zero as `Failed` (`:1988`); the cascade reaches it
      whenever Layer 2 returns `None` (`:2390`/`:2395`). Re-point it at `phase_commit_count` and add
      the unmeasurable-count arm, plus the cascade-level test and NC-12. Without this, criterion 6's
      per-layer test passes while the end-to-end outcome is unchanged.
- [ ] The 999.77 multi-cycle test — module owner to be decided by the plan (`pipeline_outcomes.rs`
      if it drives `handle_validate_outcome`; `agent_result.rs` if it drives the lower-level
      `phase_commit_count` / progress-check pair).
- [ ] `crates/devflow-core/src/state.rs` — new field(s) for 999.78's counter and 999.79's
      staleness fingerprint, each with a serde round-trip pair (present + absent-defaults)
      mirroring `last_validate_failure_commit_count`'s existing pair (`state.rs:415-447`).
- [ ] `crates/devflow-cli/src/pipeline_launch.rs` — the extended 999.84 test on the `:2302` base.
- [ ] `crates/devflow-core/src/git.rs` — the rewritten `check_ssh_signing_viability` and its
      probe-based fixtures (positive / negative / block-then-recover).
- [ ] `crates/devflow-cli/tests/release_check.rs` — rewrite the two `ssh_add_absent`-named tests to
      exercise "`ssh-keygen` absent" instead; `ssh-add` leaves the probe entirely under D-04.
- [ ] Measure and record the full-suite runtime in Test Infrastructure.

*Framework install: none — `cargo test` is already configured in this workspace.*

---

## Manual-Only Verifications

> **Audit note (2026-08-07).** Every row below was **performed** during execution and its evidence
> recorded in the owning SUMMARY — the audit re-read each one rather than trusting the checkbox.
> **No row here represents uncovered behaviour.** Each is either (a) a one-time *performed
> mutation*, which is a property of a mutated tree and therefore cannot by construction be a
> committed test asserting it about itself — and each is backed by a committed test in the map
> above — or (b) an editorial/prose accuracy check no assertion can judge. That is why
> `nyquist_compliant: true` is set despite this table being non-empty; see the audit trail.
>
> **One row left this table.** D8 (the signing probe's controlling-terminal behaviour) was
> Manual-Only when this phase closed, on the strength of a single out-of-band pty measurement.
> It is now covered by a committed test and has moved into the Per-Task Map.

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `execution_root` reaches `phase_has_blocking_human_checkpoint`, proven by a performed revert | HARDEN-04 c4 | "This test fails when the fix is reverted" is a property of a mutation, not of the committed tree; no committed test can assert it about itself | The binding is `pipeline_launch.rs:1068` (`let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);`); the call the ROADMAP criterion cites is `:1070` (`&& verify::phase_has_blocking_human_checkpoint(execution_root, phase)`). Revert **either** — bind `execution_root = project_root`, or pass `project_root` at the call — run the new test, record the failure output, revert the revert, re-run and record the pass. Both halves go in the SUMMARY. |
| Doc comment no longer over-promises | HARDEN-01 c1 | Prose accuracy; no assertion can judge whether a comment matches behaviour | Read the identified `pipeline_outcomes.rs` doc comment against the post-change code and confirm the guarantee claimed is the guarantee delivered |
| Public-API removal recorded for release | HARDEN-05 c5 / D-04 | `CHANGELOG.md` + crate-doc accuracy is editorial | Confirm the removal of `classify_ssh_add_status` and `SigningStatus` is recorded; version stays `v2.5.0` per D-08; version set in two places; `devflow-core` publishes before `devflow-cli` |
| NC-7's performed mutation — the always-stale rule | HARDEN-03 c3 | "This test fails when the rule is inverted" is a property of a mutation, not of the committed tree. The committed four-row truth table covers the same class automatically; this half covers the *wiring* the table cannot see | **Owner: `35-05` Task 3's executor.** In `select_loop_back_fix`, replace the freshness comparison with one that always reports stale. Re-run both direction tests. `stale_verification_artifact_dispatches_full_execute` must still pass and `verification_written_this_run_dispatches_gaps_only` must print a `test result: FAILED` line — the asymmetry is the evidence. Restore, re-run, record both outputs in the SUMMARY. If both tests pass under the mutation, the pair is measuring nothing and criterion 3 has no coverage |
| NC-12's performed mutation — the Layer-3-only revert | HARDEN-07 c6 | Same reason as above: it is a property of a mutation, and the point of *this* one is that it must isolate Layer 3 while leaving Layer 2 fixed, which no committed test can arrange for itself | **Owner: `35-01` Task 3's executor.** Revert only `evaluate_layer3`'s could-not-measure arm to a zero treatment, leaving `evaluate_layer2`'s `Ok(None)` in place. Re-run `evaluate_agent_result_with_unrunnable_git_does_not_report_failed` and confirm a `test result: FAILED` line. Restore, re-run, record both outputs. A demonstration that mutates Layer 2 instead does **not** establish this — it re-proves the thing that was never in doubt |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or a Wave 0 dependency
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all ❌ references above — *one recorded deviation, see Wave 0*
- [x] No watch-mode flags
- [x] Every measurement in "Mandatory Negative Controls" has its opposite-result case present, run,
      and **seen to fail** — not asserted from reading the fix — **all 12 confirmed by the audit
      against the owning SUMMARY's verbatim output** (NC-1/2/3/4/11/12 → 35-01; NC-8 → 35-02, where
      it is performed correctly but never labelled `NC-8`; NC-9/10 → 35-03; NC-5/6 → 35-04;
      NC-7 → 35-05)
- [x] NC-1 passed before any criterion-1 or criterion-6 result was believed
- [x] The 999.78 `--force` persistence choice is stated in a PLAN.md and either tested or recorded
      as accepted-not-tested — **stated AND tested**; see the resolved Open Risk
- [x] NC-12 performed: the Layer-3-only revert run and its failure recorded, so criterion 6 is
      established as an outcome and not as a property of one function
- [x] NC-7's automated half (the four-row truth table) is committed and was demonstrated to fail
      under both an always-stale and an always-fresh stub — **and the two stubs failed on
      *different rows*, which is the exhaustiveness evidence**; NC-7's performed mutation has an
      owner in the Manual-Only table and was run
- [x] NC-10's measured non-blocking exit duration is recorded beside the observation window it
      calibrated — baseline 10.63 ms, window 1000 ms (8×, floored), ratio ≈94× against an asserted
      ≥4×; the control was itself shown capable of failing by sabotaging arm 2
- [x] Full-suite runtime measured and recorded (not assumed) — re-measured this audit
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-08-07 by `/gsd-validate-phase`. Automated coverage complete; the
Manual-Only table is non-empty by construction, not by omission (see its audit note).

---

## Validation Audit 2026-08-07

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 0 |

**Input state:** A (VALIDATION.md existed, seeded at plan time with an unfilled Per-Task Map).
All 6 plans reported `status: complete`, so this was a fully-executed phase — *not* the mid-arc
case that requires blocked rows.

### The gap, and how it was closed

**D8 — the signing probe's `setsid`.** The production `pre_exec`/`setsid` at `git.rs:1026` drops the
controlling terminal, which is the precondition `SSH_ASKPASS_REQUIRE=never` is gated on. Both
existing NC-10 arms install **their own** `setsid` in the fixture (`git.rs:2641`), so they would
have passed byte-unchanged had the production call been deleted. 35-03 recorded this honestly as
`human_judgment: true` — "removing the `pre_exec` would not fail the suite" — and the operator had
scoped a pty fixture out at the time.

The audit confirmed the claim structurally before acting on it (production `setsid` at `:1026` sits
above the `#[cfg(test)]` boundary at `:1218`; the test-side one at `:2641` is below it), then, on
the operator's instruction, spawned `gsd-nyquist-auditor` to close it rather than record it
Manual-Only a second time.

**Added:** `git::tests::the_signing_probe_is_not_captured_by_a_controlling_terminal` (+ a re-entry
helper), 339 test-only lines. Three arms, of which the first two **must disagree**: a no-terminal
baseline (exits in ~10 ms), a premise arm holding a real pty acquired via `TIOCSCTTY` **without**
`setsid` (must still be blocked at window close), and the measurement arm through the production
path (must exit promptly with a real verdict). If arms 0 and 1 ever agree the test fails as
`PREMISE FAILED`, not as a regression — the measurement reports itself broken instead of reporting
on the subject. `libc` was already a regular dependency of devflow-core, so nothing was added.

### The audit did not take the auditor's word for it

Per this repo's standing rule, the subagent's report was treated as a claim:

- **Production code is untouched** — `md5sum` of lines 1–2773 matches `HEAD` exactly; the sole diff
  hunk is `@@ -2773,0 +2774,339 @@`, inside `mod tests`.
- **The mutation was re-performed independently.** Commenting out `git.rs:1024-1029` and re-running
  produced `test result: FAILED. 0 passed; 1 failed; … 575 filtered out`, panicking with
  `REGRESSION:` — *not* `PREMISE FAILED` — while the no-terminal control still exited in 10.4 ms,
  which is what establishes that the terminal handling changed and the fixture did not. Restored
  from a checksummed copy; `md5sum` matches byte-for-byte.
- **Every other row in the map was re-run live**, not transcribed: 18 exact-filtered devflow-core
  tests (556 filtered out), 20 devflow-cli tests (283 filtered out), 9 signing tests (565 filtered
  out), the 10-test `release_check` target, and `./scripts/check.sh all` → exit 0, **956 passed;
  0 failed** across 22 binaries.

### The container gate caught what the host run could not

**The first version of D8's test passed on this host and hard-failed the pre-push gate.** Running
`scripts/check-in-container.sh all` — the exact command `pre-push` runs, pinned image, `taskset -c
0,1` — returned exit 101 with:

```
control uncalibrated: ssh-keygen blocked with NO controlling terminal,
so nothing measured below can be attributed to the terminal
```

**Root cause, confirmed with a negative control.** The gate runs `docker run --rm -t`; the `-t`
allocates a pty, so the test binary *inherits* a controlling terminal. Arm 0 assumed the ambient
environment had none — true under this session's host shell, false in the container. Verified
directly against the same image: **with** `-t` → "HAS controlling terminal"; **without** `-t` →
"no controlling terminal".

The calibration guard behaved correctly — it refused to attribute a block to `setsid` once its own
baseline was contaminated, rather than passing or emitting a false regression. But the result was a
red gate for an environmental reason.

**Fixed in `d33a837`:** arm 0 now spawns via `spawn_detached_from_terminal`, putting the child in a
fresh session with no pty to acquire, so "no controlling terminal" holds *by construction* rather
than by inheritance. Arm 1 was already correct (`setsid` then `TIOCSCTTY` on the pty it owns).
The mutation was **re-performed after this change** — still `1 failed` with `REGRESSION:`, not
`PREMISE FAILED`, on a 5.08 ms baseline — because a change to the control arm invalidates the
earlier demonstration. Container gate re-run: **exit 0, 956 passed across 22 binaries.**

**The lesson generalises, and it is this phase's own thesis.** The auditor had "corroborated" the
mechanism in the container with an equivalent C harness and reported it green. That corroboration
was a **proxy**: it did not reproduce the `-t` condition, so it agreed with the host result and
disagreed with the real thing. A second measurement that cannot fail where the first would is not a
control. Filed as the sibling of 999.92/DEN-113, which is the same mistake in a different fixture.

### What this audit does NOT establish

- **D8's test is n=1 per arm, one host and one container**, Fedora/OpenSSH 10.4p1 and Debian
  bookworm/OpenSSH 9.2p1, one ed25519 encrypted key. **It has now run under the pinned container
  gate** (green after `d33a837`), but never on a GitHub runner.
- **It is timing-based.** A pathologically loaded host could push the measurement arm past its 3 s
  cap and produce a false red. It cannot silently pass — both failure directions are loud.
- **It proves the production code drops the terminal; the *mutation* is what ties that to the
  `setsid` line.** The test alone would also pass if some future mechanism achieved the same thing.
- **A green suite is n=1 for flakiness.** No repeated-run stability measurement was taken for the
  new test beyond the runs above.

### Carried forward — not fixed here

- **`agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape` —
  filed as 999.92/DEN-113.** Two defects, and the second is the serious one. (1) The flake: `/bin/sh`
  is bash, bash `exec`s a lone simple command, so `argv[0]` becomes `sleep` at t≈4 ms while
  `wait_for_exec_visibility(pid, "sh", …)` polls for `sh` every 2 ms — measured directly.
  (2) **The test is weak even when green:** after that `exec` the cmdline is plain `sleep 30` and the
  devflow-looking path is *gone from argv entirely* (confirmed twice), so a passing run asserts that
  a plain `sleep` is not discovered — not the 999.47 shape the test is named for. The obvious fix
  (wait for `sleep`) would make the wrong test pass reliably. **The audit did not reproduce the
  flake** — 10/10 targeted and 3/3 full-suite runs green, two of them in the container — and
  targeted runs lack the parallel contention the race needs, so that is weak counter-evidence rather
  than a refutation; defect (2) does not depend on it. Pre-existing, not in this phase's diff.
  **Implementation defect, not a validation gap.**
- **`staleness::tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` failed once
  in the container** (`unwrap_err()` on an `Ok`) and passed on the re-run, with no change to that
  file. `staleness.rs` was last touched in Phase 34 and is not in Phase 35's diff. Flaky and
  state-dependent rather than broken; **not root-caused**, and left unfiled pending a second
  observation.
- **999.88/DEN-109 is resolved by this audit** — it had already filed the D8 gap independently
  during 35-03, and the test built here matches its "What the test needs" specification. Marked
  RESOLVED in ROADMAP.md and closed in Linear.
- **`35-REVIEW.md` frontmatter still reads `status: issues_found`** with `critical: 1, warning: 7`,
  although CR-01 and WR-01…WR-07 each have a landed fix commit (`cf462ec`…`f8dac07`). The artifact
  is stale, not the code. Out of scope for validate-phase.
- **`35-03-SUMMARY.md`'s D8 row still reads `kind: manual_procedural` / `human_judgment: true`**
  with the rationale "No committed test covers this" — now false. Left unedited: a SUMMARY is the
  executing agent's record of what it did at the time, and this document supersedes it.
