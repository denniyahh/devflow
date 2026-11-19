---
phase: 53-the-question-channel
plan: 08
subsystem: agent-launch
tags: [mcp, devflow_ask, agents, launch, refusal, gitignore, discard]
status: complete
requires:
  - phase: 53-01
    provides: the `__ask` flag contract (`--project --phase --stage --timeout-secs --run-started-at`)
  - phase: 53-07
    provides: the `__ask` server the e2e stubs drive, and the ask lock `.devflow/.ask-NN-<stage>`
  - phase: 53-12
    provides: e2e suites already retargeted off the refused agents
provides:
  - AgentDriver::ask_support / ask_wiring (defaulted), AskRegistration, AskWiring, ArgPlacement, ASK_CLIENT_TIMEOUT_MARGIN_SECS
  - per-launch registration of the devflow MCP server for claude (--mcp-config file) and codex (-c overrides)
  - ensure_ask_registration refusal of opencode, pi, hermes and antigravity at start, resume, fallback, handoff and every stage launch
affects: [53-13, 53-14]
tech-stack:
  added: []
  patterns:
    - "one production ask_wiring launch site (wire_ask_registration, called only by prepare_stage_launch), pinned by a source scan with a planted control"
key-files:
  created:
    - crates/devflow-cli/tests/ask_launch_e2e.rs
  modified:
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-core/src/agents/claude.rs
    - crates/devflow-core/src/agents/codex.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/start_refusals_e2e.rs
    - crates/devflow-cli/tests/gitignore_coverage.rs
    - .gitignore
    - OPERATIONS.md
key-decisions:
  - "The source scan allows one more ask_wiring( call besides the launch: the driver conformance case ask_declarations_agree in agents/mod.rs, which the plan itself requires and which launches nothing"
  - "The launch call site is wire_ask_registration (a helper only prepare_stage_launch calls); the scan pins both hops"
  - "Stub agents write answer.txt beside themselves, not in the project root, so the Plan stage sees a clean tree"
  - "The registration temp file is .devflow/.ask-NN.mcp.json.tmp, covered by the ask lock's .gitignore pattern"
requirements-completed: [ASK-01]
duration: about 30 min
completed: 2026-11-19
commits: 5
plan_head_before: 09dfe8fecc14e66c9bf880a75dcf3e44911193f2
plan_head_after: 98b69fe24121185302fd3b50d7e88679fa717e06
actuals:
  tokens: 16900
  tasks: 3
  commits: 5
---

# Phase 53 Plan 08: devflow_ask Registration in Every Stage Launch Summary

Every claude and codex stage launch now registers the runner's own binary as the `devflow` MCP server. Claude gets it through a one-line `--mcp-config` file appended last, and codex through escaped `-c mcp_servers.devflow.*` overrides placed before the prompt. opencode, pi, hermes and antigravity are refused by name before any agent process starts. Runner-driven codex and claude stubs reach the server through their real argv and get the operator's `gate answer` in the same process.

## Performance

- **Duration:** about 30 min. The start time was not recorded. The first commit was at 11:31 and the last at 11:49 local.
- **Tasks:** 3 of 3.
- **Files:** 11 changed, 1272 insertions, 24 deletions.

## Task Commits

1. **Task 1: Drivers declare their ask registration.** RED `84de904`, GREEN `c1cf8a6`.
2. **Task 2: Launches carry the registration; the four unregistered drivers are refused.** RED `1e05765`, GREEN `17fbfb1`.
3. **Task 3: Runner-driven stub agents ask through their registration; start refuses the unregistered agents.** `98b69fe`. These are tests and docs over Task 2's code.

## TDD Gate Compliance

- **Task 1 RED (`84de904`).** All three target tests failed on their own assertions. `every_driver_declares_its_ask_registration` failed on "Claude Code registers the server". The claude and codex tests failed on `expect("… registers the server")`, because the defaulted methods returned the refusal. The crate built and clippy was clean.
- **Task 2 RED (`1e05765`).** The tests compiled against stubs: a no-op `splice_ask_args`, an `ensure_ask_registration` that admits everything, and a discard that did not know the file. Each failed on its intended assertion:
  - The splice test failed on the arg order.
  - The scan test listed only the conformance site.
  - The preflight test failed on "opencode has no registration and is refused".
  - The discard test failed with "a registration file left after discard".
  - `gitignore_coverage` named both `.devflow/ask-07.mcp.json` and `.devflow/.ask-07-code`.
  - The rewritten handoff test passed at RED. It is a retarget that keeps an existing property, not a new behaviour.
  - The stubs carried a temporary `#[allow(dead_code)]` so that the RED commit passed clippy. GREEN removed it.
- **Task 3.** All five new tests passed on their first run, because Task 2 had already built what they pin. Negative controls were run in their place:
  - With `splice_ask_args` made a no-op, both stub-launch tests failed: the runner exited before any gate opened.
  - With `ensure_ask_registration` admitting every agent, both start refusal tests failed with "start with pi must refuse" and "start with antigravity must refuse".
  - Both files were restored from a backup afterwards, and `git diff` was empty.
- The RED evidence was checked by hand against the nextest output. `gsd_run check tdd-red-evidence` was not run.

## Verification

- **Task 1 gates.** All three named tests pass. `cargo nextest run -p devflow-core --lib`: 933 of 933 passed.
- **Task 2 gates.** All six named tests pass, including `gitignore_covers_devflow_runtime_state_paths`. `cargo nextest run -p devflow --bin devflow`: 513 passed, 3 skipped.
- **Task 3 gates.** These pass: the three `ask_launch_e2e` tests, the two new `start_refusals_e2e` tests, and `doc_check::doc_referenced_identifiers_exist_in_source`.
- **`scripts/check.sh all`.** Exit 0 ("all OK") on the run after all code and doc changes. The known flaky monitor test did not fail.
- **semver-checks.**
  - Against `--baseline-rev origin/develop --release-type minor`: exit 100, with 202 checks, 198 passing and 4 failing. All four failures come from earlier Phase 53 plans, not from this one: the `GateKind::Question` variant, the removed checkpoint functions (`phase_has_blocking_human_checkpoint`, `checkpoint_auto_decide_prompt`, `blocking_human_checkpoint_reported`, `checkpoint_reported_in_capture`), `ClaudeDriver::exec_resume_command`, and `GATE_RESOLUTION_RULE`.
  - Against this plan's start (`--baseline-rev 09dfe8f`): exit 0, with 202 of 202 checks passing ("no semver update required"). This plan's new trait methods, types and const read as additive.

## Production `ensure_agent_binary(` sites, each followed by `ensure_ask_registration(`

1. `commands.rs` `start_dry_run`: `ensure_agent_binary(agent_program(state.agent))?;` then `ensure_ask_registration(state.agent)?`.
2. `commands.rs` `start_reserved`: `ensure_agent_binary(agent_program(agent))?;` then `ensure_ask_registration(agent)?`.
3. `pipeline_launch.rs` `launch_stage`: `ensure_agent_binary(program)?;` then `ensure_ask_registration(state.agent)?`.
4. `pipeline_launch.rs` `hand_off_agent`: `ensure_agent_binary(agent_program(to)).and_then(|()| ensure_ask_registration(to))`, which refuses as `HandoffRefusal::Unavailable`.
5. `pipeline_launch.rs` `resume_with_fallback`: `ensure_agent_binary(...).and_then(|()| ensure_ask_registration(state.agent))`, before the unit launch.
6. `preflight.rs` `validate_fallback_agent`: `ensure_agent_binary(agent_program(fallback))?;` then `ensure_ask_registration(fallback)?`.

## Unit Tests the Refusal Invalidated (Orchestrator Decision)

- **`pipeline_launch::tests::a_handoff_reruns_the_delivery_canary_for_the_new_agent`** (the known case). It was rewritten from a claude run handed to antigravity into a codex run handed to claude. Its assertions are unchanged: the recorded `Confirmed` control admits a claude launch without a canary run, the hop clears the canary and the session id, and the fallback's own canary runs once and refuses on `Absent`. The reason is operator answer (2): antigravity is refused.
- **Others.** The full bin suite (513 passed) reported no other failure, so no other test was rewritten or deleted.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The call-site scan as written would have failed on code the plan itself requires**
- **Found during:** Task 2.
- **Issue:** The plan asks for exactly one `.ask_wiring(` call across `crates/` non-test source, in `prepare_stage_launch`. Task 1's required conformance case (`contract_checks` → `ask_declarations_agree`) is also non-test source and calls `ask_wiring`. I also put the launch call in a helper, `wire_ask_registration`.
- **Fix:** The scan now covers both crates' `src` trees recursively. It asserts exactly two sites: `devflow-cli/src/pipeline_launch.rs: wire_ask_registration` and the conformance case `devflow-core/src/agents/mod.rs: ask_declarations_agree`. A second scan asserts that `wire_ask_registration(` has one caller, `prepare_stage_launch`. The planted control (an extra call site in a canary function) is reported. T-53-21's property, that no canary or preflight launch is wired, holds.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`.
- **Commit:** `1e05765` / `17fbfb1`.

**2. [Rule 2 - Missing] The refusal text names the lowercase agent**
- **Found during:** Task 2.
- **Issue:** The driver's refusal names the driver display name (`OpenCode`, `Antigravity`). The start and runner tests require the CLI agent name (`opencode`, `antigravity`).
- **Fix:** `ensure_ask_registration` and `wire_ask_registration` prefix the refusal with ``refusing to launch `<agent>`:``.
- **Commit:** `17fbfb1`.

**3. [Rule 3 - Blocking] The stubs write `answer.txt` beside themselves**
- **Found during:** Task 3.
- **Issue:** The plan has the stubs write `answer.txt` in the project.
- **Fix:** The stubs write it in their own fake-bin dir, so the Plan stage sees no untracked file in the project tree. The assertion is unchanged.
- **Commit:** `98b69fe`.

### Notes
- `ensure_ask_registration` maps the refusal to `CliError::Message`, as the plan says.
- A non-UTF-8 executable or project root is refused with `Err` by both drivers, never with `expect`. Codex has an extra unit test for this, `ask_wiring_refuses_a_non_utf8_project_root`.
- `discard` also lists the registration file in its listing and counts it as debris, so a phase whose only leftover is that file is still discardable.
- No new `### Phase 999.N` entry was filed. OPERATIONS.md names the existing 999.248.

## Threat Flags

None. The only new surface is DevFlow-authored argv and a file under `.devflow/`, which T-53-20 to T-53-23 and T-53-40/41 already cover.

## Self-Check: PASSED

- FOUND: crates/devflow-cli/tests/ask_launch_e2e.rs
- FOUND commits: 84de904, c1cf8a6, 1e05765, 17fbfb1, 98b69fe (all ancestors of HEAD)
