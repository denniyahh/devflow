---
phase: 50-addressable-monitor-liveness
plan: 28
subsystem: testing
tags: [live-run, checkpoint, evidence, attempt-gate, current-source, supervisor]
requires:
  - phase: 50-26
    provides: the WR-03 fix (bounded stream wait) with its revert check
  - phase: 50-27
    provides: check-supervisor-evidence.sh --attempt-gate --current-source, and the runbook's attempt-2 delegation text
provides:
  - "Live attempt 2, run on binary devflow-182ab8b546a8 built clean from bca7c35 (after the CR-01 and WR-03 fixes), whose evidence passes --attempt-gate --current-source with verdict=shipped attempt=2"
  - "Attempt-2 and fault-7 evidence left on disk under $HOME/.cache/devflow-phase50/ for 50-29 to judge and sanitize"
affects: [50-29]

actuals:
  tokens: 2900      # chars/4 over this SUMMARY, the only repository file this plan adds
  tasks: 1
  commits: 0        # measured before the SUMMARY commit; the SUMMARY commit is this plan's only commit
plan_head_before: bca7c3583b39db702f929855e27b5b5ab19bda22
plan_head_after: bca7c3583b39db702f929855e27b5b5ab19bda22

tech-stack:
  added: []
  patterns: []

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/50-28-SUMMARY.md
  modified: []

key-decisions:
  - "Plan 50-28 closed on the exit code of --attempt-gate --current-source (verdict=shipped attempt=2), not on the 'done' reply"
  - "SUPV-01 is not checked off in REQUIREMENTS.md here; judging and publishing attempt 2's evidence is 50-29's job"

patterns-established: []

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "One live auto attempt to Ship on the fixed, pinned binary, crossing both launch shapes, with a passing fault arm on that same binary built from the checkout's committed crate tree"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "bash .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh --attempt-gate $HOME/.cache/devflow-phase50 --current-source (50-28 Task 1 <verify>, run verbatim, exit 0)"
        status: pass
    human_judgment: true
    rationale: "The gate checks one attempt's consistency, the fault/attempt binary match and that the fault arm's crate tree is the committed one. Whether that evidence satisfies SUPV-01 is 50-29's judgment."

duration: 23min (live run 16:28:05-16:51:15 EDT, performed by the orchestrating session outside this executor)
completed: 2026-10-21
status: complete
---

# Phase 50 Plan 28: Live Attempt 2 Checkpoint Summary

**Live attempt 2 ran `devflow start --mode auto --agent claude --yes-ship` on binary `devflow-182ab8b546a8`, built clean from `bca7c35` (which contains the CR-01 fix `373f1e6` and the WR-03 fix `493156b`). It went through Define, Code and Ship and used both launch shapes (5 stream, 1 capture). Its evidence passes `check-supervisor-evidence.sh --attempt-gate --current-source` with `current_source=yes` and `verdict=shipped attempt=2`. The fault arm `fault-7`, run on that same binary, passed all six cases with `--require-all`.**

## What this does NOT establish (read first)

- **One success is not a reliability bound.** One shipped attempt on the fixed binary shows it *can* carry a full auto run to Ship. It says nothing about how often. A run that fails 1 time in 5 would pass a single attempt about 80% of the time.
- **The WR-03 condition did not occur live.** No process held the agent's stdout open after it exited during this run. So the run shows that the fixed binary ships, not that it rescues a held stdout. That rescue rests on 50-26's tests and revert check. 50-29 counts `stream_wait_bounded` events in the log.
- **No fault was injected during the live run.** There was no kill, stop or lost socket. Fault behaviour is covered only by `fault-7`'s six synthetic cases (five live, one pointer case for the publish-unlink window).
- **The gate checks consistency, not adequacy.** Whether this evidence satisfies SUPV-01 is 50-29's job.
- **The gate answers were given under delegation.** The `responded_by` field records the same user whoever typed the answer, so the event log cannot show that the orchestrating session answered Define and Code. 50-29 records it.
- **`surv-watch.py analyze` was not judged.** It printed nothing to the orchestrator's output tail. The collector and the judges passed, but this executor drew no conclusion from that empty output.

## Verify result

The plan's `<verify>` command was run verbatim from the worktree root and exited 0. Its final lines:

```
CHECK passed=11 failed=0
ATTEMPT p49_positive=pass
ATTEMPT launches=6 stream=5 capture=1
ATTEMPT sidecars_left=0
ATTEMPT verdict=pass
ATTEMPT_GATE current_source=yes
ATTEMPT_GATE fault=fault-7 bin_sha256=182ab8b546a8f93c3e62b33c6516448446593bdb767e8f2ff250ec3393f5045d
ATTEMPT_GATE verdict=shipped attempt=2
```

Before the verify, `git status --porcelain` was empty, HEAD was `bca7c35`, and no commit subject among the last 300 started `<word>(50-28)`.

**Negative control:** the same `--attempt-gate --current-source` call on an empty scratch evidence home printed `ATTEMPT_GATE verdict=not_ready attempt=ABSENT ...` and exited 1. So the gate does refuse missing evidence, and the exit 0 above is not from a gate that never fails. The scratch directory was removed. This control shows only that missing evidence is refused. 50-27's cases and real-data control are what show that attempt 1's pre-fix fault record is refused.

## Performance

- **Duration:** about 23 min of live run (workflow_started 2026-10-21T20:28:05Z, workflow_shipped and workflow_finished 2026-10-21T20:51:15Z)
- **Started:** 2026-10-21T20:28:05Z
- **Completed:** 2026-10-21T20:51:15Z
- **Tasks:** 1 (a `checkpoint:human-action`, performed by the orchestrating session under the operator's delegation)
- **Files modified:** 1 (this SUMMARY)

## Accomplishments

- **Attempt 2 shipped on the fixed binary.** Gates fired in order Define, Code, Ship. Define (fired 16:28:05, `gate_resolved` 16:30:08 EDT) and Code (fired 16:30:49, resolved 16:31:18 EDT) were the same seeded D-07 preflight refusals as in attempt 1. The orchestrating session reports that it inspected each with `gate show` before approving. `--yes-ship` answered Ship at 16:51:15. The log then has `merge_result` (`merged: true`), four `hook_run` events (Merge, VersionBump, ChangelogAppend, BranchCleanup, all `ok`), `workflow_shipped` and `workflow_finished`. A fifth `hook_run` (DocsUpdate, `ok`) ran earlier, at 16:41:46. `collected/pr.json` records an OPEN PR `feature/phase-01 -> main` on the private throwaway repository `denniyahh/p50-live-attempt-2`. The collector recorded `outcome=shipped` with no ABSENT fields.
- **Both launch shapes were used.** There were 6 `stage_launched` events: 5 stream launches and 1 capture launch. Every launch names its socket, label and id, no event carries a `monitor_pid` key, and `collect-sidecars` found no supervisor sidecars left (`sidecars=0`, `sidecars_left=0`).
- **The fault arm ran on the attempt's own binary, from the committed source.** `fault-7` records `bin_sha256` `182ab8b546a8…` (equal to the attempt's, per `attempt-2/identity.txt`: `build_commit=bca7c35…`, `build_dirty=false`), `src_matches_checkout=true`, and `FAULT verdict=pass` with `complete=yes`. The gate paired it by binary hash and confirmed its crate tree is the checkout's (`current_source=yes`).
- **Clean run environment.** The orchestrating session ran every step under `env -i` with only HOME, USER, LOGNAME, PATH, LANG, TERM, XDG_RUNTIME_DIR and DBUS_SESSION_BUS_ADDRESS. Its control: the session shell had 14 CLAUDE*/ANTHROPIC* variables and the clean environment had 0. The systemd units received only PATH, HOME and DEVFLOW_GATE_NOTIFY_CMD. The auth probe recorded `probe_exit=0 is_error=false reply_ok=yes auth_source=none`, the same as attempt 1. (These environment and probe facts are the orchestrator's record; this executor did not re-run them.)
- **Clean teardown.** The `p49-a2-*` units were stopped and released, and no attempt-binary process was left (orchestrator-observed).

## Task Commits

1. **Task 1: Live attempt 2 runs to Ship from the runbook on the fixed binary**: no commit, by design (the task writes nothing, and a commit before the reply would be an illegal partial state).

**Plan metadata:** the `docs(50-28)` commit that adds this SUMMARY is the plan's only commit.

## Files Created/Modified

- `.planning/phases/50-addressable-monitor-liveness/50-28-SUMMARY.md`: this summary

The evidence is outside the repository, under `$HOME/.cache/devflow-phase50/attempt-2/` and `$HOME/.cache/devflow-phase50/fault-7/`, until 50-29 commits a sanitized copy.

## Decisions Made

- Closed on the gate's exit code, not on the "done" reply or on re-reading the evidence.
- SUPV-01 is listed in `requirements-completed` because the template copies the plan's requirement IDs. It should not be checked off in REQUIREMENTS.md until 50-29 has judged the evidence (same handling as 50-19).

## Deviations from Plan

None. The orchestrating session reports no departure from the runbook and no unexpected gate. No `deviations.txt` exists under `attempt-2/` (checked), and the collector's gate census shows exactly Define, Code and Ship, none of them unexpected.

The orchestrating session told the operator before it created the repository, as the delegation requires.

## Issues Encountered

- **The throwaway repository was not deleted.** Deleting it is the operator's job, after Phase 50 verification. `denniyahh/p50-live-attempt-2` (private) and its open PR remain, beside `denniyahh/p50-live-attempt-1`.

## User Setup Required

None now. Later, the operator deletes the private repositories `denniyahh/p50-live-attempt-1` and `denniyahh/p50-live-attempt-2` after Phase 50 verification, per the runbook.

## Next Phase Readiness

- 50-29 can judge, sanitize and publish `$HOME/.cache/devflow-phase50/attempt-2/` and `fault-7/`, and record in 50-LIVE-RUN.md that the gates were answered under delegation.

## Self-Check: PASSED

- FOUND: `.planning/phases/50-addressable-monitor-liveness/50-28-SUMMARY.md` (this file)
- FOUND: `$HOME/.cache/devflow-phase50/attempt-2/verdict.json` (`outcome: shipped`, `stop_note: null`) and `fault-7/verdict-require-all.txt` (`FAULT verdict=pass`)
- `current-attempt` reads `2`
- Gate and hook times above come from `attempt-2/collected/devflow/events.jsonl`. The orchestrator's hand-off said Define was approved at 16:29. The log says 16:30:08, and this summary uses the log.
- CR-01 `373f1e6` and WR-03 `493156b` are both ancestors of `bca7c35` (checked with `git merge-base --is-ancestor`)
- Verify exit 0 and negative-control exit 1, both observed in this session
