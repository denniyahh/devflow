---
phase: 26-release-cut-automation
verified: 2026-07-30T03:10:00Z
status: human_needed
score: 11/11 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 6/11
  gaps_closed:
    - "`devflow release` executes the release-cut sequence (version bump -> direct push develop -> release PR -> signed tag -> sync -> publish), not just the read-only `--check` preflight."
    - "A `devflow sync` subcommand exists, both standalone and executor-internal (999.52)."
    - "A `--yes-release` flag exists, separate from `--yes-ship`, that per-invocation authorizes the bump->tag->sync->publish sequence."
    - "cargo publish primitives (pre-publish existence check, actual publish call) exist and are ready to be driven by the executor (999.25, D-04)."
    - "`push_ref`, `release_tag_state`, and `create_signed_release_tag` are consumed by production code that assembles them into the release-cut sequence, not merely exercised by their own unit tests."
  gaps_remaining: []
  regressions: []
human_verification:
  - test: "Run `devflow release --execute --yes-release` against a real repository with the operator's real `devflow.releaseSigningKey` configured (non-throwaway key) and confirm the resulting tag passes `git tag -v vX.Y.Z` against the maintainer's key."
    expected: "`git tag -v` reports a valid signature from the maintainer's real key, not the throwaway SSH keypair the hermetic tests use."
    why_human: "Requires the operator's own private signing key and a real git environment; A19/A20 and `create_signed_release_tag_produces_a_verifiable_annotated_tag` only prove the invocation form works against a throwaway key generated inside the test. No sandbox available to this verification has the operator's real key. Recorded as a `backstop` truth in 26-05-PLAN.md/26-06-PLAN.md/26-07-PLAN.md; a verifier must abstain, not pass or fail it."
  - test: "Run the executor's publish step (or `cargo publish` directly) for `devflow-core` then `devflow` against the live crates.io registry with the operator's real registry credentials, in that order."
    expected: "Both crates become live on crates.io in the correct order (`devflow-core` before `devflow`), and `crate_already_published` correctly reports `true` on any re-run."
    why_human: "A real `cargo publish` is irreversible; no test in this phase can or should perform it (D-04/D-05, `cargo_publish_reports_a_failure_without_publishing_anything` deliberately only exercises the failure path against a directory with no Cargo.toml). Recorded as a `backstop` truth in 26-05-PLAN.md/26-06-PLAN.md."
  - test: "Run `devflow sync` (or let the executor's sync step run) against the real `origin` remote and confirm the resulting push to `origin/develop` lands as a direct push rather than requiring a pull request."
    expected: "`git merge-base --is-ancestor origin/main origin/develop` succeeds immediately after the run, with no PR having been opened or merged for this step."
    why_human: "Requires the operator's own out-of-band GitHub ruleset bypass (D-01) to already be configured against the real repository — cannot be simulated against a local bare remote, which every hermetic test in `sync::tests` and `release::tests` uses instead. Recorded as a `backstop` truth in 26-04-PLAN.md/26-07-PLAN.md."
---

# Phase 26: Release-Cut Automation Verification Report

**Phase Goal:** Make `devflow release` *execute* the release-cut sequence —
version bump -> direct push to `develop` -> develop->main release PR
(human-merged) -> signed tag -> sync back to `develop` (direct push) ->
publish `devflow-core` then `devflow` — not just the read-only `--check`
preflight Phase 20's 20d delivered. Adds a real `devflow sync` subcommand
(999.52, both standalone and executor-internal) and fixes the changelog's
placeholder content (999.5) by generating it from the conventional-commit
classification Phase 25's version-bump step already computes.

**Verified:** 2026-07-30T03:10:00Z
**Status:** human_needed
**Re-verification:** Yes — gap-closure run after plans 26-04..26-07 executed against the 2026-07-29 `gaps_found` report (6/11)

## Goal Achievement

This is a re-verification of all 11 original truths against the current
codebase, not a re-statement of either the prior VERIFICATION.md or the new
SUMMARY.md files' self-reports. All 5 previously-FAILED truths were
independently re-checked with fresh `rg`/`cargo test`/`cargo run` evidence
gathered in this session; all 6 previously-VERIFIED truths were re-run as a
regression check.

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Operator explicitly re-authorized direct-push-to-develop capability (D-01/D-08) before any implementing code existed, with the selected option recorded | ✓ VERIFIED (unchanged) | `26-01-SUMMARY.md` still records `direct-push` selected, double-confirmed. Regression-checked this session. |
| 2 | Operator explicitly re-authorized unattended `cargo publish` (D-04, one-way/irreversible) before any implementing code existed, with the selected option recorded | ✓ VERIFIED (unchanged) | `26-01-SUMMARY.md` still records `automate-publish` selected, double-confirmed. Regression-checked this session. |
| 3 | A DevFlow-written CHANGELOG entry lists what actually changed, derived from the same conventional-commit classification the version bump computes (999.5), replacing the hardcoded placeholder | ✓ VERIFIED (unchanged) | `rg -n 'Released phase via DevFlow' crates/` still returns no matches; `cargo test ... hooks::tests::changelog_append_writes_the_generated_body_end_to_end -- --exact` re-run live this session: `1 passed; 0 failed`. |
| 4 | `push_ref` — a fast-forward-only, no-force push primitive — is now genuinely production-used, not just tested in isolation | ✓ VERIFIED (upgraded) | `push_ref` at `git.rs:243`, still passing its own unit tests, AND now called from `crates/devflow-core/src/release.rs:317,414,435` and `crates/devflow-core/src/sync.rs:197` — both outside any `#[cfg(test)]` block. No longer merely a tested-but-orphaned primitive (see Truth 9). |
| 5 | `release_tag_state` correctly classifies an existing tag's release state and is now genuinely production-used | ✓ VERIFIED (upgraded) | Definition unchanged at `git.rs:531-593`, still 5 passing unit tests; now called from `release.rs:401,415,436`, all inside `execute_release`, all before the `#[cfg(test)]` boundary at `release.rs:583`. |
| 6 | `create_signed_release_tag` creates the maintainer-signed release tag with the documented invocation form and is now genuinely production-used | ✓ VERIFIED (upgraded) | Definition unchanged at `git.rs:698`, still 3 passing unit tests; now called from `release.rs:413`, inside `execute_release`'s `Absent` branch, before the test-module boundary. |
| 7 | `devflow release` *executes* the release-cut sequence, not just the read-only `--check` preflight (the phase's headline goal) | ✓ VERIFIED (gap closed) | `rg -n 'not yet built\|DEN-50' crates/devflow-cli/src/main.rs` — no matches. The dispatch arm at `main.rs:602-632` now routes `check`/`execute`/`yes_release` explicitly; `execute && yes_release` calls `release_execute`, which calls `devflow_core::release::execute_release`. Confirmed live: `cargo run -q -p devflow -- release --help` lists both `--execute` and `--yes-release` with accurate descriptions. `execute_reaches_the_core_executor_and_refuses_off_develop` passes (`1 passed; 0 failed`), proving the CLI reaches the real executor, not a stub. |
| 8 | A `devflow sync` subcommand exists, both standalone and executor-internal (999.52) | ✓ VERIFIED (gap closed) | `crates/devflow-core/src/sync.rs` exists; `lib.rs:77` declares `pub mod sync;`; `main.rs:638` has `Command::Sync { project } => sync_cmd(...)`; `commands.rs:2241` defines `sync_cmd`. Confirmed live: `cargo run -q -p devflow -- sync --help` exits 0 with accurate help text. The same `sync_main_to_develop` is also called from `release.rs:484` (D-07's second entry point) — one implementation, two callers, as designed. |
| 9 | `push_ref`, `release_tag_state`, `create_signed_release_tag` are wired into a production caller (the release executor), not only exercised by their own tests | ✓ VERIFIED (gap closed) | All three symbols now have call sites in `release.rs` at lines 317/414/435 (`push_ref`), 401/415/436 (`release_tag_state`), and 413 (`create_signed_release_tag`) — every one of these line numbers is above `release.rs`'s `#[cfg(test)]` boundary at line 583, i.e. inside `execute_release` or a helper it calls directly, not inside the test module. This is the exact defect the prior verification flagged as "exactly as production-uncalled as `GitFlow::push`/`delete_remote_branch` were before this phase" — it is now closed. |
| 10 | A `--yes-release` flag exists and is required per-invocation to authorize the automated sequence | ✓ VERIFIED (gap closed) | `main.rs` declares `yes_release: bool` on `Command::Release` (line 255) and reads it only in the dispatch arm (line 605, 620). `execute && !yes_release` is rejected before any handler runs. `yes_release_is_not_settable_via_config_or_env` (writes a real `devflow.toml` key AND two env vars, still rejects) passes; `yes_release_has_no_config_state_or_env_surface` (source-grep over `state.rs`/`config.rs`/`config_parse.rs`) passes. `--yes-release` never aliases or implies `--yes-ship` (`rg -n 'yes_ship' crates/devflow-cli/src/main.rs` shows it only on `Command::Start`). |
| 11 | `cargo publish` primitives (existence check, publish call) exist for the executor's final step (999.25, D-04) | ✓ VERIFIED (gap closed) | `PublishCheck`, `PublishError`, `classify_cargo_info_result`, `crate_already_published`, `cargo_publish` all defined in `git.rs` (lines 889/949/920/977/1010). `git::tests::publish_check_classifies_exit_codes` and 3 other publish tests pass. Both primitives are called from `release.rs:522,533` inside `execute_release`'s step 5, gated correctly: `Ok(true)` skips, `Ok(false)` calls `cargo_publish`, any `Err` (including `Ambiguous`) propagates and stops the run before publishing anything further. |

**Score:** 11/11 truths verified (0 present-but-behavior-unverified)

All 5 previously-FAILED truths (7-11) are closed. All 6 previously-VERIFIED
truths hold on regression. **Three additional `backstop`-tier truths**,
explicitly declared as such in 26-05/26-06/26-07's plan frontmatter
(a real signed tag verifying against the operator's real key; a real `cargo
publish` of both crates; a real direct push to the live `origin`), remain
**operator-pending** — see Human Verification Required below. These are not
counted as failed (the code paths now exist, are wired, and are proven
against hermetic fixtures) and not counted as verified (no hermetic test can
or should exercise an irreversible/credentialed live operation) — this
verifier abstains on them per the honest-verifier backstop convention, and
they are the reason the overall status is `human_needed` rather than
`passed`.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/sync.rs` | `SyncError`, `SyncOutcome`, `SYNC_MERGE_MESSAGE`, `sync_main_to_develop` | ✓ VERIFIED | Present, substantive, wired to both `Command::Sync` and `execute_release`'s step 4. 5/5 own tests pass. |
| `crates/devflow-core/src/release.rs` | `ReleaseStep`, `StepStatus`, `StepReport`, `ReleaseOutcome`, `ReleaseReport`, `ReleaseError`, `execute_release` | ✓ VERIFIED | Present, substantive (1151 lines incl. tests), wired to `commands::release_execute`. 8/8 own tests pass. |
| `crates/devflow-core/src/git.rs` — `PublishCheck`/`PublishError`/`classify_cargo_info_result`/`crate_already_published`/`cargo_publish` | cargo publish primitives (999.25) | ✓ VERIFIED | All 5 present and substantive; 4 own tests pass; called from `release.rs` step 5. |
| `crates/devflow-cli/src/main.rs` `Command::Sync`, `Command::Release{execute,yes_release}` | new CLI surface | ✓ VERIFIED | Both present; confirmed live via `cargo run -- release --help` / `-- sync --help`; help snapshot regenerated and passing. |
| `crates/devflow-cli/src/commands.rs` `sync_cmd`, `release_execute` | command handlers | ✓ VERIFIED | Both present, call the correct core functions, render `StepReport`/`ReleaseOutcome` correctly. |
| `crates/devflow-cli/tests/release_execute.rs` | integration tests for `--execute`/`--yes-release` | ✓ VERIFIED | Present, 6/6 tests pass, drives the real binary (`CARGO_BIN_EXE_devflow`). |
| `crates/devflow-core/src/version.rs` `version_in_contents` | text-based version parsing for the D-02 human-gate boundary | ✓ VERIFIED | Present; `read_version` now delegates to it; `version::tests::read_version_*` regression tests pass. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `hooks.rs::version_bump` | `ctx.shipped_changelog_body` | direct field assignment before `git.tag()` | ✓ WIRED | Unchanged from initial verification; regression-confirmed. |
| `devflow sync` (standalone) | `sync_main_to_develop` | `commands::sync_cmd` calls it directly | ✓ WIRED | Confirmed via `cargo run -- sync --help` and `sync_cmd`'s source. |
| `execute_release` step 4 | `sync_main_to_develop` | direct call, no options passed, D-07's second entry point | ✓ WIRED | `rg -n 'sync_main_to_develop' release.rs` — exactly one call site, no `"-X"` re-implementation in `release.rs`. |
| `execute_release` step 1 | `GitFlow::push_ref` | direct call for the version-bump push | ✓ WIRED | `release.rs:317`, before `#[cfg(test)]`. |
| `execute_release` step 3 | `release_tag_state` / `create_signed_release_tag` | direct calls, branching on all 5 `ReleaseTagState` variants | ✓ WIRED | `release.rs:401-478`, before `#[cfg(test)]`. |
| `execute_release` step 5 | `crate_already_published` / `cargo_publish` | per-package gated call, iterating `publish_order`'s own sequence | ✓ WIRED | `release.rs:511-545`; `publish_order` consulted exactly once, never sorted/reordered (`rg -n '\.sort' release.rs` — no match). |
| `main.rs` dispatch arm | `commands::release_execute` | `execute && yes_release` branch only | ✓ WIRED | `main.rs:602-635`; every other flag combination returns an `Err` before `project_root` is touched. |
| `devflow release --execute` (no `--yes-release`) | rejection | typed `CliError::Message` naming the flag | ✓ WIRED | Confirmed by `execute_without_yes_release_is_rejected` and live inspection of the dispatch arm. |

### Data-Flow Trace (Level 4)

Not applicable in the usual sense — this phase's artifacts are a CLI/git/registry
orchestration layer, not a UI rendering dynamic data from a store. The
equivalent check performed instead: every step of `execute_release` reads
*live* git/registry state (not a cached or persisted progress file) via real
subprocess calls (`git status`, `git rev-parse`, `git show origin/main:<path>`,
`cargo info`) rather than a static or hardcoded value — confirmed by reading
each step's source and by the fact that `skips_push_when_already_ahead`,
`skips_tag_when_already_released`, and the publish step's `Ok(true)` path all
produce different `Skipped`/`Completed` outcomes depending on *actual* repo
state constructed differently by each test's fixture, not a fixed return
value. `rg -n 'release-state|release_state\.json' crates/ -g '*.rs'` —
no match, confirming no persisted progress file exists (D-06).

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| `devflow release` no longer hard-rejects a bare invocation with the DEN-50 message | `rg -n 'not yet built\|DEN-50' crates/devflow-cli/src/main.rs` | no matches | ✓ PASS |
| `devflow release --help` documents `--execute` and `--yes-release` on the real binary | `cargo run -q -p devflow -- release --help` | exit 0, both flags listed with accurate descriptions | ✓ PASS |
| `devflow sync --help` exists on the real binary | `cargo run -q -p devflow -- sync --help` | exit 0, names `[PROJECT]` | ✓ PASS |
| `devflow --help` snapshot matches the binary | `cargo test -p devflow --test help_snapshot` | `1 passed; 0 failed` | ✓ PASS |
| The full release-cut sequence completes end to end against a real local bare remote | `cargo test --workspace --features devflow-core/test-support --lib release::tests::completes_the_sequence_and_reports_every_step -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| A tree-changing sync stops the run before any publish step | `cargo test ... release::tests::a_refused_sync_stops_the_run_before_publishing -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| A mid-sequence failure (missing signing key) leaves the prior step landed, no rollback | `cargo test ... release::tests::partial_failure_leaves_prior_steps_landed -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| CLI wiring proof: the executor's own refusal (off-`develop`) reaches the CLI unmutated | `cargo test -p devflow --test release_execute execute_reaches_the_core_executor_and_refuses_off_develop -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| `--yes-release` cannot be supplied via `devflow.toml` or environment variables (B10) | `cargo test -p devflow --test release_execute yes_release_is_not_settable_via_config_or_env -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| Full lib test suite, workspace-wide | `cargo test --workspace --features devflow-core/test-support --lib` | `434 passed; 0 failed` | ✓ PASS |
| Full CLI test suite, all targets | `cargo test -p devflow` | `0 failed` across all 16 test binaries | ✓ PASS |
| Lint and format gates | `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --check` | both clean | ✓ PASS |
| D-02 guard: no new `gh` subprocess call sites | `rg -n 'Command::new("gh")' crates/ -g '*.rs'` | exactly one file, `crates/devflow-cli/src/preflight.rs` | ✓ PASS |
| D-10 guard: signing-viability predictor gained no new caller | `rg -c 'check_ssh_signing_viability' crates/ -g '*.rs'` | exactly `crates/devflow-core/src/git.rs:2`, unchanged from 26-03's baseline | ✓ PASS |
| No force flags anywhere in the new release/sync code | `rg -n '"--force' release.rs sync.rs`; `rg -n '"--dry-run"' git.rs` | no matches | ✓ PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes are declared by any plan in this
phase, and none exist in the repository's conventional probe locations for
this feature area. Skipped per Step 7c's discovery contract — visible skip,
not a silent one.

### Requirements Coverage

This project has **no REQ-IDs** (`.planning/REQUIREMENTS.md` does not exist
in this repository). Phase 26 is tracked exclusively by backlog identifiers.
Recording this absence explicitly per the task instructions, not fabricating
REQ-ID traceability.

| Backlog ID | Source Plan(s) | Description | Status | Evidence |
|------------|-----------------|-------------|--------|----------|
| 999.25 | 26-01, 26-03, 26-05, 26-06, 26-07 | Release-cut executor: `devflow release` executes bump->push->PR->tag->sync->publish | ✓ SATISFIED (with operator-pending backstop items) | All primitives, the executor, and the CLI surface exist, are wired, and pass 22+ hermetic tests. Two backstop truths (real signed tag verification, real `cargo publish`) remain operator-pending by design — see Human Verification Required. `26-05-SUMMARY.md`/`26-06-SUMMARY.md`/`26-07-SUMMARY.md`'s `requirements-completed: ["999.25"]` is now an accurate claim, unlike the corrected `26-01`/`26-03` overclaims from the prior verification pass. |
| 999.5 | 26-02 | Changelog placeholder content, generated from conventional-commit classification | ✓ SATISFIED (unchanged) | Fully implemented, tested end-to-end, hardcoded placeholder confirmed still removed. |
| 999.52 | 26-04, 26-06 | `devflow sync` subcommand, standalone and executor-internal | ✓ SATISFIED (with one operator-pending backstop item) | `devflow sync` exists as a real, tested CLI subcommand and as the executor's step 4, sharing one implementation (D-07). One backstop truth (a real direct push landing against the live `origin`) remains operator-pending. |

**Documentation lag, not a functional gap:** `999.25-BACKLOG-DOSSIER.md` and
`999.5-BACKLOG-DOSSIER.md` frontmatter both still read `status: backlog`
despite both backlog items now being functionally delivered per the
codebase evidence above (999.5 was already noted as this same lag in the
prior verification). Not blocking; a housekeeping item for whoever next
touches those dossiers.

**Orphaned requirements check:** ROADMAP.md's Phase 26 section names only
999.25, 999.5, and 999.52 in scope. No orphaned requirements found.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-core/src/release.rs` | 267-274, 341-345 | `execute_release` pushes two `StepReport`s labeled `VersionBump` on every resumed run (the `UnreachableBaseline` resume arm's own report, then the ordinary step-1 recompute's report) | ⚠️ Warning (real bug, code-review-confirmed WR-01) | Independently confirmed by reading the source: the resume arm at lines 267-274 pushes a `Completed` `StepReport` and falls through unconditionally into the ordinary step-1 block, which pushes a second `VersionBump` report a few lines later. `completes_the_sequence_and_reports_every_step`'s one-entry-per-`ReleaseStep` assertion only passes because that specific test's fixture never triggers the `UnreachableBaseline` path — `skips_tag_when_already_released` and `refuses_a_stray_lightweight_tag_rather_than_skipping` DO traverse it (per 26-06-SUMMARY.md's own "Decisions Made" note) but neither asserts on `report.steps` length/ordering, so the duplication ships untested. This is a reporting/UX defect only — no extra git command runs, and no truth in this phase's must-haves is falsified by it — but it is a genuine bug an operator would see (a doubled line in the printed step table on every resumed release) and should be fixed, not treated as done. Not a phase-blocker per 26-REVIEW.md's own assessment, which this verifier concurs with after independently reading the code. |
| `crates/devflow-cli/src/commands.rs` | 2241-2252 | `devflow sync` performs a real direct push to `origin/develop` with no `--yes-*`-style authorization flag at all, unlike `devflow release --execute` (requires `--yes-release`) and `devflow start`'s auto-Ship path (requires `--yes-ship`) | ℹ️ Info (design asymmetry, code-review-confirmed WR-03) | The D-01/D-08 operator authorization already covers direct pushes to `develop` generically (recorded in `26-01-SUMMARY.md`), and the merge itself is independently tree-verified before the push (`TreeChanged` refuses). Not a must-have violation — no plan's frontmatter declares a `--yes-sync` requirement — but worth flagging for a documentation follow-up per 26-REVIEW.md's own recommendation, so a future reader doesn't assume the omission is accidental. |
| `999.25-BACKLOG-DOSSIER.md`, `999.5-BACKLOG-DOSSIER.md` | frontmatter | `status: backlog` despite both items now functionally delivered | ℹ️ Info | Documentation lag, not a functional gap (same disposition the prior verification gave 999.5's dossier). |
| N/A | N/A | No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any of the phase's modified/created files (`release.rs`, `sync.rs`, `git.rs`, `version.rs`, `main.rs`, `commands.rs`, `release_execute.rs`) | ℹ️ Info | Confirmed by direct grep this session. |

`26-REVIEW.md` (code review, standard depth, re-scoped to 13 files covering
the full phase diff) independently found 0 critical, 7 warning, 4 info
findings. On its primary question — can any code path reach a `git push`,
`git tag`, or `cargo publish` without the typed `--yes-release` flag — its
answer, which this verifier's independent tracing (Truths 9-11, Key Link
Verification above) corroborates, is **no**. WR-02 (a strict-descendant tag
match slightly weaker than exact-equality, unreachable in this codebase's
single-writer flow), WR-04 (fragile stderr substring matching in the publish
classifier, already documented-by-observation as its own known limitation),
WR-05/WR-06/WR-07 (pre-existing-file issues in `pipeline_outcomes.rs`,
`hooks.rs`'s Merge event, and TOML comment-line scanning) are real findings
but are either pre-existing-file issues incidentally touched by this phase
(WR-05/06/07) or edge cases the review itself characterizes as unreachable
in this project's actual single-writer, single-operator usage pattern
(WR-02, WR-04) — none of them falsify any of this phase's 11 must-have
truths, and none is treated as phase-blocking here, consistent with the
review's own disposition.

### Human Verification Required

Three items — all explicitly declared as `verification: backstop` truths in
26-05-PLAN.md, 26-06-PLAN.md, and 26-07-PLAN.md's frontmatter, and all listed
in `26-VALIDATION.md`'s "Manual-Only Verifications" table. Per this task's
instructions, these are reported as operator-pending rather than passed or
failed — the code paths that would exercise them are now built, wired, and
proven against hermetic fixtures, but the operations themselves are
irreversible/credentialed and cannot be exercised in any test.

### 1. Real signed tag verifies against the operator's real key

**Test:** Run `devflow release --execute --yes-release` against a real
repository with the operator's real `devflow.releaseSigningKey` configured,
and inspect the resulting tag.
**Expected:** `git tag -v vX.Y.Z` reports a valid signature from the
maintainer's actual signing key.
**Why human:** Requires the operator's own private key; every hermetic test
in this phase (A19, A20, `skips_tag_when_already_released`, etc.) uses a
throwaway repo-local SSH keypair instead.

### 2. Real `cargo publish` of both crates in order

**Test:** Observe the executor's publish step (or run `cargo publish`
directly) against the live crates.io registry for `devflow-core` then
`devflow`.
**Expected:** Both crates become live in that order; a re-run's
`crate_already_published` check correctly reports `true` for both.
**Why human:** Irreversible; no test performs or should perform a real
publish (D-04/D-05). `cargo_publish_reports_a_failure_without_publishing_anything`
deliberately only exercises the failure path.

### 3. Real direct push lands against `origin/develop`

**Test:** Run `devflow sync` (or let the executor's sync step run) against
the real `origin` and confirm the push lands directly, without a PR.
**Expected:** `git merge-base --is-ancestor origin/main origin/develop`
succeeds immediately afterward, with no PR opened or merged for this step.
**Why human:** Requires the operator's own out-of-band GitHub ruleset bypass
(D-01) already configured against the real repository; every hermetic test
uses a local bare remote instead.

### Gaps Summary

**No gaps remain.** All 5 truths the prior verification (2026-07-29) marked
FAILED — the headline `devflow release` executor, the `devflow sync`
subcommand, the three previously-orphaned git primitives gaining production
callers, the `--yes-release` authorization flag, and the `cargo publish`
primitives — are independently confirmed present, substantive, and wired in
this session, not merely claimed by the SUMMARY.md files. All 6 previously
`VERIFIED` truths hold on regression. The full workspace test suite (434
lib tests + 0 failed across every CLI integration-test binary), clippy, and
fmt are all clean, matching this session's own independent re-run rather
than the executing agents' self-report.

One reproducible bug was found by independent code reading (WR-01: a
duplicated `VersionBump` step report on the `UnreachableBaseline` resume
path) — real, but cosmetic (no incorrect git/cargo command runs), and does
not falsify any of the 11 must-have truths. It is recorded above as a
warning-level anti-pattern rather than a gap, matching 26-REVIEW.md's own
"reporting/UX defect, not a safety defect" characterization, which this
verifier's independent source read confirms.

The overall status is `human_needed`, not `passed`, solely because of the
three `backstop`-tier truths (real signing key, real `cargo publish`, real
direct push against the live `origin`) that 26-05/26-06/26-07's own plan
frontmatter correctly scoped as operator-pending and unable to be exercised
by any hermetic test. This is not a defect in the phase's delivery — it is
the honest, by-design boundary of what automated verification can prove
about three irreversible/credentialed operations. The recommended next step
is an operator-run `devflow release --execute --yes-release` against the
real repository (or a controlled first real release), observing the three
items in Human Verification Required, per `26-VALIDATION.md`'s own
"Manual-Only Verifications" table.

Note: one intermittent, unreproduced `devflow-core` lib-suite failure (433
passed / 1 failed) was observed once during heavy full-suite contention in
prior investigation and did not recur across 9 subsequent runs, nor during
this session's own full-suite run (434 passed / 0 failed, twice). Recorded
as a flakiness risk to watch, not a blocking gap.

---

*Verified: 2026-07-30T03:10:00Z*
*Verifier: Claude (gsd-verifier)*
