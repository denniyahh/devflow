---
status: testing
phase: 24-release-check-signing-key-inline-classification
source: [24-VERIFICATION.md]
started: 2026-07-27T09:40:02Z
updated: 2026-07-27T09:40:02Z
---

## Current Test

number: 1
name: D-04 positive arm — inline signing key held by a live ssh-agent reports viable with a fingerprint
expected: |
  The tag-signing row of `devflow release --check` reports viable with a `SHA256:`
  fingerprint in the detail line, the process exits zero (modulo the other three
  preflight checks), and no key material or filesystem path appears anywhere in
  the output.
awaiting: user response

## Tests

### 1. D-04 positive arm — inline signing key held by a live ssh-agent reports viable with a fingerprint

expected: On a host whose ssh-agent actually holds the configured inline key, run `git config user.signingkey "key::$(cat ~/.ssh/id_ed25519.pub)"` in a scratch clone, then `devflow release --check`. The tag-signing row reports viable with a `SHA256:` fingerprint in the detail line, the process exits zero (modulo the other three preflight checks), and no key material or filesystem path appears anywhere in the output.
result: [pending]

why_human: Requires a live ssh-agent holding a real inline key. The positive `Viable`
branch of `check_ssh_signing_viability` cannot be forced deterministically in an
unattended CI run. This item is declared `verification: backstop` in
`24-02-PLAN.md`'s own `must_haves.truths` frontmatter — it is a pre-declared manual
backstop, not a gap discovered during verification. Both SUMMARY.md files carry it
forward honestly as `human_judgment: true` (24-01 D5, 24-02 D6) rather than claiming
an automated pass.

## Summary

total: 1
passed: 0
issues: 0
pending: 1
skipped: 0
blocked: 0

## Gaps
