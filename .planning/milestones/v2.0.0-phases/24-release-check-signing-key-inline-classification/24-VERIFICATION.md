---
phase: 24-release-check-signing-key-inline-classification
verified: 2026-07-27T09:38:32Z
status: passed
score: 13/13 must-haves verified
behavior_unverified: 0
backstop_unverified: 0 # RESOLVED 2026-07-27 — D-04's positive live-agent Viable arm was executed against a real ssh-agent and passed, with a pre-fix negative control isolating the fix as the cause. Full record in 24-UAT.md "Execution record".
resolved_by_uat: 2026-07-27T10:16:00Z
overrides_applied: 0
human_verification:
  - test: "D-04 positive arm — on a host whose ssh-agent actually holds the configured inline key, run `git config user.signingkey \"key::$(cat ~/.ssh/id_ed25519.pub)\"` in a scratch clone, then `devflow release --check`."
    expected: "The tag-signing row reports viable with a `SHA256:` fingerprint in the detail line, the process exits zero (modulo the other three preflight checks), and no key material or filesystem path appears anywhere in the output."
    why_human: "Requires a live ssh-agent holding a real inline key; the positive `Viable` branch of `check_ssh_signing_viability` cannot be forced deterministically in an unattended CI run. Declared `verification: backstop` in 24-02-PLAN.md's must_haves frontmatter and carried forward honestly (not silently dropped) in both 24-01-SUMMARY.md (D5) and 24-02-SUMMARY.md (D6) as `human_judgment: true`."
---

# Phase 24: `release --check` Signing-Key Inline Classification Verification Report

**Phase Goal:** `check_ssh_signing_viability` (20d, `crates/devflow-core/src/git.rs`) misclassifies an inline (non-path) `user.signingkey` value — a literal key blob configured directly rather than as a file path is treated as a path and reported as not-found. Fix classification to mirror git's own precedence, so a legitimately viable inline signing key no longer causes `devflow release --check` to hard-fail with a factually wrong reason.
**Verified:** 2026-07-27T09:38:32Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

All truths below were checked directly against source (`crates/devflow-core/src/git.rs`, `crates/devflow-cli/tests/release_check.rs`), not taken from SUMMARY.md claims. Every referenced test was independently re-run in this verification pass (not merely trusted from the SUMMARY's quoted output).

| # | Truth (plan 24-01) | Status | Evidence |
|---|---------------------|--------|----------|
| 1 | D-01/D-02/D-10: an inline value (`key::…` or raw `ssh-…`) never yields the missing-key-file `NotViable`, on any host | ✓ VERIFIED | `inline_signing_key_blob` (git.rs:751-760) is a pure prefix classifier; `check_ssh_signing_viability` (git.rs:829-836) only runs `.exists()` when `inline_blob.is_none()`. Test `check_signing_viability_never_reports_key_file_missing_for_inline_key` re-run: `test result: ok. 33 passed` (full `git::tests` module, this test included) |
| 2 | D-02: classification is prefix-only, no stat for a prefix-matched value | ✓ VERIFIED | `inline_signing_key_blob` performs no `Path`/`.exists()` I/O (confirmed by reading the function body — no `use std::path` inside it); `inline_signing_key_blob_follows_git_prefix_precedence` asserts `"ssh-key.pub"` → `Some("ssh-key.pub")` (git.rs:1722) |
| 3 | D-03: raw inline allowlist is exactly `ssh-`; bare `ecdsa-sha2-nistp256 …` / `sk-ssh-ed25519@openssh.com …` classify as PATH | ✓ VERIFIED | `rg 'starts_with("ecdsa\|starts_with("sk-\|strip_prefix("ecdsa\|strip_prefix("sk-' crates/devflow-core/src/git.rs` → no match. Test asserts both bare forms → `None` (git.rs:1729-1736), and `check_signing_viability_still_reports_missing_file_for_a_path_value` proves both bare forms take the path branch and return the exact missing-file `NotViable` (git.rs:1746-1769) |
| 4 | D-12: non-prefixed value keeps byte-for-byte path behaviour, `.exists()` before `ssh-add` spawn | ✓ VERIFIED | Same test as #3, plus code inspection shows the `.exists()` early return (git.rs:830-836) precedes the `ssh-add -l` spawn (git.rs:838) unconditionally for `inline_blob == None` |
| 5 | D-04/D-05/D-09: inline branch fingerprint via `ssh-keygen -lf -` on stdin, token identical to path branch | ✓ VERIFIED | `inline_key_fingerprint` (git.rs:778-803): `Stdio::piped()` on stdin/stdout/stderr, `child.stdin.take()` + `write_all` + explicit `drop`, never an argv element. Test `inline_key_fingerprint_matches_the_path_branch_for_the_same_key` generates a real ed25519 keypair, asserts `is_some()` + `SHA256:` prefix independently before comparing — re-run, passes |
| 6 | D-06: every inline-branch failure mode (absent tool, non-zero exit, unparseable stdout, empty `key::` blob) yields `None` → `Unknown`, no new `NotViable` | ✓ VERIFIED | `inline_key_fingerprint`'s only return paths are `?`-propagated `None`s and the parsed `Some`; `check_signing_viability_never_hard_fails_on_an_unparseable_inline_key` asserts `key::` and `key::this is not a key at all` never produce anything but the two pre-existing agent-state `NotViable` reasons, and asserts `inline_key_fingerprint("")`/`("not a key\n")` are `None` |
| 7 | D-07: `NoAgent`/`AgentEmpty`/`Unknown(code)` arms and the `stdout.contains(&fingerprint)` match are shared, unforked | ✓ VERIFIED | git.rs:848-881 — only the `fingerprint` binding inside `KeysListed` forks (`inline_key_fingerprint(blob)` vs. `public_key_fingerprint(...)`); the `Some(fp) if stdout.contains(&fp)` / `Some(_)` / `None` arms are a single shared expression |
| 8 | D-11: all 5 new unit tests live in existing `mod tests`, no new test file | ✓ VERIFIED | `git diff --stat 00c0859 HEAD -- crates/` shows only `crates/devflow-core/src/git.rs` and `crates/devflow-cli/tests/release_check.rs` modified — no new file created |

| # | Truth (plan 24-02) | Status | Evidence |
|---|----------------------|--------|----------|
| 9 | D-08/D-11: `release --check` prints no part of an inline blob (whole value, base64 body, comment) — extends T-20-04/ASVS V6/WR-02 | ✓ VERIFIED | `release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material` (release_check.rs:319-440) asserts absence of the whole blob, `base64_body_token`, and `comment_token` independently, for both `key::` and raw forms. Re-run: `ok` |
| 10 | D-10 (operator boundary): never prints the missing-key-file diagnostic, never panics, both forms | ✓ VERIFIED | Same test, lines 367-370 and 408-411 assert `!stdout.contains(MISSING_FILE_REASON)` for both forms; `!stdout.contains("panicked")` asserted for both |
| 11 | D-06 (operator boundary): with ssh tooling absent, inline key yields non-blocking `warn`, no `NotViable`-only remediation hint | ✓ VERIFIED | `release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent` (release_check.rs:513-562) asserts `stdout.contains("ssh-add not found")` AND `!stdout.contains("resolve before attempting the signed release tag")`. Re-run: `ok` |
| 12 | D-12 (integration boundary): the two Phase 20 path-based signing tests pass unmodified | ✓ VERIFIED | `git diff 00c0859 HEAD -- crates/devflow-cli/tests/release_check.rs` shows 203 insertions, 0 deletions — the two pre-existing tests are byte-for-byte untouched. Both re-run: `ok` (part of the 10/10 `release_check` suite) |
| 13 | D-04 (backstop): on a host whose ssh-agent holds the configured inline key, `release --check` reports viable with a `SHA256:` fingerprint, exits zero, leaks no key material | ⚠️ BACKSTOP — human verification required | Declared `verification: backstop` in 24-02-PLAN.md's own `must_haves.truths` frontmatter (not a gap this verifier introduced) — cannot be exercised deterministically without a live ssh-agent holding a real key. Both SUMMARY.md files record this honestly as `human_judgment: true` (D5/D6), not silently dropped. Routed to Human Verification below. |

**Score:** 12/13 truths verified programmatically; 1 explicitly-declared backstop truth requires human confirmation (not a code gap — the code path exists and is structurally exercised by every other test; only the live-agent positive-`Viable` runtime outcome is unattested).

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/git.rs` | `inline_signing_key_blob`, `inline_key_fingerprint`, rewired `check_ssh_signing_viability`, 5 new tests | ✓ VERIFIED | All 5 functions/tests present (`rg -c` confirms); read in full, matches plan description exactly; 33/33 `git::tests` pass |
| `crates/devflow-cli/tests/release_check.rs` | 2 new integration tests, 2 pre-existing tests untouched | ✓ VERIFIED | Both new tests present and read in full; `git diff --stat` confirms 0 deletions; 10/10 `release_check` tests pass |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `check_signing_viability` → `check_ssh_signing_viability` → `inline_signing_key_blob` → `inline_key_fingerprint`/`public_key_fingerprint` → `classify_ssh_add_status` match → `SigningViability` | wired | ✓ VERIFIED | Traced by reading git.rs:811-882 end-to-end; the chain is exactly as described, fingerprint selection lazy inside `KeysListed` arm only |
| `SigningViability::Viable{fingerprint}` (inline) → `devflow-cli::commands::check_signing` → `release_check` rendering | unchanged | ✓ VERIFIED | No changes to `commands.rs` in this phase's diff (`git diff --stat 00c0859 HEAD -- crates/` shows only the two files above); integration tests confirm the rendered detail line behaves identically |

### Behavioral Spot-Checks / Direct Test Re-Runs

Rather than trusting SUMMARY.md's quoted `cargo test` output, every gate command was re-run independently in this verification pass:

| Command | Result | Status |
|---------|--------|--------|
| `cargo build --workspace` | Finished, no errors | ✓ PASS |
| `cargo test -p devflow-core --lib git::tests` | `test result: ok. 33 passed; 0 failed` (includes all 5 new tests, confirmed by name in output) | ✓ PASS |
| `cargo test -p devflow --test release_check` | `test result: ok. 10 passed; 0 failed` (includes both new integration tests plus both untouched Phase 20 tests) | ✓ PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | ✓ PASS |
| `cargo fmt --check` | exit 0 | ✓ PASS |
| `rg 'starts_with("ecdsa\|starts_with("sk-\|strip_prefix("ecdsa\|strip_prefix("sk-' git.rs` | no match | ✓ PASS (D-03 falsifier absent) |
| `rg 'TBD\|FIXME\|XXX\|TODO\|HACK\|PLACEHOLDER'` on both modified files | no match | ✓ PASS |

### Requirements Coverage

This project has no `.planning/REQUIREMENTS.md` (confirmed no-REQ-ID project per PROJECT.md). The phase's `requirements:` frontmatter lists design-decision IDs D-01 through D-12, defined in `24-CONTEXT.md`. All 12 are traced below against actual code/tests, not against a REQUIREMENTS.md that is not expected to exist.

| Requirement | Description (from 24-CONTEXT.md) | Status | Evidence |
|---|---|---|---|
| D-01 | Classification follows git's `key::`/raw-`ssh-`/path precedence | ✓ SATISFIED | `inline_signing_key_blob` |
| D-02 | Prefix beats path-existence, no stat on inline values | ✓ SATISFIED | Truth #2 above |
| D-03 | Raw allowlist is `ssh-` only, not `ecdsa-`/`sk-` | ✓ SATISFIED | Truth #3 above |
| D-04 | Real Viable/NotViable verdict via shared `ssh-add -l` match | ✓ SATISFIED (structural) / backstop (positive live-agent runtime) | Truth #13 |
| D-05 | Fingerprint via `ssh-keygen -lf -` on stdin | ✓ SATISFIED | Truth #5 |
| D-06 | Fail-soft takes precedence — every inline failure → `Unknown` | ✓ SATISFIED | Truth #6, #11 |
| D-07 | Shared agent-status arms, only fingerprint acquisition forks | ✓ SATISFIED | Truth #7 |
| D-08 | No reason string embeds the configured key value | ✓ SATISFIED | Truth #9 |
| D-09 | Key blob via stdin only, never argv | ✓ SATISFIED | Truth #5 |
| D-10 | Provable without an ssh-agent (negative, agent-independent) | ✓ SATISFIED | Truth #1, #10 |
| D-11 | No new test files — existing `mod tests` / existing `release_check.rs` | ✓ SATISFIED | Truth #8 |
| D-12 | Path-branch regression, byte-for-byte unchanged | ✓ SATISFIED | Truth #4, #12 |

No orphaned requirements — `24-01-PLAN.md` and `24-02-PLAN.md`'s combined `requirements:` frontmatter (`D-01` through `D-12`, minus `D-08` only in 24-02 since 24-01 doesn't touch redaction strings directly) covers every D-ID defined in `24-CONTEXT.md`.

### Anti-Patterns Found

None. Scanned both modified files for `TBD|FIXME|XXX|TODO|HACK|PLACEHOLDER`, hardcoded empty stubs, and console-log-only implementations — no matches. `git diff` shows only additive/moved code exactly matching the plan's described action (the 11 deletions in `git.rs` are the import-line reformat, the doc-comment rewrite, the moved `.exists()` block, and the moved fingerprint call — no unrelated changes).

One advisory finding from `24-REVIEW.md` (already committed, `b896955`) is worth restating for completeness, not as a gap:

- **WR-01 (warning, not a must-have):** `inline_key_fingerprint` writes to child stdin synchronously before draining stdout/stderr — a theoretical bidirectional-pipe deadlock pattern, unreachable with today's realistic key-blob sizes (a few hundred bytes vs. the 64 KiB pipe buffer) and the specific behavior of `ssh-keygen -lf -` (reads all of stdin before producing output). Not part of any must_haves truth; disposition is advisory per the code review itself.

### Human Verification Required

1. **D-04 positive arm — live ssh-agent, real inline key**
   **Test:** On a host whose `ssh-add -l` lists a key, run `git config user.signingkey "key::$(cat ~/.ssh/id_ed25519.pub)"` in a scratch clone, then `devflow release --check`.
   **Expected:** The tag-signing check row reports viable with a `SHA256:` fingerprint in the detail line, and no key material or filesystem path appears anywhere in the output.
   **Why human:** Requires a live ssh-agent holding a real key — cannot be forced deterministically in CI. This is not a gap discovered by this verifier; it is the plan's own explicitly-declared `verification: backstop` truth (24-02-PLAN.md must_haves frontmatter), and both SUMMARY.md files already flag it honestly as unattested (`human_judgment: true`). Verification here confirms it was NOT silently dropped or falsely claimed as automated-pass — it genuinely is unverifiable in this environment and is correctly surfaced, not swept under `passed`.

### Gaps Summary

No gaps. All 12 non-backstop must-haves across both plans are independently verified against actual source and freshly re-run tests (not SUMMARY.md's quoted output) — including the two must-haves most plausibly at risk of being overstated (D-03's exact `ssh-` allowlist boundary, and D-02's no-stat-on-prefix-match rule), both of which hold exactly as claimed. The single open item (D-04's positive live-agent arm) is a pre-declared, honestly-reported backstop truth requiring a live ssh-agent — not a defect in the delivered code, and not something the executor tried to hide or falsely mark passing.

---

_Verified: 2026-07-27T09:38:32Z_
_Verifier: Claude (gsd-verifier)_
