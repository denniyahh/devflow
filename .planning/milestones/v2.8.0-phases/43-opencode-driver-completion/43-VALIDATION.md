---
phase: 43
slug: opencode-driver-completion
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-23
---

# Phase 43 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust, workspace built-in) |
| **Config file** | none — workspace `Cargo.toml`, no extra test framework |
| **Quick run command** | `cargo test -p devflow-core --lib agents::opencode:: && cargo test -p devflow-core --lib agent_result::` |
| **Full suite command** | `cargo test --workspace --no-fail-fast && scripts/check.sh all` |
| **Estimated runtime** | ~0.5s quick (opencode + agent_result unit tests) / ~72s full workspace suite |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow-core --lib agents::opencode:: && cargo test -p devflow-core --lib agent_result::`
- **After every plan wave:** Run `cargo test --workspace --no-fail-fast && scripts/check.sh all`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** ~72 seconds (full workspace suite)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 43-01-01 | 01 | 1 | OPCD-01, OPCD-02 | — | Headless launch argv is exactly `opencode run "<prompt>" --auto --format json` (5 argv elements, never `--format=json`) | unit | `cargo test -p devflow-core --lib opencode_build_command_is_headless_json opencode_wraps_prompt_in_run` | ✅ | ✅ green |
| 43-01-02 | 01 | 1 | OPCD-02 | T-43-07 | Error event anywhere in stream resolves Failed; torn tail resolves indeterminate even ahead of an error event | unit | `cargo test -p devflow-core --lib agent_result::tests::opencode_real_error_capture_is_failed agent_result::tests::opencode_error_event_overrides_earlier_success_marker agent_result::tests::opencode_torn_tail_after_marker_is_indeterminate agent_result::tests::opencode_torn_tail_beats_error_event_ordering_is_stable` | ✅ | ✅ green |
| 43-01-03 | 01 | 1 | OPCD-02 | — | Marker-in-text resolves Layer1 without forgeable provenance; real tool-use capture defers to Layer 2 instead of false-resolving Success off `step_finish` | unit | `cargo test -p devflow-core --lib agent_result::tests::opencode_marker_in_text_event_resolves_at_layer1 agent_result::tests::opencode_marker_cannot_forge_layer0_provenance agent_result::tests::opencode_real_success_capture_is_recognised_and_marker_less agent_result::tests::opencode_real_tool_use_capture_defers_to_layer2` | ✅ | ✅ green |
| 43-02-01 | 02 | 2 | OPCD-03 | T-43-11, P-04 | `health()` fails closed on zero credentials, non-zero exit, or unspawnable probe; error string leaks no provider/env-var detail | unit | `cargo test -p devflow-core --lib agents::opencode::tests::preflight_accepts_configured_credentials agents::opencode::tests::preflight_rejects_constructed_zero_credential_output agents::opencode::tests::preflight_rejects_when_probe_cannot_run agents::opencode::tests::preflight_rejects_nonzero_exit_with_credential_bearing_stdout agents::opencode::tests::health_error_leaks_no_provider_detail` | ✅ | ✅ green |
| 43-02-02 | 02 | 2 | OPCD-03 (D-10) | — | `capabilities()` reports `subagent_dispatch` only on a genuine `(subagent)`/`(all)` header marker; every failure mode fails closed to `false`, never a `Result` | unit | `cargo test -p devflow-core --lib agents::opencode::tests::agent_list_baseline_reports_no_subagent agents::opencode::tests::agent_list_with_subagent_mode_reports_true agents::opencode::tests::agent_list_with_all_mode_reports_true agents::opencode::tests::subagent_probe_fails_closed_on_spawn_error agents::opencode::tests::subagent_probe_fails_closed_on_nonzero_exit agents::opencode::tests::subagent_probe_fails_closed_on_empty_output agents::opencode::tests::capabilities_never_refuses_a_launch agents::opencode::tests::agent_list_ignores_marker_text_inside_json_dump_line` | ✅ | ✅ green |
| 43-02-03 | 02 | 2 | OPCD-03, D-11 (D-12) | — | Shared conformance suite no longer spawns the real `opencode` binary; six-driver conformance suite passes against real (non-stub) health/capabilities; doctor hint names `npm i -g opencode-ai` | unit + grep-check | `cargo test -p devflow-core --lib agents::tests::default_preflight_is_ok_for_built_in_adapters agents::tests::every_driver_passes_the_conformance_suite -- --exact && rg -n 'cargo install opencode' crates/` (expect no match) | ✅ | ✅ green |
| 43-fix-01 (WR-01..WR-04) | review-fix | 2 | OPCD-03 | — | `health()` also checks `output.status.success()`; ANSI strip terminates on full CSI final-byte range; provider count anchors to `└` glyph only; subagent marker excludes JSON-dump lines | unit | `cargo test -p devflow-core --lib agents::opencode::tests::preflight_rejects_nonzero_exit_with_credential_bearing_stdout agents::opencode::tests::strip_ansi_escapes_terminates_on_non_sgr_csi_sequence agents::opencode::tests::provider_count_ignores_unanchored_matching_substring agents::opencode::tests::agent_list_ignores_marker_text_inside_json_dump_line` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Verified 2026-08-23: `cargo test -p devflow-core --lib agents::opencode::` → 20 passed, 0 failed. `cargo test -p devflow-core --lib agent_result::` → 194 passed, 0 failed. `cargo test --workspace --no-fail-fast` → 0 `FAILED` lines across 29 test binaries, 1163 tests passed total. `scripts/check.sh all` → OK (confirmed in 43-02-SUMMARY.md and 43-REVIEW-FIX.md, both dated 2026-08-23).*

---

## Wave 0 Requirements

*Existing infrastructure (cargo test, already present in the workspace before this phase) covers all phase requirements. No new test framework or shared fixture scaffolding was needed.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Zero-credential `health()` rejection against a genuine credential-less `opencode` installation | OPCD-03 (A1, P-05) | The exact stdout shape of `opencode providers list` on a machine with zero configured credentials was never observed live — only reasoned from the one live positive-credential capture in 43-RESEARCH.md and three constructed synthetic fixtures. Confirming needs a scratch container / CI runner / throwaway `HOME` override with no `auth.json` and no provider env vars. | Run `opencode providers list` in a credential-less environment, capture stdout, compare its shape against `opencode_configured_provider_count`'s parsing assumptions; add a live-captured fixture if the shape differs from the synthetic ones. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (none — no gaps found)
- [x] No watch-mode flags
- [x] Feedback latency < 72s (full suite)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-08-23

---

## Validation Audit 2026-08-23

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

All 4 requirements (OPCD-01, OPCD-02, OPCD-03, D-11) have automated unit-test coverage, independently re-verified green in this audit: `cargo test -p devflow-core --lib agents::opencode::` (20 passed), `cargo test -p devflow-core --lib agent_result::` (194 passed), `cargo test --workspace --no-fail-fast` (1163 passed, 0 failed across 29 suites), and a direct `rg` check confirming the stale `cargo install opencode` doctor hint (D-11) is gone. This audit found the template VALIDATION.md seeded at plan-phase had never been filled in (all fields still placeholder text, `status: draft`); it has been reconstructed here from the two PLAN/SUMMARY pairs, the post-hoc review-fix commit (`35e357c`), and a live re-run of the test suite — not from the stale template content.

One requirement (OPCD-03's zero-credential health path) remains verified only against constructed fixtures, never a live credential-less `opencode` install — carried forward as a Manual-Only entry above, consistent with 43-02-SUMMARY.md's own disclosure (A1, P-05).
