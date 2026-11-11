---
phase: 52
review: 52-REVIEW.md
titles: json
findings:
  - id: CR-01
    severity: critical
    disposition: fixed
    title: "An ambiguous Validate verdict loops forever in auto mode over `gaps_found`, `stale` or `missing`"
  - id: CR-02
    severity: critical
    disposition: fixed
    title: "Starting a finished phase again force-deletes its unmerged branch, the only copy of the finished work"
  - id: WR-01
    severity: warning
    disposition: fixed
    title: "A reader whose child left a process holding stdout is reported as timed out, but that process is never killed"
  - id: IN-01
    severity: info
    disposition: fixed
    title: "Doc links point to `validate_route`, which does not exist"
  - id: IN-02
    severity: info
    disposition: fixed
    title: "The doc of `handle_gaps_plan_outcome` describes a reject route the code no longer takes"
  - id: IN-03
    severity: info
    disposition: deferred
    title: "A hand-run `devflow advance` over a finished run names `devflow resume`, which refuses it"
open: 0
total: 6
recorded: 2026-11-11T17:15:19.010Z
---

# Phase 52: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| CR-01 | critical | fixed | bc88f9a (operator decision 2026-11-11: an ambiguous verdict always gates); test `pipeline_outcomes::tests::an_ambiguous_verdict_gates_at_once_in_auto_over_every_status` |
| CR-02 | critical | fixed | 3f0714f (operator decision 2026-11-11: no `--force` deletes an unmerged phase branch); tests `phase7_cli::cleanup_force_keeps_an_unmerged_branch_and_names_the_delete_command`, `phase7_cli::start_force_refuses_over_an_unmerged_phase_branch`, `phase7_cli::start_force_no_worktree_refuses_over_an_unmerged_phase_branch`; the `--force` redesign is backlog 999.224 |
| WR-01 | warning | fixed | f4df92d; test `gsd_reader::tests::a_reader_whose_child_holds_stdout_times_out_and_the_child_is_killed`; a holder that leaves the process group is still out of reach (documented) |
| IN-01 | info | fixed | b616cbe |
| IN-02 | info | fixed | dc85bb6 |
| IN-03 | info | deferred | operator decision 2026-11-11: folded into backlog 999.224 (the `--force` redesign gives a finished run its restart verb) |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
