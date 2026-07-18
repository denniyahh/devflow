---
phase: 16
slug: pipeline-reliability-hardening
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-07-17
last_audited: 2026-07-17
---

# Phase 16 — Validation Strategy

> Post-execution Nyquist audit for Phase 16 requirements 16a–16k.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` — Rust unit, integration, and CLI snapshot tests |
| **Config file** | Workspace `Cargo.toml`; CI runs `cargo test --workspace` directly |
| **Quick run command** | `cargo test -p devflow-core <module>::` or `cargo test -p devflow <filter>` |
| **Full suite command** | `cargo test --workspace` |
| **CI-parity quality gates** | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` |
| **Audited result** | 309 tests passed; 0 failed; 0 ignored |

---

## Sampling Rate

- **After every task commit:** Run the touched module's scoped tests.
- **After every plan wave:** Run the full workspace suite, Clippy, and formatting check.
- **Before `/gsd-verify-work`:** The full workspace suite and all quality gates must be green.
- **Max feedback latency:** Under 90 seconds in the normal warm-build path.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 16-01-01 | 01 | 1 | 16k | T-16-01 | Terminal path truthfully merges or no-ops | unit | `cargo test -p devflow-core hooks::` | ✅ inline | ✅ green |
| 16-01-02 | 01 | 1 | 16k | T-16-02 | VersionBump tags post-merge develop | integration | `cargo test -p devflow-core hooks::tests::terminal_hooks_version_post_merge_develop` | ✅ inline | ✅ green |
| 16-01-03 | 01 | 1 | 16k | T-16-04 | Corrupted changelog/tag history stays removed | repo-check | `test -z "$(rg -c 'Released phase via DevFlow' CHANGELOG.md 2>/dev/null || true)"` | ✅ repo | ✅ green |
| 16-02-00 | 02 | 1 | 16a,16b,16d,16e | T-16-SC | Official `toml` package legitimacy confirmed before install | blocking-human | Recorded operator approval in 16-02 PLAN/SUMMARY | N/A | ✅ approved |
| 16-02-01 | 02 | 1 | 16a,16b,16d,16e | T-16-SC | Typed minimal config loads fail-soft | unit | `cargo test -p devflow-core config::` | ✅ inline | ✅ green |
| 16-02-02 | 02 | 1 | 16a,16b,16d,16e | T-16-06 | env > file > default; malformed input falls back | unit | `cargo test -p devflow-core config::` | ✅ inline | ✅ green |
| 16-03-01 | 03 | 2 | 16b | T-16-11 / T-16-12 | Captures are retained, paired, and bounded | unit | `cargo test -p devflow-core agent_result::archive` | ✅ inline | ✅ green |
| 16-03-02 | 03 | 2 | 16a | T-16-09 / T-16-10 | PLAN-only Layer 0 outranks self-report and fails closed | unit | `cargo test -p devflow-core verify:: && cargo test -p devflow-core agent_result::tests::failing_external_probe_outranks_success_marker` | ✅ inline | ✅ green |
| 16-04-01 | 04 | 2 | 16d | T-16-14 / T-16-15 | Multi-angle review is harness-agnostic and deduplicated | snapshot | `cargo test -p devflow-core prompt::` | ✅ inline | ✅ green |
| 16-04-02 | 04 | 2 | 16e | T-16-16 | Code self-review remains advisory and non-blocking | snapshot | `cargo test -p devflow-core prompt::tests::code_stage_prompt_is_unchanged_single_command_template` | ✅ inline | ✅ green |
| 16-05-01 | 05 | 2 | 16i | T-16-18 | Constructor-derived runtime paths are gitignored | invariant | `cargo test -p devflow-core doc_check::gitignore` | ✅ inline | ✅ green |
| 16-05-02 | 05 | 2 | 16c | T-16-19 / T-16-20 / T-16-21 | Bidirectional docs/source checks and reasoned exceptions | invariant | `cargo test -p devflow-core doc_check::` | ✅ inline | ✅ green |
| 16-06-01 | 06 | 3 | 16f | T-16-22 / T-16-24 | Nearest `.devflow/` ancestor resolves without loops | unit | `cargo test -p devflow project_root` | ✅ inline | ✅ green |
| 16-06-02 | 06 | 3 | 16g | T-16-23 / T-16-25 | Gate syntax is unambiguous and recovery hints are actionable | unit | `cargo test -p devflow gate_approve && cargo test -p devflow-core migrate_legacy_state` | ✅ inline | ✅ green |
| 16-07-01 | 07 | 4 | 16j | T-16-26 / T-16-27 | Pending-gate banner persists, escalates, and truncates safely | unit | `cargo test -p devflow status` | ✅ inline | ✅ green |
| 16-07-02 | 07 | 4 | 16h | T-16-28 / T-16-29 | Read-only cross-attempt timeline correlates retained evidence | unit | `cargo test -p devflow-core history::` | ✅ inline | ✅ green |

*Status: ✅ green/approved · ⚠ partial · ❌ red*

---

## Requirement Coverage

| Requirement | Automated Evidence | Status |
|-------------|--------------------|--------|
| 16a | `verify.rs` PLAN-frontmatter parsing and Layer-0 precedence tests | COVERED |
| 16b | Capture archive, review snapshot, failure preservation, and retention tests | COVERED |
| 16c | Five deterministic `doc_check` invariants | COVERED |
| 16d | Ship prompt angle/fan-out/config snapshot tests | COVERED |
| 16e | Code prompt advisory/non-interactive snapshot test | COVERED |
| 16f | Project-root nearest-ancestor and fallback tests | COVERED |
| 16g | Gate parsing plus migration warning tests | COVERED |
| 16h | Timeline ordering, correlation, rendering, and empty-state tests | COVERED |
| 16i | Constructor-derived gitignore invariant | COVERED |
| 16j | Pending-gate content, escalation, and terminal-safety tests | COVERED |
| 16k | Merge ordering, idempotency, post-merge tagging, retry-gate, and release-hygiene checks | COVERED |

---

## Wave 0 Requirements

- [x] `crates/devflow-core/src/verify.rs` and its Layer-0 tests exist.
- [x] `crates/devflow-core/src/doc_check.rs` and `doc-check-allowlist.toml` exist with five passing invariant tests.
- [x] `crates/devflow-core/src/history.rs` exists with passing timeline tests.
- [x] Hook ordering/idempotency tests cover 16k.
- [x] Capture archival/retention tests cover 16b.
- [x] CLI tests cover project-root resolution, gate parsing, pending-gate rendering, and terminal retry behavior.
- [x] No new test framework was required.

---

## Manual-Only Verifications

None required for requirement acceptance. The worktree status, pending-gate banner, and history rendering smokes recorded in the phase summaries are supplemental UX evidence; their behavioral contracts are also automated.

---

## Escalated Validation Gaps

None. The ten gaps recorded by the pre-execution audit are implemented and covered at the audited HEAD.

---

## Validation Sign-Off

- [x] Every task has automated verification or an explicit blocking-human supply-chain approval.
- [x] Every requirement 16a–16k has a behavioral test or deterministic repository check.
- [x] Wave 0 covers every planned test surface.
- [x] No watch-mode flags are used.
- [x] Full-suite feedback remains within the 90-second budget on the audited rerun.
- [x] `cargo test --workspace` passes: 309 passed, 0 failed, 0 ignored.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo fmt --all --check` and `git diff --check` pass.
- [x] Release hygiene passes: only `v1.0.1`, `v1.2.0`, and `v1.3.0`; no bogus changelog entries.
- [x] `nyquist_compliant: true` is set in frontmatter.

**Approval:** passed — audited 2026-07-17

## Validation Audit 2026-07-17 (pre-execution)

| Metric | Count |
|--------|-------|
| Gaps found | 10 |
| Resolved | 1 (behavior covered; delivery incomplete) |
| Escalated | 9 |

Full-suite evidence at that point: 284 tests passed. The missing implementations were subsequently delivered by plans 16-03 through 16-07.

## Validation Audit 2026-07-17 (post-execution)

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved since prior audit | 10 |
| Escalated | 0 |

Full-suite evidence: `cargo test --workspace` passed with 309 tests. One initial audit invocation stalled in the CLI concurrency test; the isolated test then passed in 0.16s and the immediate canonical full-suite rerun passed, so no reproducible coverage or behavior gap remains. No new test files were generated because every requirement already had substantive automated coverage.
