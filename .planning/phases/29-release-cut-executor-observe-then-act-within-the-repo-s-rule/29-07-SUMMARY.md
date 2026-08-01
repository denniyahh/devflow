---
phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
plan: 07
subsystem: release-automation
tags: [rust, git, cargo, crates.io, signing, release]

# Dependency graph
requires:
  - phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
    provides: "29-04's release_execute::{StepOutcome, CutReport, ReleaseStep, action_for, unit_for, pr_refs, cut/cut_with} scaffolding; 29-05/29-06's four PR-backed actions (bump_and_changelog_pr, release_pr_to_main, sync_back_pr) — all leaving SignedTagPresent/CratesPublished as the last two None arms; release_observe::{classify_http_status, crate_version_http_status} and git::{git_command, hermetic_command, publish_order, git_config}"
provides:
  - "devflow_core::release_publish::{TagPlan, plan_local_tag, tag_argv, create_and_push_tag} — the signed-tag commit point: observes the local tag namespace's four collision branches before running any tag command, runs the exact CONTRIBUTING.md-documented git tag -s form, reports git's own exit code and stderr verbatim"
  - "devflow_core::release_publish::{publish_plan, publish_all} — the crates.io publish commit point: consumes git::publish_order's computed order exactly, observes each crate's publish state before and after publishing, skips what's done, refuses on unreachable, stops on failure with no rollback, bounded re-observation between crates"
  - "action_for(ReleaseStep::SignedTagPresent) -> sign_release_tag and action_for(ReleaseStep::CratesPublished) -> publish_crates — every ReleaseStep variant now carries a real action; devflow release cut can walk end to end"
  - "git::git_config and release_observe::crate_version_http_status widened to pub(crate) for reuse"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Presence-not-verification for local signature classification: plan_local_tag inspects the tag object's own body text for a signature-block marker (BEGIN SSH SIGNATURE / BEGIN PGP SIGNATURE) rather than cryptographically verifying it — verification needs the signer's public key resolvable in whatever environment this runs in, and its absence is a local tooling gap, not evidence about the tag; the pre-push hook's fingerprint comparison remains the sole authority on whose signature it is"
    - "Injectable publish core: publish_members takes observe/publish as function parameters (mirroring cut_with's injected observer), so skip-what's-done, stop-on-unreachable, stop-on-failure-with-no-rollback, the bounded re-observation window, and order-is-consumed-not-corrected are all unit-tested with zero network dependency"
    - "No second implementation of an authoritative judgment: the tag command form is CONTRIBUTING.md's own documented invocation, the publish order is git::publish_order's own computed sequence, and the per-crate publish oracle is release_observe's own crates.io query — release_publish reimplements none of them"

key-files:
  created:
    - crates/devflow-core/src/release_publish.rs
  modified:
    - crates/devflow-core/src/release_execute.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/release_observe.rs
    - crates/devflow-cli/tests/release_cut.rs
    - .planning/phases/29-release-cut-executor-observe-then-act-within-the-repo-s-rule/29-VALIDATION.md

key-decisions:
  - "Operator resolution of Task 1 (checkpoint:decision), recorded verbatim: build unit 29c NOW, in this phase — not deferred to a follow-on phase. Given explicitly this session with full knowledge that this plan produces the signed-tag and crates.io-publish code. The flagged (not-yet-ruled-on) 'review as primary gate, one automated fix round maximum' proposal from ROADMAP.md lines 2181-2191 was NOT invoked as binding — the operator did not say so in the same reply, and per the plan's own instruction it is not encoded as a constraint anywhere in this plan set absent that explicit statement."
  - "Committed Task 2 (signed tag) and Task 3 (publishes) as a single atomic commit, matching 29-05/29-06's precedent for this identical file: action_for's exhaustive match over ReleaseStep is edited by both tasks in the same hunk (the SignedTagPresent and CratesPublished arms land together), and the accompanying test-list update (the old action_for_returns_none_for_every_step_without_an_action_in_this_build test) also needed both steps removed in the same edit. No design or scope change to either task."
  - "plan_local_tag always peels to the tag's target commit via `<tag>^{commit}` regardless of annotated/lightweight, then compares that single commit against target_commit before branching on annotated-vs-lightweight — simpler than conditionally re-deriving the commit per branch, and it means the Refuse (different-commit) check runs identically for both tag shapes."
  - "sign_release_tag (release_execute.rs) resolves the signed tag's target commit by fetching and rev-parsing origin/main fresh on every call, rather than accepting a caller-supplied commit — this is what makes the action re-runnable and keeps it consistent with the observe-fresh-every-time design; the release PR step (ReleasePrMerged) is what actually produced that commit, and this action re-derives it rather than threading it through CutReport."
  - "git::git_config and release_observe::crate_version_http_status widened from private to pub(crate) rather than reimplemented — the same widen-don't-duplicate pattern 29-05 established for version::parse_version_str and hooks::today()."
  - "Task 4 review finding (Phase 26's CR-03 class — publish_plan accepted a silently truncated publish set): git::workspace_member_paths and git::package_name ALSO widened from private to pub(crate), so publish_plan can detect when git::publish_order's own per-member 'unreadable manifest -> skip' behavior has silently dropped a declared member, without a second TOML-members-scan implementation. git::publish_order's own behavior is unchanged — the fix is entirely in publish_plan's post-check."

patterns-established:
  - "publish_members(project_root, version, members, reobserve_attempts, reobserve_interval, observe, publish): the injectable core behind publish_all, with the bound and interval also parameterized so tests exercise the bounded-reobservation-exhausted path without any real wall-clock wait."

requirements-completed: [29c]

coverage:
  - id: D1
    description: "The signed tag is created by the literal command form CONTRIBUTING.md documents — explicit key selection, -s, the target commit, and a message — and the outcome is git's own exit code and stderr, never a DevFlow judgment about whether signing would work"
    requirement: "29c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_publish.rs#release_publish::tests::tag_command_form_with_explicit_key_matches_contributing_md, ::tag_command_form_without_key_omits_override, ::tag_command_form_force_inserts_flag_only, ::tag_command_failure_surfaces_gits_stderr_verbatim"
        status: pass
    human_judgment: false
  - id: D2
    description: "A pre-existing local v<version> tag is observed and classified before any tag command runs: a lightweight or unsigned duplicate may be replaced, a correctly signed tag at the target commit is left alone, and a tag pointing at a different commit is refused"
    requirement: "29c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_publish.rs#release_publish::tests::tag_namespace_collision_no_existing_tag_yields_create_new, ::tag_namespace_collision_lightweight_at_target_commit_yields_replace_local, ::tag_namespace_collision_annotated_without_a_signature_block_at_target_commit_yields_replace_local, ::tag_namespace_collision_annotated_with_a_signature_block_at_target_commit_yields_leave_alone, ::tag_namespace_collision_different_commit_yields_refuse_naming_both_commits"
        status: pass
    human_judgment: false
  - id: D3
    description: "Publishing iterates the exact sequence git::publish_order returns, in that order, with no re-sorting and no crate name written as a literal anywhere"
    requirement: "29c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_publish.rs#release_publish::tests::publish_order_respected_follows_the_given_order_without_resorting, ::publish_order_respected_a_reversed_order_is_followed_not_corrected, ::publish_plan_follows_git_publish_order_dependency_before_dependent"
        status: pass
      - kind: other
        ref: "rg -n 'devflow-core' crates/devflow-core/src/release_publish.rs | rg -v '^\\s*[0-9]+:\\s*//' (zero matches outside a doc-comment path reference)"
        status: pass
    human_judgment: false
  - id: D8
    description: "publish_plan refuses a silently truncated publish set — a workspace member whose manifest git::publish_order could not read is never allowed to disappear from the plan unnoticed; the refusal names the specific declared path(s) missing"
    requirement: "29c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_publish.rs#release_publish::tests::publish_plan_refuses_a_silently_truncated_publish_set_naming_the_missing_path (RED confirmed pre-fix — returned [\"a\"] instead of refusing), ::publish_plan_control_returns_every_declared_member_when_all_manifests_are_readable"
        status: pass
    human_judgment: false
  - id: D4
    description: "Each crate's publish state is observed before publishing it: already published is skipped, not published is published, and unreachable stops the run"
    requirement: "29c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_publish.rs#release_publish::tests::publish_members_skips_a_crate_already_observed_published, ::publish_members_publishes_a_crate_observed_not_published, ::publish_members_stops_the_run_when_a_per_crate_observation_is_unreachable_and_publishes_nothing, ::publish_members_stops_on_a_failed_publish_and_carries_cargos_stderr_and_attempts_no_later_crate"
        status: pass
    human_judgment: false
  - id: D5
    description: "After each publish, availability is re-observed against the crates.io JSON API with a bounded wait before the next crate is attempted; exhausting the bound stops the run rather than proceeding"
    requirement: "29c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_publish.rs#release_publish::tests::publish_members_bounded_reobservation_returns_an_error_naming_the_crate_and_bound_when_availability_never_appears, ::publish_members_observes_every_member_even_when_all_are_already_published"
        status: pass
    human_judgment: false
  - id: D6
    description: "Every step in this unit reports the real tool's real result — git's exit code and stderr, cargo's exit code — and no step predicts an outcome before attempting it; the shipped executor still requires no human at the keyboard"
    requirement: "29c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_publish.rs#release_publish::tests::tag_command_failure_surfaces_gits_stderr_verbatim, ::tag_push_failure_surfaces_gits_stderr_verbatim, ::create_and_push_tag_refuses_before_running_any_command_on_a_different_commit_collision"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_cut.rs#release_cut_runs_unattended_with_stdin_closed, #cut_reaches_every_step_when_all_are_implemented"
        status: pass
      - kind: other
        ref: "cargo test --workspace (576 devflow-core lib tests + 249 devflow-cli tests, 0 failed); cargo clippy --workspace --all-targets -- -D warnings (clean); cargo fmt --check (clean); devflow release status 2.2.0 . (live, read-only — 5/6, signed tag correctly absent)"
        status: pass
    human_judgment: false
  - id: D7
    description: "The complete unit-29c diff was reviewed line by line by a human before merge, and the review — not the test suite — is the gate that was applied to it"
    requirement: "29c"
    verification: []
    human_judgment: true
    rationale: "Task 4's checkpoint:human-verify gate (gate=\"blocking\"). PERFORMED: the human reviewer read the complete diff and returned one finding (D8, the truncated-publish-set gap) plus explicit confirmation of six other properties as independently verified correct (Unreachable-before-publish, Err-on-every-failure-path, no production-code progress record, unsorted publish order with no crate-name literal, injected observe/publish for network-free testability, and the git_config pub(crate) widening). Verdict, verbatim: 'one finding, fix it, then the checkpoint is approved.' The finding was fixed (commit aa33b29, TDD RED-then-GREEN), re-verified (workspace suite/clippy/fmt all clean, unattended-stdin-closed still green), and is recorded as approved on that basis — this is a human-rendered verdict, not a self-assessment."

# Metrics
duration: ~1h 55min
completed: 2026-07-31
status: complete
---

# Phase 29 Plan 07: The Commit Point — Signed Tag and Crates.io Publish Summary

**`devflow_core::release_publish` (new): `plan_local_tag`/`tag_argv`/`create_and_push_tag` for the signed release tag, `publish_plan`/`publish_all` for the two crates.io publishes in `publish_order`'s computed sequence — wired into `release_execute::action_for`'s last two arms, so `devflow release cut` now walks all six release-cut steps end to end. Human-reviewed (Task 4): one finding (a Phase-26-CR-03-class silently-truncated publish set), fixed and re-verified; approved.**

## Performance

- **Duration:** ~1h 55min (implementation ~1h 10min; Task 4 review + one fix round ~45min)
- **Completed:** 2026-07-31
- **Tasks:** Task 1 (checkpoint:decision) resolved by explicit operator instruction, no code; Task 2 (signed tag) and Task 3 (publishes) implemented, tested, and committed together; Task 4 (checkpoint:human-verify) — reviewed by a human, one finding, fixed in a single round, approved
- **Files modified:** 8 (1 created, 7 modified — `git.rs` touched twice, across the implementation commit and the review-fix commit)

## Accomplishments

- **Unit 29c is code-complete: `devflow release cut` no longer stops at "no action in this build."** `action_for(ReleaseStep::SignedTagPresent)` and `action_for(ReleaseStep::CratesPublished)` both now resolve to real actions — every one of the six `ReleaseStep` variants carries an action for the first time in this phase.
- **The signed tag is the exact command CONTRIBUTING.md documents, run for real, with no signing-viability prediction anywhere.** `tag_argv` reproduces the documented `-c user.signingkey=... tag -s <name> <commit> -m <name>` form byte-for-byte (proven by a test asserting the joined argument string); `create_and_push_tag` never calls `git.rs`'s existing pre-flight signing helper — the command runs, and git's own exit code and stderr are the report.
- **The local tag-namespace collision — `hooks_after_ship`'s per-Ship lightweight tag versus the release's signed tag sharing a name — is observed and classified before any tag command runs.** `plan_local_tag`'s four branches (`CreateNew`, `ReplaceLocal`, `LeaveAlone`, `Refuse`) are each fixture-tested against a real local git repository, including a fixture that creates the collision exactly as the phase-Ship hook does (via `GitFlow::tag`), and a fixture proving a tag at a *different* commit is refused with both commit ids named — never silently retargeted.
- **The publishes consume `git::publish_order`'s computed sequence exactly — never re-sorted, never a hardcoded crate name.** Two tests feed deliberately reversed member orders and assert the publish loop follows them exactly; a source-level check (`rg` for the literal string `devflow-core`) confirms no crate name appears anywhere in this module's production code.
- **Every irreversible step is independently re-runnable — no progress record of any kind.** Both `plan_local_tag` and the per-crate crates.io observation are freshly evaluated on every call; `publish_members`' injectable design let every stop condition — unreachable-before-publish, failed-publish-with-no-rollback, bounded-reobservation-exhaustion — be proven with zero network dependency.
- **`cut_reaches_every_step_when_all_are_implemented`** (replacing `29-06`'s now-obsolete `cut_walks_to_the_signed_tag_step_and_stops`) proves the walk genuinely *attempts* the signed-tag step in an isolated-signing-key CLI fixture — it fails for a real git reason (no secret key resolvable), not because the arm was unimplemented — while `release_cut_runs_unattended_with_stdin_closed` stays green, confirming RD-3 (no operator-presence requirement) still holds.
- **`devflow release status 2.2.0 .`**, re-run live against this repository (read-only, no side effects), still reports the exact motivating-incident state: both crates published, signed tag correctly absent.
- **Task 4's human review found and closed the exact defect class Phase 26 was retired for.** `publish_plan` previously guarded only against an *empty* `publish_order` result, never a *short* one — `git::publish_order` silently skips any declared workspace member whose manifest cannot be read, so a two-member workspace readable in only one place would have returned a valid-looking one-crate plan and silently never published the second. `publish_plan` now re-derives the workspace's own declared member paths (reusing `git::workspace_member_paths`/`git::package_name`, both newly `pub(crate)` — never a second parser) and refuses, naming the missing path(s), when fewer members made it into the order than were declared. See "Review Finding and Resolution" below.

## Task Commits

1. **Task 1: Decide whether to build the commit point now (checkpoint:decision)** — resolved by explicit operator instruction this session, recorded above under Key Decisions. No code artifact; no commit.
2. **Task 2 + Task 3 (committed together — see Deviations): the signed tag; the publishes**
   - `bc2b889` `feat(29-07): the commit point — signed tag and crates.io publish` — `release_publish.rs` (new: `TagPlan`, `plan_local_tag`, `tag_argv`, `create_and_push_tag`, `publish_plan`, `publish_all`, `publish_members`, 21 tests), `release_execute.rs` (`sign_release_tag`/`publish_crates` wired into `action_for`, module doc updated, two tests updated for the new wiring), `lib.rs` (`pub mod release_publish;`), `git.rs` (`git_config` → `pub(crate)`), `release_observe.rs` (`crate_version_http_status` → `pub(crate)`), `release_cut.rs` (`cut_reaches_every_step_when_all_are_implemented` replacing the obsolete `cut_walks_to_the_signed_tag_step_and_stops`), `29-VALIDATION.md` (Manual-Only Verifications row updated for unit 29c)
3. **Task 4: Line-by-line review of the complete unit-29c diff (checkpoint:human-verify) — reviewed, one finding, fixed in one round, approved.**
   - `aa33b29` `fix(29-07): publish_plan refuses a silently truncated publish set` — `release_publish.rs` (`publish_plan` rewritten with the declared-vs-returned-members check, 2 new tests), `git.rs` (`workspace_member_paths`/`package_name` → `pub(crate)`)

## Review Finding and Resolution (Task 4, single fix round)

A human read the complete unit-29c diff line by line, per the plan's `checkpoint:human-verify` gate, and returned a verdict: six properties independently confirmed correct as-is, one finding that had to be fixed before approval.

**Independently confirmed correct (no change made):**
- `Unreachable` stops before any publish is attempted, with the reasoning inline.
- Every failure path returns `Err`; no exit-0-while-incomplete.
- No progress record in production code — every `fs::write` in `release_publish.rs` is inside `#[cfg(test)] mod tests` (8 occurrences after this fix's own new test fixtures added 2 more; all 8 are past the `#[cfg(test)]` boundary, none in production code — `rg -n 'fs::write' crates/devflow-core/src/release_publish.rs` returns 8 matches, all at line numbers greater than the module's `#[cfg(test)]` line).
- Publish order consumed unsorted; no crate name as a source literal.
- `observe`/`publish` injected, so the loop is testable with no network.
- The `git.rs` `pub(crate) git_config` widening is reuse, not a second implementation.

**Finding (must-fix): `publish_plan` accepted a silently truncated publish set — Phase 26's CR-03 class, verbatim: "publish set silently truncated, pre-gate reports ✓."**

`git::publish_order` skips — rather than errors on — any declared workspace member whose `Cargo.toml` cannot be read (`let Ok(contents) = std::fs::read_to_string(&manifest) else { continue; }`). `publish_plan` previously guarded only against an *empty* result, never a *short* one. On the irreversible path, the consequence is: publish `devflow-core`, silently skip `devflow`, report success.

**Fix applied (commit `aa33b29`):** `publish_plan` re-derives the workspace's own declared member paths via `git::workspace_member_paths` — the exact source `publish_order` itself starts from — and, for each declared path, looks up its package name via `git::package_name` against that path's own manifest. Both helpers were widened from private to `pub(crate)` (matching the `git_config` precedent the reviewer already confirmed correct) rather than a second TOML-members-scan implementation. When fewer members made it into the returned order than were declared, `publish_plan` refuses, naming every declared path whose manifest could not be read or whose package name never appears in the order. `git::publish_order`'s own behavior is unchanged — the fix is entirely in `publish_plan`'s post-check.

**TDD, as required:** a failing test (`publish_plan_refuses_a_silently_truncated_publish_set_naming_the_missing_path`, a two-member workspace where `crates/b` has no `Cargo.toml`) was written first and confirmed RED against the pre-fix code (`publish_plan` returned `["a"]` instead of refusing — captured verbatim in the panic message: `expected a truncated publish order to be refused, not silently returned: ["a"]`), then GREEN after the fix, naming `crates/b` in the error. A control test (`publish_plan_control_returns_every_declared_member_when_all_manifests_are_readable`) confirms an intact two-member workspace still returns both members, in order.

**Re-verification after the fix:** `cargo test --workspace` — 578 devflow-core lib tests (+2 from this fix), 0 failed across the whole workspace (24 test binaries). `cargo clippy --workspace --all-targets -- -D warnings` clean. `cargo fmt --check` clean. `release_cut_runs_unattended_with_stdin_closed` re-run individually and still green. No live `git tag -s`, tag push, or `cargo publish` was run against the real repo or registry at any point, including during this fix round.

**Corrected recipe note:** the CHECKPOINT REACHED response accompanying the prior version of this SUMMARY suggested `rg -n 'fs::write|File::create|OpenOptions' crates/devflow-core/src/release_publish.rs` would return zero matches. That was imprecise — the file's test fixtures do call `std::fs::write` (originally 6 occurrences, now 8 after this fix's 2 new test fixtures), all inside `#[cfg(test)] mod tests`. The accurate claim, confirmed above, is "zero matches in production code, all matches are past the `#[cfg(test)]` boundary" — not "zero matches in the file."

## Files Created/Modified

- `crates/devflow-core/src/release_publish.rs` (new) — `TagPlan` (`CreateNew`/`ReplaceLocal`/`LeaveAlone`/`Refuse`), `plan_local_tag`, `tag_argv`, `create_and_push_tag` (the signed-tag commit point); `publish_plan` (rewritten in the review-fix commit to refuse a truncated order), `publish_all`, `publish_members` (the injectable publish core), `observe_crate_published`, `run_cargo_publish` (the publish commit point); 23 tests total (21 from the implementation commit + 2 from the review-fix commit)
- `crates/devflow-core/src/release_execute.rs` — module doc comment updated; `sign_release_tag` (resolves `origin/main`'s tip, calls `create_and_push_tag`) and `publish_crates` (calls `publish_all`, joins the report) wired into `action_for`; `action_for_returns_none_for_every_step_without_an_action_in_this_build` renamed to `action_for_returns_an_action_for_every_step_in_this_build`; `absent_step_with_no_pr_backing_and_no_action_stops_naming_the_supplying_unit` renamed and rewritten to `absent_signed_tag_step_now_attempts_its_real_action_instead_of_reporting_no_action`
- `crates/devflow-core/src/lib.rs` — `pub mod release_publish;` declared alphabetically after `release_policy`
- `crates/devflow-core/src/git.rs` — `git_config` visibility widened to `pub(crate)` for `create_and_push_tag`'s reuse (implementation commit); `workspace_member_paths`/`package_name` also widened to `pub(crate)` for `publish_plan`'s reuse (review-fix commit)
- `crates/devflow-core/src/release_observe.rs` — `crate_version_http_status` visibility widened to `pub(crate)` for `observe_crate_published`'s reuse
- `crates/devflow-cli/tests/release_cut.rs` — `cut_reaches_every_step_when_all_are_implemented` (new), replacing `cut_walks_to_the_signed_tag_step_and_stops`
- `.planning/phases/29-release-cut-executor-observe-then-act-within-the-repo-s-rule/29-VALIDATION.md` — Manual-Only Verifications row for unit 29c updated from "not yet performable, blocked on 29-07" to "implemented, still manual-only by design, gated by Task 4's review"

## Decisions Made

See `key-decisions` in the frontmatter above — summarized:
1. Operator resolution of Task 1: build unit 29c now, in this phase (verbatim, recorded above).
2. Task 2 and Task 3 committed together (`bc2b889`), matching 29-05/29-06's precedent for the same shared `action_for` match statement.
3. `plan_local_tag` always peels to the tag's target commit via `<tag>^{commit}` before branching on annotated-vs-lightweight.
4. `sign_release_tag` re-derives its target commit fresh from `origin/main` on every call rather than threading it through `CutReport`.
5. `git::git_config` and `release_observe::crate_version_http_status` widened to `pub(crate)`, matching 29-05's precedent for `version::parse_version_str`/`hooks::today()`.
6. `git::workspace_member_paths` and `git::package_name` also widened to `pub(crate)` for the Task 4 review-fix, so `publish_plan` could detect a silently-truncated `publish_order` result without a second TOML-members-scan implementation.

## Deviations from Plan

### Auto-fixed Issues

None — no bugs, missing critical functionality, or blocking issues were discovered during Task 2/3 execution that required an unplanned fix. Two clippy findings (`manual_is_multiple_of` in the test module's alternating-observation helper) and a `cargo fmt` pass were fixed inline before committing, as part of the plan's own mandated verification loop — not deviations from the plan's design.

### Process Deviations (documented, not auto-fixes)

**1. Task 2 and Task 3 committed as a single atomic commit rather than two.**
- **Found during:** Preparing to commit Task 2 (the signed tag) separately from Task 3 (the publishes).
- **Reason:** `action_for`'s exhaustive match over `ReleaseStep` has exactly one hunk covering both the `SignedTagPresent` and `CratesPublished` arms; the existing test asserting both steps returned `None` also needed rewriting in the same edit. Splitting into two independently-committable diffs would mean re-deriving the same shared hunks twice, with no functional or review benefit — the identical reasoning `29-05-SUMMARY.md` and `29-06-SUMMARY.md` already recorded for this same file and match statement.
- **Committed in:** `bc2b889` (both tasks).

**2. The literal acceptance-criteria command `cargo test -p devflow-core release_publish -- tag_command_form` (and the other named-filter variants) do not narrow to the named substring.**
- **Found during:** Running the plan's literal acceptance commands.
- **Reason:** Confirmed the same quirk `29-05-SUMMARY.md` already documented: Rust's default test harness treats multiple positional filter arguments as an OR (union), not an AND (intersection). "release_publish" (passed by cargo as the package-scoped invocation's own test-name filter) already matches every test in the module, so a second filter word after `--` adds no further narrowing — the full 21-test module runs regardless of which named filter is appended. This does not affect correctness of the underlying "at least N passed" thresholds (21 ≥ 3, 21 ≥ 4, 21 ≥ 16, 21 ≥ 2 all hold), verified interactively for each named filter.
- **No code change.**

**3. `cargo test -p devflow-core release_publish` (the plan's literal, un-featured command form) does not compile as written.**
- **Found during:** Running the exact literal command from the plan's `<verify>` blocks.
- **Reason:** The pre-existing, already-documented command trap (`29-VALIDATION.md` § Test Infrastructure, first noted in the phase's wave-0 audit): `-p devflow-core` without `--features test-support` fails to compile two pre-existing integration test binaries that reference `devflow_core::test_support`, which is feature-gated. The correct invocation is `cargo test -p devflow-core --features test-support --lib release_publish`, which reports 21 passed, 0 failed for every named filter. Not a regression introduced by this plan.
- **No code change.**

---

**Total deviations:** 0 auto-fixed; 3 documented process/tooling notes (a git-history-granularity accommodation matching established precedent, and two pre-existing/already-documented cargo-test-filter quirks). No design, scope, or content change to either task.
**Impact on plan:** None on functionality or test coverage; both tasks' full scope, all specified tests, and all specified acceptance criteria are present and verified.

## Issues Encountered

None beyond the three documented process deviations above.

## User Setup Required

None — no external service configuration required for the code delivered by this plan. No live `git tag -s`, tag push, or `cargo publish` was run anywhere in this plan's execution, including during the review-fix round — every test is hermetic.

## Next Phase Readiness

- **All three units of Phase 29 (29a observer, 29b recoverable actions, 29c commit point) are now code-complete and human-reviewed.** `devflow release cut <version> [project] --yes-release` can carry a release all the way from a version bump to two published crates; `devflow release status` can prove afterward that it did.
- **Task 4 is closed: reviewed, one finding, fixed in a single round, approved.** The verdict was explicit and conditional — "one finding, fix it, then the checkpoint is approved" — and the fix (commit `aa33b29`) has been applied, TDD-verified (RED confirmed before the fix, GREEN after), and re-verified against the full workspace suite/clippy/fmt. See "Review Finding and Resolution" above for the complete record.
- The first real exercise of this code — a genuine `git tag -s`, `git push` of a tag, or `cargo publish` — should still be a deliberate, attended release cut against the next real version (not an unattended automated run), with `devflow release status` run before and after (per `29-VALIDATION.md`'s Manual-Only Verifications table). Approval of the diff is not itself a live-run event.
- No blockers.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_publish.rs`
- FOUND: `crates/devflow-core/src/release_execute.rs`
- FOUND: `crates/devflow-core/src/lib.rs`
- FOUND: `crates/devflow-core/src/git.rs`
- FOUND: `crates/devflow-core/src/release_observe.rs`
- FOUND: `crates/devflow-cli/tests/release_cut.rs`
- FOUND: `.planning/phases/29-release-cut-executor-observe-then-act-within-the-repo-s-rule/29-VALIDATION.md`
- FOUND commit `bc2b889` (feat: the commit point — signed tag and crates.io publish, Tasks 2+3)
- FOUND commit `aa33b29` (fix: publish_plan refuses a silently truncated publish set, Task 4 review-fix)
