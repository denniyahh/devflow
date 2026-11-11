---
phase: 52-subtract-and-stabilize
plan: 18
subsystem: testing
tags: [cut-02, cut-03, cut-04, doc_check, source-scan, docs-site, semver]

requires:
  - phase: 52-17
    provides: the stage machine is Plan → Code → Validate in code, tests, help and the scoped docs
provides:
  - doc_check::no_removed_surface_literal_remains (29 tokens over comment-stripped Rust under crates/ and tools/, HTML-comment-stripped Markdown under docs/ and the top-level docs, and the snapshots)
  - doc_check::docs_site_describes_only_the_kept_pipeline (stage words, dead links, dead nav entries, undeclared modules on the docs site)
  - the whole-Ship docs pages deleted and the docs site rewritten to Plan → Code → Validate
  - semver evidence for the 52-01..52-17 cut
affects: [52-31, 52-32]

actuals:
  tokens: 14473
  tasks: 3
  commits: 5
plan_head_before: a650fc138245aa74ccbb6850e78b3a7e63c8520f
plan_head_after: 0f14f16
commits: 5

tech-stack:
  added: []
  patterns:
    - "A control test whose subject is a removed value builds that value with a concat! split inside the token, with a comment saying why"
    - "Docs-site checks take an `exists` closure so planted cases run without the filesystem"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/doc_check.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-core/src/stage.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - crates/devflow-cli/tests/gate_sweep_e2e.rs
    - crates/devflow-cli/src/commands.rs
    - docs/index.md
    - docs/architecture/overview.md
    - docs/architecture/state-machine.md
    - docs/architecture/agent-model.md
    - docs/diagrams/agent-lifecycle.md
    - docs/diagrams/module-map.md
    - docs/diagrams/system.md
    - docs/guides/quickstart.md
    - docs/mkdocs.yml
    - devflow.toml

key-decisions:
  - "The scan applies the stage literals \"ship\"/\"define\" as whole Rust string literals in .rs files and as the double-quoted word in Markdown and snapshots; identifier-like and phrase tokens use one identifier-boundary rule everywhere"
  - "A letter right after a backslash (an escape such as \\n inside a literal) counts as a token boundary, so a fixture like \"[run]\\nyes_ship = true\" is still a finding"
  - "gate_sweep_e2e's reap-strays guard is treated as a control and split, not deleted: the plan says a control found beyond its list gets the same concat! treatment; its deletion stays in deferred-items.md"
  - "Task 3 found no scan gap, so doc_check.rs was not changed in Task 3"

patterns-established:
  - "Removed-surface tokens and planted lines live in doc_check.rs as concat! splits and @-templates so the file never holds a whole token"

requirements-completed: [CUT-04, CUT-03, CUT-02]

coverage:
  - id: D1
    description: "A removed-surface literal anywhere in the scan scope fails the core test suite; planted real lines, decoys, ordering and non-empty walkers prove the scan decides"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#no_removed_surface_literal_remains"
        status: pass
      - kind: other
        ref: "real-walker plant: const _PLANT = \"yes_ship\" appended to crates/devflow-cli/tests/phase7_cli.rs failed the test (exit 100); removed, sha256 restored, rerun passed"
        status: pass
    human_judgment: false
  - id: D2
    description: "Control tests that assert a removed value is rejected or absent still run and pass after their literals were split"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "Task 1 <verify> chain (12 named tests), exit 0"
        status: pass
      - kind: unit
        ref: "state::tests::a_state_file_with_a_leftover_yes_ship_key_still_loads, stage::tests::plan_is_the_first_stage_and_validate_the_last, gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass"
        status: pass
    human_judgment: false
  - id: D3
    description: "The docs site names no Define or Ship stage, links only pages that exist, lists only pages that exist in its nav, and maps only modules lib.rs declares"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#docs_site_describes_only_the_kept_pipeline"
        status: pass
      - kind: other
        ref: "real-file plant: a Ship sentence and a link to nope.md appended to docs/index.md gave exactly those two findings (exit 100); removed, sha256 restored, rerun passed"
        status: pass
    human_judgment: false
  - id: D4
    description: "The rewritten docs pages read correctly as prose about Plan → Code → Validate (the test is lexical and cannot judge a stale sentence)"
    requirement: CUT-04
    verification: []
    human_judgment: true
    rationale: "The docs-site test catches stage words, dead links and deleted module names only; whether each rewritten sentence is accurate is a reading judgment"
  - id: D5
    description: "devflow-core's breaking changes match the planned removals of 52-01..52-17; both check environments are green"
    requirement: CUT-03
    verification:
      - kind: other
        ref: "cargo semver-checks check-release -p devflow-core --release-type major && scripts/check.sh all && timeout -k 60 900 scripts/check-in-container.sh, each exit 0"
        status: pass
    human_judgment: false

duration: 31min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 18: Removed-surface scan, docs site and semver check Summary

**`doc_check` now fails on any of 29 removed v3 tokens in the scoped tree and on any Define/Ship stage word, dead link, dead nav entry or deleted module on the docs site. The two whole-Ship docs pages are gone, seven docs pages describe Plan → Code → Validate, and semver-checks reports only breaks that 52-01..52-17 planned.**

## Performance

- **Duration:** 31 min
- **Started:** 2026-11-11T03:53:52Z
- **Completed:** 2026-11-11T04:25:14Z
- **Tasks:** 3
- **Files modified:** 20 (18 modified, 2 deleted), plus `deferred-items.md`

## Accomplishments

- `doc_check::no_removed_surface_literal_remains` scans comment-stripped `.rs` under `crates/` and `tools/`, HTML-comment-stripped Markdown under `docs/` plus README/ARCHITECTURE/CONTRIBUTING/OPERATIONS.md, and `crates/devflow-cli/tests/snapshots/*`. The real tree has zero findings.
- `doc_check::docs_site_describes_only_the_kept_pipeline` pins the docs site. The real tree has zero findings.
- `docs/architecture/git-ship.md` and `docs/diagrams/ship-flow.md` are deleted, and so are their nav entries and links.
- 13 control sites in 8 files are split with `concat!`. Every one of them still passes when run by name.
- Semver: the default run reports 9 failing check kinds, each matched below to a removal in 52-01..52-17. The `--release-type major` run exits 0.

## Scan scope (D-40)

The scan reads exactly the scope named in amended ROADMAP criterion 2. Of the top-level docs it leaves out exactly three, by D-40:
- `CHANGELOG.md`: release history, which must name what was removed.
- `.planning/`: planning records.
- `CLAUDE.md`: the operator's file.

It does not read `scripts/`, `.github/`, `.devcontainer/`, `.gitignore` or the root config files. CUT-03's clause about the cron file outside the scan's reach therefore rests on 52-10, which removed `.gitignore:58`. Two other things are left as they are:
- `scripts/hooks/pre-commit`'s `DEVFLOW_TERMINAL_HOOK` allowance (a `defer` row in the inventory). It sits outside the scan.
- `CLAUDE.md`'s Phase 51 line that cites `define_stage_prompt`. It is outside the scope, and `define_stage_prompt` is not a token, so the scan neither reads it nor needed an allowlist entry for it. The file was not edited.

## Token reality (b8df457, `git grep -w -F` over crates tools docs README.md ARCHITECTURE.md CONTRIBUTING.md OPERATIONS.md)

| Token | b8df457 lines | Token | b8df457 lines |
|---|---|---|---|
| `cron-instructions` | 25 | `devflow reference` | 2 |
| `hermes_cron` | 27 | `devflow release` | 11 |
| `yes_ship` | 125 | `reap-strays` | 18 |
| `yes-ship` | 32 | `legacy-claude-launch` | 20 |
| `VersionBump` | 63 | `DEVFLOW_CLAUDE_LEGACY_LAUNCH` | 31 |
| `ChangelogAppend` | 44 | `DEVFLOW_YES_SHIP` | 9 |
| `BranchCleanup` | 28 | `DEVFLOW_REVIEW_ANGLES` | 8 |
| `Hook::Merge` | 8 | `SHIP_REVIEW_ANGLES` | 2 |
| `DocsUpdate` | 13 | `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` | 14 |
| `Stage::Define` | 164 | `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | 11 |
| `Stage::Ship` | 325 | `ship_evidence` | 18 |
| `StageIntent::Define` | 4 | `"ship"` | 30 |
| `StageIntent::Ship` | 4 | `"define"` | 18 |
| `devflow ship` | 85 | | |
| `devflow parallel` | 20 | | |
| `devflow evidence` | 1 | | |

All 29 counts are at least 1, and the fewest is `devflow evidence` (1), as planning measured. The negative control is the five bare tails: `YES_SHIP`, `CLAUDE_LEGACY_LAUNCH`, `REVIEW_ANGLES`, `FOREGROUND_GATE_TIMEOUT` and `CHECKOUT_LOCK_TIMEOUT` each have 0 lines. Every count agrees with the plan.

Each planted positive is a real b8df457 line, with the token replaced by `@` in `PLANTED_REMOVED_LINES`. Sources:
- `pipeline_launch.rs:6236` and `:13072`
- `commands.rs:5199`, `:368`, `:321`, `:325`, `:1658`, `:2667`, `:42`, `:9949` and `:8815`
- `hooks.rs:31`, `:23` and `:25`
- `pipeline_gate.rs:2299` and `pipeline_outcomes.rs:1200`
- `prompt.rs:481`, `:221` and `:12`
- `fallback_agent_config.rs:406` and `gate_notify_persist_e2e.rs:202`
- `OPERATIONS.md:50`, first table cell only (the full row is about 900 characters)
- `README.md:132` and `main.rs:795`
- `phase7_cli.rs:148`, which is the plan's example
- `config.rs:280` and `:241`
- `config_parse.rs:70` and `:86`

`phase7_cli.rs` `run_devflow_inner`'s `.env("DEVFLOW_CLAUDE_LEGACY_LAUNCH", "true")` is no longer in the tree: HEAD has 0 lines for that token, because 52-04 removed it. Nothing was left to delete.

## Control sites split with `concat!` (runtime bytes and assertions unchanged)

| File | Test / item | Value |
|---|---|---|
| `crates/devflow-cli/tests/phase7_cli.rs` | `REMOVED_UNTIL_STAGES` (`until_rejects_removed_stages`) | `"ship"`, `"define"` |
| `crates/devflow-cli/tests/phase7_cli.rs` | `REMOVED_SUBCOMMANDS` (`removed_subcommands_are_unknown`) | `"ship"` |
| `crates/devflow-cli/tests/phase7_cli.rs` | `REMOVED_FLAG_INVOCATIONS` (`removed_flags_are_unknown`) | `--reap-strays`, `--legacy-claude-launch` (x2), `--yes-ship` |
| `crates/devflow-cli/tests/phase7_cli.rs` | `DRY_RUN_3_0_0` and the stage-name array in `start_dry_run_lists_exactly_plan_code_validate` | `DocsUpdate`, `VersionBump`, `ChangelogAppend`, `BranchCleanup`; `"define"`, `"ship"` |
| `crates/devflow-core/src/stage.rs` | `stage::tests::removed_stage_names_do_not_parse` (both loops) | `"define"`, `"ship"` |
| `crates/devflow-core/src/stage.rs` | `stage::tests::plan_is_the_first_stage_and_validate_the_last` (**beyond the plan's list**) | `"define"`, `"ship"` |
| `crates/devflow-core/src/config.rs` | `config::tests::a_leftover_yes_ship_key_is_ignored` (TOML fixture and both messages) | `yes_ship` |
| `crates/devflow-core/src/state.rs` | `state::tests::a_state_file_with_a_leftover_yes_ship_key_still_loads` (**beyond the list**: JSON fixture, expect message, absence check) | `yes_ship` |
| `crates/devflow-cli/tests/failover_e2e.rs` | `Rig::cron_file` and the absence message in `a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate` | `cron-instructions` |
| `crates/devflow-cli/src/commands.rs` | `stray_process_finding::build_stray_process_findings_names_pid_layer_and_repair` and `render_stray_process_text_names_pid_and_repair_when_present` | `--reap-strays` |
| `crates/devflow-cli/tests/gate_sweep_e2e.rs` | `no_suite_on_the_common_builder_runs_a_real_reap_strays_pass`'s four assertion messages (**beyond the list**) | `--reap-strays` |

Each split has a `//` comment beside it saying the value is split so the removed-surface scan does not read the control as a leftover. A message with an inline `{log}` capture became a positional `{}` with `log` passed as an argument. Rust's format macros do not capture names from a `concat!`-built string, and the message bytes are unchanged. The scan's own file needed two fixes:
- `concat!("devflow ", "ship")` and `concat!("ship", "_evidence")` held a whole `"ship"` literal, so they are now split inside the word.
- A test local named `yes_ship` became `removed_key`.

There were no other real-tree findings. None was fixed by narrowing a token or exempting a path.

## Real-walker control (criterion 2)

- **Plant:** `const _PLANT: &str = "yes_ship";` appended to `crates/devflow-cli/tests/phase7_cli.rs`. The test failed with exit 100, giving exactly one finding: `crates/devflow-cli/tests/phase7_cli.rs:2167: removed surface `yes_ship``.
- **Removed:** the file's sha256 matched the pre-plant value. The rerun passed (exit 0), and `git status` showed no plant.
- **Docs-site equivalent:** `The Ship stage.` and `[x](nope.md)` were appended to `docs/index.md`. That run failed with exit 100, with findings at `docs/index.md:45` (stage word) and `:46` (missing page). After removal the sha256 matched, and the rerun passed.

## Docs-site rewrite (Task 2)

Real-tree RED before the rewrite gave 27 findings: 20 stage-word findings and 7 undeclared-module findings. No dead link and no dead nav entry remained after Task 1. Pages rewritten:
- `docs/index.md`:
  - L3-5: the intro no longer mentions "human-approved shipping".
  - L11-14: the mermaid diagram is now `Plan → Code → Validate`, with a gaps loop and a "Run finishes, branch kept" node.
  - L17-22: the paragraph gives the kept gates (failure gates, the repeated-Validate-failure gate, supervise's Validate gate) and says the run ends with the branch kept for the operator.
  - The Ship Flow link is dropped (Task 1).
- `docs/architecture/overview.md`:
  - L11: `hooks` is dropped from the core row.
  - L18: "Three explicit stages".
  - L23: "The operator integrates".
  - The git-ship link is dropped (Task 1).
- `docs/architecture/state-machine.md`:
  - L7-11: the diagram has no Define or Ship.
  - The Define and Ship rows are deleted, and the Validate row names the supervise and repeated-failure gates.
  - L26-29: the modes paragraph is rewritten.
- `docs/architecture/agent-model.md`:
  - L49: the Ship clause is dropped.
  - "review angles" is dropped from `StageIntent`'s data, because the variant fields are now only phase and fix.
- `docs/diagrams/agent-lifecycle.md` L49: the Ship clause is dropped.
- `docs/diagrams/module-map.md`:
  - The `HOOKS`, `VERSION` and `LOCK` nodes are gone, and `GIT` hangs off `WF`.
  - The table drops `lock`, `hooks`, `version` and `ship`, and adds `reset_time` to Reliability.
- `docs/diagrams/system.md`:
  - L16: "Git branches", without "and tags".
  - L24: `hooks` is dropped.
  - L29: "owns workflow state", without "and terminal Git mutations".
- `docs/guides/quickstart.md` L55: "since the Ship stage was removed" is dropped. The gh line now reads "`devflow doctor` checks for it; no DevFlow command runs it".
- `docs/guides/unattended-mode.md` and `docs/guides/configuration.md` had no residue: 52-05, 52-10, 52-14 and 52-15 had already rewritten them. Both are unchanged.

## Semver (Task 3)

`cargo semver-checks` 0.51.0. The baseline is the published `devflow-core` 3.0.0 from the crates.io registry cache.

Default run: exit 100, with 9 major checks failing. Each line is matched to a planned removal:

| Check | Items | Planned in |
|---|---|---|
| `module_missing` | `hooks` | 52-12 |
| | `lock` | 52-12 |
| | `version` | 52-13 |
| | `ship` | 52-01 moved the parser, 52-10 removed the cron part, 52-13 deleted the module |
| | `ship_evidence` | 52-13 (`devflow evidence`) |
| `enum_missing` | `hooks::Hook`, `hooks::HookError` | 52-12 |
| | `lock::LockError` | 52-12 |
| | `version::{VersionError, Bump, ChangelogHeading}` | 52-13 |
| | `ship::CronInstructionPathKind` | 52-10 |
| | `ship::ShipError` | 52-13 |
| `struct_missing` | `version::{Version, SelfPin}` | 52-13 |
| | `ship::{ResumeCommand, CronInstructions, HermesCronJob}` | 52-10 |
| | `hooks::HookContext` | 52-12 |
| | `ship_evidence::ShipEvidence` | 52-13 |
| | `lock::LockGuard` | 52-12 |
| `function_missing` | `lock::{remove_stale_locks, acquire_project, acquire_project_blocking}` | 52-12 |
| | 18 `version::*` functions | 52-13 |
| | `ship::{cron_instructions_path, load/write/list/delete/consume_cron_instructions, build_single_agent_cron_instructions, hermes_schedule_from_retry_after, shell_quote}` | 52-10 |
| | `ship::prepend_changelog` | 52-13 |
| | `ship_evidence::{collect, is_stopped_at}` | 52-13 |
| | `hooks::{hooks_for_transition, hooks_after_ship, has_version_file}` | 52-12 |
| | `config::review_angles` | 52-15 |
| | `config::yes_ship` | 52-14 |
| | `config::claude_legacy_launch` | 52-05 |
| | `worktree::{add_detached, reference_path}` | 52-02 (`devflow reference`) |
| `inherent_method_missing` | `DevflowConfig::review_angles` | 52-15 |
| | `DevflowConfig::yes_ship` | 52-14 |
| `pub_module_level_const_missing` | `version::CHANGELOG_SUBJECT_MAX_CHARS` | 52-13 (module deleted) |
| `struct_pub_field_missing` | `State.yes_ship` | 52-14 |
| | `State.legacy_claude_launch` | 52-05 |
| | `CleanReport.orphan_cron_records_removed` | 52-10 |
| | `DevflowConfig.review_angles` | 52-15 |
| | `DevflowConfig.yes_ship` | 52-14 |
| `enum_variant_missing` | `Stage::Ship`, `StageIntent::Ship`, `FixType::AuditFix` | 52-15 |
| | `Stage::Define`, `StageIntent::Define` | 52-17 |
| `enum_no_repr_variant_discriminant_changed` | `Stage::{Plan, Code, Validate}`, `StageIntent::{Plan, Code, Validate}`, `FixType::{GapsOnly, FullExecute}` shifted down by one | A consequence of removing each enum's first variant (`Define`, 52-17; `AuditFix`, 52-15) |

**Nothing unplanned.**

`--release-type major` exits 0. **Limit:** under an assumed major release it runs 0 checks and skips 260, so that exit 0 only proves the comparison could be built. The breaking-change evidence is the default run above.

The version stays 3.0.0, and `Cargo.toml` is unchanged. The 52-19..52-31 public changes are covered by 52-32's re-run.

## Task Commits

1. **Task 1 RED:** `75c0d53` test(52-18): scan for the removed v3 surface
2. **Task 1 GREEN:** `44b372a` feat(52-18): implement the removed-surface scan and split its controls
3. **Task 1 docs:** `1fe0874` docs(52-18): remove the Ship docs
4. **Task 2 RED:** `c468cb0` test(52-18): pin the docs site to the kept pipeline
5. **Task 2 GREEN:** `0f14f16` feat(52-18): docs site describes only Plan, Code and Validate
6. **Task 3:** no code commit. It ran only checks, and `doc_check.rs` needed no change.

**Plan metadata:** the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports (a scratch nextest config, `--profile red`). Each record went through `gsd_run check tdd-red-evidence`, and both verdicts were `RED_EVIDENCE_OK` / `target_test_failed` at exit 100.

| Test | Semantic assessment |
|---|---|
| `doc_check::no_removed_surface_literal_remains` | The target ran and failed on its first planned assertion: the stub finder returned `[]` for the planted `cron-instructions` line (left 0, right 1). |
| `doc_check::docs_site_describes_only_the_kept_pipeline` | The target ran and failed on its first planned assertion: the stub returned `[]` for the planted `Ship` mermaid node (left 0, right 1). After GREEN of the helpers, the real-tree RED listed the 27 findings above, and the rewrite took it to GREEN. |

Gate sequence:
- `test(52-18)` 75c0d53 → `feat(52-18)` 44b372a
- `test(52-18)` c468cb0 → `feat(52-18)` 0f14f16
- No REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode was `end-of-phase`, and Task 1's verify is automated-only. Its 12-test chain was re-run after GREEN and exited 0, so expansion continued.

## Verification run

| Command | Exit |
|---|---|
| Task 1 `<verify>` chain (12 named nextest runs, each `1 passed`) | 0 |
| Task 2 `<verify>` chain (3 named runs) | 0 |
| `cargo semver-checks check-release -p devflow-core` (default) | 100, expected; the list above |
| `cargo semver-checks check-release -p devflow-core --release-type major` | 0 |
| `scripts/check.sh all` | 0 |
| `timeout -k 60 900 scripts/check-in-container.sh` | 0 |

Detail:
- **`scripts/check.sh all`:** it printed "all OK", with 44 `test result:` lines, 1919 passed and 0 failed (summed from the result lines).
- **`timeout -k 60 900 scripts/check-in-container.sh`:** it printed "all OK", with 44 result lines, 1919 passed and 0 failed. Both new tests ran in both environments.
- **Known flake 999.221:** it did not occur.
- **Containers:** `docker ps` listed no container before or after the container run.

## Decisions Made

See key-decisions in the frontmatter.

## Deviations from Plan

**1. [Rule 3 - Blocking] Three control sites beyond the plan's list were split**
- **Found during:** Task 1, on the real-tree pass.
- **Issue:** `stage::tests::plan_is_the_first_stage_and_validate_the_last`, `state::tests::a_state_file_with_a_leftover_yes_ship_key_still_loads` and `gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` hold removed values as the subject of the test.
- **Fix:** the same `concat!` split, as the plan directs for a control beyond its list.
- **Commit:** 44b372a.

**2. [Process] The two page deletions landed in the feat commit, not in "docs: remove the Ship docs"**
- **What happened:** `git rm` staged them before the feat commit, so `44b372a` holds the deletions and `1fe0874` holds the nav, link and `devflow.toml` edits.
- **Effect:** for one commit, mkdocs nav named two missing files. Nothing reads mkdocs at build or test time, so that commit still builds and passes. The commits are left unrewritten.

**3. [Naming] Commit scopes**
- The plan names `test(core): …` and `docs: …`. The commits use the `{type}(52-18): …` scope the GSD TDD gate and the earlier 52-xx plans use, with the plan's descriptions.

**Total deviations:** 1 auto-fix (Rule 3), 2 process notes. **Impact:** no scope added; each control's assertions are unchanged.

## Deferred Items (from earlier SUMMARYs that name 52-18; not taken, outside this plan's files)

Each one is annotated in `deferred-items.md`:
- **`GitFlow::commit_all` and six other uncalled `GitFlow` functions:** deleting them would be a new `pub` removal outside 52-18's files. The semver run here reports only the removals already made.
- **`devflow doctor`'s `gh` check:** still there. Only the quickstart's "since the Ship stage was removed" was dropped.
- **Prose and test names that still describe Ship or a cron resume:** these are comments and test names in `agent_result.rs`, `state.rs`, `staleness.rs`, `pipeline_outcomes.rs` and others. The scan strips comments by design, so it does not see them.
- **`test_support.rs` `init_repo`'s comment naming `VersionBump`/`BranchCleanup`:** a comment, outside the scan and outside the files.
- **`gate_sweep_e2e`'s reap-strays guard:** split here, but its deletion is still open.

The docs-site entry ("Docs-site pages still describe the after-ship hooks") is marked resolved by this plan.

## Issues Encountered

- A decoy in the first docs-site draft put `[Map](index.md)` on `diagrams/module-map.md`, which resolves to `diagrams/index.md` and is a real dead link. The decoy was corrected to `../index.md`. This was a test-authoring error, not a scan defect.

## Limits (what the checks do not establish)

- Both scans are lexical. A removed behaviour rebuilt under a new name passes them.
- The docs-site test catches stage words, dead links and deleted module names, not every stale sentence. Those are listed for a reader under coverage D4.
- Link checking covers inline `[text](target)` links only. A reference-style link `[text][ref]` is not resolved.
- The module-map check reads backticked names in the Modules column and single-identifier mermaid `[label]`s inside a ```` ```mermaid ```` fence.

## Next Phase Readiness

Ready for 52-19. The cut's removed surface is now guarded by the core test suite.

## Self-Check: PASSED

- Files: `crates/devflow-core/src/doc_check.rs` holds `fn no_removed_surface_literal_remains`, `fn docs_site_describes_only_the_kept_pipeline`, `removed_surface_findings` and `docs_site_findings`. `docs/architecture/git-ship.md` and `docs/diagrams/ship-flow.md` are absent.
- Commits: 75c0d53, 44b372a, 1fe0874, c468cb0 and 0f14f16 are ancestors of HEAD.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
