---
phase: 18
slug: dogfood-reliability-hardening
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-20
---

# Phase 18 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `18-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in Rust harness); inline `#[cfg(test)]` modules in `main.rs`/`agent_result.rs`/`state.rs`/`mode.rs`, plus integration tests in `crates/devflow-cli/tests/` |
| **Config file** | none — workspace `Cargo.toml`; CI parity via `.github/workflows/ci.yml` |
| **Quick run command** | `cargo test -p devflow-cli <module>::` or `cargo test -p devflow-core <module>::` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~35 seconds (baseline at HEAD: 380 passed, 0 failed) |

**CI-parity quality gates:** `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --check` (both confirmed exit 0 at HEAD). Use the `--workspace --all-targets`
form, not the narrower `cargo clippy -- -D warnings` — WR-08 found the narrow form misses
`#[cfg(test)]`-only warnings.

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-cli <module>::` / `-p devflow-core <module>::` for the touched module, plus `cargo clippy --workspace --all-targets -- -D warnings`
- **After every plan wave:** `cargo test --workspace` (any new failure not explained by an in-progress RED test is a regression)
- **Before `/gsd-verify-work`:** full suite green, clippy clean, `cargo fmt --check` clean
- **Max feedback latency:** ~35 seconds

**Known flake caveat (WR-07, still open):** `build_provenance.rs` (3 tests, ~27s locally) is
the one flaky-under-contention test in the suite. If a wave's CI run shows it fail, re-run in
isolation (`cargo test -p devflow --test build_provenance`) before attributing the failure to
this phase's changes.

---

## Per-Task Verification Map

Task IDs are assigned when PLAN.md files land; rows below are requirement-scoped and become
task-scoped at execution time.

| Req | Behavior | Wave | Test Type | Automated Command | File Exists | Status |
|-----|----------|------|-----------|-------------------|-------------|--------|
| 18a | `doctor` reports a diff between `State.stage`, branch commits, latest event, and open gates for deliberately mismatched fixtures | A | unit | `cargo test -p devflow-cli doctor` | ❌ W0 | ⬜ pending |
| 18a | `doctor` mutates nothing by default (read-only contract) — no `State`/event-log writes during a mismatch-finding call | A | unit | `cargo test -p devflow-cli doctor` | ❌ W0 | ⬜ pending |
| 18b | `monitor_pid` round-trips through `State` serde | A | unit | `cargo test -p devflow-core state::tests::monitor_pid` | ❌ W0 | ⬜ pending |
| 18b | `status`/`doctor` render (monitor dead, agent dead → "stuck") distinctly from (monitor alive, agent dead → normal between-stages) | A | unit | `cargo test -p devflow-cli status_renders_dead_monitor` | ❌ W0 | ⬜ pending |
| 18c | Staleness evaluates against `state.worktree_path` HEAD, not `project_root` HEAD — RED: fixture where `project_root` is Fresh but worktree is 2 behind; assert current code wrongly reports Fresh/Ahead | C | unit | `cargo test -p devflow-cli embedded_commit_is_stale_uses_worktree_head` | ❌ W0 | ⬜ pending |
| 18c | Self-dogfood binary behind worktree HEAD is **BLOCKed**, not warned | C | unit | extend `enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring` with `state.worktree_path` set | ❌ W0 | ⬜ pending |
| 18d | `consecutive_failures` reaches `MAX_CONSECUTIVE_FAILURES` across N Code-succeeds/Validate-fails cycles — RED: assert counter currently never exceeds 1 | B | unit | `cargo test -p devflow-cli consecutive_failures_reaches_ceiling_across_cycles` | ❌ W0 | ⬜ pending |
| 18d | `transition_resets_infra_failures` still passes unchanged — regression guard that 18d did not widen `infra_failures`' reset scope | B | unit | `cargo test -p devflow-cli transition_resets_infra_failures` | ✅ exists | ⬜ pending |
| 18e | Layer 0 affirmative-success at `Stage::Validate` + Layer 1 verdict `Pass` → `advance()` computes `passed = true` — RED: current code always `false` | B | unit + integration | `cargo test -p devflow-core layer0_affirmative_success_consults_layer1_verdict_at_validate` | ❌ W0 | ⬜ pending |
| 18e | Layer 0 pass + Layer 1 verdict `Gaps` (disagreement) → **immediate gate**, not the auto-loop path | B | unit | `cargo test -p devflow-cli external_verify_disagreement_gates_immediately` | ❌ W0 | ⬜ pending |
| 18e | Layer 0 pass + no verdict at all (ambiguous) → **immediate gate** | B | unit | `cargo test -p devflow-cli external_verify_no_verdict_gates_immediately` | ❌ W0 | ⬜ pending |
| 18e | Existing cascade tests extended to pin `verdict` (neither currently asserts it, nor at `Stage::Validate`) | B | unit (extend) | `cargo test -p devflow-core layer0_affirmative_success` | ✅ needs extension | ⬜ pending |
| 18f | `GateAction::Advance` on a preflight gate does NOT re-run the failing check; agent launches exactly once, no second gate written | C | integration | `cargo test -p devflow-cli run_preflight_advance_skips_recheck_on_idempotently_failing_check` | ❌ W0 | ⬜ pending |
| 18f | `GateAction::LoopBack` still re-runs the check but recursion is bounded — aborts after N rather than re-polling | C | integration | `cargo test -p devflow-cli run_preflight_loopback_bounds_recursion` | ❌ W0 | ⬜ pending |
| 18g | `parallel_creates_two_worktrees_and_spawns_two_monitors` passes reliably under repeated runs | C | integration (fix) | `for i in (seq 1 25); cargo test -p devflow-cli --test phase7_cli parallel_creates_two_worktrees_and_spawns_two_monitors -- --exact; or break; end` | ✅ modify assertion placement | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Note on `--exact` (project gotcha):** a bare/misspelled test name matches nothing and still
exits 0. Assert on `1 passed` in the output, not just the exit code. The CLI package is
`devflow`, not `devflow-cli`, for `--test` invocations.

---

## Wave 0 Requirements

- [ ] `crates/devflow-cli/src/main.rs` test module — `doctor` reconciliation tests (18a). No existing test drives `doctor()` against a deliberately-mismatched fixture; all current `doctor` tests are environment/tool-check only.
- [ ] `crates/devflow-core/src/state.rs` — `monitor_pid` field + serde round-trip test (18b), following `infra_failures_round_trips_through_serde` / `infra_failures_absent_from_json_defaults_to_zero`.
- [ ] Git-worktree test fixture helper (18c) — no existing staleness test constructs a real `git worktree add`; all 8 current staleness tests operate on `project_root` alone. Closest precedent: `worktree::tests::add_creates_worktree_on_new_branch`.
- [ ] `AlwaysFailAdapter` test fixture (18f) — `FailOnceAdapter` returns `Ok(())` on its second call and therefore **cannot** reproduce the wedge (its own doc comment admits this). Grep the ~1,300-line test module for an existing `AlwaysReject`-style adapter before writing a new one; research did not exhaustively enumerate fixtures.
- [ ] Framework install: none — `cargo test` already configured and green.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 18g flake reproduction | 18g | The WR-03 race may not reproduce locally within a reasonable window (dedicated workstation widens the margin; CI's shared runners narrow it — cf. 19i, which hit 2/2 in CI but passed locally most of the time) | Run the 25× loop above. If no flake appears locally, treat the fix as prevention-only and rely on the documented reasoning at `phase7_cli.rs:101-105`; do not claim a RED proof that was never observed. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 35s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
