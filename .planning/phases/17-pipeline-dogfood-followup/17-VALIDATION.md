---
phase: 17
slug: pipeline-dogfood-followup
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: false
wave_0_complete: true
created: 2026-07-18
audited: 2026-07-19
---

# Phase 17 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `17-RESEARCH.md` §Validation Architecture; Task IDs fill in once PLAN.md files exist.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` — built-in Rust test harness; inline `#[cfg(test)]` modules plus integration tests under `crates/devflow-core/tests/` and `crates/devflow-cli/tests/` |
| **Config file** | none — workspace `Cargo.toml`; CI runs `cargo test` per `.github/workflows/ci.yml` |
| **Quick run command** | `cargo test -p devflow-core <module>::` (scope to the module under active edit) or per-crate `cargo test -p devflow-core` / `cargo test -p devflow-cli` |
| **Full suite command** | `cargo test` (workspace-wide, CI-parity) |
| **CI-parity quality gates** | `cargo clippy -- -D warnings` and `cargo fmt --check` (separate required CI jobs) |
| **Estimated runtime** | under ~90 seconds in the warm-build path (Phase 16 precedent) |

---

## Sampling Rate

- **After every task commit:** Run the touched module's scoped tests plus `cargo clippy -- -D warnings`
- **After every plan wave:** Run full `cargo test` (workspace-wide)
- **Before `/gsd-verify-work`:** Full suite, Clippy, and `cargo fmt --check` must be green
- **Max feedback latency:** ~90 seconds

---

## Per-Task Verification Map

Audited 2026-07-19 against final HEAD (`3c2774e`). Every row's test was confirmed to exist by
name in the tree and to run green under `cargo test --workspace`.

| # | Plan | Requirement | Secure Behavior | Test Type | Covering Tests | Status |
|---|------|-------------|-----------------|-----------|----------------|--------|
| 1 | 17-03, 17-04 | P2 / AC-3 (17a) | Layer 3 zero-commit/no-declaration routes to gate, never `transition(.., Stage::Validate)` | unit | `evaluate_layer3_zero_commits_is_failed_and_flags_human_review`, `evaluate_layer3_falls_back_to_commit_count`, `code_unknown_does_not_transition_to_validate` | ✅ green |
| 2 | 17-03 | P2 (17a, D-05) | Declared, approved, all-passing external probe with zero commits on a non-Code stage advances; `TRUST_EXTERNAL_VERIFY_ENV` mismatch still vetoes | unit | `layer0_affirmative_success_on_non_code_stage_with_zero_commits`, `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree`, `changed_external_probe_never_inherits_prior_approval`, `removed_external_probe_fails_closed_against_prior_approval` | ✅ green |
| 3 | 17-01 | P2 (17b, D-07) | Exit 137 → `ResourceKilled`; exit 127 → `AgentUnavailable`; serialized names keep word boundaries | unit | `evaluate_layer2_exit_137_is_resource_killed`, `evaluate_layer2_exit_127_is_agent_unavailable`, `multi_word_variants_serialize_with_word_boundary`, `as_wire_str_matches_serde_form_for_every_variant` | ✅ green |
| 4 | 17-01, 17-04, 17-06 | P2 (17b, D-08) | Infra outcomes never increment `consecutive_failures`; the ceiling bounds a stuck loop, not a phase lifetime | unit | `resource_killed_on_code_bumps_infra_failures_not_consecutive_failures`, `resource_killed_on_validate_bumps_infra_not_consecutive_failures`, `transition_resets_infra_failures`, `infra_ceiling_aborts_instead_of_gating` | ✅ green |
| 5 | 17-04 | P2 (17b, D-09) | `RateLimited` in the PRIMARY `advance()` path writes cron-instructions instead of a blocking gate | unit + integration | `primary_loop_rate_limited_writes_single_agent_cron_instructions`, `rate_limited_at_infra_ceiling_stops_resuming_and_aborts`, `sequentagent_hands_off_after_rate_limit_and_writes_cron_instructions` | ✅ green |
| 6 | 17-05 | P3 / AC-4 (17c) | A readiness failure is reported as a named preflight gate BEFORE `spawn_monitor`, never a hard exit | unit + integration | `run_preflight_failing_check_gates_and_never_reaches_spawn_monitor`, `run_preflight_adapter_hook_override_fires`, `preflight_interactivity_check_flags_auto_define_without_context_md`, `start_codex_without_context_fails_preflight`, `default_preflight_is_ok_for_built_in_adapters` | ⚠️ **PARTIAL** — see GAP-1 |
| 7 | 17-02, 17-05 | P1 / AC-2 (17d, D-21) | `workflow_started` carries version/commit/dirty/build-timestamp/exe-path fields | unit + integration | `workflow_started_payload_carries_build_provenance`, `build_timestamp_is_a_parseable_u64`, `build_dirty_is_exactly_true_or_false`, `build_commit_is_accessible_and_does_not_panic` | ✅ green |
| 8 | 17-05, 17-06, 17-07 | P1 / AC-2 (17d, D-17/D-19) | Stale embedded commit blocks a DevFlow-workspace launch; a *descendant* build warns instead of blocking; ordinary projects only warn | unit | `embedded_commit_is_stale_maps_ancestry_exit_codes`, `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`, `ahead_build_from_descendant_commit_warns_instead_of_blocking`, `enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring` | ✅ green |
| 9 | Phase 16 | AC-1 (regression) | Failed Merge leaves branch intact, blocks VersionBump/BranchCleanup, opens Ship gate | regression (existing) | `terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished`, `terminal_hook_failure_stops_before_branch_cleanup` | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ partial · 🔴 flaky*

---

## Open Validation Gaps

### GAP-1 — `run_preflight` gate-resolution branches are untested, and harbor an open Critical

**Row 6 · requirement 17c · ESCALATED (impl bug — not closable by a test alone)**

`run_preflight` (`crates/devflow-cli/src/main.rs:789-816`) dispatches the resolved gate three ways.
Both existing tests (`run_preflight_failing_check_gates_and_never_reaches_spawn_monitor`,
`run_preflight_adapter_hook_override_fires`) pre-write a gate response of
`{"approved":false,"note":"abort: …"}`, so **only the `GateAction::Abort` arm is ever executed**.

The two uncovered arms are exactly where `17-REVIEW.md`'s open Critical (CR-01) lives:

```rust
GateAction::Advance  => { …; launch_stage(state, None, None) }   // main.rs:803-807
GateAction::LoopBack(_) => { …; launch_stage(state, None, None) } // main.rs:808-811
```

Each recursively calls `launch_stage`, which spawns the agent — then returns `Ok(())` into the
*outer* `launch_stage` call site (`main.rs:1067`), which proceeds to `enforce_build_staleness` /
`archive_phase_files` / `spawn_monitor` and **spawns the agent a second time**.

Why this is not auto-fillable: a test asserting the correct behavior (exactly one `stage_launched`
event per preflight-gate resolution) must fail against current `main.rs`. Closing it requires an
implementation change, which is outside the validation auditor's mandate.

**Disposition:** fix CR-01 via `/gsd-code-review --fix` or during Ship, then add the two missing
branch tests as the regression proof. Phase 17 must not merge before both.

### GAP-2 — `concurrent_ship_advances_finish_both_phases_independently` is a latent race

**Not a Phase 17 regression** (reproduces at `8c653f8`, the parent of 17-04's `advance()` rewrite),
but it undermines the reliability of any "full suite green" claim. Both phases race to create tag
`v2.0.1`; the loser's ship failure reopens the `32-ship` gate, and the test pre-wrote only one gate
response, so the reopened gate polls forever with no timeout.

This audit's `cargo test --workspace` run passed — but per `STATE.md` that is a *lucky* pass, not
evidence. Treat suite-green results for this test as non-deterministic until the ship/version-bump
concurrency work lands.

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `evaluate_layer0` tests: non-Code stage + declared/approved/all-passing probe → affirmative success (D-05)
- [x] `evaluate_layer2` tests: exit 137 → `ResourceKilled`, exit 127 → `AgentUnavailable` (D-07)
- [x] `evaluate_layer3` (typed replacement) tests: zero-commit/no-declaration → failure outcome, not blanket `Unknown` (D-01/D-02/D-03)
- [x] `advance()`-level test: `RateLimited` in the primary monitor loop writes cron-instructions (D-09)
- [x] Separate-counter test: infra outcomes never touch `consecutive_failures` (D-08)
- [~] Preflight tests: each D-14 universal check + adapter `preflight()` default-method override path (D-13) — **partial**, gate-resolution branches uncovered (GAP-1); security-artifact + reviewer-set checks deferred to Phase 18 by attributed override
- [x] Provenance tests: `workflow_started` payload fields (D-21), staleness with two-commit git fixture (D-19), workspace-identity detection (D-17)
- [x] No new test framework or config needed — only new test CASES; existing `cargo test` infrastructure covers the phase

---

## Manual-Only Verifications

| # | Requirement | Behavior | Why manual | Status |
|---|-------------|----------|------------|--------|
| M-1 | 17d (17-02 D2) | `build.rs` degrades gracefully (empty commit, `dirty=false`, no `rerun-if-changed` lines) when git metadata is unavailable, and never fails the build | The build script is not linked into any test target; asserting it would require driving a full `cargo build` inside a non-git scratch tree. Disproportionate to the risk — the code path is a single `Option` fallback. Verified procedurally: `git rev-parse --git-common-dir` reproduced at exit 128 in a scratch dir, `run_git` returns `None`, which `build.rs` routes to the documented defaults. | ✅ verified (procedural) |
| M-2 | 17d (17-07 Task 2) | The rebuilt binary self-permits — a descendant build drives the primary checkout with a warning rather than the `self-dogfood stale build blocked` error | Requires a real rebuild + live stage launch against the primary checkout; the automated equivalent (`ahead_build_from_descendant_commit_warns_instead_of_blocking`) covers the decision logic but not the live binary. | ⬜ unconfirmed — no `17-07-SUMMARY.md` on record |

*(D-03 case 3's "notify human" is product behavior, asserted by automated tests on the gate/notify path.)*

---

## Deferred by Attributed Override

AC-4's missing-security-artifact and reviewer-set sub-checks are **not** validation gaps — they are
unimplemented by accepted decision, recorded as an `overrides:` entry in `17-VERIFICATION.md`
frontmatter (accepted by Dennis Kim, 2026-07-19) and disclosed in `ROADMAP.md:185-191`. They land
with Phase 18's Hermes adapter, the first adapter with real reviewer storage.

---

## Validation Sign-Off

- [x] All requirements have automated verification or a recorded manual-only/override disposition
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Test infrastructure confirmed: `cargo test --workspace` green, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean (2026-07-19)
- [x] No watch-mode flags
- [x] Feedback latency < 90s (full workspace suite completes in ~11s warm)
- [ ] `nyquist_compliant: true` — **blocked by GAP-1**

**Approval:** PARTIAL — 8 of 9 requirement rows fully automated and green. Row 6 (17c preflight) is
partial: the `GateAction::Advance`/`LoopBack` arms of `run_preflight` are untested and contain open
Critical CR-01 (double agent spawn). Not closable by a test alone; requires the impl fix first.
Re-run `/gsd-validate-phase 17` after CR-01 is fixed to flip `nyquist_compliant`.

---

## Validation Audit 2026-07-19

| Metric | Count |
|--------|-------|
| Requirement rows audited | 9 |
| Coverage refs verified present in tree | 46 / 46 |
| Fully covered + green | 8 |
| Gaps found | 2 |
| Resolved by this audit | 0 |
| Escalated (impl bug / pre-existing race) | 2 |
| Manual-only recorded | 2 |
| Deferred by attributed override | 1 |

**Method:** State A audit. Every `coverage:` ref in `17-01`…`17-06-SUMMARY.md` was checked to exist
by name in `crates/`, then the full suite was executed. No auditor subagent was spawned: GAP-1's
only fillable artifact is a test that must fail against current `main.rs`, which the auditor's
"never modify impl files" mandate routes straight to `ESCALATE`.

**Note:** `17-07` has a PLAN, a landed fix (`3c2774e`), and a passing test, but **no SUMMARY.md**.
Its coverage was mapped into row 8 manually from the plan's `must_haves` rather than from a
`coverage:` block. Worth generating for artifact completeness.
