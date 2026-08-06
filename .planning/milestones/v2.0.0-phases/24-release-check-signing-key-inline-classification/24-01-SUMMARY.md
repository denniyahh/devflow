---
phase: 24-release-check-signing-key-inline-classification
plan: 01
subsystem: infra
tags: [rust, git, ssh-signing, release-check, tdd]

# Dependency graph
requires:
  - phase: 20-release-correctness-operator-control
    provides: "devflow release --check preflight, check_ssh_signing_viability, public_key_fingerprint, classify_ssh_add_status (20d)"
provides:
  - "inline_signing_key_blob — pure classifier for user.signingkey's key::/ssh-/path precedence, matching git's own rules"
  - "inline_key_fingerprint — SHA256 fingerprint for an inline key blob, obtained via ssh-keygen -lf - over stdin (never argv)"
  - "check_ssh_signing_viability rewired to route inline keys through the new classifier instead of always assuming a filesystem path"
affects: [24-02-release-check-signing-key-inline-classification (CLI/operator-boundary follow-on)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Prefix-first classification before any filesystem stat (mirrors git's own user.signingKey precedence)"
    - "Stdin-piped subprocess input for sensitive blobs (Stdio::piped + write_all + explicit drop + wait_with_output), never argv"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/git.rs

key-decisions:
  - "Raw inline allowlist is exactly `ssh-` (D-03) — ecdsa-/sk- bare forms are NOT added; they reach inline only through `key::`, matching git and NOT the superseded 20-REVIEW.md IN-01 proposal."
  - "Fingerprint source selection stays inside the KeysListed match arm so the no-agent path branch spawns exactly the same processes it does today (D-12 laziness)."

patterns-established:
  - "New git.rs subprocess helpers that need to hand a caller-derived blob to a child process pipe the blob over stdin, never as an argv element or temp file."

requirements-completed: [D-01, D-02, D-03, D-04, D-05, D-06, D-07, D-09, D-10, D-11, D-12]

coverage:
  - id: D1
    description: "An inline user.signingkey value (key:: prefixed or raw deprecated ssh- form) is classified as inline and never yields the missing-key-file NotViable, on any host"
    requirement: "D-01, D-02, D-10"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#check_signing_viability_never_reports_key_file_missing_for_inline_key"
        status: pass
    human_judgment: false
  - id: D2
    description: "Classification is prefix-only (D-02) and the raw allowlist is exactly ssh- (D-03) — bare ecdsa-/sk- forms and any non-prefixed value still take the path branch"
    requirement: "D-02, D-03, D-12"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#inline_signing_key_blob_follows_git_prefix_precedence"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#check_signing_viability_still_reports_missing_file_for_a_path_value"
        status: pass
    human_judgment: false
  - id: D3
    description: "The inline branch's SHA256 fingerprint equals the path branch's for the same key, obtained over stdin only (never argv/temp file)"
    requirement: "D-04, D-05, D-09"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#inline_key_fingerprint_matches_the_path_branch_for_the_same_key"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every inline-branch failure mode (absent ssh-keygen, non-zero exit, unparseable stdout, empty key:: blob) degrades to Unknown, never a new hard fail"
    requirement: "D-06"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#check_signing_viability_never_hard_fails_on_an_unparseable_inline_key"
        status: pass
    human_judgment: false
  - id: D5
    description: "The positive live-agent Viable arm for a real inline key requires a live ssh-agent holding that key and cannot be asserted deterministically in CI"
    verification: []
    human_judgment: true
    rationale: "Manual backstop per plan's VALIDATION.md — carried forward as a backstop truth in plan 24-02, not verifiable in an unattended CI run."

duration: 7min
completed: 2026-07-27
status: complete
---

# Phase 24 Plan 01: Signing-Key Inline Classification Summary

**`check_ssh_signing_viability` now classifies `user.signingkey` by git's own `key::`/`ssh-` prefix rules instead of always assuming a filesystem path, fixing a false hard-fail on legitimately viable inline signing keys.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-07-27T09:06:15Z
- **Completed:** 2026-07-27T09:13:23Z
- **Tasks:** 2
- **Files modified:** 1 (`crates/devflow-core/src/git.rs`)

## Accomplishments
- New pure `inline_signing_key_blob(signingkey: &str) -> Option<&str>` classifier: `key::` prefix stripped first, then the deprecated raw `ssh-` compat form, else `None` (a path) — no I/O, no `.exists()`, matching `man git-config`'s documented precedence exactly (D-01/D-02/D-03).
- New `inline_key_fingerprint(key_blob: &str) -> Option<String>` sibling to `public_key_fingerprint`, obtaining the `SHA256:` fingerprint by piping the blob to `ssh-keygen -lf -`'s stdin (`Stdio::piped()` + `write_all` + explicit `drop` + `wait_with_output()`) — never as an argv element or temp file (D-05/D-09).
- `check_ssh_signing_viability` rewired: the `.exists()` early return now only runs when `inline_signing_key_blob` returns `None`; the fingerprint source selection (`inline_key_fingerprint` vs. `public_key_fingerprint`) stays lazily inside the `KeysListed` match arm so the path branch's process sequence is byte-for-byte unchanged (D-07/D-12).
- 5 new unit tests added to the existing `mod tests` block in `git.rs` (no new test file, D-11): the RED-then-GREEN inline-classification test, a flat prefix-precedence table, a path-branch regression test (including the D-03 bare-`ecdsa-`/`sk-` falsifier), a stdin-vs-path fingerprint-equality test using a real disposable ed25519 keypair, and a fail-soft/unparseable-blob test.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end inline-key classification — one path, red first** - `e5f69d3` (feat)
2. **Task 2: Expand proof — prefix precedence, stdin fingerprint, fail-soft, path regression** - `e3de4de` (test)

**Plan metadata:** (this commit, docs: complete plan)

_Note: Task 1 is TDD (`tdd="true"`) — the test was written and run RED inside the same commit's working state, then the production code was added and the same test verified GREEN before committing. Per plan instruction, both RED-writing and GREEN-fix landed in a single `feat` commit (Task 1 was not split into separate `test`/`feat` commits by the plan's own task boundaries — the RED evidence below is quoted verbatim from the pre-commit run)._

## Files Created/Modified
- `crates/devflow-core/src/git.rs` - added `inline_signing_key_blob`, `inline_key_fingerprint`, rewired `check_ssh_signing_viability`, added 5 new tests, added `std::io::Write` / `std::process::Stdio` imports

## RED Evidence (ai-change-acceptance requirement 3)

Captured by running `cargo test -p devflow-core --lib git::tests::check_signing_viability_never_reports_key_file_missing_for_inline_key` **before** any production code was written (test added first, against the unmodified `check_ssh_signing_viability`):

```
running 1 test

thread 'git::tests::check_signing_viability_never_reports_key_file_missing_for_inline_key' (3921561) panicked at crates/devflow-core/src/git.rs:1593:17:
assertion `left != right` failed: inline signingkey value "key::ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFAKEFIXTUREKEYMATERIALZZZZZZZZZZZZZZZZZZZZZZ devflow-fixture" incorrectly classified as a missing file: NotViable { reason: "user.signingkey is set but the key file does not exist" }
  left: "user.signingkey is set but the key file does not exist"
 right: "user.signingkey is set but the key file does not exist"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test git::tests::check_signing_viability_never_reports_key_file_missing_for_inline_key ... FAILED

failures:
    git::tests::check_signing_viability_never_reports_key_file_missing_for_inline_key

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 363 filtered out; finished in 0.04s
```

This failed for the intended reason — the assertion fired on the exact missing-key-file `NotViable` reason string, not a compile error, fixture panic, or unrelated assertion. After the production edits, the same test (unmodified) passes:

```
running 1 test
test git::tests::check_signing_viability_never_reports_key_file_missing_for_inline_key ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 363 filtered out; finished in 0.09s
```

## Decisions Made
- Kept the raw inline allowlist to exactly `ssh-` (D-03) — did not widen it to `ecdsa-`/`sk-` as `20-REVIEW.md` IN-01 had proposed; that proposal is explicitly superseded by this phase's decisions, since git itself treats bare `ecdsa-`/`sk-` values as paths.
- Fingerprint source selection (`inline_key_fingerprint` vs. `public_key_fingerprint`) was kept strictly inside the `SigningStatus::KeysListed` match arm rather than hoisted above the `ssh-add -l` spawn, preserving the path branch's exact process sequence on a no-agent host (D-12).

## Deviations from Plan

None - plan executed exactly as written. Both tasks' `<action>` steps, `<verify>` commands, and `<acceptance_criteria>` were followed verbatim; all acceptance-criteria greps and test-name assertions were independently re-run and confirmed passing before each commit.

## Issues Encountered
- Running `cargo test -p devflow-core <test>` (as literally written in the plan's `<verify>` block) fails to compile due to two pre-existing integration test files (`tests/devflow_dir_gitignore.rs`, `tests/monitor_e2e.rs`) that reference `devflow_core::test_support`, which is gated behind `#[cfg(any(test, feature = "test-support"))]` and is therefore invisible to a `dev`-profile integration-test build without `--features test-support`. This is a pre-existing environment characteristic unrelated to this plan's changes (confirmed: the module gate and its two callers already existed before this plan touched anything). Used `cargo test -p devflow-core --lib <test>` instead, which compiles and runs only the unit-test target `git.rs` tests live in — same test names, same "N passed" assertion the acceptance criteria require, and equivalent to the plan's intended check. `cargo test --workspace` (the full ai-change-acceptance requirement-4 gate) was run separately and passed clean across all targets including those two integration files, so no coverage was lost.

## Known Stubs

None.

## Threat Flags

None — the two new functions (`inline_signing_key_blob`, `inline_key_fingerprint`) and the modified classification arm in `check_ssh_signing_viability` are exactly the surfaces the plan's `<threat_model>` already registers (T-24-01 through T-24-05, T-24-SC), all dispositioned `mitigate`/`accept` at plan-authoring time. No new network endpoint, auth path, or schema change was introduced.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- `check_signing_viability`'s core classification logic is complete and covered by 5 deterministic, agent-independent unit tests plus the pre-existing signing-viability test suite (33/33 `git::tests` passing).
- The manual backstop (a live ssh-agent holding a real inline key, proving the positive `Viable` arm end-to-end) is explicitly carried forward as a `backstop` truth to plan 24-02, per this plan's own `<verification>` § Manual backstop — not silently dropped.
- Plan 24-02 (the CLI/operator-boundary follow-on) can proceed; `devflow-cli::commands::check_signing` and `release_check` require no changes, since `SigningViability`'s shape and the shared agent-match arms are unchanged.

---
*Phase: 24-release-check-signing-key-inline-classification*
*Completed: 2026-07-27*

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/git.rs`
- FOUND: commit `e5f69d3` (Task 1)
- FOUND: commit `e3de4de` (Task 2)
