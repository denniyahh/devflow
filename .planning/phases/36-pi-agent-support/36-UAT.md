---
status: complete
phase: 36-pi-agent-support
source: 36-01-SUMMARY.md, 36-02-SUMMARY.md
started: 2026-08-15T20:40:00Z
updated: 2026-08-16T00:48:33Z
---

## Current Test

[testing complete]

## Tests

### 1. Pi is a registered, selectable agent (AC #1 — presence)
expected: `devflow doctor` lists `pi` as the fourth agent alongside claude/codex/opencode
result: pass
evidence: `pi 0.84.1 ✓` row in the doctor agent table

### 2. Health check distinguishes "installed" from "headless-capable" (AC #1 — negative control)
expected: a credentialless `pi` reports NOT headless-capable via `pi auth check` (not env-var sniffing)
result: pass
evidence: `preflight` stub tests (`not_ready` → Err, `ready` → Ok) — negative control proven at the shell-out level; this host's `pi` is credentialed, so the live negative control is the automated stub proof

### 3. `release --check` version-bump row (AC #2 / 999.96)
expected: the check reports the changelog-vs-workspace version, NOT viable on a synthetic mismatch with direction
result: pass
evidence: `changelog version (matches workspace) ✓ changelog 2.5.0 matches workspace`; mismatch direction covered by `changelog_version_check_flags_mismatch_and_passes_on_agreement`

### 4. Deterministic release signing (AC #3 / 999.104)
expected: `cut-release.sh` fails loudly on unset/unreadable `devflow.releaseSigningKey`; the capability-only signing probe is gone; the pre-push fingerprint hook is retained
result: pass
evidence: `release --check` has no tag-signing row; 3 `cut_release_guard` structural guards green; `pre_push_signing_policy` green

## Summary

total: 4
passed: 4
issues: 0
pending: 0
skipped: 0

## Gaps

(none)
