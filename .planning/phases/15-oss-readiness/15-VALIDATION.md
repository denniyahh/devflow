---
phase: 15
slug: oss-readiness
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: mapped
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
| **Quick run command** | `cargo test -p devflow-cli --test help_snapshot` |
| **Full suite command** | `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds (full workspace suite; not re-measured this session) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow-cli --test help_snapshot` (fast CLI/doc-drift guard) plus a manual read-through of the specific doc file touched against its named source file(s)
- **After every plan wave:** Run `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite green, `cargo publish --dry-run -p devflow-core` clean, devcontainer build check green (if built)
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 15-01-T1 | 01 | 1 | 15b | — | README matches real CLI surface (gate/logs, per-phase state, no bare `state.json`) | integration | `cargo test -p devflow-cli --test help_snapshot && rg -q 'OPERATIONS\.md' README.md && rg -q 'state-' README.md && test "$(rg -c 'state\.json' README.md)" = "0"` | ✅ | ⬜ pending |
| 15-01-T2 | 01 | 1 | 15b | T-15-01-SC (medium) | SECURITY.md points to real evidence files, not the phantom `audit.log` | unit | `test "$(rg -c 'audit\.log' SECURITY.md)" = "0" && rg -q 'events\.jsonl' SECURITY.md && rg -q 'state-' SECURITY.md` | ✅ | ⬜ pending |
| 15-01-T3 | 01 | 1 | 15b | — | DEPENDENCIES.md free of decoy-config/phantom-command references | unit | `test "$(rg -c '\.devflow\.yaml' DEPENDENCIES.md)" = "0" && test "$(rg -c 'devflow confirm' DEPENDENCIES.md)" = "0" && rg -q '1\.2\.0' DEPENDENCIES.md` | ✅ | ⬜ pending |
| 15-02-T1 | 02 | 1 | 15b | — | ARCHITECTURE.md matches current Stage/hooks/lock/events model, no dead 8-step machine | unit | `test "$(rg -c 'Branching|Executing|Docsing' ARCHITECTURE.md)" = "0" && test "$(rg -c '\.devflow\.yaml' ARCHITECTURE.md)" = "0" && test "$(rg -c 'rejectpr' ARCHITECTURE.md)" = "0" && rg -q 'events\.jsonl' ARCHITECTURE.md && rg -q 'Define' ARCHITECTURE.md` | ✅ | ⬜ pending |
| 15-02-T2 | 02 | 1 | 15b | — | quickstart.md free of phantom `init`/config commands | unit | `test "$(rg -c 'devflow init' docs/guides/quickstart.md)" = "0" && test "$(rg -c '\.devflow\.yaml' docs/guides/quickstart.md)" = "0" && rg -q 'devflow start --phase' docs/guides/quickstart.md` | ✅ | ⬜ pending |
| 15-02-T3 | 02 | 1 | 15b | — | configuration.md reflects no-config-file reality | unit | `test "$(rg -c '\.devflow\.yaml' docs/guides/configuration.md)" = "0" && rg -q 'DEVFLOW_GATE_NOTIFY_CMD' docs/guides/configuration.md && rg -q 'OPERATIONS\.md' docs/guides/configuration.md` | ✅ | ⬜ pending |
| 15-03-T1 | 03 | 1 | 15b | — | CONTRIBUTING.md documents required checks + devcontainer path | unit | `rg -q 'devcontainer' CONTRIBUTING.md && rg -q 'cargo clippy' CONTRIBUTING.md && rg -q 'cargo fmt' CONTRIBUTING.md && test -f CODE_OF_CONDUCT.md` | ✅ | ⬜ pending |
| 15-03-T2 | 03 | 1 | 15b | T-15-03-SC (high) | Devcontainer base image pinned, not `:latest` | unit | `test -f .devcontainer/devcontainer.json && test "$(rg -c ':latest' .devcontainer/devcontainer.json)" = "0" && rg -q 'mcr.microsoft.com/devcontainers/rust' .devcontainer/devcontainer.json && rg -q 'rustup component add clippy rustfmt' .devcontainer/devcontainer.json` | ❌→delivered this wave (Wave 0 gap closed) | ⬜ pending |
| 15-03-T3 | 03 | 1 | 15b | — | Container-parity CI job builds/tests/lints inside the devcontainer | integration | `test -f .github/workflows/devcontainer.yml && rg -q 'cargo build' .github/workflows/devcontainer.yml && rg -q 'cargo test' .github/workflows/devcontainer.yml && rg -q 'cargo clippy' .github/workflows/devcontainer.yml && rg -q 'devcontainer' .github/workflows/devcontainer.yml` | ❌→delivered this wave (Wave 0 gap closed) | ⬜ pending |
| 15-04-T1 | 04 | 1 | 15b | T-15-04-SC (medium) | LICENSE-APACHE backs the declared dual license with canonical text, not adapted MIT | unit | `test -f LICENSE-APACHE && rg -q 'Apache License' LICENSE-APACHE && rg -q 'Version 2.0' LICENSE-APACHE && test "$(rg -c 'Permission is hereby granted, free of charge' LICENSE-APACHE)" = "0" && rg -q 'MIT OR Apache-2.0' Cargo.toml` | ✅ | ⬜ pending |
| 15-04-T2 | 04 | 1 | 15b | — | Both crates package/publish cleanly (dry-run) | integration | `cargo publish --dry-run -p devflow-core && cargo package --workspace` | ✅ | ⬜ pending |
| 15-05-T1 (manual) | 05 | 2 | 15b | T-15-05-SC (high) | Operator holds crates.io token outside repo (`cargo login` / CI secret, never committed) | manual | N/A — see Manual-Only Verifications | manual only | ⬜ pending |
| 15-05-T2 | 05 | 2 | 15b | — | `devflow-core` live on registry post-publish | integration | `cd "$(mktemp -d)" && cargo new _probe --bin >/dev/null 2>&1 && cd _probe && cargo add devflow-core@1.2.0 --dry-run` | ✅ | ⬜ pending |
| 15-05-T3 | 05 | 2 | 15b | — | `devflow` (CLI) live on registry post-publish, after core | integration | `cd "$(mktemp -d)" && cargo new _probe2 --bin >/dev/null 2>&1 && cd _probe2 && cargo add devflow@1.2.0 --dry-run` | ✅ | ⬜ pending |

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
