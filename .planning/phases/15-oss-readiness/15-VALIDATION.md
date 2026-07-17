---
phase: 15
slug: oss-readiness
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-07-17
---

# Phase 15 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Scope: 15b (OSS Packaging) only — 15a (dogfood enablement) already shipped and validated live.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (workspace: devflow-core + devflow-cli), plus the existing `--help` snapshot integration test |
| **Config file** | none — no `pytest.ini`/`jest.config`; test discovery is Cargo's standard `tests/` convention |
| **Quick run command** | `cargo test -p devflow --test help_snapshot` |
| **Full suite command** | `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds (full workspace suite; not re-measured this session) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow --test help_snapshot` (fast CLI/doc-drift guard) plus a manual read-through of the specific doc file touched against its named source file(s)
- **After every plan wave:** Run `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite green, `cargo publish --dry-run -p devflow-core` clean, devcontainer build check green (if built)
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 15-01-T1 | 01 | 1 | 15b | — | README matches real CLI surface (gate/logs, per-phase state, no bare `state.json`) | integration | `cargo test -p devflow --test help_snapshot && rg -q 'OPERATIONS\.md' README.md && rg -q 'state-' README.md && ! rg -q 'state\.json' README.md` | ✅ | ✅ |
| 15-01-T2 | 01 | 1 | 15b | T-15-01-SC (medium) | SECURITY.md points to real evidence files, not the phantom `audit.log` | unit | `! rg -q 'audit\.log' SECURITY.md && rg -q 'events\.jsonl' SECURITY.md && rg -q 'state-' SECURITY.md` | ✅ | ✅ |
| 15-01-T3 | 01 | 1 | 15b | — | DEPENDENCIES.md free of decoy-config/phantom-command references | unit | `! rg -q '\.devflow\.yaml' DEPENDENCIES.md && ! rg -q 'devflow confirm' DEPENDENCIES.md && rg -q '1\.2\.0' DEPENDENCIES.md` | ✅ | ✅ |
| 15-02-T1 | 02 | 1 | 15b | — | ARCHITECTURE.md matches current Stage/hooks/lock/events model, no dead 8-step machine | unit | `! rg -q 'Branching|Executing|Docsing' ARCHITECTURE.md && ! rg -q '\.devflow\.yaml' ARCHITECTURE.md && ! rg -q 'rejectpr' ARCHITECTURE.md && rg -q 'events\.jsonl' ARCHITECTURE.md && rg -q 'Define' ARCHITECTURE.md` | ✅ | ✅ |
| 15-02-T2 | 02 | 1 | 15b | — | quickstart.md free of phantom `init`/config commands | unit | `! rg -q 'devflow init' docs/guides/quickstart.md && ! rg -q '\.devflow\.yaml' docs/guides/quickstart.md && rg -q 'devflow start --phase' docs/guides/quickstart.md` | ✅ | ✅ |
| 15-02-T3 | 02 | 1 | 15b | — | configuration.md reflects no-config-file reality | unit | `! rg -q '\.devflow\.yaml' docs/guides/configuration.md && rg -q 'DEVFLOW_GATE_NOTIFY_CMD' docs/guides/configuration.md && rg -q 'OPERATIONS\.md' docs/guides/configuration.md` | ✅ | ✅ |
| 15-03-T1 | 03 | 1 | 15b | — | CONTRIBUTING.md documents required checks + devcontainer path | unit | `rg -q 'devcontainer' CONTRIBUTING.md && rg -q 'cargo clippy' CONTRIBUTING.md && rg -q 'cargo fmt' CONTRIBUTING.md && test -f CODE_OF_CONDUCT.md` | ✅ | ✅ |
| 15-03-T2 | 03 | 1 | 15b | T-15-03-SC (high) | Devcontainer base image pinned, not `:latest` | unit | `test -f .devcontainer/devcontainer.json && ! rg -q ':latest' .devcontainer/devcontainer.json && rg -q 'mcr.microsoft.com/devcontainers/rust' .devcontainer/devcontainer.json && rg -q 'rustup component add clippy rustfmt' .devcontainer/devcontainer.json` | ❌→delivered this wave (Wave 0 gap closed) | ✅ |
| 15-03-T3 | 03 | 1 | 15b | — | Container-parity CI job builds/tests/lints inside the devcontainer | integration | `test -f .github/workflows/devcontainer.yml && rg -q 'cargo build' .github/workflows/devcontainer.yml && rg -q 'cargo test' .github/workflows/devcontainer.yml && rg -q 'cargo clippy' .github/workflows/devcontainer.yml && rg -q 'devcontainer' .github/workflows/devcontainer.yml` | ❌→delivered this wave (Wave 0 gap closed) | ✅ (static structure only — see Manual-Only for live-CI caveat) |
| 15-04-T1 | 04 | 1 | 15b | T-15-04-SC (medium) | LICENSE-APACHE backs the declared dual license with canonical text, not adapted MIT | unit | `test -f LICENSE-APACHE && rg -q 'Apache License' LICENSE-APACHE && rg -q 'Version 2.0' LICENSE-APACHE && ! rg -q 'Permission is hereby granted, free of charge' LICENSE-APACHE && rg -q 'MIT OR Apache-2.0' Cargo.toml` | ✅ | ✅ |
| 15-04-T2 | 04 | 1 | 15b | — | Both crates package/publish cleanly (dry-run) | integration | `cargo publish --dry-run -p devflow-core && cargo package --workspace` | ✅ | ✅ |
| 15-05-T1 (manual) | 05 | 2 | 15b | T-15-05-SC (high) | Operator holds crates.io token outside repo (`cargo login` / CI secret, never committed) | manual | N/A — see Manual-Only Verifications | manual only | ⬜ pending — plan 15-05 not yet executed (blocked on operator crates.io token) |
| 15-05-T2 | 05 | 2 | 15b | — | `devflow-core` live on registry post-publish | integration | `cd "$(mktemp -d)" && cargo new _probe --bin >/dev/null 2>&1 && cd _probe && cargo add devflow-core@1.2.0 --dry-run` | ✅ | ⬜ pending — plan 15-05 not yet executed (blocked on operator crates.io token) |
| 15-05-T3 | 05 | 2 | 15b | — | `devflow` (CLI) live on registry post-publish, after core | integration | `cd "$(mktemp -d)" && cargo new _probe2 --bin >/dev/null 2>&1 && cd _probe2 && cargo add devflow@1.2.0 --dry-run` | ✅ | ⬜ pending — plan 15-05 not yet executed (blocked on operator crates.io token) |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] A devcontainer build/test check — delivered by 15-03-T3 (`.github/workflows/devcontainer.yml`, container-parity CI running `cargo build && cargo test && cargo clippy`).

*No other Wave 0 gap: the existing `--help` snapshot test (15a) already covers CLI-surface drift; doc-prose accuracy is inherently a read-and-compare activity, not something to force into a new automated test.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Actual `cargo publish` (not dry-run) | 15b — crates.io publish | Requires a crates.io API token held by the operator; not automatable/scriptable | 15-05-T1: operator runs `cargo login` (or sets `CARGO_REGISTRY_TOKEN`), then 15-05-T2/T3 publish `devflow-core` then `devflow` in order |
| CI badge / PR gate status rendering | 15b — CI badge | GitHub badge rendering can only be confirmed by viewing the rendered README on GitHub | View README on GitHub after merge; confirm badge renders and links to the real workflow |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (15-05-T1 is the sole manual checkpoint)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (15-03-T3 closes the devcontainer CI gap)
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** mapped — plan-checker verified all tasks carry automated verify or a correctly-typed manual checkpoint (`## VERIFICATION PASSED`); ready for execution

---

## Validation Audit 2026-07-17

| Metric | Count |
|--------|-------|
| Gaps found | 8 |
| Resolved | 8 |
| Escalated | 0 |

**What was audited:** Plans 15-01 through 15-04 (all complete, each with a green SUMMARY.md). Every automated command in the Per-Task Verification Map was re-run against the current tree.

**Root cause of all 8 gaps:** every flagged Automated Command used the idiom `test "$(rg -c PATTERN FILE)" = "0"` to assert zero matches. Under this environment's ripgrep (15.2.0), `rg -c` on zero matches prints nothing and exits 1 — not the literal string `0` — so the comparison evaluated `"" = "0"` (false) even when the underlying file was correct. Confirmed directly (`rg -c 'zzz_no_match' README.md` → exit 1, empty stdout) and cross-checked every flagged file with a direct `rg -n PATTERN FILE` presence check: zero real matches in all 8 cases — the docs themselves were never wrong, only the check script. This same quirk was already independently discovered and worked around ad hoc in three SUMMARY.md files (15-01, 15-02, 15-04) during execution, but the fix hadn't been propagated back into this file.

**Fix applied:** replaced `test "$(rg -c PATTERN FILE)" = "0"` with `! rg -q PATTERN FILE` (checks the zero-match condition via exit code, correct under any ripgrep version) in the Automated Command column for 15-01-T1, 15-01-T2, 15-01-T3, 15-02-T1, 15-02-T2, 15-02-T3, 15-03-T2, 15-04-T1. Every corrected command was re-run end-to-end and confirmed green.

**Bonus fix (same file, discovered while verifying 15-01-T1):** the Automated Command and the Test Infrastructure "Quick run command" both referenced a nonexistent Cargo package name (`devflow-cli`); the CLI crate's real package name is `devflow` (`crates/devflow-cli/Cargo.toml` declares `name = "devflow"`). Corrected both references to `cargo test -p devflow --test help_snapshot`.

**Additionally re-verified (no ripgrep-quirk bug, just stale ⬜ placeholders):** 15-03-T1, 15-03-T3, and 15-04-T2 were run as-written and confirmed green; Status column updated from `⬜ pending` to `✅`.

**Not touched — plan not yet executed:** 15-05-T1/T2/T3 remain `⬜ pending`. Plan 15-05 (crates.io publish) is gated behind a `checkpoint:human-action` task requiring the operator's crates.io API token (see 15-05-PLAN.md Task 1) and has not run yet per `.planning/STATE.md` (`stopped_at: Completed 15-04-PLAN.md`). This is not a Nyquist coverage gap — 15-05-T1 is correctly typed as the sole manual checkpoint, and T2/T3's automated registry-resolution commands are already correctly designed — the wave is simply not yet executed. Phase 15 is not fully complete; re-run `/gsd-validate-phase 15` after 15-05 executes if its commands need auditing.

**No implementation files were modified.** Only `15-VALIDATION.md`'s own command text and status cells changed.

---

## Validation Audit 2026-07-17 (re-confirmation)

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

**What was audited:** Independent re-run of all 11 Automated Commands for 15-01 through 15-04 (the corrected commands from the prior audit above), plus a re-check of `.planning/STATE.md` and the phase directory for a 15-05-SUMMARY.md.

**Result:** All 11 commands still pass, including `cargo publish --dry-run -p devflow-core && cargo package --workspace` (15-04-T2, re-run live). No drift since the prior audit. 15-05 still has no SUMMARY.md and `.planning/STATE.md` still reports `stopped_at: Completed 15-04-PLAN.md` — 15-05 remains unexecuted, unchanged from the previous audit's finding. No new gaps; nothing to fix.
