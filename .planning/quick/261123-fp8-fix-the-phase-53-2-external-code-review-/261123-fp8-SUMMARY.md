---
phase: quick-261123-fp8
plan: 01
subsystem: devflow-cli start/resume gates, status, test harness, hooks, records
tags: [53.2, external-review, D-04, D-11, D-13, D-15]
requires: [53.2 phase execution, VERDICTS.md code-round1]
provides: [strict start --reviews probe, truthful timed-out launch messages, fakes that parse /proc and quote paths, corrected 53.2-VERIFICATION.md]
affects: [crates/devflow-cli, crates/devflow-core test code, scripts/check.sh, scripts/hooks/post-commit, OPERATIONS.md]
tech-stack:
  added: []
  patterns: [fail-closed base probe beside the documented fail-open one, shared POSIX sh helper constants for the fakes]
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/start_refusals_e2e.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/common/mod.rs
    - crates/devflow-cli/tests/run_liveness_e2e.rs
    - crates/devflow-cli/tests/service_e2e.rs
    - crates/devflow-cli/tests/post_commit_marker.rs
    - crates/devflow-cli/tests/test_home_isolation.rs
    - crates/devflow-core/src/test_support.rs
    - crates/devflow-core/src/gsd_reader.rs
    - scripts/check.sh
    - scripts/hooks/post-commit
    - OPERATIONS.md
    - .planning/phases/53.2-wsx-ude-first-use-and-dogfood-remediation/53.2-VERIFICATION.md
decisions:
  - "start --reviews uses a new fail-closed require_reviews_file_on_base; phase_artifact_on_base stays fail-open for CONTEXT.md/PLAN.md"
  - "a timed-out start keeps its state when a stage_launched event appeared after start's snapshot (event log, not a new State field, to avoid a devflow-core semver change)"
  - "the isolation guard names the three pure settle_unit_reads tests as the only unignored service_e2e tests"
metrics:
  duration: 41min
  completed: 2026-11-23
status: complete
actuals:
  tokens: 30276
  tasks: 3
  commits: 7
plan_head_before: a291d9a43a5398819172dce52afb34cdd74159ea
plan_head_after: 56f2eac0c5ca0b0fb9ae1787f101ac891576f5ed
---

# Phase quick-261123-fp8 Plan 01: Phase 53.2 external code review fixes Summary

`start --reviews` now refuses unless exactly `.planning/phases/NN-<slug>/NN-REVIEWS.md` is on the base, failing closed on git errors and on two phase directories (as `resume --reviews` now does too); every other confirmed finding is fixed in code, tests, docs or the verification record, and all three final gates exit 0 at 56f2eac.

**Operator:** override 3 (the notify drain order) in 53.2-VERIFICATION.md awaits your strike or keep; override 2 (D-15 core half) was withdrawn as met by code (ce5601a).

## Commits

| Commit | Task | What |
|---|---|---|
| c320268 | 1 RED | lookalike REVIEWS.md, refused-start warning, run-checkout warning, tracked `.gsd/` file, resume runner stop |
| 9e897c7 | 1 GREEN | F02, F01, F03, F04, F05, F06, F07, F12, F20, F21, F23a, OPERATIONS.md:37-38 |
| cf382ca | 2 RED | shared `PROC_STATE_FN` and `sh_quote` with today's behaviour, kept-entry count test |
| ce5601a | 2 GREEN | F08, F09, F10, F11, F16, F19, F23b, F23c, F24 |
| 1b9aa8c | 3 | F22 post-commit chaining and its test |
| 6e436b0 | 3 | F13, F14 OPERATIONS.md rows |
| 56f2eac | 3 (deviation) | isolation guard allows the three pure service_e2e tests |

53.2-VERIFICATION.md (F15, F17, F18, F29) is edited but uncommitted, for the orchestrator's docs commit.

## RED evidence

- `start_reviews_with_only_lookalike_reviews_files_on_the_base_is_refused`: "start --reviews accepted lookalike REVIEWS.md files on the base" (the start launched)
- `a_refused_start_prints_no_gitignore_warning`: "a refused start printed the .gitignore warning: ... add these lines to .gitignore"
- `start_warns_about_gsd_derived_files_from_the_run_checkout`: left `[]`, right the three lines
- `start_dry_run_tells_to_untrack_any_tracked_file_under_gsd`: "start did not tell to untrack a tracked file under .gsd/"
- `a_timed_out_resume_whose_runner_recorded_its_stop_names_that_stop`: "the error did not name the runner's own stop: resume: phase 5395 was not launched: ..."
- `the_fakes_read_the_proc_state_after_a_command_name_with_spaces` (F10): left `"b)"`, right `"S"`
- `the_fakes_quote_a_path_with_a_space_and_a_quote` (F24): panicked "the test binary's path is written double-quoted into the fake: /x/a b/it's"
- `permission_setters_flag_a_kept_entry_that_matches_two_lines` (F16): left `[]`
- F11: `cargo nextest run -p devflow --test service_e2e --no-tests=fail -E 'test(=a_failed_read_before_inactive_is_reported_with_its_result)'` at HEAD: exit 4, "error: no tests to run"
- F22: `a_commit_outside_a_workspace_checkout_runs_the_global_post_commit` against HEAD's hook: "the global post-commit did not run outside a workspace checkout"

## Findings

| F | Result |
|---|---|
| F01 | fixed 9e897c7: the #285 warning prints after every refusal (dry run after `resolve_start_base`, real start after the fork) |
| F02 | fixed 9e897c7: `require_reviews_file_on_base` (exact path, one directory, git error refuses); `resume --reviews` names both directories |
| F03 | fixed 9e897c7: the real start reads the forked tree's ignore rules |
| F04 | fixed 9e897c7: `ls-files -z -- <line>` untracks every file under `.gsd/`; unchecked probes under their own header |
| F05 | fixed 9e897c7: resume's `written` is the committed in-memory state; a runner's own stop is named |
| F06 | fixed 9e897c7: a gone state says nothing to clear; unloadable and unread cases named |
| F07 | fixed 9e897c7: a newer `stage_launched` event keeps the state |
| F08 | fixed ce5601a: two `drained_verdict` tests (pin existing code) |
| F09 | fixed ce5601a: unheld-record test renamed to its READY-only property; cross-phase lock test, its entry and constants deleted |
| F10 | fixed ce5601a: shared `proc_state` reads after the last `) `; receives retry `Interrupted` |
| F11 | fixed ce5601a: three pure tests unignored; service target excludes `common::fake_notify_listener_entry` |
| F12 | fixed 9e897c7: live line at a question gate names `devflow gate answer N --gate-id <id>`; the open question is read once per phase in `status` |
| F13 | fixed 6e436b0 |
| F14 | fixed 6e436b0 |
| F15 | fixed in 53.2-VERIFICATION.md (truth 24) |
| F16 | fixed ce5601a: a kept entry matching more than one line is a finding |
| F17 | fixed in 53.2-VERIFICATION.md (override 1 scoped to claude) |
| F18 | fixed in 53.2-VERIFICATION.md (override 3 pending operator; drain tests cited) |
| F19 | fixed ce5601a: fixture through `write_executable`, held write descriptor; kept entry removed; override 2 withdrawn |
| F20 | fixed 9e897c7: a branch whose recorded base is gone is listed by name as not compared |
| F21 | fixed 9e897c7 (`show-ref --verify`); the O(K*B) `rev-list` half not changed (decided at planning: no wrong output, needs a devflow-core API change) |
| F22 | fixed 1b9aa8c |
| F23 | a: TZ tests 9e897c7; b: straggler ready file replaces the 500 ms sleep, ce5601a; c: `cp` resolved from `/usr/bin` or `/bin`, ce5601a |
| F24 | fixed ce5601a: POSIX single-quote escaping; notify socket in a short `mktemp -d /tmp/dfn.XXXXXX` dir removed by an EXIT trap |
| F25 | refuted in VERDICTS.md; skipped |
| F26 | final gates below |
| F27 | refuted at planning: `devflow status` never reads `.holder-NN` |
| F28 | refuted in VERDICTS.md; skipped |
| F29 | fixed in 53.2-VERIFICATION.md (#178 cites the 53.2-11 measurement) |

## Final gates (HEAD 56f2eac)

- `scripts/check.sh all`: exit 0
- `scripts/check.sh service`: exit 0, `12 tests run: 12 passed, 4 skipped` (was 16: the listener entry and the three pure tests no longer run there)
- `scripts/check-in-container.sh`: exit 0, `check.sh: all OK`
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0 (at ce5601a; `check.sh all` reran clippy at 56f2eac)

## Deviations from Plan

**1. [Rule 3 - Blocking] Isolation guard refused the un-ignored pure tests**
- Found during: final gates (first `check.sh all`, exit 101)
- Issue: `test_home_isolation::suites_using_common_set_path_only_through_hermetic_path` required every service_e2e test to be ignored ("15 #[test] but 12 #[ignore]"), contradicting F11.
- Fix: the guard names the three pure tests and still requires every unit-talking test to be ignored.
- Commit: 56f2eac

**2. [Wording] F20 line** reads `<branch> — not compared: base <error> (<source>)` (the error already names the base), instead of repeating the base name.

**3. [Shape] RED for new APIs.** Unit tests that call functions this plan adds (`require_reviews_file_on_base`, `derived_paths_warning_text`, the new `RunStatus::Live` field, the new `not_live_after_timed_out_launch` argument) landed with GREEN, since a RED commit cannot compile them; F10 and F24's RED extracted today's behaviour into the shared helpers first so the test failed on behaviour.

**4. DEV-SETUP-CHECKLIST.md** reviewed for the check.sh and post-commit changes; no entry became false (line 99 already says post-commit chains to the global hook, now true in every checkout), so it is unchanged. The plan's example commit subject (74 chars) was shortened to fit the 72-char cap.

**5. GSD incident** for the hand correction of 53.2-VERIFICATION.md appended to ~/.config/agents/gsd-manual-fix-incidents.md.

## Known Stubs

None.

## Self-Check: PASSED

All seven commits are ancestors of HEAD; every modified file exists.
