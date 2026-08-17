---
phase: 36
slug: pi-agent-support
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-15
---

# Phase 36 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Reconstructed by `validate-phase` (State B) — no plan-phase seed existed because
> `36-RESEARCH.md` lacked the `## Validation Architecture` section the seed step keys on.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` (cargo) — `devflow-core` unit tests (`agents/pi.rs`), `devflow` integration tests (`help_snapshot`, `pre_push_signing_policy`, `cut_release_guard`); `scripts/check.sh` (clippy `-D warnings` + shell lint) as the deletion gate |
| **Config file** | workspace `Cargo.toml` (no per-phase test config) |
| **Quick run command** | `cargo test -p devflow-core --lib agents::pi` |
| **Full suite command** | `cargo test --workspace` + `scripts/check.sh` |
| **Estimated runtime** | quick ~1s; full workspace ~2–4 min |

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-core --lib agents::pi` (adapter) or `cargo test -p devflow --bin devflow` (CLI rows)
- **After every plan wave:** `cargo test --workspace`
- **Before `$gsd-verify-work`:** full suite green + `scripts/check.sh` clean
- **Max feedback latency:** ~60s (`devflow-core` lib); full workspace a few minutes

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 36-01-01 | 01 | 1 | Pi adapter registration (SPEC A / AC #1) | — | argv `-p --no-approve`, positional prompt, no env-var credential sniffing | unit | `cargo test -p devflow-core --lib agents::pi` | ✅ | ✅ green |
| 36-01-02 | 01 | 1 | Pi health check (SPEC A / AC #1) | — | `pi auth check` (not `DEVFLOW_PI_PROVIDER`); credentialless → Err | unit | `cargo test -p devflow-core --lib preflight` | ✅ | ✅ green |
| 36-02-01 | 02 | 2 | 999.104 (SPEC C / AC #3) | — | unset/unreadable key fails loudly before `tag -s`; probe cluster deleted clippy-clean | structural + clippy | `cargo test -p devflow --test cut_release_guard` + `scripts/check.sh` | ✅ | ✅ green |
| 36-02-02 | 02 | 2 | 999.96 (SPEC B / AC #2) | — | synthetic mismatch → NOT viable with direction | unit | `cargo test -p devflow --bin devflow changelog_version_check_flags_mismatch_and_passes_on_agreement` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] None — existing infrastructure covers all phase requirements. `validate-phase` added the
  missing tests directly: 3 `preflight` shell-out tests in `crates/devflow-core/src/agents/pi.rs`
  (PATH-stubbed `pi`), and 3 structural guards in `crates/devflow-cli/tests/cut_release_guard.rs`.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `devflow doctor` reports "installed but not headless-capable" for a *real* credentialless Pi | AC #1 | The predicate is unit-tested via a PATH stub (`classify_auth_check` + the `preflight` shell-out); the full doctor CLI surface against a live `pi` binary is a one-time human confirmation a stub cannot reproduce | Run `devflow doctor` on a host with `pi` 0.84.1 installed but no provider credential; expect the pi row to report NOT headless-capable (not merely "installed") |

*All other phase behaviors — adapter resolution, argv shape, auth-check classification and
shell-out, version-bump row, signing guard, probe removal, pre-push retention — have automated
verification.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 60s (quick command; full suite is the wave-level gate)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-08-15

---

## Validation Audit 2026-08-15

| Metric | Count |
|--------|-------|
| Tasks audited | 4 |
| Gaps found | 2 |
| Resolved | 2 |
| Escalated | 0 |

Two test gaps were found and filled:

1. **Pi `preflight` shell-out was untested** — `36-01-PLAN.md` Task 2 required a *"stubbed `pi`
   on PATH"* negative control; only the pure `classify_auth_check` helper was tested. Added three
   tests in `pi.rs` that stub `pi` on `PATH`, assert the spawn argv is
   `auth check --json --provider google`, and cover ready→Ok / not_ready→Err / ready-with-failed-exit→Err.
2. **`scripts/cut-release.sh` unset-key guard had no automated test** — added
   `crates/devflow-cli/tests/cut_release_guard.rs`, a structural guard in the
   `pre_push_signing_policy.rs` idiom asserting the guard fails loudly (and before `tag -s`) on an
   unset or unreadable key, and that the deterministic `git -c user.signingkey=` override survives.

### Record correction (not a test gap) — the `--` delimiter deviation

`36-01-PLAN.md` locked P-04 *"`--` delimiter is mandatory"*. The executor correctly deviated: **Pi
rejects `--`** — verified live during this audit (`pi -p --no-approve -- "echo hello"` →
`Error: Unknown option: --`). `pi.rs`'s module doc records the rationale and `exec_command_shape`
correctly asserts `["-p", "--no-approve", <prompt>]`. However `36-01-SUMMARY.md` misrepresents this:
it claims "a bare `--` before the positional prompt" as a shipped decision and lists a
"`--` delimiter" test that does not exist (the 5th test is actually
`classify_auth_check_tolerates_formatted_json`). The SUMMARY is an execute-phase artifact and is
flagged here rather than silently rewritten; the code and its tests are correct.
