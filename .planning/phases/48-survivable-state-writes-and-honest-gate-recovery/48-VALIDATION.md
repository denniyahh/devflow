---
phase: "48"
slug: "survivable-state-writes-and-honest-gate-recovery"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-14"
validated: "2026-09-22"
---

# Phase 48 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `48-RESEARCH.md` § Validation Architecture; the per-task map is filled from the PLAN.md set.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust libtest via `cargo test` |
| **Config file** | none for tests; `clippy.toml` is new in this phase (TEST-01 lint) |
| **Quick run command** | `cargo test -p devflow-core --lib <module>::tests::<name> -- --exact` or `cargo test -p devflow --bin devflow <module>::tests::<name> -- --exact` — output must contain `1 passed` and a non-zero `filtered out` (a name that is not module-qualified matches nothing and still exits 0) |
| **Full suite command** | `scripts/check.sh all` (fmt, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`) |
| **Estimated runtime** | `scripts/check-in-container.sh all`: 2m10s on 2026-09-22 with a warm per-checkout target volume (not a cold-build figure) |

---

## Sampling Rate

- **After every task commit:** the task's targeted `--exact` test(s) for the touched module, plus `cargo clippy -p <devflow-core|devflow> --all-targets -- -D warnings`
- **After every plan wave:** `scripts/check.sh all`
- **Before `/gsd-verify-work`:** full suite must be green; `scripts/check-in-container.sh`; one `taskset -c 0,1 scripts/check.sh test` run reported as a sanity check only (it does not prove the PATH flakes cannot recur)
- **Max feedback latency:** 44 s for the slowest per-task plan gate (48-16 T2); median 5 s across 36 gates. Measured 2026-09-22 on a warm host build, so it excludes first-compile time.

---

## Per-Task Verification Map

Status reflects re-running every plan `<automated>` gate on 2026-09-22 at `3824a27` (source-identical to `4514ae3`),
plus the pinned container suite (exit 0, `==> check.sh: all OK`, 1394 passed / 0 failed / 0 ignored across 33 suites).
32 of 36 runnable gates exited 0; the four that did not are gate-script defects whose target behaviour is green — see notes ¹–⁴.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 48-01-T1 | 48-01 | 1 | SURV-01, SURV-02, CHKPT-01, CHKPT-02, TEST-01 | T-48-01-01, T-48-01-02 | Roadmap names the three IDs and the 17-plan execution map; `roadmap.analyze` still parses | docs structural | 48-01-PLAN T1 gate (point-in-time) | ✅ | ✅ ¹ |
| 48-01-T2 | 48-01 | 1 | SURV-01, SURV-02, CHKPT-01, CHKPT-02, TEST-01 | T-48-01-01, T-48-01-02 | Criteria 1-4 reworded; 999.38 + 999.80 recorded as worked together | docs structural | 48-01-PLAN T2 gate | ✅ | ✅ |
| 48-02-T1 | 48-02 | 1 | CHKPT-01 | T-48-02-01..03 | One parser; a prose marker neither blocks preflight nor arms resume | unit (core) | `verify::tests::{blocking_human_checkpoint_ignores_a_marker_mentioned_only_in_prose, task_level_blocking_human_gate_is_human_only_on_both_predicates, human_only_checkpoint_ignores_a_marker_mentioned_only_in_prose}` `--exact` | ✅ | ✅ |
| 48-02-T2 | 48-02 | 1 | CHKPT-01 | T-48-02-01..03 | Closed fences ignored; unclosed fences fail closed | unit (core) | six `verify::tests::*fence*` / `task_opening_must_start_the_line` `--exact` | ✅ | ✅ |
| 48-02-T3 | 48-02 | 1 | CHKPT-01 | T-48-02-01..03 | Whole-element extraction; real-plan fixtures; empty/CRLF edges | unit (core + cli) | five `verify::tests::*` `--exact`; `cargo test -p devflow --bin devflow checkpoint` (substring filter) | ✅ | ✅ |
| 48-03-T1 | 48-03 | 1 | TEST-01 | T-48-03-01..03 | Child-process helper with its controls; first pi test converted | unit (core) | `test_support::tests::*` (3, incl. 2 `should_panic`) + `agents::pi::tests::preflight_invokes_pi_auth_check_and_accepts_ready` `--exact` | ✅ | ✅ |
| 48-03-T2 | 48-03 | 1 | TEST-01 | T-48-03-01..03 | No `PathGuard` or PATH `set_var` left in pi.rs / opencode.rs | structural | 48-03-PLAN T2 gate | ✅ | ✅ |
| 48-04-T1 | 48-04 | 2 | TEST-01 | T-48-04-01..02 | First PATH-replacing test runs through the core helper | unit (cli) + structural | 48-04-PLAN T1 gate | ✅ | ✅ |
| 48-04-T2 | 48-04 | 2 | TEST-01 | T-48-04-01..02 | Every PATH block in pipeline_outcomes.rs converted; `NeutralPath` deleted; test count vs `c0fb193` kept | structural + unit | 48-04-PLAN T2 gate | ✅ | ✅ |
| 48-05-T1 | 48-05 | 2 | TEST-01 | T-48-05-01..02 | Tracer conversion in pipeline_launch.rs | unit (cli) | `pipeline_launch::tests::advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` `--exact` | ✅ | ✅ |
| 48-05-T2 | 48-05 | 2 | TEST-01 | T-48-05-01..02 | Remaining PATH blocks and three abort-fixture tests run in children | structural + unit | 48-05-PLAN T2 gate | ✅ | ✅ |
| 48-06-T1 | 48-06 | 3 | TEST-01 | T-48-06-01..02 | staleness.rs PATH block converted (the 999.38 flake site) | unit (cli) + structural | `staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking` `--exact` + gate | ✅ | ✅ |
| 48-06-T2 | 48-06 | 3 | TEST-01 | T-48-06-01..02 | preflight.rs PATH blocks and abort-fixture tests converted | structural + unit | 48-06-PLAN T2 gate | ✅ | ✅ |
| 48-06-T3 | 48-06 | 3 | TEST-01 | T-48-06-01..02 | pipeline_gate.rs converted; uncalled helper deleted | structural + unit | 48-06-PLAN T3 gate | ✅ | ✅ |
| 48-07-T1 | 48-07 | 4 | CHKPT-02 | T-48-07-01..04 | Record at Code preflight; an added checkpoint parks at the re-scan gate | unit (core + cli) | `state::tests::checkpoint_approval_defaults_to_unrecorded_for_old_state_files`, `pipeline_launch::tests::checkpoint_added_after_code_preflight_parks_at_the_rescan_gate` `--exact` | ✅ | ✅ |
| 48-07-T2 | 48-07 | 4 | CHKPT-02 | T-48-07-01..04 | Changed body / unrecorded / pending sets park; unchanged set is the control | unit (core + cli) | `state::tests::unapproved_checkpoints_is_an_order_insensitive_multiset_difference` + four `pipeline_launch::tests::*rescan*` / `unchanged_recorded_checkpoint_still_auto_decides` `--exact` | ✅ | ✅ |
| 48-08-T1 | 48-08 | 4 | TEST-01 | T-48-08-01 | Abort-fixture helper refuses to run outside a child | unit (cli) | `pipeline_outcomes::tests::{external_verify_disagreement_gates_immediately, abort_fixture_helper_refuses_to_run_outside_a_child}` `--exact` | ✅ | ✅ |
| 48-08-T2 | 48-08 | 4 | TEST-01 | T-48-08-01 | Every abort-fixture function runs in a child | structural + unit | 48-08-PLAN T2 gate | ✅ | ✅ |
| 48-09-T1 | 48-09 | 6 | TEST-01 | T-48-09-01..03 | `clippy.toml` disallows `std::env::set_var` / `remove_var`; remaining mutations annotated | structural | 48-09-PLAN T1 gate | ✅ | ✅ |
| 48-09-T2 | 48-09 | 6 | TEST-01 | T-48-09-01..03 | Lint trips on three import spellings (negative control) | lint probe | 48-09-PLAN T2 gate — 2026-09-22: probe clippy exit 101, 3 `disallowed method` lines, `lib.rs` restored byte-identical | ✅ | ✅ |
| 48-10-T1 | 48-10 | 7 | SURV-01 | T-48-10-01..04 | Unique temp name per state write; scanners ignore it; `clear_state` sweeps it | unit (core) | three `workflow::tests::*` `--exact` | ✅ | ✅ |
| 48-10-T2 | 48-10 | 7 | SURV-01 | T-48-10-01..04 | First answer wins: a second publisher gets AlreadyResponded; single responder is the control | unit (core) | five `gates::tests::*` `--exact` | ✅ | ✅ |
| 48-10-T3 | 48-10 | 7 | SURV-01 | T-48-10-01..04 | `Gates::cleanup` sweeps only its own orphan temps | unit (core) | `gates::tests::cleanup_removes_orphan_temps_for_its_gate_only` `--exact` + core lib + clippy | ✅ | ✅ |
| 48-11-T1 | 48-11 | 7 | SURV-01 | T-48-11-01..04 | Advance waits (bounded) for a held lock; expiry logged | unit (core + cli) | `lock::tests::phase_lock_blocking_*` + `pipeline_launch::tests::advance_waits_for_a_held_lock_then_proceeds` et al. `--exact` | ✅ | ✅ |
| 48-11-T2 | 48-11 | 7 | SURV-01 | T-48-11-01..04 | Advance bound to its launched stage | unit (core + cli) | `monitor::tests::*` + `pipeline_launch::tests::advance_refuses_when_the_saved_stage_moved_on` et al. `--exact` | ✅ | ✅ |
| 48-12-T1 | 48-12 | 8 | SURV-01, SURV-02 | T-48-12-01..05 | `start` refuses under a live holder, proceeds without one, clears leftovers | integration (`start_lock_e2e`) | `cargo test -p devflow --test start_lock_e2e <name> -- --exact` (3) | ✅ | ✅ |
| 48-12-T2 | 48-12 | 8 | SURV-01, SURV-02 | T-48-12-01..05 | `stop` locks before it loads; start-time identity checked | integration (`stop_e2e`) | `cargo test -p devflow --test stop_e2e <name> -- --exact` (7) | ✅ | ✅ |
| 48-13-T1 | 48-13 | 9 | SURV-02 | T-48-13-01..03 | Read-only NoHolder and Live classifications | unit (core) | two `lock::tests::holder_status_*` `--exact` | ✅ | ✅ |
| 48-13-T2 | 48-13 | 9 | SURV-02 | T-48-13-01..03 | Recycled and unconfirmable identities classified distinctly and safely | unit (core) | two `lock::tests::holder_status_*` `--exact` + module | ✅ | ✅ |
| 48-14-T1 | 48-14 | 11 | SURV-02 | T-48-14-01..03 | `recover --clean` clears a lock-free wedge; returns without cleanup when contended or recycled | unit (core) + integration | three `recover::tests::*` `--exact`; `cargo test -p devflow --test gate_wedge_e2e` → **`3 passed; 0 failed`** | ✅ | ✅ ² |
| 48-14-T2 | 48-14 | 11 | SURV-02 | T-48-14-01..03 | `doctor` reports an open gate with no waiter using the verbs' repair line | unit (cli) | four `commands::tests::reconcile_phase_*` | ✅ | ✅ |
| 48-14-T3 | 48-14 | 11 | SURV-01, SURV-02, CHKPT-01, CHKPT-02, TEST-01 | T-48-14-01..03 | Container parity | container full suite | `scripts/check-in-container.sh all` — 2026-09-22 exit 0, `==> check.sh: all OK` | ✅ | ✅ |
| 48-15-T1 | 48-15 | 5 | CHKPT-02 | T-48-15-01..03 | Approving the re-scan gate records the set and relaunches; rejecting records nothing | unit (cli) | three `pipeline_launch::tests::*rescan_gate*` `--exact` | ✅ | ✅ |
| 48-15-T2 | 48-15 | 5 | CHKPT-02 | T-48-15-01..03 | Human-approved preflight refusal records from the execution root only | unit (cli) | five `preflight::tests::*` `--exact` | ✅ | ✅ |
| 48-16-T1 | 48-16 | 10 | SURV-02 | T-48-16-01..03 | `gate reject` distinguishes the #200 wedge from a live waiter | unit (cli) + integration (`gate_wedge_e2e`) | `commands::tests::gate_response_message_does_not_claim_pickup_after_live_becomes_no_holder` + wedge / self-resolving arms `--exact` (observed pickup 989 ms) | ✅ | ✅ ³ |
| 48-16-T2 | 48-16 | 10 | SURV-02 | T-48-16-01..03 | `stop` and `sweep` share the status matrix and preserve live-gate contracts | unit (cli) | seven `commands::tests::*` `--exact`, the seventh being **`stop_at_a_non_ship_gate_with_an_unconfirmable_holder_refuses_success`** | ✅ | ✅ ⁴ |
| 48-17-T1 | 48-17 | 7 | TEST-01 | T-48-17-01..02 | PATH child-process rule published in TESTING.md; `NeutralPath` gone | docs structural + unit | 48-17-PLAN T1 gate | ✅ | ✅ |
| 48-17-T2 | 48-17 | 7 | TEST-01 | T-48-17-01..02 | Full suite plus 2-CPU pinned sanity run | full suite | 48-17-PLAN T2 gate — not re-run 2026-09-22; the container suite (pinned to CPUs 0,1) stands in for its pinned half, the host unpinned half was not repeated | ✅ | ✅ |
| N-48-01 | Nyquist | — | SURV-02 (criterion 3) | — | `resume` relaunches the saved stage and never reads a pending answer; the preflight-refusal path is the opposite-result control | unit (cli) | `cargo test -p devflow --bin devflow pipeline_launch::tests::resume_relaunches_without_consuming_a_pending_gate_answer -- --exact`; control `pipeline_launch::tests::resume_preflight_refusal_consumes_a_pending_gate_answer` | ✅ | ✅ |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Gate-script defects (behaviour green, literal plan gate exits non-zero):**

1. **48-01-T1** — `rg --count-matches` prints nothing (not `0`) for zero matches, so `[ "$c" -eq 0 ]` errors exactly when a
   prohibited string is correctly absent; already recorded in `48-01-SUMMARY.md`. Its remaining assertions pin roadmap and
   requirements wording at 48-01 time that later plans changed deliberately (for example CHKPT-01 is now `[x]`). Not re-runnable as a
   regression check; its requirements are carried by the behavioural rows above.
2. **48-14-T1** — the gate greps for exactly `2 passed; 0 failed` from `gate_wedge_e2e`; `ea0ac7e` added a third test
   (`dry_run_sweep_reports_no_waiter_gates_as_left_alone`). The suite reports `3 passed; 0 failed` in the container run.
3. **48-16-T1** — the old-poller-claim check uses `rg -c`, same empty-on-zero defect as ¹. The claim string is absent (0 matches);
   a working form is `c=$(rg -c -F "once the waiting monitor polls it" crates/devflow-cli/src/commands.rs); [ "${c:-0}" -eq 0 ]`.
4. **48-16-T2** — names `stop_at_a_non_ship_gate_with_an_unconfirmable_holder_claims_neither`, which `877a238` (require evidence
   before recovery claims) renamed to `…_refuses_success`; the old name now matches 0 tests. The renamed test passes.

---

## Wave 0 Requirements

- [x] `crates/devflow-cli/tests/gate_wedge_e2e.rs` — SURV-02 criterion 4 (#200 wedge arm plus self-resolving control; new file)
- [x] Generic child-process test helper in `crates/devflow-core/src/test_support.rs` — TEST-01 (every conversion depends on it)
- [x] Real-plan checkpoint fixtures (task elements copied from the plans measured in research) — CHKPT-01, CHKPT-02
- [x] Reusable "live foreign holder" fixture (a `devflow advance` child parked on a gate) — SURV-01, SURV-02

*Framework install: none.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|

*None. The one uncovered clause (criterion 3's resume behaviour) received an automated test in this audit.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency measured and recorded
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-09-22

**What this does not establish:** every status above comes from one run per gate. That bounds nothing about flake rates
(TEST-01's own requirement says the pinned run is a sanity check only). N-48-01 checks state immediately after `resume()`
returns and covers one gate type, a Code stage-failure gate in Supervise mode. What the detached monitor later does with a
stale answer is not covered, and neither are Validate gates or the Ship finalization-retry gate (48-CONTEXT D-05 already
marks Ship unverified).

---

## Post-Validation Code-Review Fixes (2026-09-22)

The codex + agy production-code review (`.planning/reviews/48-code-review-2026-09-22/VERIFIED.md`) found
three defects after this audit. Each got a test that failed before its fix, with an opposite-result control:

| Task ID | Requirement | Behavior | Automated Command | Control | Commit | Status |
|---------|-------------|----------|-------------------|---------|--------|--------|
| R-48-A | SURV-01 | The implicit `recover --clean` sweep keeps a stale phase whose per-phase lock is held | `cargo test -p devflow-core --lib recover::tests::clean_keeps_a_stale_phase_whose_lock_is_held -- --exact` | `recover::tests::clean_clears_stale_phase_state` | `fb2c4f8` | ✅ |
| R-48-E | SURV-02 (T-48-16-03) | `gate approve|reject` writes no Ship response for a Recycled holder | `cargo test -p devflow --bin devflow commands::tests::gate_respond_with_a_recycled_lock_pid_at_the_ship_gate_writes_nothing -- --exact` | `commands::tests::gate_approve_with_no_waiter_at_the_ship_gate_writes_and_names_ship` | `f1accee` | ✅ |
| R-48-D | SURV-02 | `recover --clean` exits non-zero and says nothing was cleaned when refused; the sweep names what it cleared | `cargo test -p devflow --test recover_clean_e2e` (4 tests) | `explicit_clean_without_a_lock_holder_cleans_and_says_so`, `sweep_names_the_stale_phase_it_cleared` | `9a33c70` | ✅ |

**Recorded limit (operator decision, 2026-09-22):** gate answers are not bound to a gate incarnation. A leftover
answer, from a waiter killed within the poll window and then `resume`d, or from a late `Gates::respond` racing a
consume-and-cleanup, decides the next gate at the same phase and stage. D-05's stale-answer hazard is only partly
closed. It is tracked under backlog 999.130 (gate-waiter registration), not fixed in Phase 48. N-48-01 pins the
`resume` half of this behavior: it shows the answer survives.

---

## Validation Audit 2026-09-22

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 0 |

- Gap: SURV-02 criterion 3, "`resume` … never reads a pending answer": existing behaviour confirmed only by reading the source.
  Resolved by `gsd-nyquist-auditor` in `3a7f9a8` (two tests: behaviour plus opposite-result control). The orchestrator re-ran
  both `--exact` (`1 passed`, 406 filtered out each) plus a misspelled-name control (`0 passed`), and fmt and clippy exited 0.
  The auditor's negative control (deleting the answer file before the assertion) failed with exit 101 on the byte-identical
  assertion.
- Plan-gate drift found, not a coverage gap: see notes ¹–⁴ above.
