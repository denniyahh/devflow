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
reaudited_2_at_commit: 636d1ab
reaudited_3_at_commit: b77c13e
reaudited_4_at_commit: e61171f
reaudited_5_at_commit: 46058a7
reaudited_6_at_commit: 1070df0
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
| 7 | 17-02, 17-05, 17-11 | P1 / AC-2 (17d, D-21) | `workflow_started` carries version/commit/dirty/exe-path fields, and the embedded provenance actually refreshes when the working tree changes | unit + integration | `workflow_started_payload_carries_build_provenance`, `build_dirty_is_exactly_true_or_false`, `build_commit_is_empty_or_a_full_hex_sha`, `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` | ✅ green — freshness now sampled (GAP-3 closed by `17-11`); vacuous test replaced (GAP-4 closed by `46058a7`). `build_timestamp_is_a_parseable_u64` retired with the timestamp itself (CR-02) |
| 8 | 17-05, 17-06, 17-07, 17-11 | P1 / AC-2 (17d, D-17/D-19) | Stale embedded commit blocks a DevFlow-workspace launch; a *descendant* build warns instead of blocking; ordinary projects only warn | unit | `embedded_commit_is_stale_maps_ancestry_exit_codes`, `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`, `ahead_build_from_descendant_commit_warns_instead_of_blocking`, `enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring` | ✅ green — decision logic unchanged, and its *inputs* are now current under CI conditions (GAP-3 closed by `17-11`) |
| 10 | 17-10, 17-11 | 17d follow-up (dogfood finding) | The second staleness arm only considers build inputs; content hooks target the worktree while terminal hooks stay on the primary checkout | unit | `dirty_flag_arm_ignores_non_build_files_but_still_flags_sources`, `content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root` | ✅ green — arm renamed mtime→dirty-flag by `17-11` (CR-02); the original guarantee is unchanged |
| 11 | audit-fix (`17-REVIEW.md`) | CR-03 / WR-02 / WR-07 | Unparseable retry hint gates instead of stalling silently; self-dogfood match is exact, not substring; the gitignore guard covers all 14 runtime paths, one `check-ignore` per path | unit + integration | `rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently`, `is_self_dogfood_workspace_requires_exact_member_paths_not_substrings`, `gitignore_covers_devflow_runtime_state_paths` | ✅ green |
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

**Not a Phase 17 regression** (reproduces at `8c653f8`, the parent of 17-04's `advance()` rewrite).
Both phases can compute the same next version from the same starting git state and race to create
tag `v2.0.1`; the loser's ship failure reopened the `32-ship` (or `31-ship`) gate, and the test
pre-wrote only one gate response per phase, so the reopened gate polled forever with no timeout.

**Prior measurements (superseded — history preserved for context).** Two earlier re-audits stressed
the test in isolation and measured the hang directly rather than inferring it:

| Audit | Runs | Hangs | Rate | Mechanism confirmation |
|-------|------|-------|------|------------------------|
| Re-audit 2026-07-19 (`cf062e6`) | 5 | 2 | ~40 % | Timing-only (bimodal 1–2 s vs. 120 s timeout) |
| Re-audit #2 2026-07-19 (`636d1ab`) | 4 (1 full-suite + 3 isolated) | 1 | — | `/proc` thread inspection: `hrtimer_nanosleep` on the reopened gate's poll thread |
| Cumulative across both | 9 | 3 | ~33 % | — |

Those audits correctly concluded this was a live, CI-stalling defect, not a documentation caveat,
and escalated it as **out of Phase 17's scope** (belongs to the ship/version-bump concurrency work).
This plan (17-09) closes the *test-level* half of that escalation — see below.

**RESOLVED by `17-09-PLAN.md` (fix `cb9359f`).**

**RED.** Before any fix, the test was run in isolation under an external 120 s `timeout` and hung on
the very first attempt: exit `124`, full 120 s elapsed. Temporary debug instrumentation added during
this RED phase (removed before the fix landed) caught the mechanism directly rather than inferring
it from timing: both phases' `version_bump()` calls were observed computing the identical version
(`2.0.1`) within **~1.8 ms** of each other, and the loser's own `git tag` call then failed with
git's native `fatal: cannot lock ref 'refs/tags/v2.0.1': reference already exists` — direct proof
the two phases' terminal hooks executed concurrently on that run, not merely that they targeted the
same version.

**Fix shape.** The binding constraint is "never hangs," not "always both succeed" — per
`17-09-PLAN.md`'s explicit framing, this is a **test-level** bound, not a change to the underlying
contention (see Product-level note below). `DEVFLOW_GATE_TIMEOUT_SECS` is overridden to `2` seconds
for this test's poll only, under the file's existing `ENV_MUTEX` guard (the same pattern used by
`checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout` and
`transition_resets_infra_failures`) — restored immediately after the run. **The 7-day production
default (`DEVFLOW_GATE_TIMEOUT_SECS`'s fallback in `parse_gate_timeout`) is untouched**; a real
operator gate still waits the configured 7 days for a human. The test no longer assumes both phases
always succeed: it accepts either outcome — no collision (both phases finish independently, as
originally written) or a bounded loser timeout (asserted explicitly: the error text contains "timed
out", the loser's state is left intact — not cleared — with `gate_pending: true`, and its Ship gate
file remains on disk, i.e. the documented "awaiting human" state, not a silent failure).

**GREEN — re-measured evidence, replacing the "2 of 5 (~40 %)" measurement.** The test was run in
isolation **25 consecutive times** under a 120 s external `timeout`:

| Metric | Count |
|--------|-------|
| Total isolated runs | 25 |
| Exit code 124 (hang) | **0** |
| Exit code non-zero (any failure) | **0** |
| Verdict | 25/25 `test result: ok` (identical verdict every run) |
| Runs that hit the race collision (loser timeout path exercised) | 9 / 25 (~36 %) |
| Runs with no collision (both phases finished normally) | 16 / 25 |

The ~36% collision rate over this sample is consistent with the prior audits' ~33–40% measurements
— the underlying contention frequency is unchanged, as expected, since this fix bounds the *poll*,
not the *race*. What changed is that every one of the 9 collision runs now resolves deterministically
in ~2–4 s (bounded by the 2 s `DEVFLOW_GATE_TIMEOUT_SECS` override) instead of hanging. Full
`cargo test --workspace` (362 passed / 0 failed / 0 ignored, 10 targets), `cargo clippy --workspace
--all-targets -- -D warnings`, and `cargo fmt --check` are all clean with this test included
unfiltered (no `--skip` needed).

**Product-level version-tag contention — explicitly OUT OF SCOPE, unresolved.** Closing the test-level
hang must not silently bury this: two phases shipping concurrently genuinely contend for the *same*
computed version tag when their terminal hooks race inside the same checkout-lock critical section
window (the ~1.8 ms overlap observed directly during RED). Whether that should serialize more tightly,
retry with a recomputed version, or fail fast with a clearer operator-facing error is an open design
question this plan deliberately does not decide. It belongs with the ship/version-bump concurrency
work referenced by the prior audits, not to Phase 17 or this gap-closure plan. The bounded test now
correctly *tolerates* the race instead of hanging on it, but the race itself — and whatever product
guarantee (or lack of one) two concurrently-shipping phases should get around a shared version
lineage — remains an unresolved, named question for future work.

Requirement coverage is unaffected: this test guards no Phase 17 requirement row, and all 9
requirement rows pass deterministically. `nyquist_compliant` stays `true` on coverage grounds, as
before. **Disposition: RESOLVED (test-level, this plan). Product-level version-tag contention:
OUT OF SCOPE, explicitly unresolved — see paragraph above.**

*Status: ✅ green (bounded — the underlying contention still occurs intermittently but the test can
no longer hang on it)*

### GAP-3 — build provenance never refreshes on working-tree changes, and no test samples freshness

**Rows 7 and 8 · requirement 17d (D-19/D-20/D-21) · OPEN — escalated, not fillable by the auditor**

This is `17-REVIEW.md`'s CR-02 restated as a *validation* finding rather than a code finding. The
review reclassified CR-02 to manual-only on the grounds that both proposed fixes reverse a recorded
decision (`build.rs:32-35`, review consensus #7 / D-20: "re-run only when git refs actually move —
not on every `cargo build`"). That reclassification is a legitimate call about **whether to fix the
code**. It does not dispose of the Nyquist question, which is separate: *no test observes this
property at all*, so the phase's headline 17d deliverable is unsampled.

**Independently reproduced at `e61171f`** — not taken from the review. A `git clone --no-hardlinks`
of this worktree into `/tmp/dfprobe` (which produces `packed-refs`, exactly as
`actions/checkout@v4` does), then build → edit `crates/devflow-cli/src/main.rs` → rebuild:

| | Build 1 | Build 2 (after source edit) |
|---|---|---|
| `DEVFLOW_BUILD_COMMIT` | `e61171f0…` | `e61171f0…` (identical) |
| `DEVFLOW_BUILD_DIRTY` | `false` | `false` — **wrong**, `git status --porcelain` reported ` M crates/devflow-cli/src/main.rs` |
| `DEVFLOW_BUILD_TIMESTAMP` | `1784494683` | `1784494683` (byte-identical) |

The binary genuinely recompiled — `target/debug/devflow` relinked at mtime `1784494696`, 13 s after
the build-script output froze at `1784494683`, and the edit is present in the linked source. So the
binary contains modified code while embedding `dirty=false` and a pre-edit timestamp. Both values
feed `enforce_build_staleness` and the `workflow_started` payload: the gate that exists to catch
"you forgot to rebuild" certifies a stale binary as Fresh.

**Why the existing tests cannot catch it.** `build.rs`'s trigger set is `HEAD`, `refs`, and
`packed-refs` (`build.rs:36-41`), but `git status --porcelain` (`:48`) reads the entire working tree
and has no trigger, and `SystemTime::now()` (`:67`) can have none. Row 7's three provenance tests
assert only that the values *parse* — a stale timestamp is still a valid `u64` and a stale dirty
flag is still exactly `"false"`, so all three stay green against frozen values. Row 8's four tests
exercise the staleness *decision logic* against synthetic fixtures; they never assert that the
decision's real inputs are current. The property is structurally unobservable by the current suite.

**This is masked on the development machine and would only appear in CI.**
`/var/home/denniyahh/Github/devflow/.git/packed-refs` does not exist (confirmed), and cargo treats a
missing `rerun-if-changed` path as *always rerun* — so locally the provenance refreshes by accident.
Any `git gc` or any CI checkout creates `packed-refs` and the bug appears. That is why neither the
suite nor the phase's own dogfood runs surfaced it, and it is why local greenness is not evidence
here.

**Why no auditor subagent was spawned.** The fillable artifact is the test the review itself names —
"builds twice across a working-tree edit and asserts the provenance actually changed." That test is
RED against current `build.rs` by construction, and turning it green requires editing `build.rs`,
which the auditor's "never modify impl files" mandate routes straight to `ESCALATE`. Writing a
committed-RED test would also wedge CI. Escalated instead.

**Disposition: ~~OPEN~~ → CLOSED by `17-11` (`3e39cf6`).** Both decisions this audit declined to
take unilaterally were referred to the operator, who took both:

1. **Reverse the D-20 caching intent so provenance is honest** — `build.rs` now declares a single
   never-existing sentinel path, forcing cargo's "missing input ⇒ always rerun" rule on every
   `cargo build`. `DEVFLOW_BUILD_TIMESTAMP` was dropped entirely in the same change: it was the only
   `rustc-env` value that changed every run, so removing it is what keeps always-rerunning the script
   from recompiling `devflow-cli` on every build. The `SystemTime::now()` input this gap named as
   unfingerprintable no longer exists.
2. **Add the double-build freshness test regardless** — `build_dirty_flips_false_to_true_across_a_
   working_tree_edit_after_rebuild` reproduces the exact table above: a synthetic checkout with
   `pack-refs --all` forced (so the fixture matches a CI checkout, not this dev checkout's accidental
   masking), built → tracked `.rs` edit → rebuilt. It asserts against cargo's own persisted
   `target/debug/build/devflow-*/output` cache rather than `env!()` from the test binary's compile,
   which is the only vantage point that can observe whether `build.rs` actually re-ran.

Re-verified at `46058a7`: `cargo test --workspace` 367 passed / 0 failed / **0 ignored** across 9
targets, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.
Observed live during that run — after an edit to `build_provenance.rs`, the build-script cache read
`DEVFLOW_BUILD_DIRTY=true`, the refresh this gap proved impossible under the old trigger set.

The mtime arm this gap's inputs fed is retired with the timestamp; `combined_staleness` now compares
the build's own dirty flag against a live `affects_compiled_binary` check. Rows 7 and 8 are ✅.

### GAP-4 — `build_commit_is_accessible_and_does_not_panic` asserts nothing

**Row 7 · requirement 17d (D-20) · OPEN — trivial, test-only**

`17-REVIEW.md` IN-01, confirmed by reading `crates/devflow-cli/tests/build_provenance.rs:31-37`:

```rust
let commit = env!("DEVFLOW_BUILD_COMMIT");
let _ = commit.len();
```

`env!` resolves at compile time to a `&'static str`; `.len()` cannot panic and the result is
discarded. The test passes unconditionally regardless of what `build.rs` emits. It is counted as one
of row 7's four covering tests, so row 7's real automated coverage is three tests, not four — and it
inflates the suite pass count by one.

Left unfixed by this audit deliberately: the honest replacement (assert the commit is either empty
per D-20's no-git case, or a 40-char hex SHA) is worth having but is a coverage change that should
land with GAP-3's freshness test rather than as a drive-by edit during a read-only audit pass.

**Disposition: ~~OPEN~~ → CLOSED by `46058a7`.** `17-11` landed GAP-3's freshness test, which is the
change this gap deferred to, so the replacement landed in its designated place rather than as a
drive-by. `build_commit_is_accessible_and_does_not_panic` → `build_commit_is_empty_or_a_full_hex_sha`,
now asserting exactly what this entry prescribes: empty (no-git build, D-20) or 40 chars of
`is_ascii_hexdigit`. Confirmed to assert the *strong* branch and not pass via the empty escape
hatch — the embedded value at that commit was `71c4ebd311272a7efadea377b29d12a2590e264b`, a real
40-char SHA. Row 7's covering-test count is now honest.

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
- [x] Test infrastructure confirmed: `cargo test --workspace` green (**367 passed / 0 failed / 0 ignored / 0 filtered** across 10 targets), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean (re-confirmed 2026-07-19 at `1070df0`, re-audit #6)
- [x] No watch-mode flags
- [x] Feedback latency < 90s — holds on a clean run (~11s warm); the ~33–40 % of runs where GAP-2's
  race manifests now resolve in a bounded ~2–4 s (via the `DEVFLOW_GATE_TIMEOUT_SECS=2` test-scoped
  override, `17-09-PLAN.md`, fix `cb9359f`) instead of hanging. This box is now honestly checkable:
  worst-case observed latency across 25 consecutive isolated runs was ~4 s, well inside the 90 s
  budget. See GAP-2.
- [x] `nyquist_compliant: true` (re-audit #5, `46058a7`) — every gap this document raised is now
  closed: GAP-1 by `17-08-PLAN.md` (`c03498d`), GAP-2's test-level hang by `17-09-PLAN.md`
  (`cb9359f`), **GAP-3 by `17-11-PLAN.md` (`3e39cf6`)** — 17d's build-provenance freshness is now
  sampled by a double-build regression test and correct under CI conditions — and **GAP-4 by
  `46058a7`**, replacing the vacuous test so row 7's covering-test count is honest. GAP-2's
  underlying product-level version-tag contention remains explicitly OUT OF SCOPE for Phase 17.

**Approval (superseded by re-audit #4 — see below):** PASS — 9 of 9 requirement rows fully automated and green. Row 6 (17c preflight) was
partial pending GAP-1 (CR-01, the `GateAction::Advance`/`LoopBack` double-spawn); `17-08-PLAN.md`
closed it with an impl fix (`c03498d`) and two RED→GREEN regression tests (`b570114` → `c03498d`).
GAP-2 (`concurrent_ship_advances_finish_both_phases_independently`)'s test-level unbounded-poll wedge
is resolved by `17-09-PLAN.md` (`cb9359f`) — see GAP-2 above. The underlying product-level
version-tag contention that causes the race is explicitly recorded there as OUT OF SCOPE and
unresolved, belonging to future ship/version-bump concurrency work, not to Phase 17.

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

---

## Re-Audit #2 2026-07-19 (HEAD `636d1ab`)

Third independent pass. HEAD moved only by the previous re-audit's own doc commit (`cf062e6` →
`636d1ab`), so no source changed between passes — this re-executes claims rather than re-reading them.

| Metric | Count |
|--------|-------|
| Requirement rows re-audited | 9 |
| `coverage:` refs in `17-01`…`17-08-SUMMARY.md` confirmed present | 48 / 48 |
| Suite result (racy test excluded) | 361 passed / 0 failed / 0 ignored (10 targets), exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (exit 0) |
| `cargo fmt --check` | clean (exit 0) |
| Rows green | 9 |
| Gaps still open | 1 (GAP-2) |
| Gaps closed by this re-audit | 0 |

**Method.** Every `#test_name` in every SUMMARY `coverage:` block was extracted and matched against a
real `fn` definition in `crates/` — 48/48, superseding the prior pass's 35-name map-only check. The
suite was then run with `-- --skip concurrent_ship_advances_finish_both_phases_independently`, which
isolates the GAP-2 race and makes the requirement-row evidence *deterministic* rather than ~60 %
likely. 361 = the prior pass's 362 minus that one filtered test, so the two runs reconcile exactly.
No auditor subagent was spawned: zero MISSING or PARTIAL rows, which is the only work that agent does.

**GAP-2 reproduced live, and its mechanism is now proven rather than inferred.** The first full-suite
run of this audit hung. While it was hung, the `devflow` bin test binary (PID 3497313, 2m14s elapsed
against an ~11 s clean-run baseline) was inspected directly under `/proc`:

| Thread | `wchan` | Interpretation |
|--------|---------|----------------|
| 3497313 | `futex_do_wait` | main thread blocked in `thread::scope` join |
| 3497457 | `futex_do_wait` | sibling `advance()` thread, finished, waiting on scope |
| 3497481 | `hrtimer_nanosleep` | **the losing phase's reopened `32-ship` gate poll, sleeping forever** |

That third thread is the defect: `Gates` polls with no timeout, so the run blocks until an external
`timeout` kills it. This is direct mechanical confirmation of the mechanism the prior audit inferred
from timing alone. Source re-read confirms the setup — `main.rs:3785-3791` pre-seeds exactly one Ship
gate response per phase, so the loser of the `v2.0.1` tag race has nothing to consume.

Three subsequent isolated runs under a 60 s timeout all passed in 1–2 s. Cumulative across both
re-audits: **3 hangs in 9 runs (~33 %)**, passes always 1–2 s — the bimodality holds. Notably this
audit's hang occurred under full-suite load while the three isolated runs passed, which is weak
evidence the race widens under CPU contention; that would make CI (parallel test targets) *more*
exposed than isolated reproduction suggests, not less.

**Nothing was corrected.** Unlike the prior pass, which fixed three stale facts, every claim in this
document re-executed as written. GAP-2's disposition, `nyquist_compliant: true`, and the ⚠️ on the
feedback-latency sign-off box all remain correct as recorded.

**Verdict: GAPS** — unchanged and for the unchanged reason. All 9 requirement rows are green and
deterministic once GAP-2's test is isolated, so Phase 17's own coverage is complete and this phase
does not block. GAP-2 is a pre-existing ship/version-bump concurrency defect that Phase 17 neither
introduced nor owns, but it is live, it stalls rather than fails CI, and it should not survive into
the next milestone.

---

## Re-Audit #3 2026-07-19 (HEAD `b77c13e`)

Fourth independent pass, and the first to run after **both** gaps were claimed closed. A `validated`
document with two "RESOLVED" gap sections is still a self-report until someone re-executes it, so
this pass re-ran the claims rather than re-reading them — and, for both gaps, tried to *break* the
fix rather than merely observe it passing.

| Metric | Count |
|--------|-------|
| Requirement rows re-audited | 9 |
| `coverage:` refs in `17-01`…`17-09-SUMMARY.md` confirmed present | 50 / 50 |
| Per-Task Map test names confirmed present | 36 / 36 |
| Suite result (**unfiltered — racy test included**) | 362 passed / 0 failed / 0 ignored (10 targets), exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (exit 0) |
| `cargo fmt --check` | clean (exit 0) |
| Rows green | 9 |
| Gaps still open | **0** |
| New gaps found | 0 |

**Method.** Every `#test_name` in every SUMMARY `coverage:` block was extracted and matched against a
real `fn` definition in `crates/` (50/50, now including `17-09`). The full suite was run twice
end-to-end **without `--skip`** — the previous pass needed to filter the racy test to get
deterministic evidence; this pass did not, which is itself the clearest signal GAP-2's wedge is gone.
362 reconciles exactly with re-audit #1's unfiltered count.

**GAP-1 re-verified by mutation, not by observation.** The two CR-01 regression tests were re-run
against a surgically reintroduced defect (`return Ok(false)` → `Ok(true)` in `run_preflight`,
`main.rs:820`). Both **FAILED** (`run_preflight_advance_gate_launches_agent_exactly_once` at
`main.rs:4829`, `run_preflight_loopback_gate_launches_agent_exactly_once` at `main.rs:4891`), then
both passed again once the source was restored (4/4 `run_preflight_*` green, tree clean). They catch
the double-spawn for real rather than passing vacuously.

**GAP-2 re-verified by independent stress, and the bound is demonstrably load-bearing.** The test was
run **15 consecutive times in isolation** under a 120 s external `timeout`:

| Metric | Count |
|--------|-------|
| Total isolated runs | 15 |
| Exit code 124 (hang) | **0** |
| Any non-zero exit / non-`1 passed` verdict | **0** |
| Runs that hit the race collision | 4 / 15 (~27 %) |
| Worst-case wall-clock latency | **3.15 s** |

The per-run timings are cleanly bimodal in a way that proves the 2 s bound is doing the work rather
than the race having quietly disappeared: 11 runs clustered at ~1.22–1.32 s (no collision) and 4 at
~3.12–3.15 s (collision) — a gap of very nearly exactly the 2 s `DEVFLOW_GATE_TIMEOUT_SECS`
override. Those 4 runs are collisions that *were* rescued by the bound; pre-fix they are the runs
that would have hung. The ~27 % collision rate is consistent with the ~33–40 % measured by the three
prior audits, confirming the fix bounds the *poll* and not the *race*, exactly as `17-09-PLAN.md`
claimed.

**The fix was also checked for the two ways it could have been cheating.** (1) `parse_gate_timeout`
(`main.rs:30-33`) still falls back to `SEVEN_DAYS`, so the production default is genuinely untouched
and a real operator gate still waits for a human. (2) The test's loser branch is not permissive: it
asserts the error text contains `"timed out"` and *rejects any other failure mode*, then asserts
state loads intact (not cleared), `gate_pending == true`, and the Ship gate file is still on disk.
A silent failure or a different error would fail the test. Test binding was confirmed non-vacuous
(`1 passed`, not `0 passed; 64 filtered out`).

**Verdict: PASS.** All 9 requirement rows are covered, automated, green, and — for the first time
across four passes — deterministic with the full suite run *unfiltered*. Both GAP-1 and GAP-2 are
closed, and each was re-verified adversarially rather than taken on the executor's word.
`nyquist_compliant` remains `true`, now on strictly stronger evidence than when it was first set.

**One named item deliberately survives this PASS**, and it is not a validation gap: the product-level
version-tag contention two concurrently-shipping phases hit (GAP-2's "Product-level" paragraph). The
test now *tolerates* that race rather than eliminating it. It guards no Phase 17 requirement, Phase 17
neither introduced nor owns it, and it is recorded as OUT OF SCOPE for the ship/version-bump
concurrency work. Phase 17 does not block on it; a future milestone should not silently inherit it.

---

**Addendum 2026-07-19 (post `17-09-PLAN.md`):** GAP-2's test-level unbounded-poll wedge — the sole
remaining open gap this audit found — is now resolved (`cb9359f`, see the GAP-2 section above and
`17-09-SUMMARY.md`). `DEVFLOW_GATE_TIMEOUT_SECS` is bounded to 2 s for this test's poll only (the
7-day production default is unchanged); 25 consecutive isolated runs under a 120 s external timeout
all exited 0 with the identical verdict, 9 of which actually hit the version-tag race and resolved
via the bounded loser-timeout path rather than hanging. The feedback-latency sign-off box flips from
⚠️ to green accordingly. The counts and verdict recorded above are left as originally written, a
point-in-time snapshot of this re-audit pass that ran before the fix landed. The underlying
product-level version-tag contention this test tolerates (rather than eliminates) remains an
explicit, named OUT-OF-SCOPE item — see the GAP-2 section's "Product-level version-tag contention"
paragraph.

---

## Re-Audit #4 2026-07-19 (HEAD `e61171f`)

Fifth pass, and the first to run after **new source landed since the document last claimed PASS**.
Re-audit #3 certified `b77c13e`; five commits have landed since — `17-10`'s staleness/hook-targeting
fixes plus the `17-REVIEW.md` audit-fix batch — touching **353 lines of `main.rs`**. A PASS verdict
does not survive its own commit range, so this pass re-derived it rather than inheriting it.

| Metric | Count |
|--------|-------|
| Requirement rows audited | 11 (9 prior + 2 added this pass) |
| Suite result (**unfiltered**) | 366 passed / 0 failed / **0 ignored** (10 targets), `cargo` exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (exit 0) |
| `cargo fmt --check` | clean (exit 0) |
| Rows green | 9 |
| Rows downgraded to ⚠️ partial | 2 (rows 7, 8) |
| Gaps closed since last pass | 0 |
| **New gaps found** | **2 (GAP-3, GAP-4)** |

**A false-green trap was hit and corrected mid-audit.** The first suite run was invoked as
`cargo test --workspace 2>&1 | tail -40`, which reports **`tail`'s** exit status, not cargo's — the
pipeline would have exited 0 even had the suite failed, and the truncated log showed only 3 of 10
targets (278 of 366 passes). Re-run with output redirected and the exit code captured directly. The
suite is genuinely green, but the first result was not evidence of it. Recorded because this is the
same class of false green this project has hit before.

The 366 total reconciles exactly with re-audit #3's 362: +4 for
`rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently` (CR-03),
`is_self_dogfood_workspace_requires_exact_member_paths_not_substrings` (WR-02),
`mtime_arm_ignores_non_build_files_but_still_flags_sources` and
`content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root` (17-10). All four were
confirmed present in the tree. WR-07's rewritten guard and WR-05/06's `--all-targets` clippy scope in
**both** `ci.yml:30` and `devcontainer.yml:26` were verified directly.

**The Per-Task Map was stale and has been extended.** It carried 9 rows covering `17-01`…`17-08`,
with no rows for `17-10` or the audit-fix batch. Rows 10 and 11 were added so the tests that landed
in the last five commits are mapped to the behavior they guard rather than merely existing.

**The substantive finding: green tests were concealing an unsampled requirement.** GAP-3 (above)
records it in full. In short — 17d's build-provenance gate is the phase's headline deliverable, and
its central property, *that the embedded provenance actually reflects the built source*, is observed
by no test. Reproduced independently in a CI-shaped clone: after a source edit that `git status`
reports as dirty, the rebuilt binary embeds a byte-identical `dirty=false` and pre-edit timestamp.
The four row-7 tests stay green because they assert only that those values parse, and a frozen value
parses fine. This is a textbook Nyquist failure — the suite samples the wrong property, so the signal
is invisible no matter how green the run is.

`17-REVIEW.md` reclassified the underlying code defect (CR-02) to manual-only, which is a defensible
call about whether to reverse a recorded D-20 decision. This audit does not dispute that. It records
the separate fact that **the behavior is untested either way**, which the manual-only reclassification
does not address and which no manual-only entry in this document covers.

**No auditor subagent was spawned.** GAP-3's only fillable artifact is RED against `build.rs` by
construction and cannot be made green without editing an impl file — the auditor's explicit
`ESCALATE` condition. GAP-4 is test-only but belongs with GAP-3's fix rather than as a drive-by edit.

**Verdict: GAPS.** Nine of eleven rows are fully automated, green, deterministic, and unfiltered.
But `nyquist_compliant` flips **`true` → `false`**: rows 7 and 8 cover 17d's decision logic while
leaving its inputs unsampled, and that gap is not hypothetical — the defective behavior was
reproduced at this HEAD. Phase 17's other requirements are well covered; 17d's provenance guarantee
is not yet earned.

---

## Validation Re-Audit #5 2026-07-19 (`46058a7`) — gap closure

| Metric | Count |
|--------|-------|
| Requirement rows audited | 11 |
| Rows fully automated + green | **11** |
| Gaps open at re-audit #4 | 2 (GAP-3, GAP-4) |
| Gaps closed this pass | **2 (GAP-3, GAP-4)** |
| New gaps found | **0** |
| `nyquist_compliant` | **`false` → `true`** |

**Trigger:** `/gsd-execute-phase 17 --gaps-only`, which executed `17-11-PLAN.md` — the follow-up plan
re-audit #4's GAP-3 said the two deferred decisions "belong to."

**GAP-3 closed.** Both decisions were taken by the operator and implemented: `build.rs` always
re-runs (never-existing sentinel path), `DEVFLOW_BUILD_TIMESTAMP` is gone entirely, and the
double-build freshness test the review named now exists and asserts against cargo's own persisted
build-script output cache. Full disposition in the GAP-3 section above.

**GAP-4 closed.** `build_commit_is_empty_or_a_full_hex_sha` replaces the discarded `.len()`. It
landed with GAP-3's freshness test, which is exactly where re-audit #4 said it belonged.

**Suite reconciliation — 366 → 367, accounted for exactly:** −1
`build_timestamp_is_a_parseable_u64` (retired with the timestamp it asserted); +1
`build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` (GAP-3's freshness test);
+1 net from `combined_staleness_mtime_arm_flags_dirty_tree_newer_than_build` being rewritten as two
tests, one per row of the new decision table (`…flags_modified_tree_when_build_was_clean` ⇒ Stale,
`…is_indeterminate_when_build_was_already_dirty` ⇒ Indeterminate). Two further renames are net-zero
(`build_commit_is_accessible_and_does_not_panic`, `mtime_arm_ignores_non_build_files_but_still_flags_sources`).
No test was deleted to make a count work; each removal is a value that no longer exists.

**Gates re-run directly, not read from a SUMMARY** (and not through a pipe whose exit status belongs
to `tail` — re-audit #4's recorded false-green trap): `cargo test --workspace` **367 passed / 0
failed / 0 ignored / 0 filtered across 9 targets**, `cargo clippy --workspace --all-targets --
-D warnings` exit 0, `cargo fmt --check` exit 0.

**The freshness fix was observed working live, not just asserted.** Editing
`crates/devflow-cli/tests/build_provenance.rs` during this pass caused the build-script cache at
`target/debug/build/devflow-*/output` to re-emit `DEVFLOW_BUILD_DIRTY=true`. Under the trigger set
re-audit #4 reproduced against, that value stayed frozen at `false` across exactly this kind of edit.

**Verdict: PASS.** All eleven requirement rows are automated, green, deterministic, and unfiltered.
`nyquist_compliant` returns to `true` — rows 7 and 8 now sample 17d's provenance *freshness*, the
property re-audit #4 correctly identified as the textbook Nyquist failure, and not merely its shape.
17d's provenance guarantee is now earned rather than assumed.

---

## Validation Re-Audit #6 2026-07-19 (HEAD `1070df0`)

Sixth pass. Re-audit #5 certified `46058a7`; two commits have landed since (`3d6e6a6`, `1070df0`)
and **both are documentation-only** — `git diff --stat 46058a7..HEAD` touches `ROADMAP.md`,
`STATE.md`, `17-VALIDATION.md`, and `17-VERIFICATION.md`, and no file under `crates/`. Re-audit #4's
rule ("a PASS verdict does not survive its own commit range") therefore does not force a re-derivation
here, but the newest claims — GAP-3 and GAP-4, closed only one pass ago — had been verified by
observation rather than by mutation. This pass closes that.

| Metric | Count |
|--------|-------|
| Requirement rows audited | 11 |
| Per-Task Map covering refs confirmed present in tree | **40 / 40** |
| `coverage:` refs across `17-01`…`17-11-SUMMARY.md` confirmed present | 49 / 51 (2 superseded names — see below) |
| Suite result (**unfiltered**) | **367 passed / 0 failed / 0 ignored / 0 filtered** (10 targets), `cargo` exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (exit 0) |
| `cargo fmt --check` | clean (exit 0) |
| Rows green | **11** |
| Gaps still open | **0** |
| New gaps found | **0** |

**Gates run directly, exit codes captured from `cargo` itself** — not through a pipe whose status
belongs to `tail` (re-audit #4's recorded false-green trap). 367 reconciles exactly with re-audit #5.

**GAP-3 re-verified by mutation, not observation — the decisive addition of this pass.** Re-audit #5
closed GAP-3 on the strength of the new test passing and a live cache observation. That shows the
test is green; it does not show the test would go *red* if the defect returned. So `build.rs`'s
always-rerun sentinel (`:43`) was surgically reverted to the exact pre-fix trigger set
(`.git/HEAD`, `.git/refs`, `.git/packed-refs`) and
`build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` was re-run:

| | Result |
|---|---|
| Against reintroduced defect | **FAILED** — `build_provenance.rs:205`, `left: "false"` / `right: "true"` |
| After restoring `build.rs` | 3/3 green, working tree clean (`git status --porcelain` empty) |

The observed `left: "false"` is precisely the frozen value re-audit #4 reproduced by hand in
`/tmp/dfprobe`. The test catches CR-02 for real; it does not pass vacuously. This is the same
adversarial standard re-audit #3 applied to GAP-1 and GAP-2, now extended to GAP-3.

**GAP-4 confirmed to bind on its strong branch.** `build_commit_is_empty_or_a_full_hex_sha` permits
an empty commit per D-20's no-git case, so a green result would be uninformative if the embedded
value were empty. It is not: the build-script cache at this HEAD reads
`DEVFLOW_BUILD_COMMIT=1070df0a7de4fd0c242b5b30dd144e0a7dbf486a` — a real 40-char SHA, so the
`len() == 40 && all hexdigit` branch is the one actually exercised. That value also tracks current
HEAD (re-audit #5 recorded `71c4ebd`), which is independent live evidence the build script re-runs.

**Two superseded coverage refs in `17-02-SUMMARY.md` — noted, not a gap.** Lines 43 and 49 still cite
`build_timestamp_is_a_parseable_u64` and `build_commit_is_accessible_and_does_not_panic`, neither of
which exists in the tree: the first was retired with `DEVFLOW_BUILD_TIMESTAMP` (CR-02) and the second
was renamed by GAP-4's closure. The behaviors are covered by their replacements, which row 7 maps
correctly. `17-02-SUMMARY.md` is a point-in-time execution record and is left unedited, consistent
with how this document has treated prior snapshots. Recorded here so a future automated ref-check
reads these as superseded rather than as phantom coverage.

**Verdict: PASS.** All eleven requirement rows are automated, green, deterministic, and unfiltered,
and every gap this document ever raised (GAP-1 through GAP-4) has now been verified adversarially —
each one re-run against a deliberately reintroduced defect and confirmed to fail. `nyquist_compliant`
stays `true`, on the strongest evidence across six passes.

**The one named item that survives is unchanged and is not a validation gap:** the product-level
version-tag contention between two concurrently-shipping phases (GAP-2's "Product-level" paragraph).
Phase 17 neither introduced nor owns it, it guards no Phase 17 requirement, and it remains recorded
as OUT OF SCOPE, belonging to future ship/version-bump concurrency work.
