---
phase: 19-release-integrity-main-rs-decomposition
plan: 09
subsystem: cli
tags: [rust, main-rs-split, commands, parallel, config]

requires:
  - phase: 19-release-integrity-main-rs-decomposition
    provides: "19-06 committed split baseline; 19-07 and 19-08 extracted staleness, preflight, and pipeline clusters"
provides:
  - "parallel.rs owns parallel and sequentagent orchestration"
  - "commands.rs owns CLI handlers and their display/reconciliation helpers"
  - "config_parse.rs owns timeout parsing and the gate escalation threshold"
  - "main.rs is a 478-line crate root containing argument types, CliError, dispatch, project_root, and module declarations"
affects: [19-10, 19-11, phase-20]

tech-stack:
  added: []
  patterns:
    - "Flat sibling modules with pub(crate)-only cross-module APIs"
    - "Tests move with the production function they exercise; nested doctor_reconciliation paths remain intact"

key-files:
  created:
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/config_parse.rs
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/tests/devcontainer_ci_failfast.rs

key-decisions:
  - "project_root_walks_up_to_nearest_devflow_ancestor remains in main.rs because it exercises main::project_root directly; the thin root therefore retains one non-env-mutating test module."
  - "config_parse.rs remains a separate module because timeout parsing and its pure parser tests form one responsibility; the plan text's claim that those tests mutate environment under ENV_MUTEX was stale and does not match the code."
  - "commands.rs stays flat: a commands/ hierarchy would not reduce the measured Phase 18 wave count and would centralize shared display helpers again."

requirements-completed: [19e, 19f]

coverage:
  - id: D1
    description: "Parallel/sequentagent cluster extracted mechanically"
    requirement: "19f"
    verification:
      - kind: other
        ref: "11/11 functions reconciled against baseline SHA f35d6c1 with zero unexplained hunks; current-HEAD range re-derived as 679-1130 before extraction"
        status: pass
      - kind: unit
        ref: "cargo test --workspace: baseline per-target counts preserved; 438-name diff empty"
        status: pass
    human_judgment: false
  - id: D2
    description: "Commands/display cluster extracted with nested test namespace preserved"
    requirement: "19f"
    verification:
      - kind: other
        ref: "48 production items reconciled against baseline SHA f35d6c1 with zero unexplained hunks; current-HEAD ranges re-derived as 467-684, 685-1268, and 1292-1966 around the retained project_root item"
        status: pass
      - kind: unit
        ref: "cargo test --workspace: 106/3/4/1/1/3/10/306/2/2/0 per-target counts; 438-name diff empty"
        status: pass
    human_judgment: false
  - id: D3
    description: "Config parsing extracted and main.rs reduced to a thin crate root"
    requirement: "19e"
    verification:
      - kind: other
        ref: "5/5 production items byte-identical against baseline SHA f35d6c1 after normalizing required pub(crate); current-HEAD range re-derived as 35-68"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow three consecutive runs: identical 106-pass unit-test result; workspace 438-name diff empty"
        status: pass
      - kind: other
        ref: "cargo build -p devflow, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --check, and git diff --check all passed"
        status: pass
    human_judgment: false

duration: 1h14m
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 09: Thin CLI Crate Root Summary

**Completed the `main.rs` decomposition by moving parallel orchestration, command handlers, and timeout parsing into flat sibling modules, reducing the crate root from the committed 8,487-line baseline to 478 lines without changing the 438-test inventory or any per-target pass count.**

## Accomplishments

- Extracted all 11 parallel/sequentagent functions and their seven attributed tests into `parallel.rs`; preserved the never-persisted synthetic-state rationale used by D-14.
- Extracted 48 commands/display production items into `commands.rs`, including `start`, workflow-start payload construction, gate target resolution, phase artifact lookup, and the intact nested `doctor_reconciliation` test namespace.
- Extracted the five timeout/configuration items and two pure parser tests into `config_parse.rs`; repointed `commands.rs`, `parallel.rs`, `pipeline_gate.rs`, and `pipeline_outcomes.rs`.
- Audited the full remaining `main.rs`. Its only production items are `Cli`, `Command`, `GateCmd`, `CliError`, `main`, `run`, and `project_root`; its only local test exercises `project_root`.
- Confirmed no empty test module, orphan source module, `commands/` subdirectory, or unrestricted `pub` export was introduced.

## Task Commits

1. `6a9f241` - `refactor(19-09): extract parallel/sequentagent cluster into parallel.rs`
2. `5da0f9b` - `refactor(19-09): extract commands/display cluster into commands.rs`
3. `6f59d69` - `refactor(19-09): extract config_parse and reduce main.rs to a thin crate root`

## Final Line Counts

| Module | Lines |
|---|---:|
| `main.rs` | 478 |
| `commands.rs` | 2,326 |
| `parallel.rs` | 530 |
| `config_parse.rs` | 75 |
| `pipeline_launch.rs` | 585 |
| `pipeline_outcomes.rs` | 1,719 |
| `pipeline_gate.rs` | 789 |
| `preflight.rs` | 772 |
| `staleness.rs` | 1,284 |
| `test_support.rs` | 288 |

## Deviations From Plan

- `project_root_walks_up_to_nearest_devflow_ancestor` stayed in `main.rs` after direct body inspection: it tests `main::project_root`, not a commands-side helper. This follows the plan's attribution rule and its acceptance criterion allowing a named remaining test.
- The plan's Task 3 rationale incorrectly described the two parser tests as environment-mutating and `ENV_MUTEX`-serialized. They call pure parse functions and do not mutate environment. The module comment records the accurate ownership rationale instead; no function or test body changed.
- Mechanical import repointing also touched `pipeline_launch.rs`, `preflight.rs`, `pipeline_outcomes.rs`, and the CI source-path assertion in `devcontainer_ci_failfast.rs`, as required by the compiler and existing structural test.

## Verification

- Recovered Task 1 and Task 2 logs match the baseline target counts exactly and contain zero failures.
- Fresh Task 3 validation passed build, clippy with warnings denied, formatting, three consecutive `devflow` test runs, and an empty diff against all 438 committed trailing test names.
- The five config items produce an empty normalized diff against `f35d6c1`; no source function body was edited.

## Next Phase Readiness

Plan 19-10 can update the architecture and roadmap documentation from measured final module sizes. Plan 19-11 remains the CI and downstream scratch-repository phase gate.
