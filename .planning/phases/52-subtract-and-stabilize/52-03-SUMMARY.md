---
phase: 52-subtract-and-stabilize
plan: 03
subsystem: cli
tags: [rust, clap, devflow-cli, cut, gate, doctor, tdd]

requires:
  - phase: 52-subtract-and-stabilize
    provides: "52-02's phase7_cli removed-subcommand test shape"
provides:
  - "`devflow gate approve|reject` take the stage only as `--stage` and the project only as `--project`; a positional stage or project is rejected by clap"
  - "`devflow start --worktree` (the hidden no-op) is rejected by clap"
  - "`devflow gate sweep --reap-strays` is rejected by clap; the stray reaper and tests/reap_strays_e2e.rs are gone"
  - "doctor still reports 2.x strays, with a manual repair hint: confirm the pid with `ps -o args= -p <pid>`, then `kill <pid>`"
  - "phase7_cli::removed_flags_are_unknown with a REMOVED_FLAG_INVOCATIONS list for later cut plans"
affects: [52-04, 52-19]

actuals:
  tokens: 14399
  tasks: 2
  commits: 4
plan_head_before: 82e4182b7de4d7230d701e307c9b6629f3b17d2e
plan_head_after: 252219b7e310cb9ede696169491247216e5215a2

tech-stack:
  added: []
  patterns:
    - "A removed flag gets an entry in REMOVED_FLAG_INVOCATIONS as (argv, token): clap must name that exact token as `unexpected argument`, so a refusal for some other reason does not pass"
    - "A removed destructive flag is listed together with `--dry-run`, so the test stays safe against a binary that still has the flag"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
    - crates/devflow-cli/tests/hermetic_spawn_sites.rs
    - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
    - ARCHITECTURE.md
  deleted:
    - crates/devflow-cli/tests/reap_strays_e2e.rs

key-decisions:
  - "resolve_gate_target was deleted, not reduced: with the positionals gone it would only return its inputs. The `stage_option` field is renamed `stage` (still `--stage`), and dispatch passes it straight to gate_respond"
  - "REMOVED_FLAG_INVOCATIONS holds (argv, token) pairs, not bare argv as the plan sketched. Before the removal, `start --phase 1 --worktree x --dry-run` was already refused, for its missing `--mode`, so a bare `unexpected argument` check could pass for the wrong reason"
  - "The sweep invocation carries `--dry-run` (`gate sweep --reap-strays --dry-run`). clap still names `--reap-strays`, and the test cannot run a real machine-wide reap against a binary that still has the flag"
  - "Doctor's repair hint text: `confirm pid <pid> is the stray with `ps -o args= -p <pid>`, then end it with `kill <pid>`` (RESEARCH Open Question 4: rewrite, not drop)"
  - "reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates is renamed reachable_pids_are_excluded_from_the_stray_findings. The filtered list it checks is the one doctor's findings come from, so its assertions stay; only the reaper reference in a comment went. gate_sweep_without_reap_strays_flag_ignores_a_live_stray is renamed gate_sweep_ignores_a_live_stray"

patterns-established:
  - "Pattern: grow REMOVED_FLAG_INVOCATIONS in phase7_cli.rs for each later flag removal, naming the token clap must reject"

requirements-completed: [CUT-04]

coverage:
  - id: D1
    description: "`devflow gate approve 1 code` and `devflow gate reject 1 code --note n` are rejected by clap naming `code`; `devflow gate approve --help` still succeeds"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_flags_are_unknown"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_approve_arg_parsing_takes_the_stage_option"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every caller that answered a gate positionally now uses `--stage`/`--project` and keeps its outcome"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "cargo nextest run -p devflow --test gate_wedge_e2e --no-tests=fail (5 passed)"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#approving_the_failed_canary_gate_stops_the_run_and_names_the_way_on"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#rejecting_the_failed_canary_gate_stops_the_run_and_names_the_way_on"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_notify_persist_e2e.rs#agent_via_default_launch_canary_never_receives_the_notify_variable"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_worktree_cli_flow"
        status: pass
    human_judgment: false
  - id: D3
    description: "`devflow start --phase 1 --worktree x --dry-run` is rejected by clap naming `--worktree`"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_flags_are_unknown"
        status: pass
    human_judgment: false
  - id: D4
    description: "`devflow gate sweep --reap-strays --dry-run` is rejected by clap naming `--reap-strays`; the reaper code and reap_strays_e2e.rs are gone and HERMETIC_SUITES no longer lists it"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_flags_are_unknown"
        status: pass
      - kind: other
        ref: "crates/devflow-cli/tests/hermetic_spawn_sites.rs#migrated_suites_spawn_devflow_only_through_the_hermetic_builder"
        status: pass
    human_judgment: false
  - id: D5
    description: "Doctor's stray repair names the pid, `ps -o args= -p <pid>` and `kill <pid>`, and not `--reap-strays`"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::stray_process_finding::build_stray_process_findings_names_pid_layer_and_repair"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::stray_process_finding::render_stray_process_text_names_pid_and_repair_when_present"
        status: pass
    human_judgment: false
  - id: D6
    description: "Full workspace check suite green after the removals"
    verification:
      - kind: other
        ref: "scripts/check.sh all (exit 0; fmt, clippy -D warnings, workspace tests: 2127 passed, 0 failed)"
        status: pass
    human_judgment: false

duration: 21min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 03: Remove the legacy gate, start and sweep forms Summary

**Three legacy CLI forms are gone, and clap now rejects each one: the positional stage or project on `gate approve|reject`, the hidden `start --worktree`, and `gate sweep --reap-strays`. The 2.x stray reaper is deleted with its e2e suite. `doctor` still reports strays, and its repair hint now tells the operator to check the pid with `ps` and then `kill` it.**

## Performance

- **Duration:** 21 min
- **Started:** 2026-11-10T18:52:08Z
- **Completed:** 2026-11-10T19:12:47Z
- **Tasks:** 2
- **Files modified:** 10 (9 modified, 1 deleted), plus `deferred-items.md`

## Accomplishments
- `GateCmd::Approve` and `GateCmd::Reject` lose `stage: Option<String>` and `legacy_project`. `--stage` is the only stage selector, and `resolve_gate_target` is deleted.
- `start` loses its hidden `worktree` flag and the `_worktree` binding.
- `GateCmd::Sweep` loses `reap_strays`. Also deleted:
  - `gate_sweep`'s stray pass;
  - `StrayReapOutcome`, `StrayReapResult` and `reap_stray_candidates`;
  - their 9 unit tests;
  - `tests/reap_strays_e2e.rs`.
- Doctor's stray detection stays (`unreachable_stray_candidates`, `build_stray_process_findings`), with the manual repair hint.
- `removed_flags_are_unknown` checks that clap names each removed token as `unexpected argument`. `gate approve --help` is the control.

## Task Commits

1. **Task 1 (tracer): gate positionals and `start --worktree`**
   - RED `78663e0` test(52-03): add failing test for removed gate positionals and start --worktree
   - GREEN `c6c8d93` feat(52-03)!: drop gate answer positionals and start --worktree
2. **Task 2: `gate sweep --reap-strays` and the reaper**
   - RED `88dd87b` test(52-03): expect sweep --reap-strays unknown, a manual stray hint
   - GREEN `252219b` feat(52-03)!: remove gate sweep --reap-strays and the stray reaper

No REFACTOR commits. `78663e0`'s subject is 79 characters; the commit-msg hook warned and did not block.

## Callers found by the plan's two searches, and what was done

| Site | Form | Done |
|---|---|---|
| `gate_wedge_e2e.rs` `reject()` | positional `"code"` + positional project `.arg(root)` | `--stage code`, `--project <root>` |
| `gate_wedge_e2e.rs` `reject_define()` | positional `"define"` + positional project | `--stage define`, `--project <root>` |
| `failover_e2e.rs` `answer_the_failed_canary_gate` | positional `"code"` after `answer[0]` | `--stage code` (its `--project` was already a flag) |
| `gate_notify_persist_e2e.rs` `agent_environment_after_define` | positional `"define"` + positional project | `--stage define`, `--project <root>` |
| `commands.rs` parse test | positional `ship`, positional project | renamed `gate_approve_arg_parsing_takes_the_stage_option`, kept the `--stage` and bare halves |
| `phase7_cli.rs:1041` (`cleanup_worktree_cli_flow`'s teardown) | **not found by the searches**: `run_devflow` appends the root as a positional | added `"--project"` as the last argument so the appended root is its value |
| `phase7_cli.rs:1002` (same test) | **not found by the searches**: `start ... --worktree` | flag dropped (a worktree is the default) |
| `stop_e2e.rs:783`, `stop_e2e.rs:1145`, `log_format_env.rs:175`, `phase7_cli.rs:1000` | already `--project`/`--stage` | unchanged |

The two missed sites came to light when `cleanup_worktree_cli_flow` failed (`unexpected argument '/tmp/...'`). After that, a full `cargo nextest run -p devflow` passed: 972 tests, 0 failed.

## TDD Gate Compliance

RED evidence is nextest JUnit XML from a scratch nextest config (`--profile red`). Each record was checked with `gsd-tools check tdd-red-evidence`.

| Target | Verdict | Semantic assessment |
|---|---|---|
| `phase7_cli::removed_flags_are_unknown` (Task 1) | RED_EVIDENCE_OK | Failed at the token assertion: `start --worktree x` was parsed, and clap complained only about the missing `--mode`. The loop stops at the first failure, so the two gate invocations were probed on their own against the pre-removal binary. Both were accepted by clap and ran the handler (`no open gate for phase 1 stage code`, exit 1). |
| `phase7_cli::removed_flags_are_unknown` (Task 2) | RED_EVIDENCE_OK | The first three invocations passed (Task 1 had already removed them). It then failed at "must be rejected": `gate sweep --reap-strays --dry-run` ran and exited 0. |
| `…stray_process_finding::build_stray_process_findings_names_pid_layer_and_repair` | RED_EVIDENCE_OK | Failed at the first hint assertion, on the old `devflow gate sweep --reap-strays --dry-run ...` text |
| `…stray_process_finding::render_stray_process_text_names_pid_and_repair_when_present` | RED_EVIDENCE_OK | Failed at the repair-line assertion, on the old text |

Each `feat(52-03)!:` commit descends from its plan's `test(52-03):` commit. The Task 1 RED commit was staged from a blob (HEAD's file plus the new test only), so it holds the test alone. The working tree already had the GREEN edits at that point.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<automated>` chain (7 nextest runs, `--no-tests=fail`) | 0 |
| `cargo nextest run -p devflow --no-fail-fast` after Task 1 | 0 (972 passed, 4 skipped) |
| Task 2 targeted runs (`removed_flags_are_unknown`, `migrated_suites_spawn_devflow_only_through_the_hermetic_builder`, both stray-finding tests) | 0 |
| `scripts/check.sh all` | 0 (fmt, clippy `--workspace --all-targets -D warnings`, workspace tests: 2127 passed, 0 failed) |
| After a comment-only edit to `registry_reachable_pids`'s doc made during the check run: `cargo fmt --all --check`, `cargo clippy -p devflow --all-targets -D warnings`, the render test | 0 |

Negative controls:
- At `88dd87b`, `commands.rs` had 39 lines with `reap_stray_candidates|StrayReapOutcome` and `reap_strays_e2e.rs` existed. At HEAD, `rg` finds no match and the file is gone.
- HEAD~4's `main.rs` had 10 lines with `legacy_project|_worktree\b`.
- At HEAD, the `_worktree\b` pattern still matches 3 lines, all `no_worktree`, which is the remaining real flag. No `_worktree` binding is left. Read literally, the acceptance criterion "no `_worktree`" would match those lines; the binding it targets is gone.
- `read_suite` panics on a missing file (`unwrap_or_else(panic!)`), so leaving the deleted file in `HERMETIC_SUITES` would have failed the hermetic scan.

**What this does not establish:**
- The e2e test proves clap rejects the three forms named. It does not cover other spellings, for example `gate approve 1 /some/path`. That is the same positional slot, but no test exercises it.
- The stray detection is only re-tested through its hint. Its detection tests are untouched, as the plan's `<verification>` says.
- `devflow-core` is not changed, so no semver check was run. Its stray primitives are now partly orphaned (see Deferred).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Two callers the plan's searches missed, both in `cleanup_worktree_cli_flow`**
- **Found during:** Task 1
- **Issue:** `run_devflow` appends the root positionally, so the teardown `gate reject 8 --stage validate --note ...` passed a positional project. The same test ran `start ... --worktree`.
- **Fix:** added `"--project"` before the appended root, and dropped `--worktree`.
- **Files modified:** `crates/devflow-cli/tests/phase7_cli.rs`
- **Verification:** `phase7_cli` 30/30, then the full devflow suite (972 passed)
- **Committed in:** `c6c8d93`

**2. [Rule 1 - Bug] The test would have accepted a refusal for the wrong reason**
- **Found during:** Task 1 RED
- **Issue:** before the removal, `start --phase 1 --worktree x --dry-run` was already refused, for its missing `--mode`, so a bare `unexpected argument` check was weak.
- **Fix:** each entry names the token clap must reject: `unexpected argument '<token>'`.
- **Committed in:** `78663e0`

**3. [Rule 2 - Safety] The sweep invocation carries `--dry-run`**
- **Found during:** Task 2 RED
- **Issue:** the plan's `gate sweep --reap-strays` ran as a **real, machine-wide reap pass** against the pre-removal binary in the first RED run. On the hermetic builder the registry holds nothing, so every 2.x-shaped process the user owns counts as a stray.
- **Evidence nothing was reaped:** the test took 32 ms in total. Afterwards, a dry-run pass found 0 candidates, and `ps` showed no `devflow advance` or `sh -c` monitor-shaped process. This is consistent with there being nothing to reap. It is not proof: no event is written when the registry is empty.
- **Fix:** the entry is `gate sweep --reap-strays --dry-run`, and RED was re-run in that form. This also keeps `gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` passing.
- **Committed in:** `88dd87b`

**4. [Rule 1 - Stale docs/comments] References to removed code**
- `ARCHITECTURE.md` called `--worktree` "a deprecated no-op kept for one release". That sentence is removed.
- Comments that named `reap_strays_e2e.rs` or the reaper were reworded:
  - in `commands.rs`: two test teardowns, the `stray_layer_label` and `unreachable_stray_candidates` docs, and the `registry_reachable_pids` RESIDUAL note, which now says nothing signals on the set;
  - in `auto_chain_leak_repair_e2e.rs`: one comment.
- **Committed in:** `c6c8d93`, `252219b`

**5. Plan items that needed no change**
- OPERATIONS.md and README.md had no positional gate-answer form and no `--reap-strays` mention: `rg` found none. They are listed in `files_modified` but are unchanged.

---

**Total deviations:** 4 auto-fixed (1 blocking, 2 bug, 1 safety), 1 no-op item
**Impact on plan:** All within the plan's goal. No scope creep: core code and `gate_sweep_e2e.rs` were left alone and logged instead.

## Issues Encountered
- The first Task 2 RED run executed a real `--reap-strays` pass on the host (deviation 3).

## Deferred Items
Two entries were added to `.planning/phases/52-subtract-and-stabilize/deferred-items.md`:
- In `crates/devflow-core/src/agent.rs`, `process_age` and `STRAY_MIN_AGE` have no caller outside core now. Three doc comments there still name `reap_stray_candidates`/`--reap-strays`. Both items are `pub` in the published crate. That fix belongs in a plan that touches devflow-core under the 4.0.0 bump.
- `gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` now guards a flag clap rejects. That file was not in this plan's files.

## User Setup Required
None.

## Next Phase Readiness
Ready for 52-04. The gate CLI surface is clear for the abort verb (52-19). Later flag removals add entries to `REMOVED_FLAG_INVOCATIONS`.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*

## Self-Check: PASSED
