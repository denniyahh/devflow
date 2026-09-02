---
status: passed
phase: 24-release-check-signing-key-inline-classification
source: [24-VERIFICATION.md]
started: 2026-07-27T09:40:02Z
updated: 2026-07-27T10:16:00Z
audit_acknowledged:
  milestone: v2.8.0
  at: 2026-09-02
  gap_snapshot: "passed::scenarios=0"
---

## Current Test

number: 1
name: D-04 positive arm — inline signing key held by a live ssh-agent reports viable with a fingerprint
expected: |
  The tag-signing row of `devflow release --check` reports viable with a `SHA256:`
  fingerprint in the detail line, the process exits zero (modulo the other three
  preflight checks), and no key material or filesystem path appears anywhere in
  the output.
awaiting: nothing — executed and passed 2026-07-27

## Tests

### 1. D-04 positive arm — inline signing key held by a live ssh-agent reports viable with a fingerprint

expected: On a host whose ssh-agent actually holds the configured inline key, run `git config user.signingkey "key::$(cat ~/.ssh/id_ed25519.pub)"` in a scratch clone, then `devflow release --check`. The tag-signing row reports viable with a `SHA256:` fingerprint in the detail line, the process exits zero (modulo the other three preflight checks), and no key material or filesystem path appears anywhere in the output.
result: PASSED 2026-07-27

why_human: Requires a live ssh-agent holding a real inline key. The positive `Viable`
branch of `check_ssh_signing_viability` cannot be forced deterministically in an
unattended CI run. This item is declared `verification: backstop` in
`24-02-PLAN.md`'s own `must_haves.truths` frontmatter — it is a pre-declared manual
backstop, not a gap discovered during verification. Both SUMMARY.md files carry it
forward honestly as `human_judgment: true` (24-01 D5, 24-02 D6) rather than claiming
an automated pass.

## Execution record — 2026-07-27

**Binary under test.** Built from this worktree at `6dc6e09`, sha256
`774c72d1…6dcaa7`. Built as the *subject* of the test, never installed as the
pipeline driver — the driver binary (`262a4b9e…620320`) was left untouched and
its hash re-confirmed unchanged after the build. This separation is the point of
999.48/DEN-73 and was honoured here rather than asserted.

**Precondition — the agent genuinely holds the configured key.** The host's
`ssh-agent` holds exactly one key, `SHA256:u84t7JjKi+hcvl31R/tGCNkAZOn/PZELUsAWoS//D5E`
(`~/.ssh/github_ed25519.pub`). The repository's own default signing key
(`devflow-agent-signing`, `SHA256:9BPyx2Mc…`) is *not* in the agent, so the test
configured the inline key to the one the agent actually holds — which is what the
item requires ("a host whose ssh-agent actually holds the configured inline key").

**Setup.** Scratch clone via `git clone --no-hardlinks` (local source, no network,
no remote side effect), then `gpg.format=ssh` and
`user.signingkey = "key::ssh-ed25519 AAAAC3Nza…"` — an inline literal, not a path.

**Result — verbatim:**

```
  self-pin (workspace member versions) ✓  1 member pin(s) match 1.8.1
  develop/main divergence (origin/main ancestor) ✓  origin/main is an ancestor of HEAD — sync would be a no-op
  crates.io publish order          ✓  publish in order: devflow-core -> devflow
  tag-signing viability            ✓  signing viable (SHA256:u84t7JjKi+hcvl31R/tGCNkAZOn/PZELUsAWoS//D5E)

release preflight passed
EXIT=0
```

**The three expectations, checked by assertion rather than by eye:**

| Expectation | Method | Result |
|---|---|---|
| Reports viable with a `SHA256:` fingerprint | `grep -q 'SHA256:'` on captured output | ✓ PASS |
| No key material in output | `grep -q "$KEYBODY"` (the literal base64 body) | ✓ PASS — absent |
| No filesystem path in output | `grep -Eq '/home/\|/var/home/\|\.ssh/'` | ✓ PASS — absent |
| Exits zero | captured exit code | ✓ PASS — `0` |

**Negative control — proves the fix is what made it pass.** The identical scratch
clone, with the identical inline `user.signingkey`, run against the **pre-fix**
binary (`262a4b9e…620320`, the driver built from `b2b97ea`):

```
  tag-signing viability            ✗  user.signingkey is set but the key file does not exist
```

Same clone, same config, only the binary differs — so the ✓/✗ delta isolates
`e5f69d3 feat(24-01): classify inline user.signingkey via git's own prefix rules`
as the cause. This is the exact misclassification INF-01 reported: an inline key
blob treated as a filesystem path and reported not-found.

## Summary

total: 1
passed: 1
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

None. The single pre-declared backstop item was executed against a live ssh-agent
and passed, with a negative control isolating the fix as the cause.
