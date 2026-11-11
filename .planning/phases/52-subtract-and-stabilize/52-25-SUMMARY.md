---
phase: 52-subtract-and-stabilize
plan: 25
subsystem: cli
tags: [status, cleanup, start, d-35, ops-02, ops-04]

requires:
  - phase: 52-08
    provides: a finished run keeps its state (`finished: true`, `stopped: true`, `FINISHED_STOP_REASON`)
  - phase: 52-24
    provides: Validate routed by GSD status only; `verification_baseline_captured` kept for `run_never_prepared`
provides:
  - "`status`: per-phase `run:` line (active / stopped / finished / not active when the worktree is gone), `stop reason:` verbatim, `fallback: armed|spent|none`; header lists only active runs and names the kind of every other phase"
  - "`cleanup`: keep line quotes the recorded stop reason; a finished run is offered only `--force`, a stopped one `resume` or `--force`; a live run's refusal says a run is live and names `devflow stop`"
  - "`cleanup --force` removes the run's `.devflow/state-NN.json` with its worktree (999.213)"
  - "`cleanup` never deletes a merged branch that is still checked out (new additive `GitFlow::cleanup_merged_keeping`)"
  - "`start` prints a replace notice over a finished, stopped (with reason) or unloadable state, also under `--dry-run`"
affects: [52-27 gap loop, 52-29/52-33 gate routes, close-out docs]

actuals:
  tokens: 12680
  tasks: 3
  commits: 6
plan_head_before: 9b987ab34eeef3fa929c3ed7bbd5cb370c577712
plan_head_after: 7350dd039d65846d0c7ffc020df325d129432eca

tech-stack:
  added: []
  patterns:
    - "status lines built by pure helpers (`status_run_lines`, `active_phases_line`) so unit tests read them without capturing stdout"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-core/src/git.rs
    - OPERATIONS.md
    - .planning/phases/52-subtract-and-stabilize/deferred-items.md

key-decisions:
  - "A run is active in `status` only when it is not stopped and its recorded worktree exists; supervisor liveness does not make a worktree-less run active"
  - "`status`'s header keeps its `active phases:` line, listing only active runs (or `none`), and adds `other phases:` naming each other phase's kind"
  - "`cleanup --force` removes a state only when that state records the removed worktree (or none), so a state joined by name that owns another path is left alone"
  - "Kept branches are every branch still checked out after the sweep (`worktree::list`), passed to an additive `GitFlow::cleanup_merged_keeping`; `cleanup_merged` keeps its old behaviour"
  - "`start`'s replace notice is printed where `start` first loads the state (`fresh_state_carrying_phase_failures`), so `--dry-run` shows it; it can precede a later refusal by the live-run guard"
  - "Whether a start over a finished state carries `phase_validate_failures` is left for the operator (deferred-items.md), not decided here"

patterns-established:
  - "Keep/replace messages quote the run's recorded `stop_reason` verbatim instead of a cause guessed from `stopped`"

requirements-completed: [OPS-02, OPS-04]

coverage:
  - id: D1
    description: "status prints the stop reason, finished state and fallback armed/spent/none"
    requirement: OPS-04
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::status_prints_the_stop_reason_and_fallback_state"
        status: pass
    human_judgment: false
  - id: D2
    description: "status never reports a run with a missing worktree as active (control: present worktree reads active)"
    requirement: OPS-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::status_does_not_report_a_run_with_a_missing_worktree_as_active"
        status: pass
    human_judgment: false
  - id: D3
    description: "cleanup names why it keeps a worktree: finished, --until halt, live run, each only its own reason"
    requirement: OPS-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_names_why_it_keeps_a_worktree"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_worktree_cli_flow"
        status: pass
    human_judgment: false
  - id: D4
    description: "cleanup --force removes the run's state file with its worktree, and leaves another phase's state"
    requirement: OPS-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_force_removes_the_run_state_with_the_worktree"
        status: pass
    human_judgment: false
  - id: D5
    description: "cleanup never deletes a merged branch a kept worktree has checked out"
    requirement: OPS-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_keeps_the_branch_of_a_kept_worktree"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::cleanup_merged_keeping_skips_a_kept_branch"
        status: pass
    human_judgment: false
  - id: D6
    description: "start says when it replaces a finished, stopped or unloadable state; nothing for a fresh phase"
    requirement: OPS-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_says_when_it_replaces_a_finished_or_stale_state"
        status: pass
    human_judgment: false

duration: 23min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 25: status, cleanup and start say why Summary

**`status` prints each run's kind, verbatim stop reason and fallback state (`armed`/`spent`/`none`) and never calls a worktree-less run active. `cleanup` quotes the recorded stop reason when it keeps a worktree, and `--force` also deletes the run's state file. A still-checked-out merged branch is never swept. `start` names the finished, stopped or unreadable state it replaces.**

## Performance

- **Duration:** about 23 min
- **Started:** 2026-11-11T08:28:37Z
- **Completed:** 2026-11-11T08:51Z
- **Tasks:** 3
- **Files modified:** 4 code/doc files, plus `deferred-items.md`

## Accomplishments

- **`status` (OPS-04, OPS-02):** each phase gets a `run:` line (`active`, `stopped`, `finished`, or `not active — its worktree <path> no longer exists`) and a `fallback:` line. A stopped or finished run also gets a `stop reason:` line, quoted verbatim. The worktree line is marked `(missing)` when the directory is gone. The `active phases:` header lists only active runs, or `none`, and a new `other phases:` line names each other phase's kind. Every earlier line is still printed.
- **`cleanup` (OPS-02, D-35):** the old message said "halted via --until; run `devflow resume`" for every stopped state. It now quotes the recorded stop reason. A finished run is offered only `--force`, because `resume` refuses a finished run; this closes the leftover from 52-08. A stopped run is offered `devflow resume --phase N` or `--force`. A live run's refusal now says "a run is live" and still names `devflow stop --phase N`.
- **`cleanup --force` (999.213):** removes the run's `.devflow/state-NN.json` after its worktree, and prints `removed run state <path>`.
- **Merged branches:** `cleanup` collects every branch still checked out after the sweep and passes it to a new, additive `GitFlow::cleanup_merged_keeping`. The sweep skips those branches instead of leaving the refusal to git. Before, git refused the delete and cleanup printed a "could not delete the merged branch" warning.
- **`start` (D-35):** where `start` first loads the state, it prints `note: phase N: this start replaces the state of a finished run (stop reason: …)`, or the same for a stopped run, or `… replaces an unreadable state (<error>)`. `--dry-run` reaches that point.
- OPERATIONS.md documents the new `status` lines, cleanup's keep reasons and `--force`, and start's replace notice.

## Task Commits

1. **Task 1 (tracer): `status`.** `95da581` test (RED), `a6d2544` feat (GREEN)
2. **Task 2: `cleanup`.** `28c0c30` test (RED), `f623fa3` feat (GREEN)
3. **Task 3: `start`.** `ef7ba9e` test (RED), `7350dd0` feat (GREEN)

The plan suggested the subject `feat(cli): status, cleanup and start say why`. The commits use the `52-25` scope instead, because the TDD gate validator greps for `test(52-25)` and `feat(52-25)`.

## TDD Gate Compliance

Each RED came from a nextest JUnit report, written with a scratch nextest config (`--profile red`), and each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `commands::tests::status_prints_the_stop_reason_and_fallback_state` | `RED_EVIDENCE_OK` (exit 100) | It failed on the planned assertion: the fallback-word list was empty, because no `fallback:` line existed. It compiled because the RED commit moved status's existing worktree line into `status_run_lines`, with output unchanged. |
| `commands::tests::status_does_not_report_a_run_with_a_missing_worktree_as_active` | `RED_EVIDENCE_OK` (exit 100) | It failed on the planned assertion: there was no `run: not active` line for a gone worktree. |
| `phase7_cli::cleanup_names_why_it_keeps_a_worktree` | `RED_EVIDENCE_OK` (exit 100) | The finished case failed: its output said "halted via --until; run `devflow resume`" and did not quote the finished reason. |
| `phase7_cli::cleanup_force_removes_the_run_state_with_the_worktree` | `RED_EVIDENCE_OK` (exit 100) | The worktree was removed, but the state file stayed. |
| `phase7_cli::cleanup_keeps_the_branch_of_a_kept_worktree` | `RED_EVIDENCE_OK` (exit 100) | Git's refusal kept the branch, but cleanup had tried to delete it and printed "could not delete the merged branch feature/phase-26". That attempt is what the test forbids. |
| `phase7_cli::start_says_when_it_replaces_a_finished_or_stale_state` | `RED_EVIDENCE_OK` (exit 100) | The finished case printed no replace line. |

- **Gate sequence:** `test` 95da581 → `feat` a6d2544; `test` 28c0c30 → `feat` f623fa3; `test` ef7ba9e → `feat` 7350dd0.
- **Tracer gate:** the run is interactive (`auto_advance` false, `_auto_chain_active` false), the mode is `end-of-phase`, and Task 1's verify is automated only. After GREEN, both named tests were re-run (exit 0) and the full `devflow` bin suite passed (606 passed, 4 ignored), so expansion continued.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<automated>` (two named nextest runs) | 0 |
| Task 2 `<automated>` (four named nextest runs, including `cleanup_worktree_cli_flow`) | 0 |
| Task 3 named test `start_says_when_it_replaces_a_finished_or_stale_state` | 0 |
| `scripts/check.sh all` (fmt, clippy, every test) | 0 on the second run. The first run exited 101: clippy `type_complexity` in the Task 3 RED test, fixed in 7350dd0 |
| `cargo nextest run -p devflow-core --lib -E 'test(/git::tests/)'` | 0 (39 passed) |
| `cargo semver-checks check-release -p devflow-core` | 100. All 12 major failures predate this plan (removed `yes_ship`, `gates` parameter counts, etc.), and none names `git.rs` or `GitFlow`; the new method is additive |
| `rg "halted via --until" crates/devflow-cli/src/commands.rs` | 1 (no match). Control: the same search on the RED commit's tree found 1 |

**What these checks do not establish:**
- The final commit changed one `status` string ("takes over if Code is rate-limited") after the full `check.sh` run. clippy, fmt and the two affected named tests were re-run on it; the full suite was not.
- The live-run case uses an agent pid file holding the test process's own pid. No real live supervisor socket is exercised.
- The missing-worktree test passes a supervisor record and a live agent pid but probes no socket. `run_kind` does not read liveness, so the test shows that those signals do not make the run active. It does not exercise an ALIVE probe.
- On a real (non-dry) `start` over an unloadable state, three lines now print: this notice, the old budget warning from `carried_phase_failures`, and `reserve_launch`'s "replaced the state file … bytes kept at" note. That was read from source, not run.

## Decisions Made

See `key-decisions` in the frontmatter.

## Leftovers from earlier SUMMARYs

1. **The cleanup message suggested `resume` for a finished run (52-08).** In scope; done in Task 2. The finished-run keep line names only `--force`, and `cleanup_names_why_it_keeps_a_worktree` asserts that it does not contain `devflow resume`.
2. **`phase_validate_failures` carries into the next `start` (52-12).** No truth in this plan covers it. Whether a new run after a pass gets a fresh Validate-failure budget changes a safety bound, so it is not decided here. It stays open in `deferred-items.md`, now with a 52-25 note and a recommendation: start at zero over a finished state. Two facts changed around it. `start` now announces that it replaces the finished run. And `cleanup --force` now removes the state, which is a second way the total resets; the doc comment on `fresh_state_carrying_phase_failures` says so.

## Open operator decision: untouched

Whether `stop` or finish wins at Validate's end is still open, and stop still wins (`finish_run`'s safe point 6). This plan did not change `pipeline_gate.rs`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Additive `GitFlow::cleanup_merged_keeping` in devflow-core**
- **Found during:** Task 2
- **Issue:** the plan names only CLI files, but the merged-branch sweep lives in `devflow-core`'s `GitFlow::cleanup_merged`, and the CLI cannot filter it.
- **Fix:** added a new `pub fn cleanup_merged_keeping(&self, keep: &[String])`. The private `cleanup_merged_to` takes the keep list, and `cleanup_merged` passes an empty one, so existing callers behave as before. Added the test `git::tests::cleanup_merged_keeping_skips_a_kept_branch`, with an unnamed merged branch as the control.
- **Files modified:** crates/devflow-core/src/git.rs
- **Verification:** core git tests (39 passed); semver-checks shows no new failure.
- **Committed in:** f623fa3

**2. [Rule 1 - Bug] Two of my own test fixes after RED**
- **Found during:** Tasks 1 and 3
- **Issue:** in Task 1, the missing-worktree test checked the whole header for the phase number, and the new `other phases:` line legitimately names it. In Task 3, the RED commit's test closure type failed clippy `type_complexity`, so `scripts/check.sh all` failed at ef7ba9e. The commit still builds.
- **Fix:** the Task 1 test now checks only the `active phases:` line; the RED header would still fail it. The Task 3 test got a type alias.
- **Committed in:** a6d2544, 7350dd0

**3. [Rule 2 - Docs] OPERATIONS.md `status` and `start` rows**
- **Issue:** the plan names OPERATIONS.md only for cleanup, but the `status` row said "All active phases", and `start` gained a visible line.
- **Fix:** both rows describe the new output.
- **Committed in:** f623fa3, 7350dd0

---

**Total deviations:** 3 (1 blocking, 1 bug in my own tests, 1 docs). **Impact:** no scope creep beyond the plan's truths. The core change is additive.

## Issues Encountered

- The first commit's subject is 80 characters. The commit-msg hook warned (the limit is 72) but did not refuse it. It was left as is, since history is not rewritten.
- Neither known flake (999.221, 999.222) came up: both full `devflow` bin runs passed.

## Threat Model

- **T-52-35 (`cleanup --force` → disk):** mitigated and tested. `--force` removes only the matched phase's state, and only when that state records the removed worktree or none. `cleanup_force_removes_the_run_state_with_the_worktree` checks that another phase's state survives. A checked-out branch is never deleted (`cleanup_keeps_the_branch_of_a_kept_worktree`). All fixtures are tempdir repos run through `hermetic_devflow` (HOME/XDG set, GIT_* scrubbed).
- **T-52-SC:** no package added.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-26. For close-out: the `status` output shape changed (new `run:`, `stop reason:` and `fallback:` lines, plus an `other phases:` header line). Anything that parses `status` text should be checked; no test in the repo did.

## Self-Check: PASSED

- All six commits (95da581, a6d2544, 28c0c30, f623fa3, ef7ba9e, 7350dd0) are ancestors of HEAD; none deletes a file.
- The modified files exist, and `commands.rs` contains `fn status_prints_the_stop_reason_and_fallback_state`.
- `scripts/check.sh all` exited 0 (second run); the targeted gates exited 0.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
