---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 04
subsystem: testing
tags: [python, ctypes, inotify, surv-01, observation-tooling]

requires: []
provides:
  - "tools/surv-watch.py watch <project_dir> <out_dir>: ctypes inotify watcher on <project>/.devflow/ (including one created after start), JSON-line watch.jsonl with create/moved_from/moved_to/delete/overflow/watch_added/initial/stopped records; stops on SIGTERM, SIGINT or <out>/stop"
  - "tools/surv-watch.py analyze <out_dir>: analysis.txt with writers=, temp_writes=, interleaved_windows=, overflow_windows=, stop_marker= field lines, overlap / void window / saw nothing lines and the fixed closing sentence"
  - "tools/surv-watch.py --selftest: 7 cases, one of them a real inotify run"
affects: [49-09, 49-10, 49-12]

actuals:
  tokens: 6472
  tasks: 2
  commits: 4
plan_head_before: f3515b788496deadf9b067ba107bbf4e97a85360

tech-stack:
  added: []
  patterns:
    - "Project-level inotify watch is installed before watch.jsonl exists, so a log's existence proves a later .devflow/ creation will be seen"
    - "Overlap analysis walks records in inotify queue order; mono_ns is reporting only"
    - "Every analysis carries all five field lines and the closing no-verdict sentence, including an empty log"

key-files:
  created:
    - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py
  modified: []

key-decisions:
  - "watch refuses an out dir inside the watched project, an existing watch.jsonl, and an absent project dir (exit 2), so the watcher cannot write under the tree it observes and two runs never share one log"
  - "analyze refuses a malformed watch.jsonl line (exit 2, line number named) instead of skipping it"
  - "An `initial` state temp (present when the watch attached) opens a window but is not counted in temp_writes or writers, which count observed creates only"

patterns-established:
  - "Negative controls for a watcher selftest: mutate the script in a scratch copy and require each mutant to FAIL a named case"

requirements-completed: [VERIFY-01]

coverage:
  - id: D1
    description: "A real two-writer overlap on disk is logged by inotify (including a .devflow/ created after the watcher started) and flagged by the analysis; the watcher leaves the watched tree unchanged"
    requirement: VERIFY-01
    verification:
      - kind: integration
        ref: "python3 tools/surv-watch.py --selftest#watch: live two-writer overlap is logged and flagged"
        status: pass
      - kind: integration
        ref: "python3 tools/surv-watch.py --selftest#watch: watched directory left unchanged"
        status: pass
      - kind: other
        ref: "49-04-PLAN Task 1 <automated> gate (gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Analysis separates overlap from sequence, reports overflow void windows, a missing stop marker, an empty log as saw nothing with all five fields, refuses a missing log, and never uses the words confirmed / no interleaving"
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: "python3 tools/surv-watch.py --selftest (5 analyze/control cases)"
        status: pass
      - kind: other
        ref: "49-04-PLAN Task 2 <automated> gate (gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The watcher behaves correctly on the live run's real .devflow/ under systemd (SIGTERM stop, no overflow, real DevFlow temp names)"
    verification: []
    human_judgment: true
    rationale: "Only a synthetic scratch directory has been watched; the live launch checkout is first watched in 49-11. A manual SIGTERM probe passed, but not under systemd-run."

duration: 8min
completed: 2026-09-29
status: complete
---

# Phase 49 Plan 04: SURV-01 State-Write Watcher Summary

**Stdlib-only ctypes inotify watcher for `<launch>/.devflow/` plus an analysis that flags one pid creating `.state-01.json.<pid>.<seq>.tmp` while another pid's temp is still open, and reports overflows, a missing stop and an empty log as blind spots, never as a verdict.**

## What this does not establish

- The selftest watches a scratch directory written by the selftest itself. It has not watched a real DevFlow run. Whether the live run's writes arrive as `create` → `moved_from` pairs with the pid in the name is taken from `workflow.rs` `unique_temp_path_with`, not observed.
- The live overlap case is timing-dependent. It passed 10 of 10 consecutive runs plus the gate runs. That is a weak bound on flakiness, not proof it cannot flake.
- inotify reports events in queue order. The watcher stamps `mono_ns` when it reads an event, not when the event happened, so `mono_ns` on an overlap line is approximate. The overlap decision uses queue order only.
- An unflagged analysis means no interleaving was seen in the observed windows. It establishes nothing about SURV-01, as the tool's closing sentence says.

## Performance

- **Duration:** about 8 min
- **Started:** 2026-09-29T20:00:28Z
- **Completed:** 2026-09-29T20:08:17Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- `watch`: `inotify_init1(IN_NONBLOCK|IN_CLOEXEC)` through `ctypes.CDLL(None, use_errno=True)`, `select` with a 0.5 s timeout, `struct inotify_event` parsing. The project watch (`IN_CREATE|IN_MOVED_TO`) catches a late `.devflow/`. The `.devflow/` watch covers `IN_CREATE|IN_MOVED_FROM|IN_MOVED_TO|IN_DELETE`. Also handled: `watch_added`, then one `initial` per existing entry; `IN_Q_OVERFLOW` becomes `overflow`, followed by a re-attach attempt; `IN_IGNORED` resets the watch so a re-created `.devflow/` is watched again. On stop, a final drain runs before the `stopped` record, whose `name` is `SIGTERM`, `SIGINT` or `stop file`.
- `analyze`: queue-order window tracking, atomic publish of `analysis.txt`, and the five field lines, each alone on its own line exactly once. The output also has `overlap pid=A seq=… with pid=B seq=… at mono_ns=…`, `void window from mono_ns=… to mono_ns=…|end of log`, a `saw nothing: …` line when `temp_writes=0`, a stop-marker-absent explanation, and the closing sentence verbatim: `This is one observation of one run; it establishes nothing about SURV-01's guarantee either way.`
- Refusals: `watch.jsonl ABSENT: <path>` (exit 2, no analysis.txt), malformed log line (exit 2), out dir inside the project, existing log, absent project dir (all exit 2).

## Task Commits

1. **Task 1 (tracer): watch a real directory, flag two overlapping writers**
   - RED `4311ec5` test(49-04): add failing selftest for the state-write watcher
   - GREEN `b543034` feat(49-04): watch state-file writes and report overlapping writers
2. **Task 2: analysis controls**
   - RED `77c92a6` test(49-04): pin the watcher analysis controls
   - GREEN `7e01492` feat(49-04): report void windows, missing stops and empty logs honestly

**Plan metadata:** this SUMMARY commit.

## TDD Gate Compliance

| Task | RED commit | RED run | RED evidence check | GREEN commit | GREEN run |
|------|------------|---------|--------------------|--------------|-----------|
| 1 | `4311ec5` | exit 1, `SELFTEST passed=0 failed=2`; target line: `  FAIL watch: live two-writer overlap is logged and flagged — no watch_added record within 5 s of creating .devflow` | `RED_EVIDENCE_OK` / `target_test_failed` (2 tests, 2 fail) | `b543034` | `SELFTEST passed=2 failed=0` |
| 2 | `77c92a6` | exit 1, `SELFTEST passed=3 failed=4`; e.g. `  FAIL analyze: overflow recorded as a void window — overflow_windows lines [], want [overflow_windows='1']`, `  FAIL control: missing watch log refused — stderr 'Traceback …FileNotFoundError…'` | `RED_EVIDENCE_OK` for each of the 4 new failing cases | `7e01492` | `SELFTEST passed=7 failed=0` |

No REFACTOR commits. `check tdd-red-evidence` parses TAP, so each RED run's `  ok` / `  FAIL` lines were transcoded one-to-one into TAP before the check (the same format bridge 49-02 and 49-03 used). In Task 2's RED run the sequential control already passed. It is a control, so that is correct: Task 1's analysis already told sequence from overlap. Task 1's RED failures are precondition failures (the stub never watched). The mutation sweep below shows the assertions also discriminate.

## Verification

- Task 1 gate, verbatim from the plan: `selftest_exit=0 passed=2 failed=0`, both `case[...]=1`, ruff clean, `pycache_dirs=0`, `gate_rc=0`. It was re-run after the Task 1 commit for the tracer gate, and again at plan end on HEAD: `gate_rc=0`.
- Task 2 gate, verbatim: `selftest_exit=0 passed=7 failed=0`, all five `case[...]=1`, `empty_analyze_exit=0 saw_nothing=1 verdict_words=0`, `key_lines=1 exact=1` for each of the five fields, ruff clean, `gate_rc=0`.
- Mutation sweep (negative controls; each mutant run in a scratch copy, each must FAIL). All 8 were killed:
  - never-close windows: killed by `analyze: sequential writer not flagged`. It survived Task 1, and Task 2's sequential control is what catches it.
  - no overlap flag: killed by the live case.
  - no late-`.devflow` attach: killed by the live case.
  - write under the project: killed by `watched directory left unchanged`.
  - no `stopped` record: killed.
  - no void window: killed.
  - `stop_marker` always absent: killed by the paired present-control.
  - no `saw nothing`: killed.
  - no ABSENT refusal: killed.
  - a verdict-word closing sentence: killed in 3 cases.
- Manual probe: a SIGTERM delivered through `timeout` produced exit 0 and a final `stopped` record named `SIGTERM`. A `.devflow/` existing at start produced `watch_added` then `initial state-01.json`. The refusals exit 2 with their messages.
- Imports checked against `sys.stdlib_module_names`: no non-stdlib module. `os.path` uses: 0.
- No watcher process left running (`ps` shows none). No `surv-watch-selftest-*` scratch dirs are left in `/tmp`.

## Files Created/Modified

- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py` — watcher, analysis and selftest (743 lines)

## Decisions Made

- The analysis contains two things beyond the field and overlap lines the plan specifies: a parenthesised explanation after each `void window` line, and a `stop marker absent: …` line. Both keep the required line prefixes and contain no verdict words.
- `initial` state temps open windows but are excluded from `writers`/`temp_writes`. Those fields count observed creates, as the plan defines them.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Dropped the shebang line**
- **Found during:** Task 1 RED
- **Issue:** `ruff check` failed with EXE001 (shebang on a non-executable file). The sibling tools are tracked as 100644, and the plan runs the script as `python3 surv-watch.py`.
- **Fix:** Removed the shebang instead of changing the file mode.
- **Committed in:** `4311ec5`

**2. [Rule 2 - Missing critical] Watcher refusals**
- **Found during:** Task 1 GREEN
- **Issue:** Nothing stopped an out dir inside the watched project, which would break the must-have "writes nothing under the watched project directory". Nothing stopped two runs appending to one log either, which would produce false overlaps across runs.
- **Fix:** Exit 2 with a message for an out dir under the project, an existing `watch.jsonl` and an absent project dir. Probed manually.
- **Committed in:** `b543034`

**3. [Rule 2 - Missing critical] Malformed log line refused**
- **Found during:** Task 1 GREEN
- **Issue:** A torn or foreign line would otherwise crash with a traceback, or be skipped silently.
- **Fix:** Exit 2, naming the line number. Probed manually.
- **Committed in:** `b543034`

**4. [Test strength] Paired control in the missing-stop case**
- The case also asserts that the same log with a `stopped` record reports `stop_marker=present`. That is what kills the always-absent mutant.
- **Committed in:** `77c92a6`

**5. [Process] Task 2 GREEN commit message amended**
- I first committed Task 2 GREEN with a shortened subject, wrongly assuming the plan's subject exceeded 72 characters. It is 71. I amended the unpushed HEAD (`6018388` → `7e01492`) to the plan's exact subject. The content is unchanged.

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 missing-critical), plus 1 test-strength addition and 1 message amend.
**Impact on plan:** No scope creep. The refusals protect the plan's own must-haves.

## Issues Encountered

None.

## User Setup Required

None. No external service configuration required.

## Next Phase Readiness

- 49-09 can launch `python3 <abs T>/surv-watch.py watch <abs project> <abs EV>/surv` in its systemd unit. 49-10 stops it with `systemctl --user stop` (SIGTERM, handled). 49-12 can run `analyze` and copy the five field lines and the closing sentence. The 49-12 gate's `\b<key>=([A-Za-z0-9_]+)` extraction was checked against a sample analysis.
- Open: D3. The watcher has never watched a real DevFlow run.

## Self-Check: PASSED

- FOUND: `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py`
- FOUND commits: `4311ec5`, `b543034`, `77c92a6`, `7e01492`
- Measured: `git rev-list --count f3515b7..HEAD` = 4 before this SUMMARY commit

---
*Phase: 49-live-unattended-run-the-milestone-s-instrument*
*Completed: 2026-09-29*
