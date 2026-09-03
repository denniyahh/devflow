---
phase: "45"
slug: "unattended-auto-mode-hardening-999-110-999-109-999-94"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: false
wave_0_complete: true
created: "2026-09-03"
---

# Phase 45 — Validation Strategy

> Reconstructed by `/gsd-validate-phase 45` on 2026-09-03 (State B — no VALIDATION.md
> was seeded during execution). Built from the 3 shipped plans (45-01/02/03), their
> SUMMARYs, and `45-VERIFICATION.md`. All `<automated>` verify commands below were
> re-run green against the post-`sync-workspace.sh` tree.
>
> **`nyquist_compliant: false`** — not a test-coverage failure. AUTO-01 and AUTO-02 are
> fully covered and green. DECN-01 is **partially delivered by recorded operator
> decision**: `CODE_STAGE_POLICY` reaches `code_stage_prompt` + `workflow_code_prompt`
> FullExecute but not the `fix_prompt` Claude/OpenCode loop-back (backlog **999.115**),
> and is contradicted by `checkpoint_auto_decide_prompt` for blocking-human gates
> (backlog **999.116**). Those are deferred implementation gaps, not missing tests —
> no auditor spawn applies.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (no external framework) |
| **Config file** | none — workspace `Cargo.toml` at repo root |
| **Quick run command** | `cargo test -p devflow-core --lib <filter>` / `cargo test -p devflow --bin devflow <filter>` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~90 seconds (workspace baseline; 1235 tests) |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` filter; assert `test result: ok` with non-zero `filtered out`
- **After every plan wave:** `cargo test --workspace` (assert `cargo_exit=0`, zero `^test result: FAILED`)
- **Before `/gsd-verify-work`:** Full suite green + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check` clean
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task / Plan | Requirement | Behavior | Test Type | Automated Command | Status |
|-------------|-------------|----------|-----------|-------------------|--------|
| 45-01 | AUTO-01 | `config::base_branch` resolver (env > file > `develop`), fallible, provenance-carrying | unit | `cargo test -p devflow-core --lib config::tests::base_branch` | ✅ 7 passed / 757 filtered |
| 45-01 | AUTO-01 | base-branch validator; explicitly-configured value fails hard | unit | `cargo test -p devflow-core --lib config::tests::validate_base_branch` + `... base_branch_errors_on_an_explicitly_configured` | ✅ 2 + 2 passed |
| 45-01 | AUTO-01 | `GitFlow::with_config` / `for_project` uses supplied base, not default | unit | `cargo test -p devflow-core --lib config::tests::git_flow_for_project` + `... with_config_uses_the_supplied_develop_not_the_default` | ✅ 1 + 1 passed |
| 45-01 | AUTO-01 | worktree forks from the resolved base and CARRIES `.planning/config.json` (in-body negative control: `develop` fork does NOT) | unit | `cargo test -p devflow --bin devflow ensure_phase_worktree_forks_from_the_supplied_base` | ✅ 1 passed / 362 filtered |
| 45-01 | AUTO-01 | `--no-worktree` forks the feature branch from the configured base (F3) | unit | `cargo test -p devflow --bin devflow no_worktree_start_forks_the_feature_branch_from_the_configured_base` | ✅ 1 passed / 362 filtered |
| 45-01 | AUTO-01 | commit-ish that is not a local branch is rejected | unit | `cargo test -p devflow --bin devflow ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch` | ✅ 1 passed / 362 filtered |
| 45-01 | AUTO-01 | base-ref currency fails open for a local-only planning branch; undeterminable warning names the branch | unit | `cargo test -p devflow --bin devflow base_ref_currency_is_undeterminable_when_the_remote_ref_is_absent` + `ensure_base_ref_current_fails_open_for_a_local_only_planning_branch` + `undeterminable_currency_warning_names_the_branch_and_its_disposition` | ✅ 1 + 1 + 1 passed |
| 45-01 | AUTO-01 | phase-artifact probe reads the supplied base, not the default trunk | unit | `cargo test -p devflow --bin devflow phase_artifact_probe_reads_the_supplied_base_not_the_default_trunk` | ✅ 1 passed / 362 filtered |
| 45-01 | AUTO-01 | run-scoped git-flow: merge target, HookContext retention, commit enumeration all use the persisted base | unit | `cargo test -p devflow-core --lib merge_feature_targets_the_configured_base_not_the_default` + `hook_context_git_flow_is_not_discarded` + `enumerate_phase_commits_ranges_from_the_configured_base` + `enumerate_phase_commits_resolves_config_from_the_project_root` | ✅ 1 + 1 + 1 + 1 passed |
| 45-01 | AUTO-01 | `DEVFLOW_BASE_BRANCH` documented (doc_check source scan sees it) | unit | `cargo test -p devflow-core --lib source_devflow_env_vars_and_subcommands_are_documented` | ✅ 1 passed / 763 filtered |
| 45-02 | AUTO-02 | `affects_compiled_binary` scoped to `crates/*` + root build files; UNSCOPED rule unchanged (zero-regression control) | unit | `cargo test -p devflow --bin devflow affects_compiled_binary` | ✅ 3 passed / 360 filtered |
| 45-02 | AUTO-02 | spikes-only dirty tree does NOT block self-dogfood; in-body control: a modified `crates/devflow-cli/src/main.rs` still `Err` | unit | `cargo test -p devflow --bin devflow spikes_only_dirty_tree_does_not_block_self_dogfood` | ✅ 1 passed / 362 filtered |
| 45-02 | AUTO-02 | non-dogfood project keeps the broad build-input rule; mixed docs+source range is stale | unit | `cargo test -p devflow --bin devflow non_dogfood_project_keeps_the_broad_build_input_rule` + `mixed_range_docs_and_source_is_stale` + `staleness::` | ✅ 1 + 1 + 1 passed |
| 45-02 | AUTO-02 | porcelain path normalization (status bytes / renames / quotes) | unit | `cargo test -p devflow --bin devflow porcelain_tracked_path_normalizes_status_bytes_renames_and_quotes` | ✅ 1 passed / 362 filtered |
| 45-03 | DECN-01 (delivered) | `CODE_STAGE_POLICY` identical across both renderers; absent from prompts that must not carry it; completion protocol preserved | unit | `cargo test -p devflow-core --lib code_policy_is_identical_across_both_renderers` + `code_policy_is_absent_from_prompts_that_must_not_carry_it` + `both_code_prompts_still_end_with_the_completion_protocol` | ✅ 1 + 1 + 1 passed |
| 45-03 | DECN-01 (delivered) | policy forbids positional option selection; requires recorded reasoning; excludes blocking-human/package checkpoints; `code_stage_prompt` template unchanged | unit | `cargo test -p devflow-core --lib code_policy_forbids_positional_option_selection` + `code_policy_requires_the_reasoning_to_be_recorded` + `code_policy_excludes_blocking_human_and_package_checkpoints` + `code_stage_prompt_is_unchanged_single_command_template` | ✅ 1 + 1 + 1 + 1 passed |
| 45-03 | DECN-01 (deferred) | policy reaches the `fix_prompt` Claude/OpenCode loop-back | — | none — feature deferred to backlog **999.115** by operator override (45-VERIFICATION.md) | ⚠️ deferred |
| 45-03 | DECN-01 (deferred) | policy not contradicted by `checkpoint_auto_decide_prompt` for blocking-human gates | — | none — feature deferred to backlog **999.116** by operator override | ⚠️ deferred |
| — | Phase SC | no regression; lint/format clean | full suite | `cargo test --workspace` (1235 passed / 0 failed, exit 0, 2026-09-03) + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check` | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ deferred (tracked backlog) · flaky*

---

## Wave 0 Requirements

*Existing infrastructure (`cargo test`) covers all Phase 45 requirements. No framework install, no new test harness.*

- [x] `crates/devflow-core/src/config.rs` — base_branch resolver + validator tests
- [x] `crates/devflow-cli/src/parallel.rs` / `crates/devflow-cli/src/commands.rs` — worktree fork-point test with negative control
- [x] `crates/devflow-cli/src/staleness.rs` — first direct unit tests for `affects_compiled_binary` (both directions)
- [x] `crates/devflow-core/src/prompt.rs` — `CODE_STAGE_POLICY` shared/absent/pinning tests

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual / Deferred | Instructions / Tracking |
|----------|-------------|-----------------------|-------------------------|
| Live `devflow start --mode auto` end-to-end: configured-base fork → `preflight_unattended_launch_check` passes → merge targets the same base | AUTO-01 | No automated test drives `devflow start` end to end; this repo has no committed `base_branch`, so the "out of the box" live run needs a dedicated setup step not reachable here | **Deferred to backlog 999.119** (operator decision 2026-09-02, `/gsd-verify-work 45`; accepted as PASSED (override) on 45-VERIFICATION.md). Live run NOT performed. |
| `CODE_STAGE_POLICY` delivered on the Claude/OpenCode `fix_prompt` loop-back path | DECN-01 | Feature not wired; the Claude/OpenCode post-Validate-failure prompt renders `/gsd-execute-phase {phase}` without the policy | **Deferred to backlog 999.115** (operator decision 2026-09-02, 45-REVIEW.md § Disposition). |
| `checkpoint_auto_decide_prompt` does not grant authority `CODE_STAGE_POLICY` withholds over blocking-human gates | DECN-01 | The resume prompt contradicts the standing policy in the same conversation | **Deferred to backlog 999.116** (operator decision 2026-09-02). |

---

## Validation Sign-Off

- [x] All non-deferred tasks have `<automated>` verify or Wave 0 dependencies
- [x] Every verify filter names unique new-work tests; all report non-zero `filtered out` (no `--exact`-matches-nothing false green)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all dependencies (existing infra)
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [ ] `nyquist_compliant: true` — **NOT set.** AUTO-01 + AUTO-02 fully covered; DECN-01 partially delivered (999.115 / 999.116) plus AUTO-01's live e2e deferred (999.119), all by recorded operator decision.

**Approval:** validated 2026-09-03 (`/gsd-validate-phase 45`) — AUTO-01 / AUTO-02 Nyquist-compliant; DECN-01 partial, remainder tracked as backlog.

---

## Validation Audit 2026-09-03

| Metric | Count |
|--------|-------|
| Gaps found | 3 |
| Resolved | 0 |
| Escalated | 0 |
| Deferred (already-filed backlog) | 3 |

State B reconstruction. No VALIDATION.md existed. AUTO-01 (17 named tests incl. a real
fork-point negative control) and AUTO-02 (6 named tests incl. a zero-regression control)
are fully covered and green against the post-sync tree; every filter reported non-zero
`filtered out`. DECN-01's delivered portion (8 named policy tests) is green; its two holes
(999.115 `fix_prompt` loop-back, 999.116 `checkpoint_auto_decide_prompt` contradiction)
and AUTO-01's live `--mode auto` run (999.119) are deferred implementation/environment
gaps already filed as backlog by recorded operator decision — no nyquist-auditor spawn
applies (nothing to test-generate; the features are intentionally not built this phase).
`nyquist_compliant: false` reflects that DECN-01 is not fully delivered, not a missing
test.
