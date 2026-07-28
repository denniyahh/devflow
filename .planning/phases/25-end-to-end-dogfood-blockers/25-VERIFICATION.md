---
phase: 25-end-to-end-dogfood-blockers
verified: 2026-07-28T02:12:39Z
status: gaps_found
score: 6/9 must-haves verified
behavior_unverified: 1
overrides_applied: 0
gaps:
  - truth: "D-09's major-bump gate never ships unattended, in the default (worktree) execution path"
    status: failed
    reason: >
      Two independently-confirmed Critical defects (CR-01, CR-02 from 25-REVIEW.md) mean the
      gate does not deliver D-09's stated contract in the default launch mode. CR-01:
      `generic_preflight_checks` (preflight.rs:718-724) `?`-chains interactivity -> gh-auth ->
      major-bump; if gh-auth fails first, major-bump never runs, and approving that gate via
      `GateAction::Advance` calls `launch_stage_inner` directly without ever re-running
      `generic_preflight_checks` -- permanently skipping the major-bump check for that stage
      launch. CR-02: `preflight_major_bump_check` shells git against `project_root` only
      (preflight.rs:614-668), never `state.worktree_path`; in the default worktree launch mode
      the phase's own commits live on the feature branch and are unreachable from
      `project_root`'s HEAD until `hooks_after_ship`'s Merge step runs -- which is AFTER this
      preflight check. A `feat!:`/`BREAKING CHANGE:` commit made during Code will therefore
      never be seen by this gate; it is first classified inside `VersionBump`, after Merge has
      already committed, with no rollback. Both confirmed by direct source read (preflight.rs,
      pipeline_launch.rs, state.rs) independent of REVIEW.md's own analysis. No fix commit
      followed the review (`ab119c0` "docs(25): add code review report" is HEAD).
    artifacts:
      - path: "crates/devflow-cli/src/preflight.rs"
        issue: "generic_preflight_checks (:718-724) short-circuits via `?` in interactivity -> gh-auth -> major-bump order; preflight_major_bump_check (:614-668) always evaluates project_root, never state.worktree_path.as_deref().unwrap_or(project_root)"
      - path: "crates/devflow-cli/src/pipeline_launch.rs"
        issue: "run_preflight's GateAction::Advance arm (~:796-806) calls launch_stage_inner(state, None, None) directly, never re-running generic_preflight_checks, so a check that never ran once (because an earlier check failed) is never run at all before the stage launches"
    missing:
      - "Make generic_preflight_checks aggregate all three checks' failures rather than short-circuit on the first (REVIEW.md's suggested fix a), so a human always sees the major-bump reason whenever it applies regardless of what else failed"
      - "Thread the execution root through preflight_major_bump_check as state.worktree_path.as_deref().unwrap_or(project_root), mirroring staleness.rs::enforce_build_staleness's existing execution_root idiom"
      - "A regression test that builds a real worktree fixture (mirroring staleness.rs::worktree_staleness_fixture) with a feat!: commit only on the worktree's feature branch, asserting preflight_major_bump_check still fires against state.worktree_path"
  - truth: "release_range_start's commit-range anchor stays correct (excludes pre-release develop history) across realistic release topologies"
    status: failed
    reason: >
      Independently confirmed by direct source read (version.rs:258-313), matching CR-03 from
      25-REVIEW.md. The function inspects only the ancestry path's first commit (C1) and checks
      whether the baseline tag is an ancestor of C1's first parent to decide "ordinary range" vs
      "squashed release via sync-merge." When a commit lands directly on trunk between the tag
      and the sync-merge-back (e.g. a hotfix pushed straight to main), --ancestry-path picks up
      that intervening commit as C1 instead of the sync-merge commit; C1's first parent is then
      the tag commit itself, `git merge-base --is-ancestor <tag> <tag>` returns true (a commit is
      its own ancestor), and the function concludes the tag already sat on mainline -- silently
      reverting to the literal, over-broad `tag..HEAD` range this function exists to avoid (the
      exact "677 commits, 62 feat" pathology the whole D-08 amendment was built to fix). Neither
      existing regression test (squash_sync_topology_classifies_only_post_merge_commits,
      two_squash_sync_cycles_anchor_to_the_second_merge_only) constructs an intervening trunk
      commit -- confirmed via grep, no test name matches this topology.
    artifacts:
      - path: "crates/devflow-core/src/version.rs"
        issue: "release_range_start (:258-313) anchors on C1's first-parent ancestry only, not the last merge commit in the full --ancestry-path list; silently falls back to the naive tag..HEAD range under the untested topology"
    missing:
      - "Walk the full --ancestry-path list and anchor at the last (closest-to-HEAD) merge commit, falling back to baseline_tag only when the path contains no merge commit at all (REVIEW.md's own sketch fix)"
      - "A fixture with a direct trunk commit between the tag and the sync-merge-back, asserting the classified range still excludes pre-release develop history"
deferred: []
behavior_unverified_items:
  - truth: "25e: the 999.47 CI flake (cmdline-inheritance race in looks_like_devflow_process's old test shape) is actually closed"
    test: "Observe cargo test --workspace (or the specific retargeted tests) green across several consecutive CI-on-branch pushes in the pinned CI container"
    expected: "No flake reproduction across multiple pushes, matching the project's own established precedent that local-green is explicitly insufficient for this class of race (19-RESEARCH.md)"
    why_human: "999.47's confirmed mechanism (fork/exec cmdline-inheritance window) only reproduces reliably inside the pinned CI container; local probes at thousands of spawns never observed it, so no local test run -- however green -- can establish closure. Both 25-02-SUMMARY.md and 25-07-SUMMARY.md explicitly disclose this limitation themselves."
human_verification:
  - test: "Observe cargo test --workspace green across several CI-on-branch pushes for the retargeted 25e tests"
    expected: "No flake reproduction over multiple pushes in the pinned CI container"
    why_human: "Only reproduces in CI container per 999.47's confirmed mechanism; local green (already obtained, 4 consecutive runs per 25-02/25-07 SUMMARYs) does not establish closure"
  - test: "Re-review 25-VALIDATION.md's Manual-Only Verifications rows 1-3 (CONTRIBUTING.md step 5 against the tracked .gitconfig; ROADMAP Acceptance paragraph vs D-15/D-16; PROJECT.md Constraints bullet vs the 25c algorithm) as a human, not as an executor self-report"
    expected: "A human confirms the same conclusions 25-04-SUMMARY.md records as 'Confirmed' -- this verifier independently re-confirmed the textual claims via grep (devflow.releaseSigningKey present, tag.gpgsign=false absent, no require-shipped in the Phase 25 Acceptance entry, ban-bullet amended), but these three rows are explicitly designated human-judgment checks in 25-VALIDATION.md, and an executor's own self-report of a human-judgment row cannot substitute for the sign-off itself"
    why_human: "25-VALIDATION.md designates these as manual-only/human-judgment verifications; 25-04-SUMMARY.md's 'Confirmed' entries are the executor's own self-assessment of exactly the checks it was not positioned to discharge"
---

# Phase 25: End-to-End Dogfood Blockers Verification Report

**Phase Goal:** Close the blockers that prevent an unattended end-to-end DevFlow-driven run from completing — units 25a (base-ref currency), 25b (staleness pin/hoist), 25c (compute_version + major-bump gate), 25d (state-orphaned process reaping), 25e (looks_like_devflow_process flake), 25f (CONTRIBUTING drift), plus 999.38 (PATH race) folded in.
**Verified:** 2026-07-28T02:12:39Z
**Status:** gaps_found
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | 25a — a run starts on a current base ref; "heading present but code stale" is closed, not just "heading absent" | ✓ VERIFIED | `ensure_base_ref_current(project_root, DEVELOP)?` wired at `commands.rs:154`, immediately before `ensure_phase_reachable_on_base(...)` at `:164`. 9 real-git-fixture tests in `preflight.rs` (`currency_*`) exercise Current/Ahead/Behind-fastforward/Behind-checked-out-refuse/Diverged/Undeterminable/fetch-failure. Spot-run `currency_behind_and_not_checked_out_fast_forwards_and_proceeds` -> `1 passed`. |
| 2 | 25b — `enforce_build_staleness` is adjudicated once per run (at `start`), no longer re-invoked on every stage transition | ✓ VERIFIED | Call hoisted to `commands.rs:278`, after `state.worktree_path` is set (`:213`); deleted from `pipeline_launch.rs::launch_stage_inner`. Behavioral regression test `mid_run_stage_transition_does_not_readjudicate_staleness` drives the same fixture through both call shapes and asserts exactly one refusal — confirmed present in `staleness.rs`. |
| 3 | 25c (derivation) — `compute_version` derives from (highest reachable semver tag, conventional-commit classification), refuses on unreachable baseline, floors correctly | ✓ VERIFIED | `version.rs`'s `highest_semver_tag`/`reachable_semver_baseline`/`classify_range_bump`/`VersionError::UnreachableBaseline` all present and composed in `compute_version`; 36+ tests including D-10 floor/refusal fixtures. **Caveat (WR-02, non-blocking):** a prerelease-tagged baseline (e.g. `v2.0.0-rc.1`) silently skips the stable release on the next bump — not exercised in this repo today, recorded as a warning, not a gap. |
| 4 | 25c (gate) — a major version bump opens a gate and never ships unattended, in the **default (worktree) execution path** | ✗ FAILED | Independently confirmed via source read (not just REVIEW.md): `generic_preflight_checks` (`preflight.rs:718-724`) short-circuits `interactivity -> gh-auth -> major-bump` via `?`; an earlier failure means major-bump never runs, and `GateAction::Advance` (`pipeline_launch.rs`) calls `launch_stage_inner` directly, never re-running `generic_preflight_checks` — permanently bypassing the gate for that launch (CR-01). Separately, `preflight_major_bump_check` always shells git against `project_root` (verified: `run_preflight(&project_root, ...)` at `pipeline_launch.rs:192` uses `state.project_root.clone()`, never `state.worktree_path`), so in the default worktree flow the phase's own commits are invisible to this check until *after* Merge has already run (CR-02). Both existing integration tests use a no-worktree fixture (`major_bump_fixture`), the one case where this bug cannot manifest. See gaps. |
| 5 | 25c (anchor) — `release_range_start`'s commit-range anchor excludes pre-release history across realistic release topologies | ✗ FAILED | Independently confirmed via source read (`version.rs:258-313`, matching CR-03): the anchor heuristic inspects only C1's first-parent ancestry; a commit landing directly on trunk between the tag and the sync-merge-back makes C1 that intervening commit, whose first parent *is* the tag — `merge-base --is-ancestor <tag> <tag>` trivially returns true, and the function silently falls back to the naive `tag..HEAD` range it was built to avoid. No existing test constructs this topology (grep for trunk/hotfix/direct-commit fixtures: 0 matches). |
| 6 | 25d — a stalled run recovers without `kill -9`: bounded TERM->KILL escalation with verified death; registry-independent discovery; wrapper+child reaped together | ✓ VERIFIED | `terminate_and_verify` and `discover_stray_devflow_processes` present in `agent.rs`; `gate sweep --reap-strays` and `doctor`'s stray finding wired in `commands.rs`/`main.rs`. Real e2e tests (`reap_strays_e2e.rs`) delete a process's root out from under it and confirm the registry-independent primitives still find and clear it (including SIGTERM-ignoring). Spot-run `terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child` and `reap_stray_candidates_escalates_to_kill_for_a_term_ignoring_child` -> both `1 passed`. |
| 7 | 25e — the 999.47 CI flake (cmdline-inheritance race) is closed | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Production guard (`(pid, starttime)` identity pair) confirmed already live at `commands.rs:1421` (`lock::holder_identity`), predating this phase. `looks_like_devflow_process` marked `#[deprecated]`; both flaky tests retargeted to assert the identity guard directly, eliminating the `execve` race by construction (no `spawn()`). But the flake's closure can only be confirmed by CI-on-branch stability across several pushes per this project's own established precedent (19-RESEARCH.md) — both 25-02-SUMMARY.md and 25-07-SUMMARY.md explicitly disclose that only local runs were performed. Routed to human verification. |
| 8 | 999.38 — the test-suite PATH race (`ahead_build_from_descendant_commit_warns_instead_of_blocking`) is de-raced | ✓ VERIFIED | `ENV_MUTEX` guard added (matching its three siblings) and fixture reads rerouted through `test_support::git_command` — confirmed present in `staleness.rs:1044-1046`. This is a structural fix (serialization + hermetic reads), not merely flake-reduction, and is independently verifiable locally (unlike 25e, which needs the specific fork/exec CI-only window). |
| 9 | 25f — CONTRIBUTING.md's release procedure and the ROADMAP/PROJECT.md versioning-policy prose no longer drift from what 25c implements | ✓ VERIFIED (3 sub-items pending human sign-off) | Confirmed via direct grep: `CONTRIBUTING.md` names `devflow.releaseSigningKey` via the `-c user.signingkey=...` indirection, `tag.gpgsign=false` no longer appears; `.planning/ROADMAP.md`'s June-2026 ban bullet records the D-06 lift; the Phase 25 "Acceptance" paragraph contains no `require-shipped` requirement and matches D-15/D-16. **However**, 25-VALIDATION.md designates these as human-judgment/manual-only checks, and 25-04-SUMMARY.md's "Confirmed" rows are the executor's own self-report of exactly those checks — per this project's own contract, that self-report does not substitute for actual human sign-off. Routed to human verification (not blocking). |

**Score:** 6/9 truths verified (1 present-but-behavior-unverified, 2 failed)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/preflight.rs::ensure_base_ref_current`/`base_ref_currency` | 25a currency probe | ✓ VERIFIED | Present, wired at `commands.rs:154`, 9 tests |
| `crates/devflow-cli/src/commands.rs::start()` staleness call site | 25b one-shot gate | ✓ VERIFIED | Present at `:278`, after `worktree_path` set; removed from `pipeline_launch.rs` |
| `crates/devflow-core/src/version.rs::compute_version` + helpers | 25c derivation | ✓ VERIFIED | Present, composed correctly; see truth 3/5 split |
| `crates/devflow-cli/src/preflight.rs::preflight_major_bump_check`/`major_bump_check_applies` | 25c D-09 gate | ⚠️ WIRED but not correctly scoped | Present, composed into `generic_preflight_checks`, but see truth 4 — CR-01/CR-02 |
| `crates/devflow-core/src/agent.rs::terminate_and_verify`/`discover_stray_devflow_processes` | 25d primitives | ✓ VERIFIED | Present, tested with real spawned children |
| `crates/devflow-cli/src/commands.rs::reap_stray_candidates`, `gate_sweep --reap-strays`, doctor stray finding | 25d CLI surface | ✓ VERIFIED | Present, wired in `main.rs`/`commands.rs`, e2e tests present |
| `crates/devflow-core/src/agent.rs` retargeted `looks_like_devflow_process` tests | 25e | ✓ VERIFIED (behavior CI-pending) | `#[deprecated]`, tests retargeted to identity guard |
| `crates/devflow-cli/src/staleness.rs` de-raced 999.38 test | 999.38 | ✓ VERIFIED | `ENV_MUTEX` + hermetic reads present |
| `CONTRIBUTING.md`, `.planning/ROADMAP.md`, `.planning/PROJECT.md` | 25f docs | ✓ VERIFIED (human sign-off pending) | Confirmed via grep |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `commands.rs::start()` | `preflight.rs::ensure_base_ref_current` | direct call, before `ensure_phase_reachable_on_base` | ✓ WIRED | Confirmed ordering at `:154`/`:164` |
| `commands.rs::start()` | `staleness.rs::enforce_build_staleness` | direct call, after `worktree_path` set | ✓ WIRED | Confirmed at `:278` |
| `pipeline_launch.rs::launch_stage` | `preflight.rs::run_preflight` -> `generic_preflight_checks` -> `preflight_major_bump_check` | call chain | ⚠️ WIRED, wrong scope | `run_preflight(&project_root, ...)` at `:192` uses `state.project_root.clone()` — never `state.worktree_path`, so the D-09 gate is wired but evaluates the wrong ref in the default launch mode (CR-02) |
| `preflight.rs::run_preflight`'s `GateAction::Advance` arm | `pipeline_launch.rs::launch_stage_inner` | direct call, bypassing `generic_preflight_checks` | ⚠️ WIRED, by design — but composes badly with CR-01 | Confirmed: Advance calls `launch_stage_inner(state, None, None)` directly; a check that never ran (because an earlier one failed first) is never given a second chance |
| `main.rs::Sweep{reap_strays}` | `commands.rs::gate_sweep` -> `reap_stray_candidates` | CLI flag threading | ✓ WIRED | Confirmed at `main.rs:526-527`, `commands.rs:1149-1151` |
| `commands.rs::stop_via_lock` | `lock::holder_identity` | direct call | ✓ WIRED | Confirmed at `commands.rs:1421` — the production guard 25e's tests were retargeted to |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| 25a fast-forward actually advances the local ref | `cargo test --package devflow --bin devflow preflight::tests::currency_behind_and_not_checked_out_fast_forwards_and_proceeds -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| 25d TERM->KILL escalation clears a TERM-ignoring child (devflow-core) | `cargo test --package devflow-core --lib agent::tests::terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| 25d TERM->KILL escalation clears a TERM-ignoring child (CLI reap path) | `cargo test --package devflow --bin devflow commands::tests::reap_stray_candidates_escalates_to_kill_for_a_term_ignoring_child -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| 25c D-09 gate fires in the (untested-by-review, no-worktree) fixture shape | `cargo test --package devflow --bin devflow preflight::tests::run_preflight_major_bump_gates_and_never_ships_unattended -- --exact` | `1 passed; 0 failed` | ✓ PASS (confirms the gate works only in the fixture shape that does NOT exercise CR-02) |
| Full workspace regression | `cargo test --workspace --no-fail-fast` | `387 passed` (devflow-core lib) + other targets, 0 failed observed in this run; orchestrator's independently-measured 674/0 accepted per task brief | ✓ PASS |
| Debt markers in phase-touched files | `rg -n 'TBD|FIXME|XXX'` across preflight.rs, commands.rs, pipeline_launch.rs, staleness.rs, agent.rs, version.rs, pipeline_gate.rs, main.rs, reap_strays_e2e.rs, CONTRIBUTING.md | no matches | ✓ PASS |

### Requirements Coverage

*This project has no `.planning/REQUIREMENTS.md`; tracked by unit identifier per the phase's own convention. Not reported as a gap.*

| Unit | Backlog ID | Description | Status | Evidence |
|------|-----------|--------------|--------|----------|
| 25a | 999.51/DEN-76 | Base-ref currency | ✓ SATISFIED | Truth 1 |
| 25b | 999.48/DEN-73 | Staleness hoist | ✓ SATISFIED | Truth 2 |
| 25c | 999.49/DEN-74 | Version derivation + major-bump gate | ✗ PARTIALLY SATISFIED | Truth 3 (derivation) satisfied; truths 4/5 (gate correctness, anchor robustness) FAILED |
| 25d | 999.44/DEN-68 | Orphan process reaping | ✓ SATISFIED | Truth 6 |
| 25e | 999.47/DEN-72 | Flaky test dead predicate | ⚠️ PRESENT, CI-pending | Truth 7 |
| 25f | (no backlog ID) | CONTRIBUTING drift | ✓ SATISFIED (docs sign-off pending) | Truth 9 |
| 999.38 | folded in | PATH race | ✓ SATISFIED | Truth 8 |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `25-01-SUMMARY.md` | (whole file) | Missing required `## Self-Check` section | ⚠️ Warning | Executor→verifier contract violation — self-report cannot be cross-checked against the executor's own claimed verification steps for this plan. Independently mitigated in this verification by direct source/test re-execution. |
| `25-02-SUMMARY.md` | (whole file) | Missing required `## Self-Check` section | ⚠️ Warning | Same as above. |
| `crates/devflow-cli/src/preflight.rs` | 614-668, 718-724 | Gate composition + scope defect (CR-01/CR-02) | 🛑 Blocker | See truths 4 and gaps |
| `crates/devflow-core/src/version.rs` | 258-313 | Anchor heuristic regression under untested topology (CR-03) | 🛑 Blocker | See truth 5 and gaps |
| `crates/devflow-cli/src/preflight.rs` | 355-386, 445-479 | WR-01 (shallow-clone misclassifies as Diverged), WR-03 (fast-forward doesn't check other worktrees) | ℹ️ Info (recorded, not gating) | Latent, not currently reachable through this project's own commands per REVIEW.md's own severity call — not re-litigated here |
| `crates/devflow-core/src/version.rs` | 430-466 | WR-02 (prerelease-tag baseline skips stable release) | ℹ️ Info (recorded, not gating) | Not currently exercised by this repository's tagging practice |

## Human Verification Required

### 1. 25e flake closure — CI-on-branch confirmation

**Test:** Observe `cargo test --workspace` (or the two retargeted tests specifically) green across several consecutive pushes on a branch, inside the pinned CI container.
**Expected:** No reproduction of the 999.47 cmdline-inheritance race over multiple pushes.
**Why human:** The confirmed mechanism only reproduces reliably inside the pinned CI container; local runs (however many, however green) cannot establish this per the project's own documented precedent (19-RESEARCH.md), and both 25-02-SUMMARY.md and 25-07-SUMMARY.md say so themselves.

### 2. 25f's three designated human-judgment doc checks

**Test:** A human (not the executor) reviews 25-VALIDATION.md's Manual-Only Verifications rows 1-3: CONTRIBUTING.md step 5 against the tracked `.gitconfig`; the ROADMAP Acceptance paragraph against D-15/D-16; PROJECT.md's Constraints bullet against what 25-01 implements.
**Expected:** Independent confirmation matching what this verifier's own greps and 25-04-SUMMARY.md's self-report both already suggest.
**Why human:** 25-VALIDATION.md explicitly designates these as human-judgment/manual-only rows because "no assertion can prove" prose-correctness or doc-consistency claims of this shape. 25-04-SUMMARY.md's "Confirmed" entries are the executor's own self-assessment of the very checks it is not positioned to discharge — recorded per the task brief's `known_self_report_gaps` item 2 as ASSESSED, PENDING HUMAN SIGN-OFF, not satisfied.

## Gaps Summary

Two Critical, source-confirmed defects (CR-01, CR-02 from `25-REVIEW.md`, independently re-confirmed by this verifier by reading `preflight.rs`, `pipeline_launch.rs`, and `state.rs` directly — not merely trusting the review) mean **25c's own headline contract — "a major bump opens a gate; it never ships unattended" (D-09) — is not actually delivered in the default worktree execution path**, which is this project's default `devflow start` behavior (`worktree = true` unless `--no-worktree`). The only integration tests exercising this gate use the `--no-worktree` fixture shape, which structurally cannot expose either defect. A third Critical defect (CR-03), also independently confirmed by direct source read, means the commit-range anchor `release_range_start` depends on can silently revert to the exact naive, over-broad range the whole 25c effort was built to eliminate, under a plausible-but-untested release topology (a commit landing directly on trunk between a tag and the next sync-merge-back).

None of these three defects were fixed after `25-REVIEW.md` was written and committed (`ab119c0` is the current HEAD for this phase's tracked files; no subsequent commit touches `preflight.rs` or `version.rs`).

All other units — 25a, 25b, 25d, 999.38 — are solidly implemented, wired, and behaviorally verified against real git/process fixtures, not merely present. 25e is fully implemented and structurally sound (the race is removed by construction, not just made rarer) but its closure claim cannot be verified until CI-on-branch stability is observed, consistent with the project's own stated precedent and both relevant SUMMARYs' own disclosures. 25f's documentation corrections are textually confirmed by this verifier's own greps but three rows remain formally pending human sign-off per 25-VALIDATION.md's own design, not a code gap.

Two SUMMARY.md files (25-01, 25-02) are missing the required `## Self-Check` section — a documentation/process gap in the executor→verifier contract, independently mitigated here by direct source/test re-verification but worth remediating for future waves.

**This looks like real, closeable work, not a design failure of the phase's approach.** REVIEW.md itself proposes concrete, small fixes for all three Critical findings (aggregate-checks or re-run-on-Advance for CR-01; thread `state.worktree_path` for CR-02; walk-the-ancestry-path-for-the-last-merge-commit for CR-03), each scoped to the same files already touched by 25-06/25-01.

---

*Verified: 2026-07-28T02:12:39Z*
*Verifier: Claude (gsd-verifier)*
