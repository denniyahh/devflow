---
phase: 24-release-check-signing-key-inline-classification
plan: 02
subsystem: testing
tags: [rust, git, ssh-signing, release-check, tdd, integration-test]

# Dependency graph
requires:
  - phase: 24-release-check-signing-key-inline-classification (plan 01)
    provides: "inline_signing_key_blob, inline_key_fingerprint, and check_ssh_signing_viability rewired to classify user.signingkey by git's own key::/ssh- prefix rules"
provides:
  - "Operator-boundary proof that an inline user.signingkey (key:: or raw ssh- form) produces a truthful, non-blocking devflow release --check result"
  - "Operator-boundary proof that the configured inline blob never reaches stdout, in whole or in part"
  - "RED evidence that both new integration tests fail against pre-24-01 behaviour for the intended reason"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Derive redaction assertions from the generated key blob (whole value, base64 body token, comment token) rather than a hardcoded string, so a partial-leak regression is still caught"
    - "Copy reason strings and remediation hints character-for-character from production source into test assertions, never paraphrased"

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/release_check.rs

key-decisions:
  - "RED evidence was captured via a controlled single-file overwrite (git show <pre-24-01-commit>:<path> > <path>, then git checkout HEAD -- <path> to restore) instead of the plan's literal `git stash push`/`git stash pop` instruction. `git stash push -- crates/devflow-core/src/git.rs` reported \"No local changes to save\" because the file was already committed (by plan 24-01, in a prior commit), not locally modified in this working tree — there was nothing to stash. Proceeding to `git stash pop` in that state is exactly the failure mode the destructive-git-prohibition rule warns about: the stash ref (`refs/stash`) is shared across the main checkout and every linked worktree, so a pop with nothing of its own to restore can silently apply a sibling worktree's leftover WIP. The substitute achieves an identical outcome (both new tests observed to fail against the exact pre-24-01 tree, then the tree restored byte-identical) without touching `refs/stash`."

requirements-completed: [D-04, D-06, D-08, D-10, D-11, D-12]

coverage:
  - id: D1
    description: "An inline user.signingkey (key:: prefixed or raw deprecated ssh- form) is reported truthfully at the operator boundary — never the pre-24-01 missing-key-file diagnostic — and the configured blob never reaches stdout in whole or in part, for both forms"
    requirement: "D-08, D-10, D-11"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material"
        status: pass
    human_judgment: false
  - id: D2
    description: "With ssh tooling absent from PATH, an inline user.signingkey degrades to a non-blocking warn (ssh-add not found) and the signing check's NotViable-only remediation hint never appears — the headline defect (a correct inline configuration turning release --check into a hard failure) is closed"
    requirement: "D-06, D-08, D-10, D-11"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent"
        status: pass
    human_judgment: false
  - id: D3
    description: "The two Phase 20 path-based signing tests (release_check_signing_output_leaks_no_key_material_or_path, release_check_signing_degrades_when_ssh_add_absent) remain byte-for-byte unmodified and still pass — the path-branch regression (D-12)"
    requirement: "D-12"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_signing_output_leaks_no_key_material_or_path"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_signing_degrades_when_ssh_add_absent"
        status: pass
    human_judgment: false
  - id: D4
    description: "Both new integration tests fail against pre-24-01 behaviour for the intended reason (naming the missing-key-file diagnostic and/or the remediation hint, not a compile error or unrelated panic) — ai-change-acceptance requirements 1 and 3"
    verification:
      - kind: other
        ref: "RED evidence captured below (git.rs temporarily overwritten with pre-24-01 revision 0833a6c, cargo test -p devflow --test release_check inline_signingkey)"
        status: pass
    human_judgment: false
  - id: D5
    description: "The full three-job CI gate chain (cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --check) is green — ai-change-acceptance requirement 4"
    verification:
      - kind: other
        ref: "cargo test --workspace (0 failed across all binaries); cargo clippy --workspace --all-targets -- -D warnings (exit 0); cargo fmt --check (exit 0, after an in-task fmt auto-fix)"
        status: pass
    human_judgment: false
  - id: D6
    description: "D-04's positive live-agent arm: on a host whose ssh-agent actually holds the configured inline key, devflow release --check reports the tag-signing check as passing with a SHA256: fingerprint and exits zero, printing no key material"
    verification: []
    human_judgment: true
    rationale: "Requires a live ssh-agent holding a real inline key and cannot be asserted deterministically in CI — carried forward as a backstop truth per this plan's own must_haves and VALIDATION.md § Manual-Only. Not run in this unattended execution."

duration: 8min
completed: 2026-07-27
status: complete
---

# Phase 24 Plan 02: Release-Check Signing-Key Inline Classification — Operator-Boundary Proof Summary

**Two new integration tests in `release_check.rs` prove `devflow release --check` reports an inline `user.signingkey` (both `key::` and raw `ssh-` forms) truthfully and non-blockingly, and that the configured blob never reaches stdout — closing the operator-visible half of the defect plan 24-01 fixed in the library.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-07-27T09:14:39Z
- **Completed:** 2026-07-27T09:22:53Z
- **Tasks:** 2
- **Files modified:** 1 (`crates/devflow-cli/tests/release_check.rs`)

## Accomplishments
- `release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material`: generates a real disposable ed25519 keypair, configures `user.signingkey` first as `key::<blob>` then as the raw `<blob>` itself, and asserts on each run that stdout never contains the missing-key-file diagnostic (D-10), never contains the whole blob or its base64-body/comment tokens independently (D-08), never contains `PRIVATE KEY` or the fixture root's path (T-20-04), never panics, and does contain `no ssh-agent reachable` — proving the inline value reached the shared agent-status arm rather than short-circuiting on a path-existence check (D-07).
- `release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent`: with a synthetic single-entry `PATH` containing only `git` (mirroring the adjacent Phase 20 fixture), configures a literal inline `key::ssh-ed25519 ...` value and asserts stdout contains `ssh-add not found` but never the signing check's `NotViable`-only remediation hint (`resolve before attempting the signed release tag`) — the precise falsifier for the headline defect (D-06).
- Both Phase 20 path-based signing tests (`release_check_signing_output_leaks_no_key_material_or_path`, `release_check_signing_degrades_when_ssh_add_absent`) remain byte-for-byte unmodified in the diff (D-12) — confirmed via `git diff` showing insertions only.
- RED evidence captured for both new tests (see below), then the full three-job CI gate chain run clean.

## Task Commits

Each task was committed atomically:

1. **Task 1: Operator-boundary proof — inline key is neither leaked nor reported missing (D-08, D-10, D-11)** - `2976142` (test)
2. **Task 2: Fail-soft proof plus RED evidence and the full CI gate chain (D-06, D-11, D-12)** - `aa0e113` (test)

_Both tasks are `tdd="true"` regression-test additions against already-fixed production code (plan 24-01 landed the fix in a prior, separate plan) — each is a single `test` commit; there is no accompanying `feat` commit in this plan because no production code changes here._

## Files Created/Modified
- `crates/devflow-cli/tests/release_check.rs` - added `release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material` and `release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent`, placed immediately after their respective Phase 20 analogs; no other lines touched

## RED Evidence (ai-change-acceptance requirements 1 and 3)

Captured with `crates/devflow-core/src/git.rs` temporarily overwritten with its pre-24-01 revision (commit `0833a6c`, the parent of 24-01's `e5f69d3`) — see "Deviations from Plan" below for why `git show`/`git checkout HEAD --` was used instead of the plan's literal `git stash` instruction. Command run: `cargo test -p devflow --test release_check inline_signingkey`.

```
running 2 tests
test release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent ... FAILED
test release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material ... FAILED

failures:

---- release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent stdout ----

thread 'release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent' panicked at crates/devflow-cli/tests/release_check.rs:535:5:
expected the fail-soft 'ssh-add not found' reason (D-06), got:   self-pin (workspace member versions) ⚠  could not read Cargo.toml: No such file or directory (os error 2)
  develop/main divergence (origin/main ancestor) ⚠  origin/main not fetched — cannot determine divergence
      — run `git fetch` first, then re-run this check
  crates.io publish order          ⚠  could not determine workspace publish order
  tag-signing viability            ✗  user.signingkey is set but the key file does not exist
      — resolve before attempting the signed release tag

stderr: error: release preflight failed — see checks above

---- release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material stdout ----

thread 'release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material' panicked at crates/devflow-cli/tests/release_check.rs:367:5:
key:: form: expected no missing-key-file diagnostic for an inline value, got:   self-pin (workspace member versions) ⚠  could not read Cargo.toml: No such file or directory (os error 2)
  develop/main divergence (origin/main ancestor) ⚠  origin/main not fetched — cannot determine divergence
      — run `git fetch` first, then re-run this check
  crates.io publish order          ⚠  could not determine workspace publish order
  tag-signing viability            ✗  user.signingkey is set but the key file does not exist
      — resolve before attempting the signed release tag

failures:
    release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent
    release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.04s
```

Both failures name the exact missing-key-file diagnostic (`user.signingkey is set but the key file does not exist`) and the exact remediation hint (`resolve before attempting the signed release tag`) — not a compile error, a fixture panic, or an unrelated assertion. This is the pre-24-01 defect: the path branch classified the inline value as a missing filesystem path and hard-failed. After restoring `git.rs` to its post-24-01 (HEAD) state, both tests pass (confirmed by the standalone reruns quoted in "Deviations from Plan" and the full `cargo test -p devflow --test release_check` run showing `10 passed`).

## Decisions Made
- Used the generated key's own whitespace tokens (base64 body, comment) as independent redaction assertions rather than only asserting absence of the whole blob — a partial leak (e.g. printing just the key body) would pass a whole-blob-only check but is exactly what D-08 ("in whole or in part") forbids.
- Copied the missing-key-file reason string and the `NotViable` remediation hint character-for-character from `crates/devflow-core/src/git.rs` and `crates/devflow-cli/src/commands.rs` respectively, per the plan's instruction — a paraphrase would silently turn either assertion into a permanent false green.
- Did not assert the process exit code in Task 1's test (per plan instruction) — the fixture repo has no workspace `Cargo.toml`, so the other three preflight checks each contribute their own status independent of the signing check under test.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Substituted the RED-evidence revert mechanism: `git show`/`git checkout HEAD --` instead of `git stash push`/`git stash pop`**
- **Found during:** Task 2 (the RED-evidence step)
- **Issue:** The plan's `<action>` specifies `git stash push -- crates/devflow-core/src/git.rs` to temporarily revert the 24-01 production change. Running it verbatim printed `No local changes to save` — the file was already committed (by plan 24-01, in a prior, separate commit on this same branch), not locally modified in this execution's working tree, so there was nothing for `stash push` to capture. Proceeding to `git stash pop` in that state is precisely the scenario this project's destructive-git-prohibition rule warns about: `refs/stash` is a single ref shared across the main checkout and every linked worktree, and a pop with nothing of its own to restore can silently apply a sibling worktree's leftover WIP, producing a contaminated working tree with no indication of where the changes came from.
- **Fix:** Captured the current file's content to a scratch backup, overwrote `crates/devflow-core/src/git.rs` in place with `git show 0833a6c:crates/devflow-core/src/git.rs` (the parent commit of 24-01's `e5f69d3`, i.e. the exact pre-24-01 revision), ran the RED capture, then restored via `git checkout HEAD -- crates/devflow-core/src/git.rs` and confirmed the restore was byte-identical to the pre-mutation content (`diff` against the scratch backup) and that both new symbols (`inline_signing_key_blob`, `inline_key_fingerprint`) were present again (`rg -c` reports 2). No `refs/stash` entry was ever created (`git stash list` empty throughout).
- **Files modified:** none permanently — `crates/devflow-core/src/git.rs` was returned to its exact pre-mutation state; only `crates/devflow-cli/tests/release_check.rs` carries a lasting change from this plan.
- **Verification:** `diff` confirmed byte-identical restore; `rg -c 'fn inline_signing_key_blob\(|fn inline_key_fingerprint\('` reported 2 after restore; `git stash list` empty; the full `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --check` gate chain (run after restore) was green.
- **Committed in:** not committed — this was a transient local-tree operation entirely undone before any commit; the RED-evidence *output* is recorded verbatim above and in this task's commit message.

**2. [Rule 1 - Bug] `cargo fmt` reformatted two lines in the new Task 2 test**
- **Found during:** Task 2 (the mandatory `cargo fmt --check` gate step)
- **Issue:** `cargo fmt --check` reported a diff on the newly-added `release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent` test — a long `let inline_blob = "..."` string literal and a long `!stdout.contains(...)` assertion both needed rustfmt's standard line-wrapping.
- **Fix:** Ran `cargo fmt` (workspace-wide, standard formatter, no manual reformatting).
- **Files modified:** `crates/devflow-cli/tests/release_check.rs` (formatting only, no logic change).
- **Verification:** `cargo fmt --check` exits 0 after the fix; re-ran `cargo test -p devflow --test release_check` (still 10 passed) to confirm the reformat changed no behavior.
- **Committed in:** `aa0e113` (Task 2 commit, folded in before commit — no separate `style` commit needed since it landed before the task's single commit).

---

**Total deviations:** 2 auto-fixed (1 blocking — safer RED-evidence mechanism, 1 bug — pre-commit fmt fix)
**Impact on plan:** Both deviations are process/tooling substitutions with zero effect on test behavior or coverage. The RED-evidence substitution produces byte-identical evidentiary value (both tests observed to fail against the exact pre-24-01 tree, for the exact intended reason) while avoiding the shared-stash-ref hazard. No scope creep.

## Issues Encountered
None beyond the two deviations documented above.

## Known Stubs

None. This plan adds test-only code against already-fixed production code; no new data source, UI, or component is introduced.

## Threat Flags

None new. The two new tests exercise exactly the trust boundary already registered in this plan's own `<threat_model>` (DevFlow process → operator stdout, T-24-02/T-24-06/T-24-07/T-24-08) — no new network endpoint, auth path, file-access pattern, or schema change was introduced.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Both plan 24-01 (library classification) and plan 24-02 (operator-boundary proof) are complete. The inline-`user.signingkey` false-hard-fail defect is closed end-to-end: classified correctly in `devflow-core::git`, and proven truthful/non-leaking/non-blocking at the `devflow release --check` CLI boundary.
- D-04's positive live-agent arm remains an explicit, carried-forward manual backstop (not silently dropped) — recorded in this SUMMARY's `coverage` block (D6) and in the plan's own `<human-check>`. It requires a host with a live ssh-agent holding the configured inline key and cannot be asserted deterministically in CI.
- `cargo test --workspace` (all binaries), `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` are all green as of this plan's final commit — this phase's own gate is satisfied.
- No further plans are scoped in this phase; Phase 24 is complete pending the standard verify/ship flow.

---
*Phase: 24-release-check-signing-key-inline-classification*
*Completed: 2026-07-27*

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/tests/release_check.rs`
- FOUND: commit `2976142` (Task 1)
- FOUND: commit `aa0e113` (Task 2)
- FOUND: `.planning/phases/24-release-check-signing-key-inline-classification/24-02-SUMMARY.md`
