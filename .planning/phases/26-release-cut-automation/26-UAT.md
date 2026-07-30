---
status: testing
phase: 26-release-cut-automation
source: [26-VERIFICATION.md]
started: 2026-07-30T02:58:28Z
updated: 2026-07-30T02:58:28Z
---

## Current Test

number: 1
name: Signed release tag verifies against the operator's real key
expected: |
  `git tag -v vX.Y.Z` reports a valid signature from the maintainer's real key,
  not the throwaway SSH keypair the hermetic tests generate inside the test.
awaiting: user response

## Tests

### 1. Signed release tag verifies against the operator's real key

Run `devflow release --execute --yes-release` against a real repository with the
operator's real `devflow.releaseSigningKey` configured (a non-throwaway key), then
confirm the resulting tag verifies.

expected: `git tag -v vX.Y.Z` reports a valid signature from the maintainer's real key, not the throwaway SSH keypair the hermetic tests use.
why_human: Requires the operator's own private signing key and a real git environment. A19/A20 and `create_signed_release_tag_produces_a_verifiable_annotated_tag` only prove the invocation form works against a throwaway key generated inside the test. Recorded as a `backstop` truth in 26-05/26-06/26-07-PLAN.md — a verifier must abstain, not pass or fail it.
result: [pending]

### 2. Real `cargo publish` of both crates, in order

Run the executor's publish step (or `cargo publish` directly) for `devflow-core`
then `devflow` against the live crates.io registry with the operator's real
registry credentials, in that order.

expected: Both crates become live on crates.io in the correct order (`devflow-core` before `devflow`), and `crate_already_published` correctly reports `true` on any re-run.
why_human: A real `cargo publish` is irreversible; no test in this phase can or should perform it (D-04/D-05). `cargo_publish_reports_a_failure_without_publishing_anything` deliberately only exercises the failure path against a directory with no Cargo.toml. Recorded as a `backstop` truth in 26-05/26-06-PLAN.md.
result: [pending]

### 3. `devflow sync` lands a direct push against the real `origin`

Run `devflow sync` (or let the executor's sync step run) against the real `origin`
remote and confirm the push to `origin/develop` lands directly rather than
requiring a pull request.

expected: `git merge-base --is-ancestor origin/main origin/develop` succeeds immediately after the run, with no PR having been opened or merged for this step.
why_human: Requires the operator's own out-of-band GitHub ruleset bypass (D-01) to already be configured against the real repository. Cannot be simulated against a local bare remote, which every hermetic test in `sync::tests` and `release::tests` uses instead. Recorded as a `backstop` truth in 26-04/26-07-PLAN.md.
result: [pending]

## Summary

total: 3
passed: 0
issues: 0
pending: 3
skipped: 0
blocked: 0

## Gaps
