---
phase: 16
slug: pipeline-reliability-hardening
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-07-17
---

# Phase 16 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` — plain Rust `#[test]`, no nextest / external harness |
| **Config file** | none — CI (`.github/workflows/ci.yml`) runs `cargo test` directly; devcontainer CI-parity (`.github/workflows/devcontainer.yml`) runs `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` under `set -e` |
| **Quick run command** | `cargo test -p devflow-core <module>::` (or `-p devflow` for CLI-crate tests) — scope to the touched module during iteration |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~15-20s scoped module run; ~60s full workspace (cold compile dominates) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate> <touched module>::` (the crate the task's files live in — `devflow-core` for library work, `devflow` for CLI work)
- **After every plan wave:** Run `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` (mirrors CI exactly)
- **Before `/gsd-verify-work`:** Full suite must be green — per D-05 the 16c/16i checkers ARE `#[test]` fns, so this same command already enforces them (no separate invocation)
- **Max feedback latency:** ~90 seconds (full CI-parity run including compile)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 16-01-01 | 01 | 1 | 16k | T-16-01 | Terminal path truthfully merges (no false-success signal) | unit | `cargo test -p devflow-core hooks::` | ✅ extend | ⬜ pending |
| 16-01-02 | 01 | 1 | 16k | T-16-02 | VersionBump tags post-merge develop (no wrong-checkout tamper) | integration | `cargo test -p devflow-core hooks::tests` | ✅ extend | ⬜ pending |
| 16-01-03 | 01 | 1 | 16k | T-16-04 | Corrupted CHANGELOG/tag history removed | repo-check | `test -z "$(rg -c 'Released phase via DevFlow' CHANGELOG.md)" && git tag -l \| sort -V` | ✅ repo file | ⬜ pending |
| 16-02-00 | 02 | 1 | 16a,16b,16d,16e | T-16-SC | Package legitimacy verified before install (supply-chain tamper) | manual | MANUAL — blocking-human: crates.io repo/age/downloads + exact pin + no typosquat (non-auto-approvable) | N/A manual | ⬜ pending |
| 16-02-01 | 02 | 1 | 16a,16b,16d,16e | T-16-SC | toml pin confirmed; config loader fail-soft | unit | `cargo test -p devflow-core config:: && cargo clippy -p devflow-core -- -D warnings` | ✅ extend | ⬜ pending |
| 16-02-02 | 02 | 1 | 16a,16b,16d,16e | T-16-06 | env>file>default precedence; malformed→default (no DoS abort) | unit | `cargo test -p devflow-core config::` | ✅ extend | ⬜ pending |
| 16-03-01 | 03 | 2 | 16b | T-16-11 / T-16-12 | Capture retained not wiped + bounded (no telemetry loss / disk DoS) | unit | `cargo test -p devflow-core agent_result::` | ✅ extend/replace | ⬜ pending |
| 16-03-02 | 03 | 2 | 16a | T-16-09 / T-16-10 | Layer-0 outranks self-report; cmd source PLAN.md-only (no injection) | unit | `cargo test -p devflow-core verify:: && cargo test -p devflow-core agent_result::evaluate` | ❌ W0 (verify.rs new) | ⬜ pending |
| 16-04-01 | 04 | 2 | 16d | T-16-14 / T-16-15 | Multi-angle review (no missed defect); harness-agnostic | unit (snapshot) | `cargo test -p devflow-core prompt::` | ✅ extend | ⬜ pending |
| 16-04-02 | 04 | 2 | 16e | T-16-16 | Advisory non-blocking self-review (no headless halt) | unit (snapshot) | `cargo test -p devflow-core prompt::` | ✅ extend | ⬜ pending |
| 16-05-01 | 05 | 2 | 16i | T-16-18 | Source-derived gitignore invariant (no telemetry leak) | unit | `cargo test -p devflow-core doc_check::gitignore` | ❌ W0 (doc_check.rs new) | ⬜ pending |
| 16-05-02 | 05 | 2 | 16c | T-16-19 / T-16-20 / T-16-21 | Doc/source drift caught; no eval of doc content; reason-required allowlist | unit | `cargo test -p devflow-core doc_check::` | ❌ W0 (doc_check.rs new) | ⬜ pending |
| 16-06-01 | 06 | 3 | 16f | T-16-22 / T-16-24 | Walk-up returns nearest `.devflow/` ancestor (no wrong-target/loop) | unit | `cargo test -p devflow project_root` | ❌ W0 (new main.rs test) | ⬜ pending |
| 16-06-02 | 06 | 3 | 16g | T-16-23 / T-16-25 | Gate footgun actionable (no silent swallow); WARN hints recover | unit | `cargo test -p devflow gate_approve && cargo test -p devflow-core migrate_legacy_state` | ⚠ partial (legacy test exists, footgun new) | ⬜ pending |
| 16-07-01 | 07 | 4 | 16j | T-16-26 / T-16-27 | Persistent banner independent of notify exit; `truncate_reason` (no flood) | unit | `cargo test -p devflow status` | ⚠ partial (status tests exist, banner new) | ⬜ pending |
| 16-07-02 | 07 | 4 | 16h | T-16-28 / T-16-29 | Cross-attempt timeline; read-only correlation (no new store) | unit | `cargo test -p devflow-core history:: && cargo build -p devflow` | ❌ W0 (history.rs new) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*
*File Exists: ✅ = test module exists (TDD extends it) · ❌ W0 = new module/test authored in the owning task's TDD RED step · ⚠ partial = module exists, the new behavior's assertions are net-new · N/A manual = blocking-human checkpoint, no code test*
*All code-producing tasks are `tdd="true"`; each task's RED step authors its failing test before implementation, so the "❌ W0" rows are discharged inline by their owning task, not by a separate Wave 0 plan.*

---

## Wave 0 Requirements

New test infrastructure required before the corresponding task can go green (each authored in the owning task's TDD RED step — no separate Wave 0 plan):

- [ ] `crates/devflow-core/src/verify.rs` — new module + tests (16a) — 16-03 Task 2 RED
- [ ] `crates/devflow-core/src/doc_check.rs` + `doc-check-allowlist.toml` — new module + tests + allowlist format (16c / 16i) — 16-05 Tasks 1 & 2 RED
- [ ] `crates/devflow-core/src/history.rs` — new module + tests (16h) — 16-07 Task 2 RED
- [ ] Extended `crates/devflow-core/src/hooks.rs` test module — merge-inclusive assertion replacing `after_ship_runs_version_and_cleanup` (16k) — 16-01
- [ ] Extended `crates/devflow-core/src/agent_result.rs` test — retention assertion replacing the delete-on-cleanup test (16b) — 16-03 Task 1
- [ ] New `crates/devflow-cli/src/main.rs` tests — `project_root_walks_up` (16f), `gate_approve_arg_parsing` (16g), and the pending-gate banner helper (16j) — 16-06 / 16-07
- **Framework install: none** — `cargo test` is already fully configured; CI runs it directly. No new test framework or dependency (the one new dependency, `toml`, is production code, not test infra, and is gated by 16-02's blocking-human legitimacy checkpoint).

*Source: `16-RESEARCH.md` §"Validation Architecture" → "Wave 0 Gaps".*

---

## Manual-Only Verifications

Every task's *mechanism* has automated coverage (see the map above). The following *outcomes* additionally warrant a manual smoke in a live run — the plans call for them and require the result recorded in the plan SUMMARY. They supplement, not replace, the automated tests.

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Worktree-run status resolves the real checkout | 16f | Real cwd/worktree behavior; the unit test uses a tempdir, a live smoke confirms end-to-end | From inside a `.devflow/`-descendant dir (a phase worktree), run `devflow status`; confirm it shows the active phase, not `idle` (16-06 SUMMARY) |
| Pending-gate banner is genuinely prominent | 16j | Visual prominence is subjective; the unit test asserts content + truncation only | Open a gate, run `devflow status`; confirm the banner is hard to miss and shows phase/stage/age/answer command (16-07 SUMMARY) |
| History timeline renders readably | 16h | Human-readability of the rendered timeline is not unit-assertable | Run `devflow history <phase>`; confirm a chronological, readable attempt timeline (16-07 SUMMARY) |
| Multi-angle review surfaces more findings than single-pass | 16d | Review efficacy is an agent-behavior outcome; only the prompt text is snapshot-tested | Observe a live Ship review producing a merged, deduplicated `REVIEW.md` across the four angles + generalist pass |
| Code-stage advisory self-review actually fires | 16e | Runtime agent behavior; only the prompt instruction is snapshot-tested | Observe a Code-stage run performing the shallow self-review without halting/gating (13-06 headless constraint) |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies — the sole exception, 16-02-00, is a blocking-human package-legitimacy checkpoint (a gate, not a code-producing task)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify — only 16-02-00 lacks one, isolated between automated tasks
- [x] Wave 0 covers all MISSING references — no `MISSING` sentinels in any `<automated>`; the new modules (verify.rs, doc_check.rs, history.rs) are authored in their owning TDD task's RED step and listed under Wave 0 Requirements
- [x] No watch-mode flags — every command is one-shot `cargo test`; no `--watch`
- [x] Feedback latency < 90s — full CI-parity run incl. compile
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
