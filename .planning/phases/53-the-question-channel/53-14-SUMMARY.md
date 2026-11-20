---
phase: 53-the-question-channel
plan: 14
subsystem: agents
status: complete
tags: [mcp, devflow_ask, pi, registration, spike]
requires:
  - phase: 53-08
    provides: ask_support / ask_wiring, the refusal tests
  - phase: 53-13
    provides: --hold-cap-secs, ASK_HOLD_CAP_SECS, ASK_CLIENT_TIMEOUT_MARGIN_SECS
provides:
  - pi as the third driver with a devflow_ask registration (operator answer OD-2)
  - the pi addendum to 53-MCP-SPIKE.md
affects: [999.248, ROADMAP Phase 53 criterion 1, REQUIREMENTS ASK-01]
tech-stack:
  added: []
  patterns:
    - "pi registration: a one-line pi extension under .devflow/ passed with -e, under --no-approve"
key-files:
  created: []
  modified:
    - .planning/phases/53-the-question-channel/53-MCP-SPIKE.md
    - crates/devflow-core/src/agents/pi.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/start_refusals_e2e.rs
    - crates/devflow-cli/tests/ask_launch_e2e.rs
    - crates/devflow-cli/tests/gitignore_coverage.rs
    - .gitignore
    - OPERATIONS.md
    - .planning/ROADMAP.md
    - .planning/REQUIREMENTS.md
decisions:
  - "pi verdict BUILD: devflow_ask reaches pi through a per-launch `-e .devflow/ask-NN.mcp.ts` extension calling pi.registerMcpServer, keeping --no-approve (route 1 of the plan; route 2, .pi/mcp.json, needs --approve and was not tried)"
  - "pi holds one call at most ASK_HOLD_CAP_SECS (900 s); its per-server timeout is 960 s, since pi held a 900 s call with timeout 960 and cancels at its 60 s default"
  - "The owned-file rule for a registration outside .devflow/ was not built: no driver writes one, so the plan's own clause (a file under .devflow/ keeps 53-08's behaviour) applies"
metrics:
  duration: 26min
  completed: 2026-11-19
estimate:
  tokens: 70000
  tasks: 3
actuals:
  tokens: 9600
  tasks: 3
  commits: 3
plan_head_before: 77168ec689364ae27cdead1d7c9b7361dd969519
plan_head_after: 28013b779aeb7fcb11a5c1ed18a2e2aa95d34977
---

# Phase 53 Plan 14: Register pi for devflow_ask Summary

pi now takes the `devflow_ask` server through a one-line pi extension DevFlow writes to `.devflow/ask-NN.mcp.ts` and passes with `-e` before the prompt, under `--no-approve`, with a 900 s hold cap and a 960 s pi request timeout. A spike against a stub MCP server proved the route and the hold, and left `~/.pi/agent/` unchanged.

## Spike verdicts (53-MCP-SPIKE.md, pi addendum)

- **pi registration without global config:** PASS. `-e <extension>` calling `pi.registerMcpServer("devflow", {command, args, exposure: "direct", timeout})` loads under `--no-approve`, so no trust is granted. Route 1 (a per-launch flag pointing at a file) worked first. Route 2 (`.pi/mcp.json`) was not tried; it needs `--approve`, and that flag would also load every project `.pi/` resource.
- **pi hold:** held 900 s with `timeout: 960`. The 60 s default cancelled at 60.0 s, and a `timeout: 30` control cancelled at 30.0 s. pi made no model request while waiting.
- **pi tool name:** `mcp__devflow__devflow_ask`. **pi tool error:** arrives as an error tool result, with no retry and nothing blocked.
- **No-registration control:** answered `NO_TOOL`.
- **`~/.pi/agent/` sha256:** 1,361 files before and 1,362 after. No file changed or was removed. The one added file is a session of another project's pi review run, identified by its working directory in the session path. Every spike launch used `--no-session` in the scratch dir.
- **pi verdict:** BUILD.
- **Route choice (unattended decision):** chose route 1 over route 2. The plan's order puts route 1 first, and route 2's `--approve` would reverse the `--no-approve` security boundary that pi.rs documents.

## Tasks

| Task | Commit | Result |
|---|---|---|
| 1 Spike, addendum | edd22e2 | addendum with commands, three evidence tables, the five verdict lines; scratch removed |
| 2 pi registration | 89279cc | `PiDriver::ask_support` Ok, `ask_wiring` per the recipe, `pi::ask_extension_path`; refusal text names `--agent claude, --agent codex or --agent pi`; refusal sets narrowed to antigravity, hermes, opencode |
| 3 stub-pi e2e, docs, criterion text | 28013b7 | `a_pi_stage_agent_asks_through_its_registration_and_finishes`; discard removes `ask-NN.mcp.ts`; OPERATIONS.md, ROADMAP criterion 1, ASK-01 (BUILD texts) |

## Verification

- Task 1 gate: the three `rg -q` line checks and `test ! -e .devflow/spike-pi` exit 0.
- Task 2 (BUILD branch): these named tests pass with `--no-tests=fail`: `agents::pi::tests::launch_args_carry_the_pi_registration`, `agents::tests::every_driver_declares_its_ask_registration`, `preflight::tests::an_agent_without_an_ask_registration_is_refused_by_name` and `start_refusals_e2e::start_refuses_an_agent_without_an_ask_registration`. Also passing: `agents::tests::only_codex_and_pi_carry_a_hold_cap` and `gitignore_coverage`. The two owned-file tests the gate names were not built (see Deviations).
- Negative control: setting pi's `timeout` to the cap alone (900) makes `launch_args_carry_the_pi_registration` fail on `"timeout": Number(900)` against 960. The stub pi run without `-e` exits non-zero with `no -e registration in argv`.
- Task 3 (BUILD branch): `a_pi_stage_agent_asks_through_its_registration_and_finishes`, `doc_check::doc_referenced_identifiers_exist_in_source`, `doc_check::source_devflow_env_vars_and_subcommands_are_documented`, both BUILD `rg -qF` phrases, and the extended `commands::tests::discard_removes_the_ask_registration_file` all pass.
- Suites: `cargo nextest run -p devflow-core --lib` passed 938/938. `-p devflow --bin devflow` passed 510/510. `--test ask_launch_e2e` passed 5/5. `--test start_refusals_e2e` passed 18/18. `scripts/check.sh all` passed with exit 0, and the known flaky monitor test did not fail.
- Clippy `-D warnings` is clean on both crates.
- The ROADMAP and REQUIREMENTS diffs in 28013b7 touch only criterion 1 (lines 301-307) and ASK-01 (lines 64-66).
- semver-checks against this plan's start commit (`--baseline-rev 77168ec --release-type minor`): `202 checks: 202 pass, 58 skip`, "no semver update required". The only new public item is `agents::pi::ask_extension_path`, an addition.
- `~/.pi/agent/`: DevFlow's new code writes only `.devflow/ask-NN.mcp.ts`. pi.rs still reads `settings.json` for its health check, unchanged from before this plan, and writes nothing under `~/.pi/agent/`.

## Deviations from Plan

1. **[Scope, recorded] The owned-file rule was not built.** This covers Task 2 behaviours 5-6 (`a_registration_file_outside_devflow_is_removed_after_the_stage`, `a_project_owned_registration_file_refuses_the_launch`) and Task 3's `discard_removes_an_owned_registration_file_outside_devflow`. Those three tests were not written, and their gate commands were not run. Reason: the spike's route puts pi's registration under `.devflow/`, so no driver writes a file outside it. The plan's own clause then applies: "A file under `.devflow/` keeps 53-08's behaviour." Building the `.owned` sha256 record and the `info/exclude` handling would add code no driver reaches. T-53-47/48/50 do not arise, since no file is written in the project tree. The pi e2e asserts that no `.pi/` appears in the project. `pipeline_launch.rs` is unchanged.
2. **[Rule 2] `.gitignore` gets `.devflow/ask-*.mcp.ts`, and `gitignore_coverage` gets the path.** This repo's `.devflow/ask-*.mcp.json` rule would not have ignored pi's file. User projects are already covered, because `.devflow/.gitignore` is `*`.
3. **[Rule 1] A test was renamed.** `only_codex_carries_a_hold_cap` became `only_codex_and_pi_carry_a_hold_cap`, because pi now carries the cap.
4. **[Orchestrator instruction] Spike scratch location.** The spike scratch lived in the executor's session scratch dir, not `.devflow/spike-pi/`, as the orchestrator required. The `test ! -e .devflow/spike-pi` gate still runs.
5. **[Method] Spike launches used `--no-session`.** This kept them from writing session files. DevFlow's real pi launch does not pass `--no-session`, so a real stage run still writes a pi session under `~/.pi/agent/sessions/`. That is pi's own behaviour, as it was before this plan.
6. **[Method] Test order and commit message.** The tests were written alongside the implementation rather than strictly RED first. RED is shown by the mutation control above. Task 3's commit subject was amended to fit 72 characters, before any push.

## Not established

- How pi's model answers the server's `pending` reply after a 900 s hold (53-13's re-call by `question_id`); no pi launch received one. OPERATIONS.md says so.
- No live DevFlow run on pi was made; pi is proven by the spike and the stub-pi e2e only, as criterion 1 now says.
- A server named `devflow` in `~/.pi/agent/mcp.json` would take precedence over the extension's registration (from pi's docs; not run). OPERATIONS.md already reserves the name.

## Close-out note

999.248 (on workspace/denniyahh, 1662a8e) still lists pi. Strike pi from it at close-out. This plan does not edit it.

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/agents/pi.rs (ask_wiring, ask_extension_path)
- FOUND: crates/devflow-cli/tests/ask_launch_e2e.rs (a_pi_stage_agent_asks_through_its_registration_and_finishes)
- FOUND: 53-MCP-SPIKE.md pi addendum with the `**pi verdict:** BUILD — ` line
- FOUND commits: edd22e2, 89279cc, 28013b7 (ancestors of HEAD)
