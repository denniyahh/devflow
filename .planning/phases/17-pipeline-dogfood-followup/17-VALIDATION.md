---
phase: 17
slug: pipeline-dogfood-followup
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-18
---

# Phase 17 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `17-RESEARCH.md` §Validation Architecture; Task IDs fill in once PLAN.md files exist.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` — built-in Rust test harness; inline `#[cfg(test)]` modules plus integration tests under `crates/devflow-core/tests/` and `crates/devflow-cli/tests/` |
| **Config file** | none — workspace `Cargo.toml`; CI runs `cargo test` per `.github/workflows/ci.yml` |
| **Quick run command** | `cargo test -p devflow-core <module>::` (scope to the module under active edit) or per-crate `cargo test -p devflow-core` / `cargo test -p devflow-cli` |
| **Full suite command** | `cargo test` (workspace-wide, CI-parity) |
| **CI-parity quality gates** | `cargo clippy -- -D warnings` and `cargo fmt --check` (separate required CI jobs) |
| **Estimated runtime** | under ~90 seconds in the warm-build path (Phase 16 precedent) |

---

## Sampling Rate

- **After every task commit:** Run the touched module's scoped tests plus `cargo clippy -- -D warnings`
- **After every plan wave:** Run full `cargo test` (workspace-wide)
- **Before `/gsd-verify-work`:** Full suite, Clippy, and `cargo fmt --check` must be green
- **Max feedback latency:** ~90 seconds

---

## Per-Task Verification Map

Task IDs are assigned at planning time; rows below are seeded per requirement from `17-RESEARCH.md`.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | — | — | P2 / AC-3 (17a) | — | Layer 3 zero-commit/no-declaration outcome routes to gate, never `transition(..., Stage::Validate)` | unit | `cargo test -p devflow-core evaluate_layer3` + `cargo test -p devflow-cli advance` | ❌ W0 | ⬜ pending |
| TBD | — | — | P2 (17a, D-05) | — | Declared, approved, all-passing external probe with zero commits on a non-Code stage advances; `TRUST_EXTERNAL_VERIFY_ENV` mismatch still vetoes | unit | `cargo test -p devflow-core evaluate_layer0` | ❌ W0 | ⬜ pending |
| TBD | — | — | P2 (17b, D-07) | — | Exit 137 → `ResourceKilled`; exit 127 → `AgentUnavailable`; serialized names keep word boundaries | unit | `cargo test -p devflow-core evaluate_layer2` | ❌ W0 | ⬜ pending |
| TBD | — | — | P2 (17b, D-08) | — | Infra outcomes (`ResourceKilled`, `RateLimited`, `AgentUnavailable`) never increment `consecutive_failures` | unit | `cargo test -p devflow-cli` (consecutive-failures fixtures near `main.rs:3291`) | ❌ W0 | ⬜ pending |
| TBD | — | — | P2 (17b, D-09) | — | `RateLimited` in the PRIMARY `advance()` path writes cron-instructions instead of a blocking gate | integration | `cargo test -p devflow-cli` (extend `phase7_cli.rs:444` pattern to the primary loop) | ❌ W0 | ⬜ pending |
| TBD | — | — | P3 / AC-4 (17c) | — | Missing security artifact / invalid credential / empty reviewer set reported as a named preflight gate BEFORE `spawn_monitor` | unit/integration | `cargo test -p devflow-cli` (mirror `ensure_agent_binary` preflight test pattern, `main.rs:680-687`) | ❌ W0 | ⬜ pending |
| TBD | — | — | P1 / AC-2 (17d, D-21) | — | `workflow_started` carries version/commit/dirty/build-timestamp/exe-path fields | integration | `cargo test -p devflow-cli` (extend `emit_appends_parseable_lines_with_envelope_fields` pattern) | ❌ W0 | ⬜ pending |
| TBD | — | — | P1 / AC-2 (17d, D-17/D-19) | — | DevFlow-workspace target with non-ancestor embedded commit blocks stage launch; ordinary projects only warn | unit | `cargo test` (two-commit git fixture via `init_repo_with_feature_commit` pattern, `agent_result.rs:1001`) | ❌ W0 | ⬜ pending |
| — | — | — | AC-1 (regression, Phase 16) | — | Failed Merge leaves branch intact, blocks VersionBump/BranchCleanup, opens Ship gate | regression (existing) | `cargo test -p devflow-cli` — verify against final HEAD only; do NOT re-plan | ✅ Phase 16 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `evaluate_layer0` tests: non-Code stage + declared/approved/all-passing probe → affirmative success (D-05)
- [ ] `evaluate_layer2` tests: exit 137 → `ResourceKilled`, exit 127 → `AgentUnavailable` (D-07)
- [ ] `evaluate_layer3` (typed replacement) tests: zero-commit/no-declaration → failure outcome, not blanket `Unknown` (D-01/D-02/D-03)
- [ ] `advance()`-level test: `RateLimited` in the primary monitor loop writes cron-instructions (D-09)
- [ ] Separate-counter test: infra outcomes never touch `consecutive_failures` (D-08)
- [ ] Preflight tests: each D-14 universal check + adapter `preflight()` default-method override path (D-13)
- [ ] Provenance tests: `workflow_started` payload fields (D-21), staleness with two-commit git fixture (D-19), workspace-identity detection (D-17)
- [ ] No new test framework or config needed — only new test CASES; existing `cargo test` infrastructure covers the phase

---

## Manual-Only Verifications

*All phase behaviors have automated verification. (D-03 case 3's "notify human" is product behavior, asserted by automated tests on the gate/notify path.)*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
