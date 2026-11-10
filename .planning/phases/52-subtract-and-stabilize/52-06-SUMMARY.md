---
phase: 52-subtract-and-stabilize
plan: 06
subsystem: release-tooling
tags: [release, cut-release, workspace-tool, rust, cargo-deny]

requires:
  - phase: 52-05
    provides: legacy Claude launch removed (previous plan in the cut order)
provides:
  - "Unpublished workspace member tools/release-preflight (package release-preflight, publish = false) with `check` and `verify`"
  - "scripts/cut-release.sh step_check/step_verify run `cargo run -q -p release-preflight -- check|verify`"
  - "devflow_core::git::publish_order skips members whose manifest sets publish = false (or publish = [])"
  - "`devflow release` removed from the CLI, its tests, help snapshot and docs"
affects: [52-13 version.rs deletion, release cut, cut-release.sh]

actuals:
  tokens: 23900
  tasks: 3
  commits: 6
plan_head_before: ae0b2be6f73bb1c09733ef487db892cfc04997b7
plan_head_after: 8d7249bc6bb1e184fa531d3693fb790309130457

tech-stack:
  added: []
  patterns:
    - "Repo tooling as an unpublished workspace binary with end-to-end tests that spawn it through devflow_core::git::hermetic_command"
    - "Each negative control mutates one thing from a release-ready fixture, so only the targeted check can fail"

key-files:
  created:
    - tools/release-preflight/Cargo.toml
    - tools/release-preflight/src/main.rs
    - tools/release-preflight/tests/preflight.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/devflow-core/src/git.rs
    - scripts/cut-release.sh
    - crates/devflow-cli/tests/cut_release_guard.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/hermetic_spawn_sites.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/gate_sweep_e2e.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - README.md
    - OPERATIONS.md
    - CONTRIBUTING.md
    - .planning/user/DEV-SETUP-CHECKLIST.md
  deleted:
    - crates/devflow-cli/tests/release_check.rs

key-decisions:
  - "D-02 choice: the release preflight is a selftested Rust tool (tools/release-preflight), not shell, under the operator's avoid-shell-check-gates rule; each of the six checks has a named passing and failing (or warning) fixture"
  - "The tool reads the workspace self-pins with the toml crate (already a workspace dependency) instead of porting version.rs's line scanner, so it does not depend on version.rs, which 52-13 deletes"
  - "The tool takes no CLI-parser dependency: `check|verify [PROJECT]` is parsed by hand, exit 1 on a failed check, exit 2 on a usage error, warnings alone exit 0"

patterns-established:
  - "Release checks live in tools/release-preflight and are called by scripts/cut-release.sh; cut_release_guard::cut_release_runs_the_release_preflight_tool pins the calls inside step_check and step_verify"

requirements-completed: [CUT-04]

coverage:
  - id: D1
    description: "release-preflight check runs the self-pin, divergence, publish-order and changelog checks with a passing and a failing or warning fixture each"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "tools/release-preflight/tests/preflight.rs (check_passes_when_pins_match, check_flags_self_pin_drift, check_passes_on_a_release_ready_fixture, check_reports_divergence_when_main_not_ancestor, check_divergence_degrades_when_origin_main_absent, check_states_publish_order, check_warns_when_the_publish_order_cannot_be_derived, check_flags_a_changelog_version_mismatch)"
        status: pass
      - kind: unit
        ref: "tools/release-preflight/src/main.rs#tests (six changelog_check_* tests, compare_versions_orders_a_release_after_its_prerelease)"
        status: pass
    human_judgment: false
  - id: D2
    description: "release-preflight verify runs the tag-on-main and main-to-develop sync checks with passing and failing fixtures"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "tools/release-preflight/tests/preflight.rs (verify_passes_on_a_tagged_synced_fixture, verify_flags_a_tag_not_on_main, verify_flags_an_unsynced_develop)"
        status: pass
    human_judgment: false
  - id: D3
    description: "publish_order leaves out a publish = false member"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::publish_order_skips_an_unpublished_member"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::publish_order_derives_core_before_cli_from_a_fixture_workspace"
        status: pass
    human_judgment: false
  - id: D4
    description: "scripts/cut-release.sh check and verify call the tool and no longer call devflow release"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/cut_release_guard.rs#cut_release_runs_the_release_preflight_tool"
        status: pass
    human_judgment: false
  - id: D5
    description: "devflow release is rejected by clap and absent from --help"
    requirement: CUT-04
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_subcommands_are_unknown"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D6
    description: "A real release cut still works with the tool (live run of scripts/cut-release.sh check and verify)"
    verification: []
    human_judgment: true
    rationale: "The tool is tested against fixtures and was run read-only against this checkout, not through a real release cut; the first scripts/cut-release.sh check after this phase is the live proof."

duration: 19min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 06: Release preflight port Summary

**`devflow release --check`/`--verify` is now `tools/release-preflight`, an unpublished workspace binary that `scripts/cut-release.sh check|verify` runs through `cargo run -q -p release-preflight`; it carries all six checks, each with a named control, and the `release` verb is gone from the CLI.**

## Performance

- **Duration:** 19 min
- **Started:** 2026-11-10T20:34:22Z
- **Completed:** 2026-11-10T20:53:44Z
- **Tasks:** 3 (tracer + 2)
- **Files modified:** 19 (3 created, 1 deleted)

## Accomplishments

- `tools/release-preflight` (`publish = false`, `license.workspace = true`) gives `check` (self-pin, divergence with the absent-`origin/main` warning, publish order, changelog version) and `verify` (tag on `main`, sync done). Messages, severities and the output format match the old command.
- `scripts/cut-release.sh` calls the tool in `step_check` and `step_verify`; `run_devflow` is deleted because nothing else used it. A guard test pins both calls inside those two functions and refuses any `release --check`/`release --verify` text in the script.
- `git::publish_order` skips `publish = false` members, so the order stays `devflow-core -> devflow` with the tool in the workspace.
- `Command::Release`, `release_check`, `release_verify`, their seven helpers, `tests/release_check.rs`, its `HERMETIC_SUITES` entry, the help-snapshot line and the OPERATIONS.md row are deleted. README now names `scripts/cut-release.sh check`; CONTRIBUTING's release section describes the tool.

## Task Commits

1. **Task 1 (tracer): self-pin check from `cut-release.sh check`**: RED `0aa6b0c` (test), GREEN `e3d0969` (feat)
2. **Task 2: the other five checks, `verify`, script guard**: RED `f490177` (test), GREEN `a0e54c7` (feat)
3. **Task 3: remove `devflow release`**: RED `5cf2c30` (test), GREEN `8d7249b` (feat, `!` and a `BREAKING CHANGE:` footer)

## TDD Gate Compliance

Every task has a `test(52-06)` commit followed by a `feat(52-06)` commit. RED evidence was taken from nextest JUnit reports (a scratch nextest profile, `--config-file`), passed to `gsd-tools check tdd-red-evidence`, and every target returned `RED_EVIDENCE_OK`:

- Task 1: `check_passes_when_pins_match` and `check_flags_self_pin_drift` failed on their own assertions against a skeleton binary that exits 2 with no report. The drift test's "exits non-zero" assertion held trivially against the skeleton, so the failing assertion was the missing self-pin line. `publish_order_skips_an_unpublished_member` failed with `left: ["devflow-core", "devflow", "release-preflight"]`.
- Task 2: the nine new tool tests failed on a missing report line or an unexpected pass; `cut_release_runs_the_release_preflight_tool` failed on `step_verify must run cargo run -q -p release-preflight -- verify`. `check_flags_self_pin_drift` and `check_passes_when_pins_match` still passed, which was expected because they cover Task 1's check.
- Task 3: `removed_subcommands_are_unknown` failed because `devflow release` was still parsed (stderr was the old "requires --check" refusal, not clap's unknown-subcommand error).

## Verification

| Command | Exit |
|---|---|
| Task 1 `<automated>` chain (4 named nextest runs) | 0 |
| Task 2 `<automated>` chain (9 named nextest runs) | 0 |
| Task 3: `removed_subcommands_are_unknown`, `help_output_matches_committed_snapshot`, `doc_check::doc_referenced_identifiers_exist_in_source` | 0 |
| `scripts/check.sh all` (fmt, clippy `--workspace --all-targets -D warnings`, `cargo test --workspace`; the tool's unit and e2e targets ran in it) | 0 (second run; the first failed, see Deviations) |
| `scripts/check.sh deps` (cargo deny + cargo machete) | 0 |
| Negative control: `license.workspace = true` removed from the tool's manifest, then `cargo deny check licenses` | 4 (`unlicensed release-preflight`), then the line was restored |
| `cargo run -q -p release-preflight -- check` / `-- verify` on this checkout | 0 / 0, all lines ok, output the same as `devflow release --check .` / `--verify .` before the removal |

**What this does not establish:** the tool has been run against fixtures and read-only against this checkout, where every check passes. It has not run through a real release cut; the first `scripts/cut-release.sh check` after this phase is the live proof. The parity comparison with the old command covered only the all-ok state of this repo. On the real repo the tool is called through `cargo run`, so it needs a Rust toolchain and builds the tool on first use.

## Decisions Made

- Rust tool, not shell (D-02): repo tooling falls under the operator's avoid-shell-check-gates rule, and the six checks port directly as named Rust tests.
- Self-pins are read with `toml` (already a workspace dependency), so the tool does not depend on `version.rs`, which 52-13 deletes. Behaviour difference: the old line scanner saw only single-line inline tables; `toml` reads every form. A path dependency with no `version` is still not a pin.
- No CLI parser: two subcommands and an optional path are parsed by hand. Exit 1 on a failed check, exit 2 on a usage error.
- `publish_order` also treats `publish = []` as unpublished, since Cargo reads both spellings as "never publish".

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Ported the changelog unit test that `scripts/check.sh all` found**
- **Found during:** Task 3
- **Issue:** `commands.rs` had a unit test, `changelog_version_check_flags_mismatch_and_passes_on_agreement`, that called the deleted `check_changelog_version`. The plan's anchors did not list it, so the first `scripts/check.sh all` failed to compile the `devflow` test binary (exit 101).
- **Fix:** The test was deleted from `commands.rs` and its five cases moved into the tool as atomic unit tests (Cargo ahead, changelog ahead, plain heading, bracketed heading, no heading). Two more were added: a numbered `## 1. Overview` section is skipped, and prerelease ordering in `compare_versions`.
- **Files modified:** crates/devflow-cli/src/commands.rs, tools/release-preflight/src/main.rs
- **Verification:** 18/18 tool tests pass; second `scripts/check.sh all` exit 0
- **Committed in:** 8d7249b

**2. [Rule 2 - Missing critical] Added `check_passes_on_a_release_ready_fixture`**
- **Found during:** Task 2
- **Issue:** The plan's named tests gave the divergence and changelog checks a failing fixture but no passing one, while the acceptance criterion requires both for every check.
- **Fix:** A release-ready fixture on which all four `check` lines must be ok. Each negative control changes one thing from it.
- **Committed in:** f490177 (RED), a0e54c7 (GREEN)

**3. [Rule 1 - Bug] Stale references to the removed command and file**
- **Found during:** Tasks 2 and 3
- **Issue:** A `cut-release.sh` comment said `release --verify` (the guard test caught it). `git.rs` doc comments and `gate_sweep_e2e.rs`'s module doc named `release --verify` / `release_check.rs`.
- **Fix:** These now say `release-preflight verify`, and `release_check.rs` is no longer cited.
- **Committed in:** a0e54c7, 8d7249b

**4. Commit subjects.** The plan suggested `build: add the release-preflight tool` and `feat(cli)!: remove devflow release`. The commits use the executor's `{type}(52-06)` format instead, so the TDD gate can find the RED/GREEN pairs: `feat(52-06): add the release-preflight tool ...` and `feat(52-06)!: remove devflow release`. The `!` and a `BREAKING CHANGE:` footer are kept.

---

**Total deviations:** 3 auto-fixed (1 blocking, 1 missing critical, 1 bug), plus 1 commit-subject format change.
**Impact on plan:** no scope added beyond what the acceptance criteria require.

## Issues Encountered

- Between `e3d0969` and `a0e54c7` the tool's `[dependencies] devflow-core` was not yet used by `main.rs`, so `cargo machete` would have flagged it. `scripts/check.sh deps` was only run at the end, where it is clean.
- `crates/devflow-core/src/version.rs:920` still says `devflow release --check` in a doc comment. It was left alone because 52-13 deletes the whole file.

## User Setup Required

None.

## Next Phase Readiness

- `version.rs` no longer has a caller in the release path (`commands.rs` dropped its `version` import), so 52-13 can delete it as planned (RESEARCH safe order, row 3).
- The first real `scripts/cut-release.sh check` after this phase is the live proof of the tool.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*

## Self-Check: PASSED

- The three created files exist, and `crates/devflow-cli/tests/release_check.rs` is deleted.
- All six task commits are ancestors of HEAD, and `check evaluation-scope --plan 52-06 --commits-only` resolves them.
- `scripts/check.sh all` exit 0 and `scripts/check.sh deps` exit 0 (Verification table).
