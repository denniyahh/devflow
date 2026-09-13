---
phase: "47"
slug: "unattended-decision-policy-consistency"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-10"
---

# Phase 47 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
>
> **Not to be confused with `47-PHASE49-OBSERVATION.md`.** That file carries D-09/D-10's
> *behavioural* evidence standard for Phase 49's live run — what would count as the agent having
> followed or ignored the gate rule. This file is the Nyquist test-coverage map for executing
> Phase 47 itself. D-09 originally named `47-VALIDATION.md` for the memo; it was renamed so this
> path keeps its GSD-conventional meaning and `/gsd-validate-phase` finds a test map here.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | built-in `cargo test`, plus `insta` 1.48.0 (`Cargo.lock`; `insta = "1"` in the workspace manifest, `insta = { workspace = true }` in both crate manifests) — installed by 47-01 |
| **Config file** | none for cargo; `scripts/check.sh` is the single definition of green (`scripts/check.sh:1-9`) |
| **Quick run command** | `cargo test -p devflow-core --lib prompt::` (31 tests at `0afbff5`) |
| **Full suite command** | `scripts/check.sh test` → `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast` (`scripts/check.sh:64`), then the worktree-guard harness; the target fails if either fails |
| **Estimated runtime** | Quick run, measured once each on this host on 2026-09-12: **2.7 s** after `touch crates/devflow-core/src/prompt.rs` (one `devflow-core` rebuild), **0.14 s** with nothing to rebuild. One sample each. Not measured: a content edit that rebuilds dependent crates, or the pinned container, where a cold build has run 2–9 min on this repo. |

**`-p devflow --bin devflow`, never `-p devflow --lib`.** `devflow` is binary-only, so `--lib` exits
non-zero with `error: no library targets found` before running a single test. `-p devflow-core --lib`
*is* valid — that crate has a lib target.

---

## Sampling Rate

- **After every task commit:** the scoped `cargo test -p <crate>` for the crate that task touched.
- **After every plan wave:** `scripts/check.sh test`.
- **Before `/gsd-verify-work`:** full suite green.
- **Phase gate:** `scripts/check.sh all` green — also what `scripts/hooks/pre-push:217` runs.
- **Max feedback latency:** about 2.7 s for the scoped quick run after a one-file rebuild (see Test Infrastructure).

---

## Per-Task Verification Map

Statuses were set by `/gsd-validate-phase 47` on 2026-09-12, at HEAD `0afbff5`. For the code rows,
the Automated Command column names the durable check that was run green there, not the plan's own
`<automated>` gate. Several of those gates checked a pre-fix tree (TDD red steps, unmodified line
ranges), so they cannot pass on finished work; the plans keep them as the execution record. The docs
rows are the exception: their plan gates only count text and were re-run as written.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 47-01-01 | 01 | 1 | DECN-02, DECN-03 | T-47-01 | Snapshot guard cannot be disabled by env | integration | `tests/ci_parity_guards.rs::check_script_pins_snapshots_against_env_override`; `tests/gitignore_coverage.rs::gitignore_ignores_snapshot_scratch_but_not_the_baseline` | ✅ `crates/devflow-cli/tests/` | ✅ green |
| 47-01-02 | 01 | 1 | DECN-02, DECN-03 | T-47-01 | Guard fails on a drifted baseline even with a caller-exported `INSTA_FORCE_UPDATE=1` | integration | snapshot drift check (Validation Audit below); `tests/check_script_run_test.rs` (3 tests, wrapper exit propagation) | ✅ `crates/devflow-cli/tests/check_script_run_test.rs` | ✅ green |
| 47-01-03 | 01 | 1 | DECN-02 | T-47-03 | New dev-dep leaves `cargo deny` / `machete` green on the REAL workspace | integration | `cargo deny check`; `cargo machete` (run by hand — no CI job or hook runs either) | ✅ `deny.toml` | ✅ green |
| 47-02-01 | 02 | 2 | DECN-02 | T-47-04 | Policy present on `FullExecute` across all six adapters; `GapsOnly`/`AuditFix` omit it | unit | `prompt::tests::code_policy_reaches_the_full_execute_fix_arm_on_every_adapter`; `prompt::tests::claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it` | ✅ `prompt.rs` | ✅ green |
| 47-02-02 | 02 | 2 | DECN-02 | T-47-07 | Shared arm helper; `workflow_code_prompt` behaviour preserved | unit | `prompt::tests::code_policy_is_absent_from_prompts_that_must_not_carry_it`; codex/pi baselines in `prompt::tests::policy_carrying_full_execute_fix_prompt_snapshots` | ✅ `prompt.rs` | ✅ green |
| 47-02-03 | 02 | 2 | DECN-02 | T-47-05 | Omission control discriminates (the unmodified-test constraint was checked at 47-02 execution time) | unit | `prompt::tests::code_policy_is_absent_from_prompts_that_must_not_carry_it` | ✅ `prompt.rs` | ✅ green |
| 47-03-01 | 03 | 3 | DECN-03 | T-47-06 | `resume_launch_shape` is production code, audit emission unmoved | unit | `pipeline_launch::tests::relaunch_checkpoint_session_emits_exactly_one_audit_event`, `…_does_not_change_stage`, `…_increments_and_persists_counter` | ✅ `pipeline_launch.rs` | ✅ green |
| 47-03-02 | 03 | 3 | DECN-03 | T-47-06 | Two-turn delivery pair asserts the rule in both turns | unit | `pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns` | ✅ `pipeline_launch.rs` | ✅ green |
| 47-03-03 | 03 | 3 | DECN-03 | T-47-09 | One definition site; carve-out scoped to `blocking-human` only (D-02) | unit | `prompt::tests::the_gate_rule_has_one_definition_site`; `prompt::tests::package_verification_prohibition_is_unconditional` | ✅ `prompt.rs` | ✅ green |
| 47-04-01 | 04 | 4 | DECN-02, DECN-03 | T-47-07 | Named baselines for the six policy-carrying prompts | unit | `prompt::tests::policy_carrying_full_execute_fix_prompt_snapshots`; `prompt::tests::claude_style_full_execute_fix_prompt_snapshot` (7 core `.snap`) | ✅ `crates/devflow-core/src/snapshots/` | ✅ green |
| 47-04-02 | 04 | 4 | DECN-03 | T-47-07 | The two delivered turns snapshot distinctly | unit | `pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns` (2 CLI `.snap`) | ✅ `crates/devflow-cli/src/snapshots/` | ✅ green |
| 47-05-01 | 05 | 4 | DECN-03 | — | `unattended-mode.md`'s false "refusal is final" claim corrected | docs | `47-05-PLAN.md` Task 1 `<automated>`, as written | ✅ `docs/guides/unattended-mode.md` | ✅ green |
| 47-05-02 | 05 | 4 | DECN-03 | — | Phase 49 observation memo + single-line ROADMAP pointer | docs | `47-05-PLAN.md` Task 2 `<automated>`, scratch paths redirected | ✅ `47-PHASE49-OBSERVATION.md` | ✅ green |
| 47-05-03 | 05 | 4 | DECN-02, DECN-03 | — | ROADMAP/REQUIREMENTS name the real adapter split; 999.125/126 filed; declined follow-on absent | docs | `47-05-PLAN.md` Task 3 `<automated>`, scratch paths redirected | ✅ `.planning/ROADMAP.md` | ✅ green |
| 47-06-01 | 06 | 1 (gaps) | DECN-02, DECN-03 | T-47-10, T-47-11 | Decision reasoning above the last-line `DEVFLOW_RESULT` parses to that result; long trailing text still hides it | unit | `agent_result::tests::decision_reasoning_above_the_result_line_parses_to_that_result` | ✅ `agent_result.rs` | ✅ green |
| 47-06-02 | 06 | 1 (gaps) | DECN-03 | T-47-14 | A real gate declaration is detected; the rendered resume prompt and resolved-gate prose are not | unit | `agent_result::tests::resume_prompt_does_not_read_as_a_blocking_human_checkpoint`; review-fix tests `agent_result::tests::blocking_human_checkpoint_reported_sees_through_list_and_quote_markup`, `agent_result::tests::literal_backslash_n_in_agent_text_is_not_a_line_break` | ✅ `agent_result.rs` | ✅ green |
| 47-06-03 | 06 | 1 (gaps) | DECN-02, DECN-03 | T-47-12, T-47-13 | Nine re-blessed baselines match rendered output; package-verification prohibition unchanged | unit | the three snapshot tests above; `prompt::tests::package_verification_prohibition_is_unconditional` | ✅ 9 tracked `.snap` | ✅ green |
| 47-07-01 | 07 | 1 (gaps) | WR-01 | T-47-15, T-47-17, T-47-18 | Fixture bootstrap isolated from inherited hooks, signing and injected git config | integration | `scripts/test-phase-worktree-guard.sh` (`passed=13 failed=0`); `tests/ci_parity_guards.rs::worktree_guard_harness_queries_the_checkout_before_nulling_global_config` | ✅ `scripts/test-phase-worktree-guard.sh` | ✅ green |
| 47-07-02 | 07 | 1 (gaps) | WR-01 | T-47-15, T-47-16 | Host-independent regression case 7c, with armed controls 7a/7b | integration | `scripts/test-phase-worktree-guard.sh` rows 7a, 7b, 7c; `tests/check_script_run_test.rs` (harness runs even when cargo fails) | ✅ `scripts/test-phase-worktree-guard.sh` | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Sampling continuity:** no three consecutive tasks lack an automated verify — all 19 have one, and
each was run green at `0afbff5`.

---

## Wave 0 Requirements

Wave 0 is plan **47-01** in its entirety, which is why it is its own plan at wave 1 rather than
folded into the fix work (operator decision, 2026-09-10). Baselines laid behind an unproven guard
would be worthless, so the guard must be demonstrably failing-then-fixed first.

- [x] `insta` in the workspace manifest and in both crate manifests (`Cargo.toml:41`, `crates/devflow-core/Cargo.toml:35`, `crates/devflow-cli/Cargo.toml:29`)
- [x] `run_test` hardened to `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no …` (`scripts/check.sh:64`; behaviour re-checked by the drift check below)
- [x] `*.snap.new` ignored in `.gitignore`, with `*.snap` explicitly kept tracked (`gitignore_coverage` test green)
- [x] Initial `.snap` baselines created via `INSTA_UPDATE=always` and **read before committing** (9 tracked; wording confirmed by the operator in `47-UAT.md` tests 14 and 18)
- [x] `resume_launch_shape` extracted (RESEARCH § B-0 step 1) — 47-03 Task 1; the three `relaunch_checkpoint_session_*` tests are green

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Whether a model actually *follows* the carve-out when both rules are present | DECN-03 | Criterion 4 is explicitly **not closed by this phase** — it is a question about model instruction-priority, not answerable by reading or testing source | Phase 49's live `devflow start --mode auto` run, judged against the evidence standard in `47-PHASE49-OBSERVATION.md` (D-10). Claude adapter only — the resume route is gated on `AgentKind::Claude` (`pipeline_launch.rs:1569`), so the other three claude-style adapters cannot reach it (D-11) |
| That the `.snap` diff was READ before a baseline was blessed | DECN-02 | No machine can distinguish a read diff from an unread one; the automated gates can only prove a baseline changed, not that a human looked | 47-02 Task 2 requires the SUMMARY to carry the pre-re-bless failing output and an explicit statement that the diff was read (D-15). The operator re-read the committed baselines in `47-UAT.md` tests 14, 18, 22 and 23 — all pass, 2026-09-12 |
| Guide, memo and planning-record wording is accurate and usable | DECN-02, DECN-03 | The 47-05 gates count headings, labels and pointers; they cannot judge whether the prose is right | `47-UAT.md` tests 6, 19, 20 and 21 — all pass, 2026-09-12 |
| A resumed agent copies the gate-declaration line despite the instruction | DECN-03 | Accepted risk T-47-19: a prompt cannot enforce what a model writes, and changing the detector is out of scope (operator decision A1) | Phase 49's live run observes it. Worst case is up to three extra resumes before the ceiling routes to the review gate |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies — 19/19 (14 original, plus 47-06 ×3 and 47-07 ×2)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency measured — 2.7 s scoped quick run after a one-file rebuild, 0.14 s with nothing to rebuild (one sample each; see Test Infrastructure)
- [x] `nyquist_compliant: true` set in frontmatter — `/gsd-validate-phase 47`, 2026-09-12

**Approval:** validated 2026-09-12 (`/gsd-validate-phase 47`, existing-map audit, 0 gaps)

---

## Validation Audit 2026-09-12

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

Audited at HEAD `0afbff5`. No code, script or dependency manifest has changed since `49bb324`.

**Run green here:**

- **21 named tests**, all under `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`, with no `.snap.new` written:
  - 11 in the `devflow-core` lib (763 filtered out)
  - 4 in the `devflow` bin (360 filtered out)
  - 3 in `tests/check_script_run_test.rs`
  - 2 in `tests/ci_parity_guards.rs` (25 filtered out)
  - 1 in `tests/gitignore_coverage.rs` (1 filtered out)
- **`scripts/test-phase-worktree-guard.sh`:** `passed=13 failed=0`, with rows 7a, 7b and 7c ok.
- **The three 47-05 `<automated>` gates**, as written except for their `/tmp/47-05-*` scratch paths: `gate_rc=0` each.
  - Control: each gate was re-run in a throwaway repo against a broken copy (heading re-added, `%` added, `hermes` removed). Each failed with the matching counter.
- **Snapshot drift (T-47-01):** `claude_style_full_execute_fix_prompt_snapshot.snap` was drifted by one line. The hardened invocation, with the caller exporting `INSTA_FORCE_UPDATE=1`, exited 101 and left the file untouched.
  - Control: the same run without `env -u INSTA_FORCE_UPDATE` exited 0 and rewrote the file.
  - The baseline was restored to its HEAD blob afterwards.
- **`cargo deny check`:** advisories, bans, licenses and sources all ok. There was no control for this one.
- **`cargo machete`:** no unused dependencies.
  - Control: machete flagged `serde` in a throwaway crate that declares it without using it.

**Not established here:**

- **The full workspace suite was not re-run.** The previous session recorded `scripts/check.sh test` green on `49bb324`, and no code has changed since.
- **The drift check ran the pinned invocation on one test, not the whole `check.sh test` wrapper.** `tests/check_script_run_test.rs` covers the wrapper's exit propagation, using a stub cargo.
- **The discrimination demonstrations recorded during execution were not repeated:** the 47-02 helper widening, the 47-03 prohibition widening, and the 47-07 two-line isolation mutant.
- **47-01-03 holds today but has no regression guard.** Nothing in CI or the git hooks runs `cargo deny` or `cargo machete`.
- **Model behaviour** is not tested here: whether a live agent writes its reasoning above the result line, or avoids copying the gate line, is Phase 49's to observe.
