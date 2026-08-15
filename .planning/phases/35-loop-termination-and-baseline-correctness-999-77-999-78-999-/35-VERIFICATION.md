---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
verified: 2026-08-07T00:00:00Z
status: passed
score: 6/6 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Phase 35: Loop-Termination and Baseline Correctness Verification Report

**Phase Goal:** Operator can trust the Code↔Validate loop's failure-gating mechanics and the
release signing preflight behave as documented and are enforced by regression tests, not by
correctness-by-construction alone — a transient `git` failure can no longer forge a fresh
baseline, the loop has a bound independent of trivial per-cycle commits, a `--force` re-run
doesn't inherit a stale verdict, the worktree-mode checkpoint call site is regression-tested, and
`release --check`'s signing result reflects a real probe rather than a predictor that has already
false-negatived live twice.

**Verified:** 2026-08-07
**Status:** passed
**Re-verification:** No — initial verification

## Method

This report does not take SUMMARY.md claims at face value. For every roadmap criterion I read the
production code at the cited line, confirmed the mechanism actually matches what the plan and
SUMMARY describe, then independently re-ran the named regression tests myself (not the executor's
transcribed output) with `cargo test -p <pkg> --bin/--lib <path> -- --exact`, checking for a real
`N passed` line with a non-zero `filtered out` count. I also independently ran `cargo test
--workspace` once, cold, and confirmed `576 passed / 0 failed` (devflow-core lib), `303 passed / 0
failed` (devflow bin unit tests) and every integration-test binary green — matching the
orchestrator's pre-verification claim rather than assuming it. I cross-checked all 8 code-review
findings (CR-01, WR-01..WR-07) against the current source, not just the commit hashes the
orchestrator supplied, and confirmed each fix is actually present and correct in the code, not just
that a commit with a matching name exists.

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria 1–6)

| # | Truth (ROADMAP criterion) | Status | Evidence |
|---|---|---|---|
| 1 | A transient `git` failure no longer overwrites the persisted `consecutive_failures` baseline with a false zero; "could not count" is distinguished from "counted zero"; doc comments corrected (999.77) | ✓ VERIFIED | `phase_commit_count` returns `Option<u32>` (`agent_result.rs:1881`). `handle_validate_outcome`'s `None` arm increments the streak but never touches `state.last_validate_failure_commit_count` (`pipeline_outcomes.rs:617-632`). Doc comments corrected (`rg 'the gate stays reachable'` → 0 matches). Behavioral test `pipeline_outcomes::tests::validate_failure_with_unmeasurable_count_accumulates_the_streak` re-run by me: `1 passed; 302 filtered out`. |
| 2 | An unattended loop committing trivial artifacts every cycle still reaches a bound distinguishable from streak resets; Supervise gate message leads with the cumulative total (999.78) | ✓ VERIFIED | `State::phase_validate_failures` (`state.rs:150`) is never reset by `consecutive_failures_made_progress`; `MAX_PHASE_VALIDATE_FAILURES=10 > MAX_CONSECUTIVE_FAILURES` enforced by a `const _: () = assert!(...)` (`mode.rs:49`); `phase_failure_ceiling_reached` is the single named predicate (`mode.rs:206`), used identically by the gate message and the reset. Message construction at `pipeline_outcomes.rs:690-694` leads with `state.phase_validate_failures`. Re-ran `phase_validate_failure_ceiling_gates_despite_trivial_commit_progress`, `validate_gate_message_leads_with_the_per_phase_total`, `ceiling_clause_appears_only_at_the_ceiling_even_in_supervise_mode`, `phase_validate_failures_survive_a_forced_restart`, `phase_validate_failures_reset_when_the_phase_completes`, `phase_validate_failures_reset_on_operator_approval_at_the_ceiling_gate`, `phase_validate_failures_increment_saturates` — all pass. |
| 3 | `devflow start --phase N --force` against a phase with a stale `{N}-VERIFICATION.md` no longer inherits that verdict — staleness detected, mid-arc dispatches full execute (999.79) | ✓ VERIFIED | `State::last_verification_fingerprint` captured after the evidence root resolves (`commands.rs:389-390`, sited after the worktree fork). `select_loop_back_fix` compares content fingerprint + mtime via `verification_authored_this_run` (`pipeline_outcomes.rs:335-388, 445-469`). Re-ran `stale_verification_artifact_dispatches_full_execute` and `verification_written_this_run_dispatches_gaps_only` in the same suite run — both directions pass, so the rule is not always-stale or always-fresh. **Stated limitation, carried forward correctly rather than hidden**: the rule keys on content/mtime change, not run-owned provenance — an artifact changed by a worktree merge-back would still misread as authored-this-run. This is explicitly filed as its own phase, **Phase 35.2 "Verification Provenance" (999.89/HARDEN-03)**, not left as a silent gap in Phase 35. |
| 4 | The worktree-mode `GateReview` checkpoint auto-decide call site is covered by a regression test that fails when reverted to `project_root`, revert actually performed (999.84) | ✓ VERIFIED | `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)` (`pipeline_launch.rs:1068`) feeds `phase_has_blocking_human_checkpoint` at `:1070`. Test `advance_with_worktree_declared_checkpoint_reads_the_execution_root` re-run by me: `1 passed; 302 filtered out`. SUMMARY carries the verbatim revert-and-fail output (`0 passed; 1 failed`, zero `checkpoint_auto_decided` events, generic gate written) and the verbatim restore-and-pass output, plus a localisation control (the no-worktree sibling stays green under the same revert). |
| 5 | `release --check`'s signing preflight reports from a real `ssh-keygen -Y sign` probe, not `ssh-add -l` (999.86) | ✓ VERIFIED | `classify_ssh_add_status` / `SigningStatus` / `inline_key_fingerprint` fully removed (`rg` finds zero matches workspace-wide). `check_ssh_signing_viability` (`git.rs:1085`) now spawns `ssh-keygen` with `SSH_ASKPASS_REQUIRE=never`, a 10s wall-clock ceiling, and a per-call unique non-recursive workspace (`0o700`, `Drop`-guarded per WR-07 fix). Re-ran `cargo test -p devflow-core --lib git::` myself: `47 passed; 0 failed`, including `ssh_signing_probe_reports_viable_with_on_disk_private_key`, `ssh_signing_probe_reports_not_viable_without_a_private_key`, `encrypted_key_blocks_without_the_askpass_require_env_var`, `the_signing_probe_is_not_captured_by_a_controlling_terminal`. **35-E2E.md's live evidence corroborates from a second direction**: a real repo reports `viable` in 0.05s and a control repo with a non-key reports `NotViable` naming the signing operation specifically — not transcribed, independently legible in the artifact. |
| 6 | A transient `git` failure no longer causes `evaluate_layer2` to classify a successful agent as `Failed`; `None` distinguished from measured-zero at **both** consumers; Layer 2 returns `Ok(None)` (999.87) | ✓ VERIFIED | `evaluate_layer2`'s guard (`agent_result.rs:1994`) is `commit_gated && exit_code == 0 && commits.is_none()` — the CR-01-corrected narrow form, not a blanket early return (I confirmed this personally; a broad guard would have discarded `ResourceKilled`/`AgentUnavailable`/non-commit-gated `Success` classifications, which the review caught as the phase's one Critical). `evaluate_layer3` re-pointed at the shared `phase_commit_count`, classifying unmeasurable as `Unknown` not `Failed` (`agent_result.rs:2093-2124`). Re-ran `evaluate_layer3_unmeasurable_count_is_unknown_not_failed` (devflow-core), `evaluate_layer2_unrunnable_git_falls_through_to_layer3` and `evaluate_agent_result_with_unrunnable_git_does_not_report_failed` (devflow-cli, where these tests actually live per the F-2 harness-relocation deviation) — all pass. **35-E2E.md's live before/after on real binaries corroborates the CR-01 fix specifically**: pre-fix `73311d0` misclassified exit-137-with-unrunnable-git as `unknown`/layer-3; post-fix `d9c4349` correctly reports `resource_killed`/layer-2, while the unrelated exit-0 case is unchanged — the asymmetry the fix requires. |

**Score:** 6/6 truths verified. Every criterion has both a direct code inspection and at least one test I personally re-ran to a real pass, not transcribed from the SUMMARY.

### Code-Review Findings (35-REVIEW.md) — Independently Re-Verified, Not Trusted from Commit Hashes

| Finding | Claimed fix commit | Verified in current source? |
|---|---|---|
| CR-01 (Critical — infra faults routed into Validate loop) | `d0ea0ac` (docs) / `cf462ec` (fix) | ✓ Confirmed: guard narrowed to `commit_gated && exit_code == 0 && commits.is_none()` at `agent_result.rs:1994` |
| WR-01 (probe timeout rendered as hard `NotViable`) | `a8a92c5` | ✓ Confirmed: `SignProbeOutcome::TimedOut → SigningViability::Unknown` (`git.rs:1147-1152`) |
| WR-02 (unreadable state silently zeroes budget) | `b8aacdc` | ✓ Confirmed: `commands.rs:180-185` discriminates `Ok`, `MissingState`, and other `WorkflowError` variants |
| WR-03 (ceiling reset logs `0` for the event that should record the spend) | `5bfd5a4` (part of `9773cc8` group) | ✓ Confirmed: `reset_phase_failures_at_ceiling` captures `spent` before zeroing and emits it in both the event and the println (`pipeline_outcomes.rs:776-796`) |
| WR-04 (passing Validate at ceiling gates silently) | `9773cc8` | ✓ Confirmed: ceiling clause present in the `ValidateResult::Passed` arm (`pipeline_outcomes.rs:671-681`) |
| WR-05 (upgraded-binary baseline-absent case unsignalled) | `698e046` | ✓ Confirmed: `baseline_captured` parameter in `verification_authored_this_run` and `verification_baseline_absent` event emission (`pipeline_outcomes.rs:344-355, 463-466`) |
| WR-06 (idempotent rewrite misreads as inherited) | `253cbf4` | ✓ Confirmed: `phase_verification_mtime_nanos` + `written_since_baseline` comparison added alongside content hash (`pipeline_outcomes.rs:338, 452-460`) |
| WR-07 (probe workspace not private/panic-safe) | `f8dac07` | ✓ Confirmed: `DirBuilderExt::mode(0o700)` + `impl Drop for ProbeWorkspace` (`git.rs:914, 959-961`) |

All 8 findings are genuinely resolved in the code as it stands today, not merely associated with a commit that exists.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/agent_result.rs` | `phase_commit_count -> Option<u32>`, Layer 2/3 honoring the distinction | ✓ VERIFIED | Confirmed signature, both call sites, CR-01-corrected guard placement |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | Diverging Some/None arms, per-phase ceiling, freshness selector | ✓ VERIFIED | All three mechanisms present and wired, tests re-run directly |
| `crates/devflow-cli/src/pipeline_launch.rs` | Worktree-mode `GateReview` regression test | ✓ VERIFIED | Test exists, passes, revert-demonstration recorded |
| `crates/devflow-core/src/git.rs` | Real `ssh-keygen -Y sign` probe, predictor removed | ✓ VERIFIED | Predictor gone workspace-wide, probe present with all WR-07 hardening |
| `crates/devflow-core/src/state.rs` | `phase_validate_failures`, `last_verification_fingerprint` | ✓ VERIFIED | Both fields present, `#[serde(default)]`, round-trip tests pass |
| `crates/devflow-core/src/mode.rs` | `MAX_PHASE_VALIDATE_FAILURES`, `phase_failure_ceiling_reached`, widened `should_gate` | ✓ VERIFIED | All present; compile-time assertion pins the ceiling ordering |
| `CHANGELOG.md` | 2.5.0 entry enumerating the public-API break | ✓ VERIFIED | All named symbols present; Known Issues section corrected post-UAT (999.88 no longer misreported) |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `agent_result.rs::phase_commit_count` | `pipeline_outcomes.rs::handle_validate_outcome` | `Option<u32>` match forces baseline-write divergence | ✓ WIRED |
| `agent_result.rs::phase_commit_count` | `agent_result.rs::evaluate_layer2` | narrow CR-01-corrected guard | ✓ WIRED |
| `agent_result.rs::phase_commit_count` | `agent_result.rs::evaluate_layer3` | re-pointed, inline copy deleted | ✓ WIRED |
| `pipeline_launch.rs` execution_root binding | `verify::phase_has_blocking_human_checkpoint` | `:1068` → `:1070` | ✓ WIRED, regression-tested |
| `commands.rs::start()` | `agent_result::phase_verification_fingerprint` | captured post-worktree-fork | ✓ WIRED |
| `pipeline_outcomes.rs::select_loop_back_fix` | `agent_result::phase_verification_fingerprint` + mtime | freshness predicate | ✓ WIRED |
| `pipeline_outcomes.rs` | `mode::Mode::should_gate` (widened) | both numbers passed at every call site | ✓ WIRED, compiler-enumerated |
| `CHANGELOG.md` | `git.rs` removal notes | `999.86` cross-reference | ✓ WIRED |

### Behavioral Spot-Checks (independently re-run, not transcribed)

| Behavior | Command | Result |
|---|---|---|
| Full workspace suite | `cargo test --workspace` | `576 passed` (devflow-core lib) + `303 passed` (devflow bin) + all integration binaries green, `0 failed` overall |
| Criterion 1 test | `cargo test -p devflow --bin devflow pipeline_outcomes::tests::validate_failure_with_unmeasurable_count_accumulates_the_streak --exact` | `1 passed` |
| Criterion 2 tests (7) | individually named, `--exact` | all `1 passed` |
| Criterion 3 tests | `stale_verification_artifact_dispatches_full_execute`, `verification_written_this_run_dispatches_gaps_only` | both `1 passed` in the same suite run |
| Criterion 4 test | `advance_with_worktree_declared_checkpoint_reads_the_execution_root` | `1 passed` |
| Criterion 5 tests | `cargo test -p devflow-core --lib git::` | `47 passed; 0 failed` |
| Criterion 6 tests (3) | layer2, layer3, cascade — individually named | all `1 passed` |
| Debt-marker scan | `git diff 749a151..HEAD` grepped for `TBD\|FIXME\|XXX\|TODO\|HACK\|PLACEHOLDER` | zero real hits (one false-positive match is descriptive prose about "a placeholder" in a comment about `--dry-run`, not a stub) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| HARDEN-01 | 35-01, 35-06 | `consecutive_failures` baseline not silently overwritten by a measurement failure | ✓ SATISFIED | Criterion 1 above |
| HARDEN-02 | 35-04 | Loop bound independent of trivial per-cycle commits; cumulative Supervise message | ✓ SATISFIED | Criterion 2 above |
| HARDEN-03 | 35-05 | `--force` re-run does not inherit stale `VERIFICATION.md` | ✓ SATISFIED (with the residual provenance gap explicitly filed as Phase 35.2, not silently accepted) | Criterion 3 above |
| HARDEN-04 | 35-02 | Worktree-mode `GateReview` checkpoint regression-tested | ✓ SATISFIED | Criterion 4 above |
| HARDEN-05 | 35-03, 35-06 | Real signing probe replaces the false-negative-prone predictor | ✓ SATISFIED | Criterion 5 above |
| HARDEN-07 | 35-01 | Transient `git` failure doesn't misclassify a successful agent as `Failed` | ✓ SATISFIED | Criterion 6 above |

**Every requirement ID declared in the six plans' frontmatter (HARDEN-01, 02, 03, 04, 05, 07) is present in `.planning/REQUIREMENTS.md`'s v1 list and mapped to Phase 35 in its Traceability table — no orphaned or unmapped requirement.**

### Anti-Patterns Found

None. No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers, no empty implementations, no hardcoded-empty stub returns were introduced by this phase's diff (`749a151..HEAD` across the 10 modified source files, checked individually).

### Gaps Summary

No functional gaps. All six ROADMAP success criteria are genuinely implemented, wired, and covered
by regression tests that I re-ran myself rather than trusting the SUMMARY transcripts. All 8
code-review findings (1 Critical, 7 Warnings) are verifiably resolved in the current source, not
merely associated with a commit hash. The one UAT-caught documentation drift (CHANGELOG.md
misreporting resolved backlog item 999.88 as open) was already corrected before this verification
ran (gap G-35-11, resolved 2026-08-07) — confirmed directly against the current `CHANGELOG.md`
text, not taken on the UAT report's word.

**One documentation-only item worth the operator's attention, not rising to a phase-goal gap:**
`.planning/REQUIREMENTS.md`'s Traceability table still lists HARDEN-01, 02, 03, 04, 07 as `Pending`
— only HARDEN-05 was marked `Complete` (commit `6160dee`, scoped to plan 35-03 alone). Every
requirement ID is present and correctly mapped to Phase 35 (satisfying the "every ID must be
accounted for" check), so this does not affect functional correctness or the phase goal — it is a
bookkeeping omission in a document GSD's own tooling reads for progress reporting, not a code gap.
Recommend a follow-up edit marking HARDEN-01/02/03/04/07 `Complete` alongside HARDEN-05, since the
underlying work for all six is verified above.

---

_Verified: 2026-08-07_
_Verifier: Claude (gsd-verifier)_
