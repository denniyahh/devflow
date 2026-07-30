---
phase: 26-release-cut-automation
plan: 08
subsystem: core
tags: [git, release-automation, rust, core, tdd, tracer, gap-closure, c-02]

# Dependency graph
requires:
  - phase: 26-release-cut-automation (26-06)
    provides: "devflow_core::release::run_release, its four entry guards, the UnreachableBaseline/StrayBaselineTag version-resolution block, and the StepReport ledger this plan persists"
  - phase: 26-release-cut-automation (26-07)
    provides: "the CLI surface (commands::release_execute) that renders ReleaseError via to_string(), which is why new ReleaseError variants are safe while new ReleaseOutcome/StepStatus variants are not"
provides:
  - "devflow_core::release_ledger — ReleaseLedger, LedgerStep, LedgerStatus, LEDGER_VERSION, ledger_path, read, write, LedgerError"
  - "a resumable release cut: run_release pins an in-flight release's version from the ledger instead of recomputing it (C-02)"
  - "the mid-flight vs. finished distinction: ReleaseError::LastReleaseCompleted refuses a re-run after a clean completion when live HEAD still names the recorded commit (D-06a)"
  - "ReleaseError::LedgerContradicted — a refusal naming both the ledger's claim and the live fact when they disagree"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Persisted record scoped to the git common directory, never the working tree: `git rev-parse --git-common-dir` + `<dir>/devflow-release-ledger.json`. Load-bearing, not cosmetic — run_release's first entry guard requires `git status --porcelain` to be empty, so a working-tree ledger would make the resume run refuse with DirtyWorkingTree, the fix breaking itself on its own second invocation. Asserted on git's real answer (`ledger_is_invisible_to_git_status`), not on the shape of the path."
    - "Versioned on-disk format with a refuse-on-unknown contract: LEDGER_VERSION is defined once and consulted by both the writer and the reader; the format version is checked BEFORE deserialization, and an unrecognized value refuses naming both numbers rather than being ignored — ignoring it silently restores exactly the C-02 behavior being removed. Rated `costly` to reverse by D-06a."
    - "Live-state-wins implemented structurally rather than by discipline: the ledger contributes only the version identity and the in-flight bit; no code path lets it mark a step Skipped, and `compute_version` is unreachable from the in-flight path by construction (single call site, inside `compute_release_version`)."
    - "One helper that pushes AND persists (`record_step`), used at every one of the ten step-report sites, rather than paired calls a future edit can separate — the in-memory ledger returned on failure (C-03) and the persisted ledger cannot drift."
    - "Hermetic offline registry-failure fixture: a committed `.cargo/config.toml` redirects `[source.crates-io]` at the loopback discard port, so `crate_already_published`'s `cargo info` fails with a connection error that classifies as PublishCheck::Ambiguous — an Err, so `cargo_publish` is structurally never reached. No test contacts crates.io or attempts a real publish."

key-files:
  created:
    - crates/devflow-core/src/release_ledger.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/release.rs

key-decisions:
  - "Followed the plan's own conditional instruction for the C-01 non-regression: `refuses_a_stray_unreachable_tag_instead_of_adopting_its_version` already runs with no ledger present, so rather than duplicating its whole fixture into a separate `a_run_without_a_ledger_still_refuses_a_stray_unreachable_tag`, it was extended with one added assertion (`release_ledger::read(root) == Ok(None)`) and a comment saying why. Its existing assertions are byte-for-byte unmodified. Consequence: the plan's `## Artifacts this phase produces` list names four new release.rs tests; three exist, and the fourth is that added assertion. Recorded here rather than left as a silent shortfall."
  - "The resume path carries the existing in-flight record forward (preserving `started_unix`, version, and tag) but CLEARS the persisted step list at the start of each run. The record's job is identity plus the in-flight bit, not an ever-growing append log across runs; C-03's operator-facing 'what already landed' record is the in-memory `ReleaseFailure::steps` of the current run, which is unchanged."
  - "`StepStatus` gained no derive and no method: its persisted label comes from a free `step_status_label` function in release.rs, so the enum devflow-cli matches exhaustively is untouched. `LedgerStep` is a separate type from `StepReport` with owned Strings for the same reason — persisting the in-memory reporting type would make every future change to it a change to a released on-disk format."
  - "`clear`/`remove` is deliberately absent from release_ledger and stated as such in the module doc: removing the record is how a stale-ledger bug becomes silent, and every state the executor can be in is expressible as in-flight or complete."

# HONEST: this gap-closure plan closed review finding C-02 only. 999.25 (the
# release-cut automation backlog item) was already claimed complete by
# 26-07-SUMMARY.md; this plan did not complete a backlog item of its own, and
# listing 999.25 again would restate someone else's delivery as this plan's.
requirements-completed: []

coverage:
  - id: D1
    description: "A release whose publish step failed is resumable: the identical re-run resolves the SAME version, pushes no second version-bump commit to origin/develop, and reaches the same publish step (closes 26-REVIEW.md C-02)"
    requirement: "26-REVIEW.md C-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release.rs#release::tests::resume_after_publish_failure_does_not_start_a_new_release"
        status: pass
    human_judgment: false
  - id: D2
    description: "The executor tells 'a release is mid-flight' from 'the last release finished cleanly': a re-run after a complete release with no new work on develop refuses by name instead of computing the next version (D-06a's stated primary job for the ledger)"
    requirement: "26-CONTEXT.md D-06a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release.rs#release::tests::a_completed_release_is_not_restarted_by_a_re_run"
        status: pass
    human_judgment: false
  - id: D3
    description: "Live git state remains authoritative: a ledger claiming the tag step completed does not skip it — the step still runs and the tag really exists afterwards (D-06a's live-state-wins clause)"
    requirement: "26-CONTEXT.md D-06a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release.rs#release::tests::a_ledger_claiming_a_step_completed_does_not_skip_it"
        status: pass
      - kind: other
        ref: "rg -n 'compute_version' crates/devflow-core/src/release.rs -> the single non-test call site is inside compute_release_version, reachable only from the no-ledger and complete-and-HEAD-moved arms"
        status: pass
    human_judgment: false
  - id: D4
    description: "A ledger that disagrees with live state loses: an in-flight ledger whose version the highest reachable semver baseline has already passed refuses naming both, with no auto-correction, no deletion, and no fallback to a fresh computation"
    requirement: "26-CONTEXT.md D-06a"
    verification:
      - kind: other
        ref: "crates/devflow-core/src/release.rs ReleaseError::LedgerContradicted — constructed on the in-flight path after `version::reachable_semver_baseline`; no test drives it (see Deviations)"
        status: partial
    human_judgment: false
  - id: D5
    description: "An unreadable, corrupt, or newer-format ledger refuses loudly and is never silently treated as absent; an absent ledger is not an error"
    requirement: "26-REVIEW.md C-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_ledger.rs#release_ledger::tests::refuses_an_unsupported_ledger_version, refuses_a_corrupt_ledger, absent_ledger_is_not_an_error"
        status: pass
    human_judgment: false
  - id: D6
    description: "The ledger is written where `git status --porcelain` cannot see it, so its own existence never trips the executor's DirtyWorkingTree entry guard on the resume run"
    requirement: "26-08-PLAN.md constraint 1"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_ledger.rs#release_ledger::tests::ledger_is_invisible_to_git_status"
        status: pass
    human_judgment: false
  - id: D7
    description: "The ledger triggers no compensating action of any kind (D-05), and no ReleaseOutcome or StepStatus variant was added (commands.rs matches both exhaustively and is outside this plan's files_modified)"
    requirement: "26-CONTEXT.md D-05"
    verification:
      - kind: other
        ref: "rg -n 'tag -d|push --force|--force-with-lease|--delete|revert|yank' crates/devflow-core/src/release.rs crates/devflow-core/src/release_ledger.rs -> no match; rg -n 'fn clear|remove_file' crates/devflow-core/src/release_ledger.rs -> no match; ReleaseOutcome still has exactly HaltedAtHumanGate/Completed/CompletedWithoutPublish; cargo test -p devflow -> 0 failed across every target"
        status: pass
    human_judgment: false
  - id: D8
    description: "No signing-viability check is added, called, or reused anywhere in this plan (D-10)"
    requirement: "26-CONTEXT.md D-10"
    verification:
      - kind: other
        ref: "rg -c 'check_ssh_signing_viability' crates/ -g '*.rs' -> exactly crates/devflow-core/src/git.rs:2"
        status: pass
    human_judgment: false
  - id: D9
    description: "backstop truth: an operator whose real crates.io publish failed mid-sequence re-runs the identical command and finishes the SAME release rather than cutting a second one"
    verification: []
    human_judgment: true
    rationale: "Requires a real partial publish against crates.io — a one-way operation no hermetic test may perform. The two-run sequence, the pinned version, and the byte-identical remote ref are proven against a local bare remote and an offline registry redirection; the live registry leg is the operator's, matching 26-05/26-06/26-07's identical treatment of their own live-publish backstops."

# Metrics
duration: 45min
completed: 2026-07-30
status: complete
---

# Phase 26 Plan 08: The release ledger — an interrupted cut is resumable (C-02) Summary

**A release whose publish step failed no longer starts a second release on the re-run: `run_release` now pins the in-flight cut's version from a persisted, versioned, git-directory-scoped ledger instead of recomputing it from live state the interrupted run has already moved — and it can finally tell "this release is mid-flight" from "the last release finished cleanly".**

## Performance

- **Duration:** ~45 min
- **Tasks:** 2/2
- **Files modified:** 3 (1 new, 2 modified)

## Accomplishments

- `crates/devflow-core/src/release_ledger.rs` (new): `ReleaseLedger` (`ledger_version`, `status`, `version`, `tag`, `started_unix`, `updated_unix`, `head_at_completion`, `steps`), `LedgerStep`, `LedgerStatus`, `LEDGER_VERSION`, `ledger_path`, `read`, `write`, and `LedgerError`. The module doc comment is the durable statement of the contract D-06a rates *costly* to reverse: executor-only scope, live-state-wins, no compensating action, an explicit format version, and refuse-rather-than-ignore on an unrecognized one.
- The record lives at `<git rev-parse --git-common-dir>/devflow-release-ledger.json`, never in the working tree — `ledger_is_invisible_to_git_status` asserts git's real `--porcelain` answer after a real write, so the fix cannot defeat itself on its own second invocation via `DirtyWorkingTree`. `--git-common-dir` rather than `--git-dir` so a cut started in one linked worktree is the same cut resumed from another.
- `run_release` reads the ledger after the four entry guards and the fetch, and **before** version resolution, then branches three ways: **no ledger** → unchanged behavior including C-01's `StrayBaselineTag` refusal; **complete** → corroborated against a live `git rev-parse HEAD`, refusing with `LastReleaseCompleted` only when HEAD still names the recorded commit (HEAD moved ⇒ new work ⇒ a fresh computation, exactly as before); **in flight** → the version and tag are pinned from the ledger, `compute_version` is not consulted at all, and the ledger is corroborated against `version::reachable_semver_baseline` — a baseline strictly above the pinned version refuses with `LedgerContradicted` naming both facts.
- The old version-resolution block was extracted verbatim into `compute_release_version`, which is now the file's only non-test `compute_version` call site. That makes "the in-flight path never asks for a new version" a checkable source property rather than a claim.
- The ledger is written exactly once before step 1's first mutation (this single write *is* the C-02 fix — it pins the cut's identity before anything external moves), then after every step through the single `record_step` helper, and finalized with the live `HEAD` on `Completed` and `CompletedWithoutPublish` only. `HaltedAtHumanGate` deliberately leaves it in flight — that halt is the definition of mid-flight. Error paths write nothing extra; the record is already in flight with the correct version, which is exactly what the next run needs.
- `ReleaseError` gained three variants — `Ledger(#[from] LedgerError)`, `LastReleaseCompleted`, `LedgerContradicted` — and **no** variant was added to `ReleaseOutcome` or `StepStatus`, both of which `crates/devflow-cli/src/commands.rs` matches exhaustively from outside this plan's `files_modified`. `cargo test -p devflow` (230 bin + every integration target) is green, which is the proof.
- Both module doc comments that asserted the old D-06 "live-state predicate rather than a persisted progress file" contract, and the paragraph claiming a re-run after a completed release begins the next one, were rewritten to state what is now true rather than left standing as false documentation in the file where the fix lives.

## Task Commits

Each task was committed atomically, after a real RED.

1. **Task 1: The release ledger — a versioned record that refuses rather than guesses** — `5824fdb`
   - RED: the six tests were written first and run against a file containing only the test module; `cargo test -p devflow-core --lib release_ledger::` failed to compile with 20+ `cannot find` errors (`ReleaseLedger`, `LedgerStep`, `LEDGER_VERSION`, `ledger_path`, `read`, `write`, `LedgerError`) — the tests genuinely exercise the module surface rather than a stub.
   - GREEN: `cargo test -p devflow-core --lib release_ledger::` → **6 passed; 0 failed**. `ledger_is_invisible_to_git_status` and `refuses_an_unsupported_ledger_version` each **1 passed; 0 failed** under `-- --exact`. `LEDGER_VERSION` defined once, used by both writer and reader; `rg 'fn clear|remove_file'` no match; `rg 'release_ledger' lib.rs` exactly one match. clippy/fmt clean.
2. **Task 2: Resume the release that is in flight instead of starting a new one (C-02)** — `c5f7ea0`
   - RED, and it reproduced the review's own executed log exactly. Against the unfixed `run_release`, `resume_after_publish_failure_does_not_start_a_new_release` failed with: `run 2 must fail at the publish step too: ReleaseReport { version: "0.1.1", tag: "v0.1.1", steps: [VersionBump/Completed "wrote and committed version 0.1.1, pushed develop to origin", Tag/Skipped "halted at the human gate: origin/main declares 0.1.0, release version is 0.1.1"], outcome: HaltedAtHumanGate }` — run 1 cut `0.1.0` and failed at publish; run 2 computed `0.1.1`, pushed a second bump to the shared branch, and returned `Ok`. `a_completed_release_is_not_restarted_by_a_re_run` also failed RED.
   - GREEN: `cargo test -p devflow-core --lib release::` → **12 passed; 0 failed** (pre-plan count was 9). All three named tests **1 passed; 0 failed** under `-- --exact`. Every pre-existing `release::tests::` test passes with its assertions unmodified; the single exception is the one added assertion documented under Deviations.

_No plan-metadata commit in this worktree — STATE.md/ROADMAP.md are updated centrally after the wave merges._

## Files Created/Modified

- `crates/devflow-core/src/release_ledger.rs` (new) — the record, its path resolution, the atomic write, the read/refuse rules, `LedgerError`, and 6 unit tests
- `crates/devflow-core/src/lib.rs` — `pub mod release_ledger;` in its alphabetical position between `release` and `ship`
- `crates/devflow-core/src/release.rs` — module doc comment rewritten to the amended D-06a contract; three `ReleaseError` variants; ledger-pinned version resolution; `compute_release_version`, `step_status_label`, `record_step`, `finalize_ledger`; 3 new tests plus one added assertion on an existing one

## Verification Results

Real counts, from the commands themselves — not exit statuses.

| Command | Result |
|---|---|
| `cargo test -p devflow-core --lib release_ledger::` | **6 passed; 0 failed** |
| `cargo test -p devflow-core --lib release::` | **12 passed; 0 failed** (pre-plan: 9) |
| `cargo test --workspace` (devflow-core lib) | **445 passed; 0 failed** (pre-plan: 436) |
| `cargo test --workspace` (devflow bin) | **230 passed; 0 failed** (unchanged) |
| `cargo test --workspace` (all other targets) | **0 failed** across every target, including `release_execute` 8, `release_check` 11, `help_snapshot` 1 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |

Source-property checks:

- `rg -n 'compute_version' crates/devflow-core/src/release.rs` — the only non-test call site is inside `compute_release_version`, reachable from the no-ledger and complete-and-HEAD-moved arms only.
- `rg -n 'tag -d|push --force|--force-with-lease|--delete|revert|yank' release.rs release_ledger.rs` — no match (D-05).
- `rg -n 'fn clear|remove_file' release_ledger.rs` — no match.
- `rg -c 'check_ssh_signing_viability' crates/ -g '*.rs'` — exactly `crates/devflow-core/src/git.rs:2` (D-10).
- `ReleaseOutcome` still has exactly its three pre-existing variants; `StepStatus` gained no derive and no method.

## Decisions Made

See `key-decisions` in the frontmatter. In short: the C-01 non-regression was added as an assertion on the existing stray-tag test rather than a duplicated fourth test (the plan's own stated preference when the condition holds); the resume path clears the persisted step list per run rather than appending forever; `StepStatus` was kept untouched by using a free label function; and `clear`/`remove` is deliberately absent and documented as such.

## Deviations from Plan

1. **[Rule 1] The C-01 non-regression is an added assertion, not a fourth test.** The plan's `<action>` block says: "If that existing test already runs with no ledger present, extend it with an explicit assertion that no ledger was written by the refused run rather than duplicating it, and say so in a comment." The condition holds, so `refuses_a_stray_unreachable_tag_instead_of_adopting_its_version` gained `assert_eq!(release_ledger::read(root), Ok(None))` plus a comment. Its pre-existing assertions are unmodified. Consequence: the plan's artifact list names four new `release.rs` tests; three exist. `release::` went 9 → 12, not 9 → 13.
2. **[Rule 2] `ReleaseError::LedgerContradicted` has no dedicated test.** The plan's `<action>` requires the corroboration and the refusal (both implemented and exercised on every in-flight run — the *non*-contradicted branch runs in `resume_after_publish_failure_does_not_start_a_new_release`), but its `<tests>` list and acceptance criteria name no test that drives the contradicted branch, and none was added. Recorded as `status: partial` on coverage row D4 rather than claimed. Constructing the case hermetically is straightforward (plant an in-flight ledger at a version below a reachable tag) and would be a one-test follow-up; it is called out here rather than quietly added, since the plan did not ask for it.
3. **[Rule 1] The C-02 fixture needed a publish set the existing fixtures cannot provide.** The plan assumed "the existing fixtures cannot publish, so the failure arrives on its own". They cannot — `workspace_cargo_toml` deliberately has no `members` key, so `publish_order` resolves nothing and step 5 returns `CompletedWithoutPublish` rather than failing. A new `publish_failure_fixture` adds a real workspace member plus a committed `.cargo/config.toml` redirecting `[source.crates-io]` at the loopback discard port, so `cargo info` fails with a connection error that `classify_cargo_info_result` classifies as `Ambiguous` — an `Err`, so `cargo_publish` is structurally unreachable. No test contacts crates.io or attempts a real publish, and the test asserts *which* step failed so it cannot degrade into asserting an earlier failure.

## Issues Encountered

- The plan's acceptance criterion "`rg -n 'release_ledger' crates/ -g '*.rs' -l` lists exactly three files" returns **two** — `release.rs` and `lib.rs`. The third, `release_ledger.rs` itself, never spells its own module path in its source, so the grep cannot match it. This is a criterion artifact, not a scope violation: no fourth file references the module, which is the D-06a scope limit the criterion exists to check.

## User Setup Required

None. One backstop (D9: a real partial crates.io publish, resumed) remains operator-pending and is not exercisable hermetically.

## Next Phase Readiness

- 26-REVIEW.md **C-02** is closed: the two-run sequence the review reproduced by execution now ends with the first release being finished instead of a second one being started, and the executor can say which of those two situations it is in.
- This plan is core-only by construction and shares no file with 26-09 (CLI-only). `crates/devflow-cli/` was not modified.
- One follow-up worth filing: a test driving `ReleaseError::LedgerContradicted` (Deviation 2).

---
*Phase: 26-release-cut-automation*
*Completed: 2026-07-30*

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_ledger.rs`
- FOUND: `crates/devflow-core/src/lib.rs`
- FOUND: `crates/devflow-core/src/release.rs`
- FOUND: `.planning/phases/26-release-cut-automation/26-08-SUMMARY.md`
- FOUND: commit `5824fdb` (Task 1)
- FOUND: commit `c5f7ea0` (Task 2)
