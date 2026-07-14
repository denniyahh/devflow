---
phase: 13
slug: mvp-core-loop
status: mapped
nyquist_compliant: true
wave_0_complete: false
created: 2026-07-14
---

# Phase 13 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust workspace: devflow-core + devflow-cli) |
| **Config file** | Cargo.toml (workspace root) |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace`
- **After every plan wave:** Run `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 13-01-T1 | 01 | 1 | 13c | T-13-01/02 | notify passes context via env vars (no shell interpolation); fail-soft | unit | `cargo test -p devflow-core notify_hook` | ❌ new | ⬜ pending |
| 13-01-T2 | 01 | 1 | 13a, 13c, WR-11 | T-13-01..04 | non-Validate/Ship failure always gates + notifies (never silent) | build/clippy | `cargo build -p devflow-cli && cargo clippy -p devflow-cli -- -D warnings` | n/a | ⬜ pending |
| 13-01-T3 | 01 | 1 | 13a, 13c, WR-11 | T-13-01..04 | gate written on failure; timeout env-overridable | unit | `cargo test -p devflow-cli ship_agent_failed_fires_gate ship_review_failed_loops_to_code non_validate_failure_fires_gate_and_hook gate_timeout_env_override` | ❌ new | ⬜ pending |
| 13-02-T1 | 02 | 1 | 13a | T-13-06 | dead-code deletion breaks no live caller | unit | `cargo test -p devflow-core ship::` | ✅ subset | ⬜ pending |
| 13-02-T2 | 02 | 1 | 13a | T-13-05 | Ship prompt sequences non-interactive code-review (headless-safe) | unit | `cargo test -p devflow-core prompt` | 🟡 partial | ⬜ pending |
| 13-03-T1 | 03 | 1 | 13b | T-13-08 | is_error read; no panic on malformed envelope | unit | `cargo test -p devflow-core claude_envelope` | ❌ new | ⬜ pending |
| 13-03-T2 | 03 | 1 | 13b | T-13-08 | Codex JSONL parsed; unparseable lines skipped | unit | `cargo test -p devflow-core codex_event_stream` | ❌ new | ⬜ pending |
| 13-03-T3 | 03 | 1 | 13b | — | Layer 2 skips commit gate for Define/Validate | unit | `cargo test -p devflow-core evaluate_layer2 layer2_skips_commit_gate_for_define_and_validate` | 🟡 partial | ⬜ pending |
| 13-04-T1 | 04 | 2 | 13d | T-13-11/12 | worktree is the default (isolation boundary) | build/clippy | `cargo build -p devflow-cli && cargo clippy -p devflow-cli -- -D warnings` | n/a | ⬜ pending |
| 13-04-T2 | 04 | 2 | 13d | T-13-11 | default (no flag) path creates a worktree | integration | `cargo test -p devflow-cli --test phase7_cli start_defaults_to_worktree start_no_worktree_uses_feature_branch` | ❌ new | ⬜ pending |
| 13-05-T1 | 05 | 3 | 13b | T-13-14 | verdict Option with serde default; no panic | unit | `cargo test -p devflow-core parse_devflow_result_reads_verdict parse_devflow_result_verdict_absent_is_none` | ❌ new | ⬜ pending |
| 13-05-T2 | 05 | 3 | 13b | T-13-13 | Validate-with-gaps does not advance to Ship | unit | `cargo test -p devflow-cli validate_gaps_does_not_advance_to_ship validate_pass_advances` | ❌ new | ⬜ pending |
| 13-06 (manual) | 06 | 4 | 13a, 13b, 13c, 13d, 13e | T-13-15/16/17 | worktree-isolated dogfood; headless-safe Ship; real PR flow | manual | N/A — see Manual-Only Verifications | manual only | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — `cargo test` harness,
temp-git-repo integration test patterns (see `crates/devflow-cli/tests/phase7_cli.rs`),
and the 12-09 advance()/finish test harness are already in place.

---

## Manual-Only Verifications

| Behavior | Plan / Task | Requirement | Why Manual | Test Instructions |
|----------|-------------|-------------|------------|-------------------|
| Pre-flight: workspace green, doctor clean, notify hook fires, Ship prompt headless-safe | 06 / checkpoint 1 | 13a–13d | Gates the live run on merged+green code and a live notify round-trip | See 13-06-PLAN.md checkpoint 1 how-to-verify |
| Full-Ship end-to-end (re-run of 12-12 BLOCKED item) + Claude full-loop dogfood | 06 / checkpoint 2 | 13a, 13c, 13e | Requires live agent + real repo + real PR flow + gate/notify round-trip | Run full Define→Ship loop on a real external project (worktree default); answer gates via notify hook; record PR URL |
| Codex adapter through Code→Validate + real `--json` envelope parsing | 06 / checkpoint 3 | 13b, 13e | Requires real Codex CLI + credentialed run | Exercise loop with Codex; confirm the JSONL parser matches real `--json` output; verify verdict-gaps does not advance |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (manual-only tasks are 13-06 dogfood checkpoints)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify (every code task in Plans 01–05 has a `cargo` command)
- [x] Wave 0 covers all MISSING references (existing `cargo test` harness + 12-09 advance()/finish + phase7_cli fake-bin integration harness cover all new tests; no new infrastructure needed)
- [x] No watch-mode flags
- [x] Feedback latency < 90s (crate-scoped `cargo test -p ...` ~≤60s)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** mapped by planner — ready for execution
