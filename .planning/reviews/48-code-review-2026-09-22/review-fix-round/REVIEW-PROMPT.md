# Phase 48 review-fix review — WR-01..WR-04 remediations

You are an adversarial reviewer. A code review of Phase 48 (`48-REVIEW.md`, in this directory) found five
warnings. WR-01..WR-04 were fixed in four commits; WR-05 was deliberately deferred (see "Already known").
Attack the fixes, and — more important — look for **other instances of the same defect classes** that the
fixes did not touch. This phase has repeatedly fixed one instance of a class and missed its siblings; assume
that is happening again until the code shows otherwise.

## Where everything is (paths relative to the directory containing this file)

- `snapshot/` — `git archive` of the current tip (`4d119fb`). No `.git`. **Do not modify it**; copy it to a
  sibling directory if you want to build or experiment (`cargo test -p devflow-core --lib <name> -- --exact`
  works in a copy; module-qualify test names and require `1 passed`).
- `fix.diff` — `git diff 69c3ada..4d119fb -- crates OPERATIONS.md`: exactly the fixes and their tests.
- `48-REVIEW.md` — the findings being fixed, with the reviewer's reproduction notes.

## The fixes

- **WR-01 (`b1f1f3f` test, `c5f40fc` fix) — class: the Legacy monitor's `cleanup` trap acting on an agent
  it no longer owns.** `crates/devflow-core/src/monitor.rs` `spawn_monitor_inner`: the shell script now sets
  `reaped=1` after `wait $apid`, and `cleanup` skips both the stop-marker write and the `kill` once reaped.
  Check: every window between fork, exec, exit, reap, the exit-file write and the `devflow advance` tail —
  under **dash** (the CI `/bin/sh`) and bash. Is there a window where a TERM still writes the marker after the
  next stage's launch removed it, or kills a pid the monitor no longer owns? Does `wait` interrupted by a
  trapped signal behave as the comment claims on both shells? Does the `MonitorTail` test seam change any
  production behaviour? Is the new test non-vacuous on both shells?
- **WR-02/03/04 (`abb35e8` tests, `4d119fb` fix) — class: `recover --clean` must reach every artifact it is
  meant to reset, report only what it actually removed, and report a per-phase failure without aborting or
  hiding the rest.** `crates/devflow-core/src/recover.rs` (`clean_report`, `sweep_phase`,
  `sweep_orphan_gates`, `clean_phase_report`), `gates.rs` (`Gates::cleanup` now returns whether it removed
  anything; new `Gates::phases_on_disk`), `workflow.rs` (`clear_state` returns bool; new
  `state_file_phases`), `ship.rs` (`delete_cron_instructions` returns bool), and
  `crates/devflow-cli/src/commands.rs` `recover_cmd` (exits non-zero on any removal failure).
  Check:
  - Enumerate **every** artifact a phase can leave under `.devflow/` (state, legacy state, gate
    request/response/ack, their `.tmp` temps, cron records incl. legacy, agent pid / exit / stdout / stderr /
    stop-marker files, locks, anything else) and say for each whether the sweep and `--phase` reach it, and
    whether that is right.
  - `Gates::phases_on_disk` parses a phase from any gate-dir file name: can it match a non-gate file, and
    can the orphan sweep then take a lock or delete something it should not? Can it miss a real gate file?
  - The orphan sweep deletes gate files of a phase with **no state file**. Is there any live-run window in
    which a phase legitimately has gate files but no state file and its per-phase lock is free (start,
    abort, ship completion, `resume`, `stop`, parallel)? That would make the sweep destroy a live gate.
  - Every success/"nothing to clean"/"cleaned up" message and exit code in `recover_cmd`: can any still
    claim an action that did not happen, or stay silent about one that did?
  - Callers of the three helpers whose return type changed from `()` to `bool`: any caller whose behaviour
    changed?
  - Do the new tests fail if their fix is reverted (the author ran revert-mutations; try your own)?

## Already known — do not report

WR-05 / backlog 999.136: the sweep's staleness check ignores `state.monitor_pid` and is not re-checked under
the lock, so a sweep between agent exit and `advance` taking the lock can delete a live run. Also: recycled
pid blocks lock-taking verbs (999.128); split-lock / flock / fsync / gate-waiter registration and stale gate
answers (999.130); `gate list` vs `status` (999.132); other commands' false-success messages outside
`recover` (999.133); PipeOwning monitor orphaning its agent on TERM (999.134); the OpenCode probe flake
(999.135). The review's IN-01..IN-03 info items are out of scope.

## Output format

```
## CONFIRMED
### C-n. <title>
**File:** path:line (every file:line involved)
**Mechanism:** step by step
**Reproduction:** concrete state and wrong outcome; commands and output if you ran any
**Severity:** critical | high | medium | low — and why

## SUSPECTED
### S-n. <title>
Same fields; say what evidence would confirm or refute it.

## CHECKED AND CLEAN
One line per artifact / window / message / caller you enumerated and found sound, with its file:line.
```

CONFIRMED needs exact lines and a concrete path from the current code. Cite `file:line` for every claim.
Paths should be relative to `snapshot/`.
