---
status: testing
phase: 20-release-correctness-operator-control
source: [20-VERIFICATION.md]
started: 2026-07-23T10:28:37.724Z
updated: 2026-07-23T10:28:37.724Z
---

## Current Test

number: 1
name: Real ssh-agent signing viability (20d)
expected: |
  Run `devflow release --check` on a machine with `git config gpg.format ssh` and
  (a) an unlocked signing key loaded in ssh-agent, then (b) no key loaded (or an
  unrelated key loaded). (a) reports Viable with a `SHA256:...` fingerprint matching
  the configured `user.signingkey`; (b) reports NotViable with an actionable,
  non-crashing message. Neither case prints private key bytes or the signing key's
  filesystem path.
awaiting: user response

## Tests

### 1. Real ssh-agent signing viability (20d)
expected: Run `devflow release --check` with `git config gpg.format ssh` and (a) a signing key loaded in ssh-agent, then (b) no key / an unrelated key loaded. (a) → Viable with a `SHA256:...` fingerprint matching `user.signingkey`; (b) → NotViable with an actionable, non-crashing message ("no ssh-agent reachable" / "agent has no identities loaded" / "has keys loaded, but not the configured signing key"). Neither case leaks private key bytes or the key's filesystem path.
result: [pending]

### 2. CI-on-branch sign-off for the two ex-flaky fixtures (20b)
expected: Push the phase-20 branch (or open its PR) and confirm `cargo test --workspace` is green on a real CI runner — specifically `reference_and_cleanup_worktree_cli_flow` and `start_worktree_mode_ignores_main_checkout_divergence` (the CI-concurrency-dependent flakes 20b fixes). 0 failed on the pushed CI run. Local 5x-green is necessary but not sufficient per the phase's own VALIDATION.md (Phase 19 ENV_MUTEX precedent).
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
