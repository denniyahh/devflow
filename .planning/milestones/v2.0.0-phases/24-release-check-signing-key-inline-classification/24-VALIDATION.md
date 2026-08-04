---
phase: 24
slug: release-check-signing-key-inline-classification
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-27
---

# Phase 24 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `24-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` harness — no separate framework or assertion crate |
| **Config file** | none — CI behavior driven by `.github/workflows/ci.yml` (three jobs: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`) |
| **Quick run command** | `cargo test -p devflow-core git::tests` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | quick run ~seconds (single module); full workspace suite ~minutes across 13 test binaries |

**Package-name note:** the CLI crate's cargo package is `devflow`, not
`devflow-cli` — integration tests run as `cargo test -p devflow --test release_check`.

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-core git::tests` plus
  `cargo clippy -p devflow-core -- -D warnings`
- **After every plan wave:** `cargo test --workspace`
- **Before `/gsd-verify-work`:** `cargo test --workspace` +
  `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check`
  all green (the exact three CI jobs)
- **Max feedback latency:** ~30 seconds for the quick run

---

## Per-Task Verification Map

This project tracks no requirement IDs (`.planning/REQUIREMENTS.md` does not
exist; the ROADMAP entry reads `Requirements: TBD — promoted from backlog
999.27`). The acceptance surface is the twelve locked decisions in
`24-CONTEXT.md`. Task IDs are filled in by `/gsd-validate-phase 24` once the
plan set is final; the decision → behavior → command mapping below is the
contract those tasks must satisfy.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | 1 | D-01 / D-02 / D-03 | — | `key::` → inline (prefix stripped); raw `ssh-` → inline; everything else → path. Prefix wins over path existence; no `ecdsa-`/`sk-` allowlist. | unit | `cargo test -p devflow-core git::tests` | ❌ new test | ⬜ pending |
| TBD | TBD | 1 | D-04 / D-05 / D-09 | T-20-04 | Inline key yields a real verdict via `ssh-keygen -lf -` with the blob on **stdin**, never argv. | unit | `cargo test -p devflow-core git::tests` | ❌ new test | ⬜ pending |
| TBD | TBD | 1 | D-06 | — | `ssh-keygen` absent / non-zero / unparseable → `Unknown` (CLI `warn`), never `NotViable`. | unit | `cargo test -p devflow-core git::tests` | ❌ new test | ⬜ pending |
| TBD | TBD | 1 | D-07 | — | `NoAgent` / `AgentEmpty` / `Unknown(code)` arms of `classify_ssh_add_status` shared unchanged across both branches. | unit | `cargo test -p devflow-core git::tests` | ✅ existing coverage at `git.rs:1518-1526` | ⬜ pending |
| TBD | TBD | 1 | D-08 | T-20-04 / ASVS V6 / WR-02 | No operator-visible reason string embeds the configured `user.signingkey` value, in whole or in part — now extended to the inline blob. | integration | `cargo test -p devflow --test release_check` | ❌ extend `release_check_signing_output_leaks_no_key_material_or_path` | ⬜ pending |
| TBD | TBD | 1 | D-10 | — | With `user.signingkey` set to `key::ssh-ed25519 …` **and** to raw `ssh-ed25519 …`, the result is **never** the `"key file does not exist"` `NotViable`. Agent-independent negative assertion; must not assert a specific one of Viable/NotViable/Unknown. | unit | `cargo test -p devflow-core git::tests` | ❌ new test | ⬜ pending |
| TBD | TBD | 1 | D-12 | — | Path-branch regression: a real path not starting `ssh-`/`key::` produces today's exact behavior. Required, not optional — D-02 reorders classification. | unit | `cargo test -p devflow-core git::tests` | ❌ new test | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase behaviors — no Wave 0 work.*

- No new test **file** is needed. All new unit tests land in the existing
  `mod tests` block in `crates/devflow-core/src/git.rs` (D-11).
- The optional operator-visible assertion is an addition to the existing
  `crates/devflow-cli/tests/release_check.rs`, reusing its already-defined
  `init_repo` / `git` / `run_release` / `git_only_path` helpers (D-11).
- No framework install, no new fixture module, no new dependency.
- Reusable fixtures already present: `HOME_ENV_MUTEX` + save/restore `HOME`
  (`git.rs:1538`) for git-global-config-sensitive tests, and
  `devflow_core::test_support::{git_command, hermetic_command}` behind the
  off-by-default `test-support` feature for anything that shells out to git.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| End-to-end `devflow release --check` exits **zero** on a host whose ssh-agent actually holds the inline key | D-04 (positive arm) | The `Viable` outcome depends on a live ssh-agent holding the key; asserting it in CI would be host-dependent and flaky, which is exactly what D-10 forbids. | On a host with `ssh-add -l` listing the key: `git config user.signingkey "key::$(cat ~/.ssh/id_ed25519.pub)"` then `devflow release --check`; expect exit 0 and the signing check reported as passing, with no key material in stdout. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
