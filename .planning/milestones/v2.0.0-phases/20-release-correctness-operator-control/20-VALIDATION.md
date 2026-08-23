---
phase: 20
slug: release-correctness-operator-control
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-22
---

# Phase 20 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` / `cargo test`, workspace-wide |
| **Config file** | none — plain `cargo test --workspace` (no nextest/custom harness) |
| **Quick run command** | `cargo test -p devflow-core <name>` / `cargo test -p devflow <name>` (NO `--lib` — `devflow` is binary-only; `--lib` hard-errors, per the 18-01 decision in STATE.md) |
| **Full suite command** | `cargo test --workspace` (438+ tests, all green at research time) |
| **Estimated runtime** | ~60–90 seconds (full workspace) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate> <specific test name>` (no `--lib`)
- **After every plan wave:** Run `cargo test --workspace` (full suite, 0 failed required)
- **Before `/gsd-verify-work`:** Full suite must be green **on a pushed CI run**, not just local
- **Max feedback latency:** ~90 seconds (full suite)

> **20b sign-off is CI-on-branch, not local-green.** This phase's own subject
> matter (20b) is CI-concurrency-dependent flakiness that does not reproduce
> reliably locally, so local-green is explicitly insufficient for signing off
> on 20b (mirrors the Phase 19 `ENV_MUTEX` precedent).

---

## Per-Task Verification Map

> Seeded by unit from 20-RESEARCH.md § Phase Requirement → Test Map. The planner
> refines this into per-task rows (Task ID / Plan / Wave) during planning; the
> validator completes Status. This project has **no formal REQ-ID scheme** —
> units are `20a`..`20e` (per ROADMAP.md/CONTEXT.md), mapped by unit below.

| Unit | Wave | Behavior | Test Type | Automated Command | File Exists | Status |
|------|------|----------|-----------|-------------------|-------------|--------|
| 20a | 1 | Workspace self-pin rewritten alongside `[workspace.package] version` | unit + existing guard | `cargo test -p devflow --test workspace_version_pin` (existing) + new unit tests in `crates/devflow-core/src/version.rs` | ✅ guard / ❌ new units | ⬜ pending |
| 20b-1 | 1 | `cleanup --force` refuses (hard) on a live phase's worktree; bounded-retry on dead | integration | new test in `phase7_cli.rs` (or new file) | ❌ W1 | ⬜ pending |
| 20b-1 | 1 | `reference_and_cleanup_worktree_cli_flow` no longer flakes | existing integration, stabilized | `cargo test -p devflow --test phase7_cli reference_and_cleanup_worktree_cli_flow` | ✅ exists | ⬜ pending |
| 20b-2 | 1 | `start_worktree_mode_ignores_main_checkout_divergence` no longer flakes (fixture durability) | existing integration, stabilized | `cargo test -p devflow --test phase7_cli start_worktree_mode_ignores_main_checkout_divergence` | ✅ exists | ⬜ pending |
| 20c | 2 | `--until plan` halts with no monitor and no `Problem` doctor finding | integration | new test (`phase7_cli.rs` or new `pipeline_stop.rs`) | ❌ W2 | ⬜ pending |
| 20c | 2 | `--until ship` rejected with a clear error (semantic no-op) | unit | new test alongside the flag parser | ❌ W2 | ⬜ pending |
| 20d | 2 | `devflow release --check` flags self-pin drift, develop/main divergence, publish order, signing viability | unit (per-check) + one integration smoke | new file, e.g. `crates/devflow-cli/tests/release_check.rs` | ❌ W2 | ⬜ pending |
| 20e | 3 | `devflow ship --phase N` advances via an already-written Ship response, no live process | integration | new test in `pipeline_gate.rs` test module (mirrors `advance_ship_success_runs_finish_workflow`) | ❌ W3 | ⬜ pending |
| 20e | 3 | `--force` refuses when `state.stage != Stage::Ship` | unit | new test alongside the new command handler | ❌ W3 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.* No new framework or
fixture scaffolding is required — `cargo test --workspace`, the `phase7_cli.rs`
integration harness, and the `ENV_MUTEX`/PATH-neutralization patterns in
`test_support.rs` cover every test shape this phase needs. Only new test *files*
within the existing harness are added (see map above).

---

## Manual-Only Verifications

| Behavior | Unit | Why Manual | Test Instructions |
|----------|------|------------|-------------------|
| Signing-viability check against a real `ssh-add` agent state | 20d | Depends on live OpenSSH agent / key presence, which CI does not provision deterministically | Run `devflow release --check` on a machine with `gpg.format=ssh` and (a) an unlocked key loaded, then (b) no key loaded; confirm actionable pass/fail messages, no key material leaked |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (none — existing infra suffices)
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] 20b signed off against a **pushed CI run**, not local-green
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
