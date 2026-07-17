---
phase: 15
slug: oss-readiness
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
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

> Task IDs are not yet assigned — no PLAN.md exists for 15b yet. Rows below are keyed by
> requirement/topic from RESEARCH.md's Phase Requirements → Test Map; the planner should
> replace the Task ID / Plan / Wave columns with real values once tasks are created.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | 15b — README/ARCHITECTURE accuracy | V14 (N/A) | Docs match real CLI surface | manual-only + existing guard | `cargo test -p devflow-cli --test help_snapshot` | ✅ (`crates/devflow-cli/tests/help_snapshot.rs`, 15a) | ⬜ pending |
| TBD | TBD | TBD | 15b — crates.io publish | V14 (token handling — never commit `CARGO_REGISTRY_TOKEN`) | Both crates package/publish cleanly, in dependency order | integration | `cargo publish --dry-run -p devflow-core && cargo package --workspace` | ✅ (used in 12-06, re-verified live this session) | ⬜ pending |
| TBD | TBD | TBD | 15b — devcontainer | V14 (base image pin, not `:latest`) | Container builds and runs `cargo build && cargo test && cargo clippy` cleanly | integration/manual | `devcontainer build --workspace-folder .` (or a CI job invoking it) | ❌ Wave 0 — none exists | ⬜ pending |
| TBD | TBD | TBD | 15b — CI badge / PR gate | — | Badge renders and links to the real workflow | manual | visual check of rendered README on GitHub | n/a (not automatable meaningfully) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] A devcontainer build/test check — either a documented local `devcontainer build --workspace-folder .` step in CONTRIBUTING.md, or (preferred — automatable, matches CI-parity) a new `.github/workflows/devcontainer.yml` job running `cargo build && cargo test && cargo clippy` inside the built container image. Neither currently exists.

*No other Wave 0 gap: the existing `--help` snapshot test (15a) already covers CLI-surface drift; doc-prose accuracy is inherently a read-and-compare activity, not something to force into a new automated test.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Actual `cargo publish` (not dry-run) | 15b — crates.io publish | Requires a crates.io API token held by the operator; not automatable/scriptable | Operator runs `cargo publish -p devflow-core` then `-p devflow-cli` after dry-run + docs are verified green |
| CI badge / PR gate status rendering | 15b — CI badge | GitHub badge rendering can only be confirmed by viewing the rendered README on GitHub | View README on GitHub after merge; confirm badge renders and links to the real workflow |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
