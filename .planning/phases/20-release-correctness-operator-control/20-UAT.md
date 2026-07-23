---
status: partial
phase: 20-release-correctness-operator-control
source: [20-VERIFICATION.md]
started: 2026-07-23T10:28:37.724Z
updated: 2026-07-23T10:45:00.000Z
---

## Current Test

[testing complete — 1 passed, 1 blocked on pushed CI]

## Tests

### 1. Real ssh-agent signing viability (20d)
expected: Run `devflow release --check` with `git config gpg.format ssh` and (a) a signing key loaded in ssh-agent, then (b) no key / an unrelated key loaded. (a) → Viable with a `SHA256:...` fingerprint matching `user.signingkey`; (b) → NotViable with an actionable, non-crashing message ("no ssh-agent reachable" / "agent has no identities loaded" / "has keys loaded, but not the configured signing key"). Neither case leaks private key bytes or the key's filesystem path.
result: pass
source: agent-verified (2026-07-23, this session)
evidence: |
  Verified against current HEAD binary across all four states on the real
  gpg.format=ssh setup (user.signingkey = ~/.ssh/github_ed25519.pub, fingerprint
  SHA256:u84t7JjKi+hcvl31R/tGCNkAZOn/PZELUsAWoS//D5E):
  - (a) correct key loaded (real agent): `✓ signing viable (SHA256:u84t7…)` — fingerprint matches user.signingkey.
  - (b1) no agent (SSH_AUTH_SOCK dead): `✗ no ssh-agent reachable (SSH_AUTH_SOCK unset or dead)`.
  - (b2) empty agent (temp agent, 0 keys): `✗ ssh-agent reachable but has no identities loaded`.
  - (b3) unrelated key (throwaway temp agent): `✗ ssh-agent has keys loaded, but not the configured signing key`.
  Every message actionable + non-crashing; only fingerprints shown — no private-key
  bytes and no filesystem path leaked. (b) states tested via throwaway temp agents;
  the real ssh-agent was never modified. Note: current user.signingkey is a file
  path, so INF-01's inline-key edge (backlog 999.27 / DEN-52) is not exercised here.

### 2. CI-on-branch sign-off for the two ex-flaky fixtures (20b)
expected: Push the phase-20 branch (or open its PR) and confirm `cargo test --workspace` is green on a real CI runner — specifically `reference_and_cleanup_worktree_cli_flow` and `start_worktree_mode_ignores_main_checkout_divergence` (the CI-concurrency-dependent flakes 20b fixes). 0 failed on the pushed CI run. Local 5x-green is necessary but not sufficient per the phase's own VALIDATION.md (Phase 19 ENV_MUTEX precedent).
result: blocked
blocked_by: release-build
reason: "Requires a pushed CI run — closes when the PR (feat/phase-20-release-correctness-operator-control → develop) runs CI green. Local suite is 491/0 (incl. both fixtures 13/13 across 5 runs), but per 20-VALIDATION.md local-green is necessary-not-sufficient for this CI-concurrency-dependent flake class. Not a code issue — a prerequisite gate on opening the PR."

## Summary

total: 2
passed: 1
issues: 0
pending: 0
skipped: 0
blocked: 1

## Gaps

[none — no code issues found; test 2 is a prerequisite gate, not a gap]
