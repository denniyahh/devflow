---
phase: 28
slug: close-the-checkpoint-answer-return-path
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: true
created: 2026-07-30
---

# Phase 28 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in Rust test harness), workspace-wide |
| **Config file** | `scripts/check.sh` — the single canonical "is this green?" definition (also run by CI and the pre-push hook) |
| **Quick run command** | `cargo test -p devflow-core <module>::tests::` (targeted, e.g. `cargo test -p devflow-core state::tests::`) |
| **Full suite command** | `scripts/check.sh test` (== `cargo test --workspace`) |
| **Estimated runtime** | ~60–120 seconds full workspace; targeted module runs are seconds |

**Package naming caution:** the crates are `devflow-core` and `devflow-cli`. A
bare `cargo test --exact <name>` that matches nothing still exits 0 — assert on
`N passed` in output, never on exit code alone.

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow-core <touched module>::tests::`
  or `cargo test -p devflow-cli <touched module>::tests::` (targeted to the file just changed)
- **After every plan wave:** Run `scripts/check.sh test` (full `cargo test --workspace`)
- **Before `/gsd-verify-work`:** `scripts/check.sh all` (fmt + clippy + test) must be green
- **Max feedback latency:** ~120 seconds

---

## Per-Task Verification Map

> Task IDs are filled in by `/gsd-validate-phase` once PLAN.md files exist. The
> behavior→test mapping below is lifted verbatim from RESEARCH.md § Validation
> Architecture and is authoritative for what must be covered.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | 999.57 (D-01) | — | Static PLAN.md scan detects a `gate="blocking-human"` task | unit | `cargo test -p devflow-core verify::tests::` | ❌ new test in `verify.rs` | ⬜ pending |
| TBD | TBD | TBD | 999.57 (D-01) | — | Scan returns `false` when no task carries the attribute (today's path unchanged) | unit | `cargo test -p devflow-core verify::tests::` | ❌ new test | ⬜ pending |
| TBD | TBD | TBD | 999.57 (D-04) | — | `session_id` round-trips through `AgentResult`/`State` serde; absent defaults to `None` | unit | `cargo test -p devflow-core state::tests::` / `agent_result::tests::` | ❌ new tests, 4 existing siblings to mirror | ⬜ pending |
| TBD | TBD | TBD | 999.57 (D-04/D-05) | T-28-01 | `--resume` command construction includes `--dangerously-skip-permissions` and `--output-format json` (Pitfall 1 regression guard) | unit | `cargo test -p devflow-core agents::claude::tests::` | ❌ new test | ⬜ pending |
| TBD | TBD | TBD | 999.57 (D-07) | — | Auto-decide emits exactly one `checkpoint_auto_decided` event, never silent | integration | `cargo test -p devflow-cli pipeline_outcomes::tests::` | ❌ new test, mirror `handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution` | ⬜ pending |
| TBD | TBD | TBD | 999.59 (D-14) | — | Define with missing CONTEXT.md no-ops without invoking `/gsd-discuss-phase` | unit | `cargo test -p devflow-core prompt::tests::` | ✅ split existing `define_and_plan_prompts_are_idempotent` (`prompt.rs:365-388`) | ⬜ pending |
| TBD | TBD | TBD | 999.59 (D-14) | — | Plan stage's existing idempotent-artifact behavior is unaffected | unit | `cargo test -p devflow-core prompt::tests::` | ✅ existing coverage, preserve | ⬜ pending |
| TBD | TBD | TBD | 999.60 (D-15) | — | `resume` does NOT clear an unfired `--until` cap when `state.stopped == false` | unit | `cargo test -p devflow-cli pipeline_launch::tests::` | ❌ new sibling to `resume_clears_stop_marker_and_advances_past_stop_point` (`pipeline_launch.rs:456-526`) | ⬜ pending |
| TBD | TBD | TBD | D-12 | — | `yes_ship` config file key sets `state.yes_ship` when CLI flag omitted | integration | new test at the `commands::start` level (e.g. `crates/devflow-cli/tests/yes_ship_config.rs`) | ❌ new test — see correction note below | ⬜ pending |
| TBD | TBD | TBD | D-12 | — | CLI flag still wins/ORs correctly over the config value | integration | `cargo test -p devflow-cli pipeline_outcomes::tests::` | ❌ new test alongside the flipped one | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

> **Correction (post-pattern-mapping).** An earlier revision of this table, written
> from RESEARCH.md § Validation Architecture, claimed the pre-existing test
> `config_file_with_yes_ship_key_loads_but_never_sets_the_flag`
> (`pipeline_outcomes.rs:1630-1648`) must have its assertion **flipped** for D-12.
> `28-PATTERNS.md` checked this against live source and found that test's premise —
> that `State::new` *alone* ignores config — **still holds** after D-12. Its assertion
> must be **preserved**; only the test's name / doc-comment / failure message need
> updating to stop reading as a blanket "config never sets this" claim. The new
> positive case (config key → `state.yes_ship`) belongs in a **new** test at the
> `commands::start` level. `28-06-PLAN.md` Task 3 implements the corrected shape and
> is authoritative over this file where they differ.

---

## Wave 0 Requirements

*None — existing infrastructure covers all phase requirements.*

Existing `cargo test --workspace`, the `ENV_MUTEX`-serialized PATH-stubbing
pattern (already used in `pipeline_launch.rs`/`preflight.rs`/`gates.rs` for
env-dependent tests), and the `tempfile`-backed fixture-repo pattern cover
every behavior above. Every new test is an addition to an already-present
`#[cfg(test)] mod tests` block in the touched file.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| The executor's `**Gate:** blocking-human` line survives unmodified through the `gsd-executor` → `execute-phase.md` orchestrator → DevFlow captured-stdout chain under a fully headless run | 999.57 (D-01) | RESEARCH.md assumption **A1** / Pitfall 2 — confirmed in source at both endpoints but never observed end-to-end with no human present. A unit test can only assert DevFlow's side of the contract, not the agent's actual emitted output. | Run one real headless phase whose plan carries a `gate="blocking-human"` task; capture `.devflow/phase-NN-stdout` and confirm the `Gate:` literal is present and parseable. RESEARCH.md recommends this as an early probe task rather than a post-hoc check. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references *(N/A — no Wave 0 gaps)*
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
