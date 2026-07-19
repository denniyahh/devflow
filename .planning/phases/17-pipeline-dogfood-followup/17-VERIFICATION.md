---
phase: 17-pipeline-dogfood-followup
verified: 2026-07-19T21:34:26Z
status: passed
score: 14/14 must-haves verified
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "A non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential is reported before the stage launch (retrospective AC-4)"
    reason: "D-14(b) named no concrete security-artifact path/key at planning time; cross-AI review consensus #6 deferred it and reviewer-set enforcement to Phase 18's Hermes adapter, which is the first adapter with real reviewer storage"
    accepted_by: "Dennis Kim (gap-closure disposition, /gsd-plan-phase --gaps, phase 17)"
    accepted_at: "2026-07-19T11:26:28Z"
re_verification:
  previous_status: passed
  previous_score: 12/12
  gaps_closed:
    - "CR-02 (17-REVIEW.md) — build.rs's rerun-if-changed set (HEAD/refs/packed-refs) could not fingerprint the two inputs it actually reads (git status --porcelain, SystemTime::now()), so DEVFLOW_BUILD_DIRTY went stale under any CI checkout (packed-refs present) even though it was accidentally masked on this dev machine (no local packed-refs). The 17d staleness gate certified dirty binaries as Fresh. Resolved by 17-11: build.rs now always re-runs via an unfingerprintable sentinel path, DEVFLOW_BUILD_TIMESTAMP is removed entirely (the only value that changed every run, which is what would have forced a devflow-cli recompile on every build once the script always reruns), and the staleness gate's second signal is now a live dirty-flag comparison (build_dirty vs. tree_has_modified_build_inputs/affects_compiled_binary) replacing the old mtime arm."
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

**Verified:** 2026-07-19T21:34:26Z
**Status:** passed
**Re-verification:** Yes — fresh pass after gap-closure plan `17-11` (CR-02, commits `3e39cf6`,
`fd065e3`, `71c4ebd`) landed on top of the previously-passed 12/12 verification (2026-07-19T12:05Z,
superseded by this report). Note: the prior file's frontmatter also carried an addendum recording
that `17-08` landed after that pass without requiring re-verification (no new requirement, defect
within already-verified 17c scope) — that reasoning is not repeated here since `17-08` predates
this report's baseline.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `Unknown` completion never auto-advances any stage (17a, D-01/D-06) | ✓ VERIFIED | `outcome_policy::decide_action` still maps `Unknown -> GateReview` exhaustively (`outcome_policy.rs:38-56`); unaffected by 17-11's diff (`git diff cb9ddab^..71c4ebd -- crates/devflow-core/src/outcome_policy.rs` is empty) |
| 2 | A legitimately external-only stage with zero commits and a passing approved probe completes cleanly (17a, D-02/D-03/D-05) | ✓ VERIFIED | `agent_result::` module untouched by 17-11's diff |
| 3 | Typed outcomes `ResourceKilled`/`AgentUnavailable` route through a fail-closed exhaustive policy table (17b, D-07/D-11/D-12) | ✓ VERIFIED | Unchanged by 17-11's diff |
| 4 | Infrastructure-outcome ceiling (`state.infra_failures`) resets on every successful stage transition, bounding a stuck loop rather than the phase's lifetime (17b, D-08, CR-01) | ✓ VERIFIED | `state.infra_failures = 0;` still present immediately after `state.consecutive_failures = 0;` in `transition()` (`main.rs:1755`, shifted from `:1641` by 17-11's unrelated line additions). Directly executed `cargo test -p devflow transition_resets_infra_failures` → 1 passed |
| 5 | `RateLimited` outcomes auto-resume via a safe, per-phase `devflow resume` path (17b, D-09) | ✓ VERIFIED | Unchanged by 17-11's diff |
| 6 | Every terminal advance decision emits structured, machine-readable evidence (17b, D-10) | ✓ VERIFIED | Unchanged by 17-11's diff |
| 7 | A preflight readiness gate runs before every stage launch, never a hard exit, and never double-spawns on a resolved gate arm (17c, D-13/D-15/D-16, 17-08 CR-01) | ✓ VERIFIED | `run_preflight` still returns `Result<bool, CliError>` and `launch_stage` short-circuits on `Ok(false)` (`main.rs:1133-1135`); unchanged by 17-11's diff |
| 8 | A non-interactive plan or unavailable Ship-scoped `gh auth` is reported before stage launch (retrospective AC-4, narrowed scope) | ✓ **PASSED (override)** | Interactivity + `gh auth status` checks unchanged; missing-security-artifact and reviewer-set sub-checks remain deferred to Phase 18 per the standing, formally attributed override (unchanged since the prior pass) |
| 9 | `workflow_started` records executable and build provenance — commit + dirty flag (17d, D-21) | ✓ VERIFIED | `workflow_started_payload` at `main.rs:835-847` emits `commit`/`dirty`/`exe_path` (no `build_timestamp` field — removed by 17-11, see Truth 13). Directly executed `cargo test -p devflow workflow_started_payload_carries_build_provenance` → 1 passed |
| 10 | A stale self-dogfood binary is detected and blocked before stage launch, including a clean-tree strict-ancestor build (17d, D-17/D-18/D-19, retrospective AC-2, 17-06 WR-01) | ✓ VERIFIED | `embedded_commit_is_stale`'s exit-0 branch still nests a `git rev-parse HEAD` equality check (`main.rs:882-887`) — unchanged by 17-11's diff. Directly executed `cargo test -p devflow wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` → 1 passed |
| 11 | AC-1 (Phase 16's failed-Merge terminal-hook regression) still holds against final HEAD | ✓ VERIFIED | Unaffected by 17-11's diff; orchestrator-confirmed full-suite green includes these tests |
| 12 | Full workspace test suite, clippy, and fmt are green at final HEAD | ✓ VERIFIED | Orchestrator-confirmed (this session, prior to this report): `cargo test --workspace` → 367 passed, 0 failed, 0 ignored, 0 filtered across all 9 suites; `cargo clippy --workspace --all-targets -- -D warnings` → exit 0; `cargo fmt --check` → exit 0 |
| 13 | `build.rs` always re-runs (no path-fingerprinted `rerun-if-changed` for inputs it cannot fingerprint), so `DEVFLOW_BUILD_DIRTY` is never stale, and `DEVFLOW_BUILD_TIMESTAMP` is fully removed (17d, 17-REVIEW.md CR-02) | ✓ **VERIFIED** (new — CR-02 closed by 17-11) | `build.rs:43` declares a single never-existing sentinel path replacing the old `HEAD`/`refs`/`packed-refs` set; `DEVFLOW_BUILD_TIMESTAMP` has zero live emission or consumption (`rg -n "DEVFLOW_BUILD_TIMESTAMP"` matches only rationale comments in `build.rs` and a doc comment in `build_provenance.rs` explaining its removal). New end-to-end regression test builds a synthetic packed-refs checkout twice across a real working-tree edit and inspects cargo's own cached build-script `output` file (not just `env!()` from a single compile): directly executed `cargo test -p devflow --test build_provenance` → 3 passed, including `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` (asserts `false → true` across the edit — this is the exact reviewer reproduction and it now fails to reproduce the bug) |
| 14 | Staleness's second signal is a live dirty-flag comparison (`build_dirty` vs. `tree_has_modified_build_inputs`/`affects_compiled_binary`, reused not duplicated) replacing the old mtime arm; ancestry (17-06 Stale, 17-07 Ahead) is unchanged; an already-dirty build against a still-dirty tree is Indeterminate and never hard-blocks (Pitfall 4) | ✓ **VERIFIED** (new — CR-02 closed by 17-11) | `combined_staleness` (`main.rs:976-986`) takes `build_dirty: bool`; decision table matches plan exactly: `(false, modified)=>Stale`, `(true, modified)=>Indeterminate`, `(_, not modified)=>fall through to ancestry`. `affects_compiled_binary` has exactly one definition (`main.rs:951`), reused verbatim by `tree_has_modified_build_inputs` (`:940`) — confirmed via `rg -n "fn affects_compiled_binary"` (single hit). Directly executed all four: `combined_staleness_dirty_flag_arm_flags_modified_tree_when_build_was_clean` → 1 passed; `combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty` → 1 passed; `dirty_flag_arm_ignores_non_build_files_but_still_flags_sources` → 1 passed; `ahead_build_from_descendant_commit_warns_instead_of_blocking` (17-07, unchanged) → 1 passed |

**Score:** 14/14 truths verified (0 failed, 0 partial, 1 verified via standing attributed override)

### Gap Closure Detail (since prior `passed` verification, score 12/12)

| Gap | Prior Status | Fix Commits | Resolution |
|-----|--------------|-------------|------------|
| CR-02 — `build.rs` rerun-if-changed set could not fingerprint the inputs it actually reads, so `DEVFLOW_BUILD_DIRTY`/`DEVFLOW_BUILD_TIMESTAMP` went stale under CI (packed-refs) conditions, silently masked on dev machines | ✗ FAILED (17-REVIEW.md Critical, reclassified manual-only pending operator decision; prior 12/12 pass predates this review) | `3e39cf6` (fix), `fd065e3` (docs disposition), `71c4ebd` (plan completion) | `build.rs` always re-runs via an unfingerprintable sentinel path; `DEVFLOW_BUILD_TIMESTAMP` fully removed (emission and every consumer); staleness's second signal is now a live `build_dirty` vs. `tree_has_modified_build_inputs` comparison, reusing 17-10's `affects_compiled_binary` predicate. New end-to-end regression test (`build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild`) reproduces the reviewer's exact CI-shaped fixture and proves the fix by inspecting cargo's own build-script output cache across two real builds, not just a single compile's `env!()` |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/build.rs` | Single always-rerun sentinel; no `DEVFLOW_BUILD_TIMESTAMP` emission | ✓ VERIFIED | Lines 43 (sentinel), 59-69 (only `DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY` emitted); rationale comment at `:1-32` explains the CR-02 fix |
| `crates/devflow-cli/src/main.rs` (`combined_staleness`/`enforce_build_staleness`) | Take `build_dirty: bool`, not `build_timestamp: u64`; dirty-flag decision table replaces mtime arm | ✓ VERIFIED | `main.rs:976-986` (`combined_staleness`), `:1058-1063` (`enforce_build_staleness` signature), `:1140-1145` (call site passes `env!("DEVFLOW_BUILD_DIRTY") == "true"`, no timestamp arg) |
| `crates/devflow-cli/src/main.rs` (`workflow_started_payload`) | No `build_timestamp` field | ✓ VERIFIED | `main.rs:835-847`: `commit`, `dirty`, `exe_path` only |
| `crates/devflow-cli/src/main.rs` (`tree_has_modified_build_inputs`) | Reuses `affects_compiled_binary`, does not duplicate it | ✓ VERIFIED | `main.rs:934-941` calls `affects_compiled_binary` (defined once at `:951`); `rg` confirms single definition |
| `crates/devflow-cli/tests/build_provenance.rs` | End-to-end regression test proving `build.rs` reruns across a working-tree edit | ✓ VERIFIED | `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` (lines 139-202); directly executed, passes |
| `.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` | CR-02 marked Fixed, no open Critical | ✓ VERIFIED | Header line 40-42: "0 Critical open"; CR-02 entry and disposition table row (`:541`) both say Fixed via `17-11`, commit `3e39cf6` |
| `.planning/ROADMAP.md` | Phase 17 line reflects 11/11 plans executed | ✓ VERIFIED | `ROADMAP.md:194`: "Plans: 11/11 plans executed"; `17-11-PLAN.md` listed at `:199` |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `build.rs` `main()` | cargo's rerun cache | `cargo:rerun-if-changed=<never-existing sentinel>` | ✓ WIRED | `build.rs:43`; a missing path forces cargo's "always rerun" rule (documented cargo behavior, matches the comment's claim) |
| `enforce_build_staleness` call site (`launch_stage`) | `combined_staleness`'s dirty-flag arm | `env!("DEVFLOW_BUILD_DIRTY") == "true"` passed as `build_dirty` | ✓ WIRED | `main.rs:1140-1145` → `:1064` → `:981-984` |
| `tree_has_modified_build_inputs` | `affects_compiled_binary` | direct call, not a re-derived predicate | ✓ WIRED | `main.rs:940` calls the single definition at `:951` |
| `17-REVIEW.md` CR-02 disposition | `17-11-PLAN.md`/`17-11-SUMMARY.md` | named fix commit `3e39cf6` | ✓ WIRED | Both documents cross-reference the same commit and decision |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| CR-01 fix (infra_failures reset) still holds post-17-11 | `cargo test -p devflow transition_resets_infra_failures` | 1 passed, 0 failed | ✓ PASS |
| WR-01 fix (17-06, strict-ancestor Stale) still holds post-17-11 | `cargo test -p devflow wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` | 1 passed, 0 failed | ✓ PASS |
| 17-07 Ahead classification unchanged | `cargo test -p devflow ahead_build_from_descendant_commit_warns_instead_of_blocking` | 1 passed, 0 failed | ✓ PASS |
| `workflow_started` payload carries commit+dirty, no timestamp | `cargo test -p devflow workflow_started_payload_carries_build_provenance` | 1 passed, 0 failed | ✓ PASS |
| CR-02 fix: clean build + dirtied tree ⇒ Stale | `cargo test -p devflow combined_staleness_dirty_flag_arm_flags_modified_tree_when_build_was_clean` | 1 passed, 0 failed | ✓ PASS |
| CR-02 fix: dirty build + still-dirty tree ⇒ Indeterminate, never blocks | `cargo test -p devflow combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty` | 1 passed, 0 failed | ✓ PASS |
| CR-02 fix: non-build-affecting file changes ignored, source changes still flagged | `cargo test -p devflow dirty_flag_arm_ignores_non_build_files_but_still_flags_sources` | 1 passed, 0 failed | ✓ PASS |
| CR-02 end-to-end regression: `build.rs` actually reruns and flips `DEVFLOW_BUILD_DIRTY` across two real `cargo build -p devflow` invocations in a synthetic packed-refs checkout | `cargo test -p devflow --test build_provenance` | 3 passed, 0 failed (34s, includes two real cargo builds) | ✓ PASS |
| `affects_compiled_binary` has exactly one definition (not duplicated) | `rg -n "fn affects_compiled_binary" -g '!target' .` | single hit, `main.rs:951` | ✓ PASS |
| `DEVFLOW_BUILD_TIMESTAMP` has no live emission/consumption | `rg -n "DEVFLOW_BUILD_TIMESTAMP" -g '!target' .` | 3 hits, all rationale/doc comments (`build.rs:28,63`; `build_provenance.rs:6`) — zero `cargo:rustc-env=` emission, zero `env!(...)` consumer | ✓ PASS |
| Full workspace suite, clippy, fmt (orchestrator-confirmed, this session) | `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --check` | 367 passed / 0 failed / 0 ignored across 9 suites; clippy exit 0; fmt exit 0 | ✓ PASS |

### Requirements Coverage

No `.planning/REQUIREMENTS.md` file exists in this project (confirmed absent); per-phase requirements
are tracked in `17-DOGFOOD-RETROSPECTIVE.md` (P1–P4, mapped 1:1 to scope units 17a–17d) and
`17-CONTEXT.md` (D-01…D-21), unchanged from the prior verification pass.

| Requirement | Source | Description | Status | Evidence |
|-------------|--------|-------------|--------|----------|
| P1 / 17a (D-01–D-06) | Retrospective + CONTEXT | `Unknown` non-advance + Layer 0/3 rework | ✓ SATISFIED | Truths 1, 2 |
| P2 / 17b (D-07–D-12) | Retrospective + CONTEXT | Typed outcomes + deterministic retry policy | ✓ SATISFIED | Truths 3, 4, 5, 6 |
| P3 / 17c (D-13–D-16) | Retrospective + CONTEXT | Preflight readiness gate | ✓ SATISFIED | Truths 7, 8 (Truth 8 narrowed via standing override) |
| P4 / 17d (D-17–D-21) | Retrospective + CONTEXT | Build provenance + stale-binary detection | ✓ SATISFIED | Truths 9, 10, 13, 14 (13/14 new — CR-02 closure) |
| AC-1 (criterion 1) | Retrospective | Failed-Merge terminal contract, verify only | ✓ SATISFIED | Truth 11 |
| D-01…D-21 | 17-CONTEXT.md | All 21 numbered decisions | ✓ SATISFIED | Every decision maps to a truth above; no orphaned decision |

Plan-frontmatter requirement IDs across all 11 plans (`17a`×2, `17b`×4, `17c`×3, `17d`×5, counting
`17-06`'s multi-tag plan once per tag) cover all four scope units with no gaps and no orphans —
confirmed by grepping `requirements:` across every `17-*-PLAN.md`.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| ~~`crates/devflow-cli/tests/build_provenance.rs`~~ | ~~22-28~~ | ~~`build_commit_is_accessible_and_does_not_panic` still asserts nothing (`let _ = commit.len();`)~~ — **SUPERSEDED (round-2 review WR-12).** That symbol no longer exists: GAP-4 was closed by `46058a7`, which renamed it to `build_commit_is_empty_or_a_full_hex_sha` (`build_provenance.rs:23`) and it now asserts empty-or-40-char-hex. Row 7 has 4 real covering tests, not 3 | ✅ Resolved | None — the row understated completeness and pointed readers at a nonexistent symbol |
| ~~`.planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md`~~ | ~~frontmatter~~ | ~~`nyquist_compliant: false`, written against the pre-17-11 state~~ — **SUPERSEDED (round-2 review WR-13).** `17-VALIDATION.md:7` reads `nyquist_compliant: true`, flipped by `3d6e6a6` and re-verified by mutation in re-audit #6 (`41345fc`). The re-validation pass this row asked a future auditor to open is complete | ✅ Resolved | None — leaving it would have opened a redundant re-validation task |
| `crates/devflow-cli/src/main.rs` | 261-296 (17-REVIEW.md WR-01), `:1367` (WR-03) | Two pre-existing Warning-level findings (misleading universal warning message on non-self-dogfood projects; a doc comment misdescribing `infra_failures` call graph) remain open, confirmed unchanged in the current diff | ℹ️ Info | Both explicitly out of this plan's scope (WR-03's fix is named in `ROADMAP.md:183` as deferred to Phase 18); not must-haves for any 17a–17d truth |

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any file `17-11` modified
(`build.rs`, `main.rs`'s touched region, `build_provenance.rs`).

### Human Verification Required

None. CR-02's closure is confirmed by direct source reading and live execution of every named
regression test (not SUMMARY.md narration), including a genuine two-cargo-build end-to-end fixture
that reproduces the reviewer's exact CI-shaped repro conditions. The AC-4 override remains a
standing, previously-accepted disposition — not a new judgment call for this pass.

### Gaps Summary

No gaps remain. The one gap this pass tracked (CR-02) is closed:

1. **CR-02 (`build.rs` rerun-if-changed set could not fingerprint its real inputs)** — closed by
   code fix (`3e39cf6`), confirmed by direct source read (`build.rs:43` sentinel;
   `main.rs:976-986` dirty-flag decision table) and by directly executing every new and
   affected regression test: `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild`
   (the end-to-end reproduction, 3 passed in the `build_provenance` integration suite),
   `combined_staleness_dirty_flag_arm_flags_modified_tree_when_build_was_clean`,
   `combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty`,
   `dirty_flag_arm_ignores_non_build_files_but_still_flags_sources`,
   `workflow_started_payload_carries_build_provenance`, plus regression checks confirming
   17-06's strict-ancestor Stale (`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`)
   and 17-07's Ahead classification (`ahead_build_from_descendant_commit_warns_instead_of_blocking`)
   are unchanged, exactly as the plan's "behaviors" must-have required. All ran individually with
   explicit "N passed" assertions (never a bare exit-code check).

Phase 17's goal — closing the pipeline-reliability holes the Phase 16 dogfood exposed, including the
build-provenance staleness gate's own latent false-negative (CR-02) discovered during this phase's
own review — is achieved.

---

_Verified: 2026-07-19T21:34:26Z_
_Verifier: Claude (gsd-verifier)_
_Supersedes: prior 17-VERIFICATION.md (2026-07-19T12:05:00Z, 12/12 passed, addendum 19:25:00Z for 17-08) — that pass predates plan 17-11 and 17-REVIEW.md's CR-02 finding entirely._
