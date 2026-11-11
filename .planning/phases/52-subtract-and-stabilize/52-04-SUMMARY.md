---
phase: 52-subtract-and-stabilize
plan: 04
subsystem: testing
tags: [rust, e2e, devflow-cli, codex, capture-shape, tdd]

requires:
  - phase: 52-subtract-and-stabilize
    provides: "52-03's gate/start/sweep cuts (the suites this plan edits already carry them)"
provides:
  - "common::capture_agent_bin(logs) -> FakeBin: a fake `codex` that logs argv (agent-argv) and stdin (agent-stdin), commits once, and prints a successful codex --json turn whose agent_message ends in a DEVFLOW_RESULT success line"
  - "common::waiting_capture_agent_bin(logs), common::capture_agent_bin_then(logs, tail), common::CODEX_SUCCESS_EVENTS, common::AGENT_ARGV_LOG / AGENT_STDIN_LOG"
  - "common::write_phase_context(root, phase): the CONTEXT.md `start` requires before it launches codex"
  - "No file under crates/devflow-cli/tests/ uses --legacy-claude-launch, DEVFLOW_CLAUDE_LEGACY_LAUNCH or state.legacy_claude_launch"
  - "The capture witness asserts the launch shape directly (stage_launched launch == \"capture\")"
affects: [52-05, 52-17]

actuals:
  tokens: 12735
  tasks: 2
  commits: 3
plan_head_before: 87054921b86ba1f9805c215c9304fa6406a6fd63
plan_head_after: a0077fd62a0c54b1e7cfd32570f369cf66cf9f8d

tech-stack:
  added: []
  patterns:
    - "A capture-shape e2e uses `--agent codex` plus a fake codex first on PATH, never the Claude legacy flag"
    - "A fixture that starts codex commits a phase CONTEXT.md (common::write_phase_context)"

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/common/mod.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-cli/tests/pending_supervisor_e2e.rs
    - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
    - crates/devflow-cli/tests/start_lock_e2e.rs
    - crates/devflow-cli/tests/test_home_isolation.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "The capture witness's original assertions (stdout/exit-code files, advance_evaluated success) do not tell the two shapes apart: a stream-shape claude run passes all of them. The witness now asserts the stage_launched event's launch field is \"capture\""
  - "Where a suite's fake agent had a specific body (`exit 0`, `sleep 60`, finish-once), the body is kept and only the program name changes to `codex` (common::fake_bin_dir_for or capture_agent_bin_then); capture_agent_bin is used where the fake must finish a stage"
  - "Fixtures that start codex commit a phase CONTEXT.md, because `start` refuses codex without one on the base branch (commands.rs, the RequiresExistingArtifact Define check)"
  - "Planted states that `resume` relaunches name AgentKind::Codex (resume takes the agent from state); the state.legacy_claude_launch line is dropped"
  - "Commit subjects use `test(52-04): …`: type `test` as the plan asks, scope by GSD's {phase}-{plan} rule so tracking can find them"

patterns-established:
  - "Pattern: to get the capture launch shape in an e2e, run codex with a fake from common::capture_agent_bin*"

requirements-completed: [CUT-04]

coverage:
  - id: D1
    description: "The capture-shape witness runs a fake codex end to end and still proves the capture shape"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#capture_supervisor_owns_a_fake_agent_and_records_devflow_result"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every e2e suite that used the legacy flag (or its env var or state field) for the capture shape runs codex instead, with its assertions kept"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "cargo nextest run -p devflow --test pending_supervisor_e2e|gate_notify_persist_e2e|start_lock_e2e|test_home_isolation|gate_wedge_e2e|supervisor_e2e|phase7_cli --no-tests=fail"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 22min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 04: Capture-shape e2e coverage without the legacy flag Summary

**The e2e suites get the capture launch shape from a fake `codex` (`common::capture_agent_bin`) instead of `--legacy-claude-launch`, and the capture witness now checks the launch shape itself, which its old assertions never did.**

## Performance

- **Duration:** 22 min
- **Started:** 2026-11-10T19:21:05Z
- **Completed:** 2026-11-10T19:43:14Z
- **Tasks:** 2
- **Files modified:** 8 (all under crates/devflow-cli/tests/)

## Accomplishments

- `common::capture_agent_bin` and its waiting and custom-tail variants: a fake codex that answers `--version`, logs argv and stdin, and prints the codex `--json` turn the driver parses.
- `supervisor_e2e`'s `Shape::Capture` is `--agent codex` with a fake codex for each run kind. The witness passes, and a control shows a stream-shape run now fails it.
- Seven suites no longer use the legacy flag, its env var or its state field. Every suite passes.

## Finding: the old witness did not discriminate the shapes

The plan assumed the witness's stdout-path, exit-code-path and `advance_evaluated` assertions are capture-only. They are not. With `Shape::Capture` temporarily mapped to `--agent claude` and the finishing Claude stub (stream shape), all of them passed. The run failed only on the argv-log check added in this plan, and that check only shows which fake ran, not which shape. A stream-shape supervisor also writes the stdout and exit-code files. Before this plan the witness therefore never proved the shape. The capture-only flag was the only thing pinning it.

Fix: the witness now also asserts that the only `stage_launched` event has `"launch": "capture"`. Negative control, recorded once and then restored: with the stream mapping the test fails at `supervisor_e2e.rs:529`:

```
assertion `left == right` failed: the one launch ran in the capture shape:
... {"event":"stage_launched","stage":"define","agent":"claude",...,"launch":"stream"}
```

What this check covers and what it leaves out: it reads DevFlow's own launch label (`monitor.rs` `MonitorLaunch` label). It does not separately observe the transport. The stdin-empty assertion adds a behavioural check for codex only.

A second control showed the codex `--json` parser decides the verdict. When the fake's final event was `turn.failed` instead of `turn.completed`, with the same success marker, the witness failed: the run never reached its `--until define` stop within 60 s. This is the plan's stated limit: the fake prints lines this repo's parser accepts, which does not prove a real codex CLI prints them.

## Task Commits

1. **Task 1 (tracer), RED:** `8256aac` test(52-04): run the capture-shape witness as codex, failing without a fake agent
2. **Task 1 (tracer), GREEN:** `55daf7e` test(52-04): add capture_agent_bin, a fake codex for capture runs
3. **Task 2:** `a0077fd` test(52-04): move the e2e suites off --legacy-claude-launch

## TDD Gate Compliance

- **RED** (`8256aac`): the witness is run as codex, with a silent codex stub (`exit 0`). Command: `cargo nextest run -p devflow --test supervisor_e2e --no-tests=fail -E 'test(=capture_supervisor_owns_a_fake_agent_and_records_devflow_result)'`, with JUnit output from a scratch nextest profile. Exit 100. `gsd check tdd-red-evidence` returned `RED_EVIDENCE_OK` (`target_test_failed`, junit, 1 test, 1 fail). Semantic assessment: the target test ran and failed on the planned assertion, `captured stdout missing marker: ""`. A first RED attempt failed on a fixture precondition instead (`phase 67 has no CONTEXT.md on develop, and OpenAI Codex cannot run an interactive discussion headless`). That attempt was rejected as RED, the fixture was fixed, and the run was repeated.
- **GREEN** (`55daf7e`): the witness passed (exit 0). All 10 `supervisor_e2e` tests passed.
- **No `feat(52-04)` commit.** The plan changes no production code, and its own subject rule is `test(...)`. The GREEN step is a `test(52-04)` commit. The `task.is-behavior-adding` verb returned `false` ("<files> has no non-test source file"), so the MVP+TDD gate did not apply.
- Tracer gate: the run was interactive with `end-of-phase` mode and an `<automated>`-only verify. The tracer verify was re-run, passed, and expansion went ahead.

## Call-site dispositions

Every call site was migrated. None was kept, because no test's subject is the flag itself. The flag's own unit tests live in `src/` (`#[cfg(test)]`), not under `tests/`, and are left for 52-05.

| File | Original site | Disposition |
|------|---------------|-------------|
| supervisor_e2e.rs | `Shape::Capture => &["--legacy-claude-launch"]` (start_publishes, sigterm, finishing_run_advances, the witness) | migrated: `Shape::agent()` = `codex`, `Shape::fake_bin()` = capture_agent_bin / waiting_capture_agent_bin |
| supervisor_e2e.rs | `spawn_start` (~L753): race/lock tests | migrated: `--agent codex` + waiting_capture_agent_bin |
| pending_supervisor_e2e.rs | `Fixture::start` (~L168) | migrated: `--agent codex`; `FakeAgent::{Waiting, FinishOnce}` built on capture_agent_bin_then |
| gate_notify_persist_e2e.rs | `spawn_parked_start_with(.., legacy_launch)` (~L202) | migrated: the bool became the agent name (codex for capture, claude for stream); both agent_* arms kept |
| gate_notify_persist_e2e.rs | `resume --legacy-claude-launch` (~L402) | migrated: planted state names Codex, flag dropped |
| start_lock_e2e.rs | `child()` (~L76) | migrated: `--agent codex`; every fake renamed `codex` with its body kept |
| start_lock_e2e.rs | `resume_prints_the_answer_it_set_aside` (~L284), `resume_command` (~L1031) | migrated: flag dropped; planted states name Codex |
| start_lock_e2e.rs | `plant_resumable_run` `state.legacy_claude_launch = true` (~L1005) | migrated: line dropped, agent Codex |
| test_home_isolation.rs | `add_start_args` (~L82) | migrated: `--agent codex`, fake `codex` (`exec sleep 60`) |
| gate_wedge_e2e.rs | `start_child` (~L180) | migrated: `--agent codex`, local fake renamed `codex` |
| phase7_cli.rs | `DEVFLOW_CLAUDE_LEGACY_LAUNCH=true` via `run_devflow_legacy_launch` (~L148; 4 tests: phases 11, 12, 13, 44) | migrated: `--agent codex`, fake `codex` with its body kept; the helper and its parameter removed; CONTEXT.md pre-baked for those phases |

Check: `rg -- '--legacy-claude-launch' crates/devflow-cli/tests` and `rg 'DEVFLOW_CLAUDE_LEGACY_LAUNCH|legacy_claude_launch' crates/devflow-cli/tests` both found nothing (rg exit 1). Control: the same pattern finds 7 hits in `crates/devflow-cli/src/main.rs`.

## Files Created/Modified

- `crates/devflow-cli/tests/common/mod.rs`: capture-agent helpers, log-name constants, `write_phase_context`.
- `crates/devflow-cli/tests/supervisor_e2e.rs`: `Shape::agent`/`fake_bin`, `AgentRun`, a CONTEXT.md fixture, the witness's shape, argv and stdin assertions.
- The six other suites: the migrations in the table above.

## Decisions Made

See `key-decisions` in the frontmatter.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The capture witness did not discriminate the two shapes**
- **Found during:** Task 1 (negative control)
- **Issue:** a stream-shape run passed every original witness assertion, so the plan's negative-control truth could not hold as written.
- **Fix:** added an assertion that the `stage_launched` launch label is `capture`. The control now fails on it.
- **Files modified:** crates/devflow-cli/tests/supervisor_e2e.rs
- **Committed in:** 55daf7e

**2. [Rule 3 - Blocking] `start` refuses codex without a phase CONTEXT.md**
- **Found during:** Task 1 (first RED run)
- **Fix:** the fixtures commit one through `common::write_phase_context`. phase7_cli extends its existing pre-bake loop instead.
- **Files modified:** all suites that start codex
- **Committed in:** 8256aac, a0077fd

**3. [Safety] The RED run used a silent codex stub, not "no fake codex on PATH"**
- **Issue:** the host has a real `codex` at `~/.local/bin/codex`. With no stub, the test's inherited PATH would have launched it.
- **Fix:** RED used `fake_bin_dir_for("codex", "exit 0")`.
- **Committed in:** 8256aac

**4. [Scope] More call sites than the plan listed**
- **Issue:** phase7_cli used the env var in 4 tests, not 1 site. start_lock_e2e set `state.legacy_claude_launch` on a planted state.
- **Fix:** migrated both under the same rule, then removed `run_devflow_legacy_launch` and `run_devflow_inner`, which nothing called any more.
- **Committed in:** a0077fd

**Total deviations:** 4 (1 bug, 1 blocking, 1 safety, 1 scope).
**Impact on plan:** the witness is now stronger than before. The other deviations follow from codex's preconditions. No production code changed.

## Issues Encountered

- The RED commit's subject was 81 characters. The commit-msg hook only warned. Later subjects are shorter.
- The `gate_notify_persist_e2e` test `agent_via_legacy_monitor_never_receives_the_notify_variable` keeps its name, because ROADMAP.md and v3.0.0 archives reference it. It now runs codex through the capture-shape monitor, which is still `MonitorLaunch::Legacy` in `monitor.rs`.

## Verification

- `cargo nextest run -p devflow --test supervisor_e2e --no-tests=fail -E 'test(=capture_supervisor_owns_a_fake_agent_and_records_devflow_result)'`: exit 0.
- Task 2 chain (`pending_supervisor_e2e`, `gate_notify_persist_e2e`, `start_lock_e2e`, `test_home_isolation`, `gate_wedge_e2e`, `supervisor_e2e`, then `scripts/check.sh all`): exit 0. Counts: 5, 11, 20, 2, 5 and 10 tests. `check.sh` ran fmt, clippy `-D warnings` and the workspace tests: 44 `test result: ok` lines, none failed.
- `phase7_cli`: 30/30 passed (it also ran inside `check.sh all`).
- No production file changed: `git log --format= --name-only 87054921b86ba1f9805c215c9304fa6406a6fd63..HEAD -- crates/devflow-core/src crates/devflow-cli/src` printed nothing. BASE = 87054921b86ba1f9805c215c9304fa6406a6fd63.
- Limits: each suite passed once. That shows the migrated outcomes hold, not that they are reliable over repeated runs. The fake codex output matches this repo's parser, not necessarily a real codex CLI.

## User Setup Required

None. No external service configuration is required.

## Next Phase Readiness

Plan 52-05 can delete `--legacy-claude-launch`, `DEVFLOW_CLAUDE_LEGACY_LAUNCH` and `state.legacy_claude_launch` without silently turning any `tests/` e2e into a stream-shape test. The flag's remaining tests are the in-crate unit tests in `crates/devflow-cli/src/` (preflight.rs, commands.rs, main.rs, parallel.rs).

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*

## Self-Check: PASSED

Commits 8256aac, 55daf7e, a0077fd are ancestors of HEAD; `pub fn capture_agent_bin` is in crates/devflow-cli/tests/common/mod.rs; every acceptance check above was re-run in this session (exit codes recorded).
