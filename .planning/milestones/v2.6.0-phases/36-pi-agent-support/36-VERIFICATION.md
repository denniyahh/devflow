---
phase: 36-pi-agent-support
verified: 2026-08-16T00:48:33Z
status: passed
score: 9/9 must-haves verified
behavior_unverified: 0
---

# Phase 36: Pi Adapter Registration + Release Signing — Verification Report

**Phase Goal:** Register Pi as a fourth selectable agent adapter with a `pi auth check`-backed
health check, and close two release-path items (999.96 version-bump row, 999.104 deterministic
signing key).
**Verified:** 2026-08-16T00:48:33Z
**Status:** passed

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `AgentKind::Pi` + `adapter_for(AgentKind::Pi)` + `FromStr("pi")` + parse-error string lists pi | ✓ VERIFIED | `adapter_for_returns_correct_names` asserts the Pi arm; `cargo test -p devflow-core --lib` 624 passed |
| 2 | `exec_command` argv is positional `-p --no-approve` (prompt positional, `--no-approve` unconditional) | ✓ VERIFIED (corrected) | `exec_command_shape` asserts `["-p", "--no-approve", <prompt>]`. The planned bare `--` delimiter was dropped — Pi rejects `--` (live: `Error: Unknown option: --`), recorded in `pi.rs`'s module doc |
| 3 | `preflight` shells out to `pi auth check` (authoritative verb), not env-var sniffing | ✓ VERIFIED | 3 `preflight` shell-out stub tests (argv + ready/not_ready/failed-exit) + 4 `classify_auth_check` tests |
| 4 | `devflow doctor` reports Pi presence + headless-capability with a negative control | ✓ VERIFIED | doctor lists `pi 0.84.1 ✓`; negative control proven by `not_ready` stub → credentialless `Err` |
| 5 | `scripts/cut-release.sh` fails loudly before `tag -s` on unset/unreadable key | ✓ VERIFIED | 3 `cut_release_guard` structural guards (unset, unreadable, ordering before `tag -s`) |
| 6 | `check_signing_viability` cluster removed, clippy-clean | ✓ VERIFIED | only a prose comment remains at `git.rs:755`; `cargo clippy -p devflow-core -p devflow --all-targets -- -D warnings` clean |
| 7 | `release --check` tag-signing row removed; `release_finish`/`tag()` untouched | ✓ VERIFIED | `release --check` output has no tag-signing row |
| 8 | `scripts/hooks/pre-push` fingerprint hook retained | ✓ VERIFIED | `pre_push_signing_policy` test passes; hook compares `RELEASE_FPR` via `ssh-keygen -lf` |
| 9 | 999.96: `release --check` version-bump row with synthetic fixture | ✓ VERIFIED | `changelog_version_check_flags_mismatch_and_passes_on_agreement` (mismatch direction + agreement + missing/malformed) |

**Score:** 9/9 truths verified (0 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/agents/pi.rs` | PiAgent adapter | ✓ EXISTS + SUBSTANTIVE | `exec_command`, `preflight` via `pi auth check`, `classify_auth_check`, 8 tests |
| `crates/devflow-core/src/state.rs` | `AgentKind::Pi` | ✓ EXISTS | enum + `Display` + `FromStr` + error string |
| `crates/devflow-core/src/agents/mod.rs` | `adapter_for` Pi arm | ✓ EXISTS | `pub mod pi` + dispatch + test |
| `crates/devflow-cli/src/commands.rs` | doctor row + version-bump row | ✓ EXISTS | pi doctor row; `check_changelog_version` + test |
| `scripts/cut-release.sh` | unset-key guard | ✓ EXISTS | fails loudly before `tag -s` |
| `crates/devflow-core/src/git.rs` | probe cluster removed | ✓ REMOVED | clippy-clean |

**Artifacts:** 6/6 verified

## Requirements Coverage

| Requirement | Status | Blocking Issue |
|-------------|--------|----------------|
| Pi adapter registration + health check (SPEC A / AC #1) | ✓ SATISFIED | — |
| 999.96 — `release --check` version-bump row (SPEC B / AC #2) | ✓ SATISFIED | — |
| 999.104 — deterministic signing key (SPEC C / AC #3) | ✓ SATISFIED | — |

**Coverage:** 3/3 requirements satisfied

## Anti-Patterns Found

None in code. (Record-only: `36-01-SUMMARY.md` misstates the `--` delimiter as shipped/tested —
flagged in `36-VALIDATION.md`'s audit trail; the code and tests are correct.)

**Anti-patterns:** 0 blockers, 1 doc-note

## Human Verification Required

None — all verifiable items were checked programmatically or confirmed in UAT (4/4 passed in `36-UAT.md`).

## Gaps Summary

**No gaps found.** Phase goal achieved. Ready to proceed.

## Verification Metadata

**Verification approach:** Goal-backward (derived from ROADMAP phase goal)
**Must-haves source:** `36-01-PLAN.md` + `36-02-PLAN.md` frontmatter
**Automated checks:** `cargo test -p devflow-core --lib` 624 passed / 0 failed; `cargo test -p devflow --bin devflow` 322 passed / 0 failed; clippy clean
**Human checks required:** 0

---
*Verified: 2026-08-16T00:48:33Z*
*Verifier: gsd-verifier (inline — no subagent runtime available)*
