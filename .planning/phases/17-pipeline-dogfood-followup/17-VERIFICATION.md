---
phase: 17-pipeline-dogfood-followup
verified: 2026-07-20T08:18:32Z
status: passed
score: 15/15 must-haves verified
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "A non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential is reported before the stage launch (retrospective AC-4)"
    reason: "D-14(b) named no concrete security-artifact path/key at planning time; cross-AI review consensus #6 deferred it and reviewer-set enforcement to Phase 18's Hermes adapter, which is the first adapter with real reviewer storage"
    accepted_by: "Dennis Kim (gap-closure disposition, /gsd-plan-phase --gaps, phase 17)"
    accepted_at: "2026-07-19T11:26:28Z"
re_verification:
  previous_status: passed
  previous_score: 14/14
  trigger: "Plan 17-12 (WR-04 gap closure, requirement 17d) landed after the prior 21:34:26Z pass (commits a3a1067, 31757ef, b81ec7d, 39e2e65, d9701d7, e89a25b, all 18:21Z-18:59Z-04:00 = 22:21Z-22:59Z). One additional commit (1de9e3c, WR-03/WR-05 staleness-gate fixes) also landed after the prior pass and before 17-12; it was independently confirmed here as already adversarially validated in 17-VALIDATION.md re-audit #7 and is not part of any open gap."
  gaps_closed:
    - "17-12 (WR-04, both rounds): ChangelogAppend ran in the Validate→Ship batch, before VersionBump ever tagged, so the changelog heading named a version that did not exist and got worse every retry loop (root cause of two prior CR-01 Critical findings in 17-REVIEW.md rounds 1-2). Closed by reordering ChangelogAppend into hooks_after_ship() strictly after VersionBump, sourcing the version via a new git-free version::read_version instead of re-deriving it with compute_version (which would see VersionBump's own new tag and calculate one version too high), and committing the write via a new scoped GitFlow::commit_path so it can no longer be lost when Merge/BranchCleanup run. version_bump itself was found to have the identical uncommitted-write defect during this plan's own clean-tree regression test and was fixed the same way."
  gaps_remaining: []
  regressions: []
---

# Phase 17: Pipeline Dogfood Follow-Up Verification Report

**Phase Goal:** Close the pipeline-reliability holes the Phase 16 dogfood exposed — `Unknown`
completion must never auto-advance a stage (17a), typed agent outcomes with a deterministic retry
policy (17b), a preflight readiness gate that fails before agent time is consumed (17c), and build
provenance in `workflow_started` so a stale self-dogfood binary is detectable (17d). The
terminal-Ship alarm was traced to a stale executable, not a live regression; state/event
reconciliation and the WR-03 test fix were deferred to Phase 18 on 2026-07-18.

**Verified:** 2026-07-20T08:18:32Z
**Status:** passed
**Re-verification:** Yes — this pass closes the coverage gap left by the prior 14/14 report
(2026-07-19T21:34:26Z), which predates plan `17-12`. `17-12` is the twelfth and final plan in
`ROADMAP.md`'s "Plans: 12/12 plans executed" line for this phase; this report is the first
verification pass to cover the full 17-01..17-12 plan set at current HEAD.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `Unknown` completion never auto-advances any stage (17a, D-01/D-06) | ✓ VERIFIED | `outcome_policy::decide_action` unaffected — `git diff 71c4ebd..HEAD -- crates/devflow-core/src/outcome_policy.rs` is empty |
| 2 | A legitimately external-only stage with zero commits and a passing approved probe completes cleanly (17a, D-02/D-03/D-05) | ✓ VERIFIED | `agent_result::` module untouched since the prior pass |
| 3 | Typed outcomes `ResourceKilled`/`AgentUnavailable` route through a fail-closed exhaustive policy table (17b, D-07/D-11/D-12) | ✓ VERIFIED | Unchanged since the prior pass |
| 4 | Infrastructure-outcome ceiling (`state.infra_failures`) resets on every successful stage transition (17b, D-08, CR-01) | ✓ VERIFIED | `state.infra_failures = 0;` still present immediately after `state.consecutive_failures = 0;` in `transition()` (`main.rs`, read directly this pass). Directly executed `cargo test -p devflow transition_resets_infra_failures` → 1 passed |
| 5 | `RateLimited` outcomes auto-resume via a safe, per-phase `devflow resume` path (17b, D-09) | ✓ VERIFIED | Unchanged since the prior pass |
| 6 | Every terminal advance decision emits structured, machine-readable evidence (17b, D-10) | ✓ VERIFIED | Unchanged since the prior pass |
| 7 | A preflight readiness gate runs before every stage launch, never a hard exit, never double-spawns (17c, D-13/D-15/D-16) | ✓ VERIFIED | `run_preflight`/`launch_stage` logic unchanged by the 17-12 diff (only an unrelated doc-comment line moved) |
| 8 | A non-interactive plan or unavailable Ship-scoped `gh auth` is reported before stage launch (retrospective AC-4, narrowed scope) | ✓ **PASSED (override)** | Interactivity + `gh auth status` checks unchanged; standing, previously-accepted override carried forward unmodified |
| 9 | `workflow_started` records executable and build provenance — commit + dirty flag, no `build_timestamp` field (17d, D-21) | ✓ VERIFIED | `workflow_started_payload` (`main.rs`) still emits `commit`/`dirty`/`exe_path` only. Directly executed `cargo test -p devflow workflow_started_payload_carries_build_provenance` → 1 passed |
| 10 | A stale self-dogfood binary is detected and blocked before stage launch, including a clean-tree strict-ancestor build (17d, D-17/D-18/D-19, 17-06 WR-01) | ✓ VERIFIED | `embedded_commit_is_stale`'s exit-0 branch unchanged. Directly executed `cargo test -p devflow wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` → 1 passed. **Known scope boundary, not a Phase 17 gap:** this check compares the embedded commit to `project_root`'s HEAD, not a worktree's HEAD. Round-4 review (`17-REVIEW.md` CR-01/WR-01) traced a live incident (a stale binary running the pre-17-12 hook batch, re-emitting a false `1.4.106` CHANGELOG heading) to exactly this boundary for worktree-based phases, and separately found `build.rs`'s `DEVFLOW_BUILD_DIRTY` is unfiltered (any dirty file, not just build-affecting ones — systematically true during self-run since `.planning/` is normally dirty), which weakens the block the same way. Both were explicitly triaged out of Phase 17 into `ROADMAP.md` Phase 19 item **19d** on 2026-07-19 (commits `4dce785`, `58e978b`) with an enforceable interim rule (rebuild before re-validating). D-17/D-18/D-19 as originally scoped (project_root ancestor comparison) are implemented and pass adversarially; worktree-HEAD comparison was never part of Phase 17's committed scope. Confirmed the tree is currently clean of the false `1.4.106` entry (`git status --porcelain` empty, `CHANGELOG.md` tops out at `1.3.69` matching the latest real tag) — the incident is not a live defect at this HEAD |
| 11 | AC-1 (Phase 16's failed-Merge terminal-hook regression) still holds against final HEAD | ✓ VERIFIED | Unaffected by 17-12's diff; full-suite green (this pass) includes these tests |
| 12 | Full workspace test suite, clippy, and fmt are green at final HEAD | ✓ VERIFIED | Directly executed this pass: `cargo test --workspace` → 0 failed across all 10 targets (see per-target counts below); `cargo clippy --workspace --all-targets -- -D warnings` → exit 0; `cargo fmt --check` → exit 0 |
| 13 | `build.rs` always re-runs, `DEVFLOW_BUILD_DIRTY` is never stale, `DEVFLOW_BUILD_TIMESTAMP` is fully removed (17d, 17-REVIEW.md CR-02, closed by 17-11) | ✓ VERIFIED | Directly read `build.rs` this pass: line 43 declares the never-existing sentinel; only `DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY` are emitted; `rg -n "DEVFLOW_BUILD_TIMESTAMP"` matches only rationale comments. Unchanged by 17-12 |
| 14 | Staleness's second signal is a live `build_dirty` vs. `tree_has_modified_build_inputs` comparison (17d, CR-02, closed by 17-11) | ✓ VERIFIED | `combined_staleness` decision table unchanged in shape by 17-12. `tree_has_modified_build_inputs` was independently hardened between the prior pass and 17-12 (commit `1de9e3c`, WR-03/WR-05: enumerates from `git status --porcelain` instead of `git ls-files -m`, which was blind to staged-but-uncommitted source edits) — a strengthening, not a regression, confirmed by adversarial test and independently re-run here: `cargo test -p devflow -- combined_staleness_dirty_flag_arm_flags_modified_tree_when_build_was_clean combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty dirty_flag_arm_ignores_non_build_files_but_still_flags_sources ahead_build_from_descendant_commit_warns_instead_of_blocking is_self_dogfood_workspace_anchors_on_members_not_default_members` → 8 passed, 0 failed |
| 15 | `ChangelogAppend` runs strictly after `VersionBump` in `hooks_after_ship()`, reads the version via a git-free `version::read_version` (never recomputing), and commits its own write; a regression test proves three-way agreement (changelog heading version == created git tag == version-file version) and a clean working tree after the full after-ship batch (17d, WR-04, closed by 17-12, both rounds) | ✓ VERIFIED | Read directly this pass: `hooks_for_transition(Validate, Ship) = vec![Hook::DocsUpdate]` (`hooks.rs:86`); `hooks_after_ship() = [Merge, VersionBump, ChangelogAppend, BranchCleanup]` (`hooks.rs:99-106`); `changelog_append` calls `version::read_version` not `compute_version` and commits via `GitFlow::commit_path("CHANGELOG.md", ...)`, propagating a failed commit as a `HookError` so terminal-batch fail-fast still stops `BranchCleanup`; `version_bump` was also fixed to commit its version-file write before tagging. Directly executed: `cargo test -p devflow-core --lib version:: hooks:: git::tests::commit_path` → 13 + 12 + 1 = 26 passed; `cargo test -p devflow -- terminal_hook_failure_stops_before_branch_cleanup` → 1 passed. **Known edge case, not a Phase 17 gap:** round-4 review (CR-03) found that when a project has *no* supported version file at all, `version_bump` still tags `v{compute_version()}` while `changelog_append`'s `read_version` errors and falls back to the literal `"unreleased"` — a desync the plan's own fixture (`init_repo`, which always writes a `Cargo.toml`) cannot reproduce. This is real (confirmed at source: `hooks.rs:234-236` `else` branch vs. `hooks.rs:196-198` fallback) and is a gap in 17-12's design scope, not a regression of what it built — explicitly triaged to `ROADMAP.md` Phase 19 item **19f** on 2026-07-19 |

**Score:** 15/15 truths verified (0 failed, 0 partial, 1 verified via standing attributed override)

### New in This Pass — 17-12 (WR-04) Verification Detail

| Must-have (17-12-PLAN.md frontmatter) | Verified | Evidence |
|---|---|---|
| `Hook::ChangelogAppend` removed from `hooks_for_transition(Validate, Ship)`, added to `hooks_after_ship()` ordered after `VersionBump` | ✓ | Source read, `hooks.rs:84-106` |
| `changelog_append` does not call `compute_version()` | ✓ | Source read, `hooks.rs:196` calls `version::read_version` |
| `changelog_append` reads back the version `VersionBump` wrote | ✓ | `hooks.rs:196-198`; `after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean` passes |
| Regression test asserts three-way agreement (heading == tag == version file) | ✓ | `hooks::tests::after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean` — 1 passed |
| `changelog_append` commits its own write | ✓ | `hooks.rs:210-214` via new `GitFlow::commit_path`; `changelog_append_commits_its_own_write` — 1 passed |
| Regression test asserts a clean working tree after the full after-ship batch | ✓ | Same test as above asserts `git status --porcelain` empty |
| `DocsUpdate` stays in the Validate→Ship batch, targets the worktree (17-10 unchanged) | ✓ | `hook_context_root` — non-terminal batch still resolves `state.worktree_path` |
| `ChangelogAppend`, now terminal, targets `project_root` | ✓ | `hook_context_root` — `terminal_batch => project_root` (line 1689-1690) |
| Terminal-batch fail-fast preserved | ✓ | `run_checkout_hooks`'s `if terminal_batch && outcome.is_err() { break; }` unchanged; `terminal_hook_failure_stops_before_branch_cleanup` — 1 passed |

### Requirements Coverage

No `.planning/REQUIREMENTS.md` file exists in this project (confirmed absent again this pass); per-phase
requirements are tracked in `17-DOGFOOD-RETROSPECTIVE.md` (P1–P4, mapped 1:1 to scope units 17a–17d) and
`17-CONTEXT.md` (D-01…D-21).

| Requirement | Source | Description | Status | Evidence |
|-------------|--------|-------------|--------|----------|
| P1 / 17a (D-01–D-06) | Retrospective + CONTEXT | `Unknown` non-advance + Layer 0/3 rework | ✓ SATISFIED | Truths 1, 2 |
| P2 / 17b (D-07–D-12) | Retrospective + CONTEXT | Typed outcomes + deterministic retry policy | ✓ SATISFIED | Truths 3, 4, 5, 6 |
| P3 / 17c (D-13–D-16) | Retrospective + CONTEXT | Preflight readiness gate | ✓ SATISFIED | Truths 7, 8 (Truth 8 narrowed via standing override) |
| P4 / 17d (D-17–D-21) | Retrospective + CONTEXT | Build provenance + stale-binary detection + release-record integrity | ✓ SATISFIED | Truths 9, 10, 13, 14, 15 |
| AC-1 (criterion 1) | Retrospective | Failed-Merge terminal contract, verify only | ✓ SATISFIED | Truth 11 |
| D-01…D-21 | 17-CONTEXT.md | All 21 numbered decisions | ✓ SATISFIED | Every decision maps to a truth above; no orphaned decision |

Plan-frontmatter requirement IDs across all 12 plans (`requirements:` grepped from every `17-*-PLAN.md`
this pass) cover all four scope units with no gaps and no orphans: `17-01`→17b, `17-02`→17d, `17-03`→17a,
`17-04`→17a/17b, `17-05`→17c/17d, `17-06`→17b/17c/17d, `17-07`→17d, `17-08`→17c, `17-09`→17b, `17-10`→17d,
`17-11`→17d, `17-12`→17d.

### Behavioral Spot-Checks (this pass)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| `version::read_version` round-trips and never recomputes from git | `cargo test -p devflow-core --lib version::` | 13 passed, 0 failed | ✓ PASS |
| `hooks_after_ship()` reorder + commit-scoping (17-12) | `cargo test -p devflow-core --lib hooks::` | 12 passed, 0 failed | ✓ PASS |
| `GitFlow::commit_path` scopes to its pathspec (round-3 CR-01 fix) | `cargo test -p devflow-core --lib git::tests::commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted` | 1 passed, 0 failed | ✓ PASS |
| CR-01 fix (`infra_failures` reset) still holds | `cargo test -p devflow -- transition_resets_infra_failures` | 1 passed, 0 failed | ✓ PASS |
| `workflow_started` payload carries commit+dirty, no timestamp | `cargo test -p devflow -- workflow_started_payload_carries_build_provenance` | 1 passed, 0 failed | ✓ PASS |
| WR-01 (17-06) strict-ancestor Stale still holds | `cargo test -p devflow -- wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` | 1 passed, 0 failed | ✓ PASS |
| 17-07 Ahead classification still holds | `cargo test -p devflow -- ahead_build_from_descendant_commit_warns_instead_of_blocking` | 1 passed, 0 failed | ✓ PASS |
| CR-02 (17-11) dirty-flag arms still hold | `cargo test -p devflow -- combined_staleness_dirty_flag_arm_flags_modified_tree_when_build_was_clean combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty dirty_flag_arm_ignores_non_build_files_but_still_flags_sources` | 3 passed, 0 failed | ✓ PASS |
| WR-05 (`is_self_dogfood_workspace` anchoring) confirmed live and green | `cargo test -p devflow -- is_self_dogfood_workspace_anchors_on_members_not_default_members` | 1 passed, 0 failed | ✓ PASS |
| Terminal-batch fail-fast preserved | `cargo test -p devflow -- terminal_hook_failure_stops_before_branch_cleanup` | 1 passed, 0 failed | ✓ PASS |
| Full workspace suite (run once, per verifier constraint) | `cargo test --workspace` | 0 failed across 10 test targets (devflow-core lib 284, devflow-core monitor_e2e 2, devflow lib 70, build_provenance 3, devcontainer_ci_failfast 1, gitignore_coverage 1, help_snapshot 1, log_format_env 3, phase7_cli 11, doc-tests 0) | ✓ PASS |
| Full workspace clippy | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | ✓ PASS |
| Full workspace fmt | `cargo fmt --check` | exit 0 | ✓ PASS |

**Note on the documented flake (not a Phase 17 gap):** `phase7_cli.rs`'s
`parallel_creates_two_worktrees_and_spawns_two_monitors` passed in this run (11/11 in that target). The
orchestrator reported it failed 1 of 3 runs before this verification pass. Independently confirmed this
matches `ROADMAP.md` item **18e**: the test polls a live stdout capture with `wait_for`, runs unrelated
assertions against `.devflow/` state, then re-asserts the same capture paths without re-waiting — a fast
monitor can archive the capture into per-generation history in between, which is a real intermittent
race in test *timing*, not in the pipeline behavior it exercises. It was explicitly deferred out of
Phase 17 to Phase 18 on 2026-07-18 (recorded non-blocking debt in `16-REVIEW.md` originally). Agreed with
the orchestrator's classification.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-core/src/hooks.rs` | `changelog_append`'s `today()` fallback / entry body | Generated changelog entry body remains the placeholder `"Released phase via DevFlow"` | ℹ️ Info | Content-quality defect, deliberately out of scope for 17-12 (recorded in its SUMMARY, previously flagged in `17-10-SUMMARY.md:104`); does not affect version/tag/commit truthfulness, which is what WR-04 required |
| `.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` | frontmatter (`status: issues_found`, `ship_gate: BLOCKED`, `3 Critical`) | The round-4 review's own frontmatter was never updated after `ROADMAP.md` formally triaged all 3 Criticals into Phase 19 (commits `4dce785`, `58e978b`) | ℹ️ Info | Documentation staleness in a secondary artifact, not the phase's authoritative scheduling record (`ROADMAP.md` is authoritative and is current); does not affect the truthfulness of any must-have above — verified independently that CR-01's tree state is clean, CR-02 is confirmed pre-existing/unrelated to Phase 17, and CR-03 is a confirmed real edge case correctly attributed to 17-12's design scope, not fabricated or silently dropped |
| `crates/devflow-core/src/version.rs` | `replace_version_in_contents` (CR-02, round 4) | Drops the trailing token remainder (e.g. JSON's `,`) when rewriting a version field not on the last line of its object | ℹ️ Info | Confirmed pre-existing — last touched by `aac079f` (Phase 2b), untouched by any Phase 17 commit including 17-12; correctly triaged to Phase 19 item 19e as "unrelated to Phase 17's changes" |
| `crates/devflow-cli/build.rs` | `dirty` computation (line ~52) | Unfiltered `git status --porcelain` (any dirty file) vs. the runtime arm's `affects_compiled_binary`-filtered check | ℹ️ Info | Real asymmetry (round-4 WR-01), introduced by 17-11's CR-02 fix (predates 17-12); weakens but does not defeat Truth 14's decision table (an over-eager `Indeterminate` is fail-open on a corner case, not fail-closed-broken); functionally overlaps with 19d's interim mitigation (rebuild-before-revalidate) — not yet individually triaged with its own Phase 19 item letter, flagged here for visibility |

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any file 17-12 modified (`version.rs`,
`hooks.rs`, `git.rs`, `main.rs`).

### Human Verification Required

None. Every truth above is confirmed by direct source reading and live execution of named regression
tests this pass (not SUMMARY.md narration) — including the two behavior-dependent claims (three-way
version/tag/changelog agreement, and terminal-batch fail-fast on a commit failure), both proven by a
single named test each. The AC-4 override remains a standing, previously-accepted disposition. The two
known edge cases surfaced by round-4 review (Truth 10's worktree-HEAD boundary, Truth 15's
no-version-file desync) are formally triaged, operator-attributed Phase 19 items (19d, 19f), not
ambiguous judgment calls requiring a human check here.

### Gaps Summary

No gaps remain open against Phase 17's stated goal. This pass:

1. **Verified plan 17-12 closes WR-04 (both rounds)** — the changelog-heading-before-the-tag defect that
   produced two prior Critical findings — with its own must-haves and behaviors confirmed directly
   against source and by executing every named regression test (26 hooks/version/git tests plus the
   terminal fail-fast test, all green).
2. **Confirmed no regression** in the 14 previously-verified truths at current HEAD: targeted regression
   tests for the CR-01/CR-02/17-06/17-07 fixes all still pass (11 individually named tests, 0 failed),
   and the full workspace suite is green across all 10 test targets with clippy and fmt clean.
3. **Independently confirmed** the orchestrator's classification of the intermittent `phase7_cli.rs`
   flake as pre-existing, already-deferred debt (ROADMAP 18e), not a Phase 17 regression.
4. **Independently confirmed** all three round-4 review Criticals (CR-01/CR-02/CR-03) are correctly
   triaged out of Phase 17 into Phase 19 (19d/19e/19f) — verified each at source rather than trusting the
   triage note: CR-01's false CHANGELOG entry is not present in the current clean tree; CR-02's
   `replace_version_in_contents` defect predates Phase 17 entirely (`aac079f`, Phase 2b) and 17-12 never
   touched it; CR-03's no-version-file desync is real and correctly scoped as a 17-12 design gap rather
   than a broken promise (the plan's own fixture always includes a `Cargo.toml`, so the scenario was
   never claimed to be covered).
5. **One documentation-hygiene finding flagged, not gated on:** `17-REVIEW.md`'s own frontmatter still
   reads `ship_gate: BLOCKED — 3 Critical` and was never updated after `ROADMAP.md` recorded the Phase 19
   triage disposition. `ROADMAP.md` is this project's authoritative scheduling record and is current;
   this is noted for hygiene, not treated as a blocker.

Phase 17's goal — closing the pipeline-reliability holes the Phase 16 dogfood exposed, including the
build-provenance staleness gate's own latent false-negative (CR-02) and the changelog/tag ordering defect
(WR-04) both discovered during this phase's own review cycles — is achieved. The two residual edge cases
(worktree-HEAD staleness comparison, no-version-file changelog desync) are explicitly out-of-scope
extensions correctly deferred to Phase 19, not unmet Phase 17 commitments.

---

_Verified: 2026-07-20T08:18:32Z_
_Verifier: Claude (gsd-verifier)_
_Supersedes: prior 17-VERIFICATION.md (2026-07-19T21:34:26Z, 14/14 passed) — that pass predates plan
17-12 (WR-04 closure) and round-3/round-4 code review entirely._
