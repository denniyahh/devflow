---
phase: 17
slug: pipeline-dogfood-followup
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-07-18
audited: 2026-07-19
reaudited: 2026-07-19
reaudited_at_commit: cf062e6
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

Audited 2026-07-19 against HEAD `cf062e6` (re-audit; the original audit ran at `3c2774e`). Every
row's test was confirmed to exist by name in the tree and to run green under `cargo test --workspace`.

| # | Plan | Requirement | Secure Behavior | Test Type | Covering Tests | Status |
|---|------|-------------|-----------------|-----------|----------------|--------|
| 1 | 17-03, 17-04 | P2 / AC-3 (17a) | Layer 3 zero-commit/no-declaration routes to gate, never `transition(.., Stage::Validate)` | unit | `evaluate_layer3_zero_commits_is_failed_and_flags_human_review`, `evaluate_layer3_falls_back_to_commit_count`, `code_unknown_does_not_transition_to_validate` | ✅ green |
| 2 | 17-03 | P2 (17a, D-05) | Declared, approved, all-passing external probe with zero commits on a non-Code stage advances; `TRUST_EXTERNAL_VERIFY_ENV` mismatch still vetoes | unit | `layer0_affirmative_success_on_non_code_stage_with_zero_commits`, `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree`, `changed_external_probe_never_inherits_prior_approval`, `removed_external_probe_fails_closed_against_prior_approval` | ✅ green |
| 3 | 17-01 | P2 (17b, D-07) | Exit 137 → `ResourceKilled`; exit 127 → `AgentUnavailable`; serialized names keep word boundaries | unit | `evaluate_layer2_exit_137_is_resource_killed`, `evaluate_layer2_exit_127_is_agent_unavailable`, `multi_word_variants_serialize_with_word_boundary`, `as_wire_str_matches_serde_form_for_every_variant` | ✅ green |
| 4 | 17-01, 17-04, 17-06 | P2 (17b, D-08) | Infra outcomes never increment `consecutive_failures`; the ceiling bounds a stuck loop, not a phase lifetime | unit | `resource_killed_on_code_bumps_infra_failures_not_consecutive_failures`, `resource_killed_on_validate_bumps_infra_not_consecutive_failures`, `transition_resets_infra_failures`, `infra_ceiling_aborts_instead_of_gating` | ✅ green |
| 5 | 17-04 | P2 (17b, D-09) | `RateLimited` in the PRIMARY `advance()` path writes cron-instructions instead of a blocking gate | unit + integration | `primary_loop_rate_limited_writes_single_agent_cron_instructions`, `rate_limited_at_infra_ceiling_stops_resuming_and_aborts`, `sequentagent_hands_off_after_rate_limit_and_writes_cron_instructions` | ✅ green |
| 6 | 17-05, 17-08 | P3 / AC-4 (17c) | A readiness failure is reported as a named preflight gate BEFORE `spawn_monitor`, never a hard exit; an Advance/LoopBack-resolved gate launches the agent exactly once, never twice (CR-01) | unit + integration | `run_preflight_failing_check_gates_and_never_reaches_spawn_monitor`, `run_preflight_adapter_hook_override_fires`, `preflight_interactivity_check_flags_auto_define_without_context_md`, `start_codex_without_context_fails_preflight`, `default_preflight_is_ok_for_built_in_adapters`, `run_preflight_advance_gate_launches_agent_exactly_once`, `run_preflight_loopback_gate_launches_agent_exactly_once` | ✅ green |
| 7 | 17-02, 17-05 | P1 / AC-2 (17d, D-21) | `workflow_started` carries version/commit/dirty/build-timestamp/exe-path fields | unit + integration | `workflow_started_payload_carries_build_provenance`, `build_timestamp_is_a_parseable_u64`, `build_dirty_is_exactly_true_or_false`, `build_commit_is_accessible_and_does_not_panic` | ✅ green |
| 8 | 17-05, 17-06, 17-07 | P1 / AC-2 (17d, D-17/D-19) | Stale embedded commit blocks a DevFlow-workspace launch; a *descendant* build warns instead of blocking; ordinary projects only warn | unit | `embedded_commit_is_stale_maps_ancestry_exit_codes`, `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`, `ahead_build_from_descendant_commit_warns_instead_of_blocking`, `enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring` | ✅ green |
| 9 | Phase 16 | AC-1 (regression) | Failed Merge leaves branch intact, blocks VersionBump/BranchCleanup, opens Ship gate | regression (existing) | `terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished`, `terminal_hook_failure_stops_before_branch_cleanup` | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ partial · 🔴 flaky*

---

## Open Validation Gaps

### GAP-1 — `run_preflight` gate-resolution branches are untested, and harbor an open Critical

**Row 6 · requirement 17c · RESOLVED by `17-08-PLAN.md` (fix `c03498d`, regression tests `b570114`)**

`run_preflight` (`crates/devflow-cli/src/main.rs:789-816`) dispatched the resolved gate three ways.
Both existing tests (`run_preflight_failing_check_gates_and_never_reaches_spawn_monitor`,
`run_preflight_adapter_hook_override_fires`) pre-write a gate response of
`{"approved":false,"note":"abort: …"}`, so **only the `GateAction::Abort` arm was ever executed**.

The two previously-uncovered arms were exactly where `17-REVIEW.md`'s open Critical (CR-01) lived:

```rust
GateAction::Advance  => { …; launch_stage(state, None, None) }   // main.rs:803-807
GateAction::LoopBack(_) => { …; launch_stage(state, None, None) } // main.rs:808-811
```

Each recursively called `launch_stage`, which spawns the agent — then returned `Ok(())` into the
*outer* `launch_stage` call site (`main.rs:1067`), which proceeded to `enforce_build_staleness` /
`archive_phase_files` / `spawn_monitor` and **spawned the agent a second time**.

**Fix (`c03498d`):** `run_preflight` now returns `Result<bool, CliError>` — `Ok(true)` means the
caller should continue `launch_stage`, `Ok(false)` means a failing check was already resolved via a
full retried launch (Advance/LoopBack) or an abort, and the caller must run no further launch
steps. The call site now short-circuits: `if !run_preflight(&project_root, state,
adapter.as_ref())? { return Ok(()); }`.

**Regression proof (`b570114` RED → `c03498d` GREEN):** `run_preflight_advance_gate_launches_agent_exactly_once`
and `run_preflight_loopback_gate_launches_agent_exactly_once` each drive `run_preflight` through a
`FailOnceAdapter` (fails only on its first `preflight()` call) and assert exactly one
`stage_launched` event across the whole phase event log. Both were confirmed to fail against
unmodified `main.rs` (observing 2 events) before the fix landed, and pass after.

**Disposition:** closed. Independently re-verified at `708499c` by the orchestrator, not just
self-reported by the executor: the two regression tests were re-run against a surgically
reintroduced defect (`return Ok(false)` → `Ok(true)` in `run_preflight`) and both failed, proving
they actually catch CR-01 rather than passing vacuously. Full `cargo test --workspace` is green —
64/64 in the `devflow` bin target (including both new tests) and 276/276 in `devflow-core`, **0
ignored** — plus `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`
clean.

GAP-2's test ran and passed in that same suite run, but it hung during 17-08's own execution. That
divergence is direct evidence for GAP-2's nondeterminism, not against it — a green suite still is
not by itself evidence for that row.

### GAP-2 — `concurrent_ship_advances_finish_both_phases_independently` is a latent race

**Not a Phase 17 regression** (reproduces at `8c653f8`, the parent of 17-04's `advance()` rewrite),
but it undermines the reliability of any "full suite green" claim. Both phases race to create tag
`v2.0.1`; the loser's ship failure reopens the `32-ship` gate, and the test pre-wrote only one gate
response, so the reopened gate polls forever with no timeout.

This audit's `cargo test --workspace` run passed — but per `STATE.md` that is a *lucky* pass, not
evidence. Treat suite-green results for this test as non-deterministic until the ship/version-bump
concurrency work lands.

**Re-audit 2026-07-19 — the hang is now measured, not inferred.** The 2026-07-19 re-audit stressed
the test in isolation five times under a 120 s timeout:

| Run | Exit | Elapsed | Result |
|-----|------|---------|--------|
| 1 | 124 (timeout) | 120 s | **hung** |
| 2 | 0 | 2 s | passed |
| 3 | 0 | 1 s | passed |
| 4 | 124 (timeout) | 120 s | **hung** |
| 5 | 0 | 2 s | passed |

**2 of 5 runs (~40 %) hang indefinitely** — the runs that pass do so in 1–2 s, so the hang is
bimodal, not slow-path variance. There is no internal timeout on the reopened `32-ship` gate poll,
so a hung run blocks until an external timeout kills it; in CI with no `timeout` wrapper this stalls
the job rather than failing it.

This has two consequences for the sign-off below:

1. The "`cargo test --workspace` green" evidence is a ~60 %-likely outcome, not a reproducible
   property. Two consecutive green full-suite runs during this re-audit are consistent with chance
   (~36 %) and are *not* independent confirmation.
2. The **Sampling Rate** contract's "max feedback latency: ~90 s" is violated whenever this test
   hangs — feedback latency becomes unbounded.

Requirement coverage is unaffected: this test guards no Phase 17 requirement row, and all 9 rows
pass deterministically. `nyquist_compliant` therefore stays `true` on coverage grounds. But the
suite is not a dependable sampling instrument until this is fixed, and that is a live defect, not a
documentation caveat. **Disposition: open, escalated to the ship/version-bump concurrency work.**

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `evaluate_layer0` tests: non-Code stage + declared/approved/all-passing probe → affirmative success (D-05)
- [x] `evaluate_layer2` tests: exit 137 → `ResourceKilled`, exit 127 → `AgentUnavailable` (D-07)
- [x] `evaluate_layer3` (typed replacement) tests: zero-commit/no-declaration → failure outcome, not blanket `Unknown` (D-01/D-02/D-03)
- [x] `advance()`-level test: `RateLimited` in the primary monitor loop writes cron-instructions (D-09)
- [x] Separate-counter test: infra outcomes never touch `consecutive_failures` (D-08)
- [x] Preflight tests: each D-14 universal check + adapter `preflight()` default-method override path (D-13), plus the Advance/LoopBack gate-resolution branches (GAP-1, resolved by 17-08); security-artifact + reviewer-set checks deferred to Phase 18 by attributed override
- [x] Provenance tests: `workflow_started` payload fields (D-21), staleness with two-commit git fixture (D-19), workspace-identity detection (D-17)
- [x] No new test framework or config needed — only new test CASES; existing `cargo test` infrastructure covers the phase

---

## Manual-Only Verifications

| # | Requirement | Behavior | Why manual | Status |
|---|-------------|----------|------------|--------|
| M-1 | 17d (17-02 D2) | `build.rs` degrades gracefully (empty commit, `dirty=false`, no `rerun-if-changed` lines) when git metadata is unavailable, and never fails the build | The build script is not linked into any test target; asserting it would require driving a full `cargo build` inside a non-git scratch tree. Disproportionate to the risk — the code path is a single `Option` fallback. Verified procedurally: `git rev-parse --git-common-dir` reproduced at exit 128 in a scratch dir, `run_git` returns `None`, which `build.rs` routes to the documented defaults. | ✅ verified (procedural) |
| M-2 | 17d (17-07 Task 2) | The rebuilt binary self-permits — a descendant build drives the primary checkout with a warning rather than the `self-dogfood stale build blocked` error | Requires a real rebuild + live stage launch against the primary checkout; the automated equivalent (`ahead_build_from_descendant_commit_warns_instead_of_blocking`) covers the decision logic but not the live binary. | ✅ verified (procedural) — `17-07-SUMMARY.md` §Verification, Task 2: the rebuilt binary resumed phase 17 into Validate with no `self_dogfood_stale_blocked` event |

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
- [x] Test infrastructure confirmed: `cargo test --workspace` green (362 passed / 0 failed / **0 ignored** across 10 targets), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean (re-confirmed 2026-07-19 at `cf062e6`)
- [x] No watch-mode flags
- [⚠️] Feedback latency < 90s — holds on a clean run (~11s warm), but **unbounded on the ~40 % of runs where GAP-2 hangs**. See GAP-2. This box is not honestly checkable until that race is fixed.
- [x] `nyquist_compliant: true` — GAP-1 resolved by `17-08-PLAN.md` (`c03498d`)

**Approval:** PASS — 9 of 9 requirement rows fully automated and green. Row 6 (17c preflight) was
partial pending GAP-1 (CR-01, the `GateAction::Advance`/`LoopBack` double-spawn); `17-08-PLAN.md`
closed it with an impl fix (`c03498d`) and two RED→GREEN regression tests (`b570114` → `c03498d`).
GAP-2 (`concurrent_ship_advances_finish_both_phases_independently`, a pre-existing race predating
Phase 17) remains escalated and out of this phase's scope — see GAP-2 above.

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

**Note (superseded 2026-07-19):** at original-audit time `17-07` had a PLAN, a landed fix
(`3c2774e`), and a passing test but **no SUMMARY.md**, so its coverage was mapped into row 8
manually from the plan's `must_haves`. `17-07-SUMMARY.md` has since been written and its
§Verification block confirms row 8's coverage and closes manual item M-2. Note resolved.

**Addendum 2026-07-19 (post `17-08-PLAN.md`):** GAP-1 (the sole ESCALATED-not-RESOLVED gap this
audit found) is now closed — see the GAP-1 section above. `nyquist_compliant` flips to `true`; the
counts in the table above are left as originally recorded, a point-in-time snapshot of the
2026-07-19 audit that ran before the fix landed.

---

## Re-Audit 2026-07-19 (HEAD `cf062e6`)

Independent re-verification of the above, run because a `validated` document is a self-report until
someone re-executes its claims.

| Metric | Count |
|--------|-------|
| Requirement rows re-audited | 9 |
| Named tests confirmed present in tree | 35 / 35 |
| Full-suite result | 362 passed / 0 failed / 0 ignored (10 targets) |
| Rows green | 9 |
| Gaps still open | 1 (GAP-2) |
| Gaps closed by this re-audit | 0 |
| Manual items promoted to verified | 1 (M-2) |
| Stale doc claims corrected | 3 |

**Method.** Every test name in the Per-Task Verification Map was grepped for a matching `fn`
definition in `crates/` (35/35 present — no phantom coverage refs). `cargo test --workspace` was
then run twice end-to-end, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --check` once each. All green. No auditor subagent was spawned: there were no MISSING or
PARTIAL requirement rows to fill, which is the only work that agent does.

**What changed.**

1. **GAP-2 escalated from inferred to measured.** Five isolated 120 s-timeout runs → 2 hangs, 3
   passes in 1–2 s. The prior audit correctly suspected a lucky pass; this quantifies it at ~40 %
   and shows the hang is bimodal. The "feedback latency < 90 s" sign-off box was downgraded to ⚠️
   accordingly — it is not honestly checkable while this race is live.
2. **M-2 promoted to ✅ verified.** `17-07-SUMMARY.md` now exists and its §Verification Task 2
   records the live self-permit check the prior audit was waiting on.
3. **Three stale facts corrected**: the audited-HEAD reference (`3c2774e` → `cf062e6`), the M-2
   status, and the "no `17-07-SUMMARY.md`" note.

**Verdict: GAPS.** Requirement coverage is complete and deterministic — all 9 rows green, so
`nyquist_compliant` stays `true`. But GAP-2 remains open with hard evidence it is a live ~40 %
CI-stalling defect rather than a latent caveat, and it belongs to the ship/version-bump concurrency
work, not to Phase 17. This phase does not block on it; the next milestone should not ship without
it.
