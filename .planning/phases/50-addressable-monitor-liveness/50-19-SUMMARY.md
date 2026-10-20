---
phase: 50-addressable-monitor-liveness
plan: 19
subsystem: testing
tags: [live-run, checkpoint, evidence, attempt-gate, supervisor]
requires:
  - phase: 50-18
    provides: check-supervisor-evidence.sh --attempt-gate (and its selftest cases for shipped, cap_reached, not_ready)
  - phase: 50-17
    provides: 50-RUNBOOK.md (prepare, fault arm on the pinned binary, repository, launch, gates, collect, attempt gate)
provides:
  - "An operator-run live attempt (attempt 1) whose evidence passes check-supervisor-evidence.sh --attempt-gate with verdict=shipped"
  - "Attempt-1 and fault-5 evidence left on disk under $HOME/.cache/devflow-phase50/ for 50-20 to judge and sanitize"
affects: [50-20, 50-25]

actuals:
  tokens: 2700      # chars/4 over this SUMMARY, the only repository file this plan adds
  tasks: 1
  commits: 0        # measured before the SUMMARY commit; the SUMMARY commit is this plan's only commit
plan_head_before: 1deaa4d8a29f3a843834fe4c3187c2a70faeba9e
plan_head_after: 1deaa4d8a29f3a843834fe4c3187c2a70faeba9e

tech-stack:
  added: []
  patterns: []

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/50-19-SUMMARY.md
  modified: []

key-decisions:
  - "Plan 50-19 closed on the attempt gate's exit code (verdict=shipped attempt=1), not on the operator's reply (R6-G6)"
  - "SUPV-01 is not marked complete in REQUIREMENTS.md here; judging and publishing the evidence is 50-20's job"

patterns-established: []

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "One live auto attempt to Ship on the new pinned binary, crossing both launch shapes, with a passing fault arm on that same binary"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "bash .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh --attempt-gate $HOME/.cache/devflow-phase50 (50-19 Task 1 <verify>, run verbatim, exit 0)"
        status: pass
    human_judgment: true
    rationale: "The gate checks one attempt's internal consistency and the fault/attempt binary match. Whether that evidence supports SUPV-01 is 50-20's judgment."

duration: 35min (live run 14:51:41-15:26:30 EDT, operator time outside this session)
completed: 2026-10-20
status: complete
---

# Phase 50 Plan 19: Live Attempt Checkpoint Summary

**The operator's first live `devflow start --mode auto --agent claude --yes-ship` attempt on binary `devflow-9b0ec70c7190` (built clean from `1deaa4d`) went through Define, Code and Ship and used both launch shapes (5 stream, 1 capture). Its evidence passes `check-supervisor-evidence.sh --attempt-gate` with `verdict=shipped attempt=1`. The fault arm `fault-5`, run on that same binary, passed with `--require-all`.**

## What this does NOT establish (read first)

- **One success is not a reliability bound.** A single shipped attempt shows the supervisor path *can* carry a full auto run to Ship on this binary. It says nothing about how often it does. A run that hangs or loses the monitor 1 time in 5 would pass a single attempt about 80% of the time.
- **The gate checks consistency, not adequacy.** It confirms the evidence is complete and consistent (events, launch records, sidecars cleaned up, fault binary equals attempt binary, Phase 49 positive checks). It does not decide whether that evidence satisfies SUPV-01. That is 50-20's job.
- **Only the happy path ran live.** No fault was injected during the live run (no kill, no stop, no lost socket). Fault behaviour is covered only by the fault arm's six synthetic cases, not by this attempt.
- **The run used seeded gates.** The Define and Code gates were the expected D-07 preflight refusals that the disposable project seeds on purpose, and Code's `blocking-human` decision was auto-decided by design. Neither shows how a real unexpected gate is handled.

## Verify result

The plan's `<verify>` command was run verbatim from the worktree root and exited 0. Its final lines:

```
ATTEMPT launches=6 stream=5 capture=1
ATTEMPT sidecars_left=0
ATTEMPT verdict=pass
ATTEMPT_GATE fault=fault-5 bin_sha256=9b0ec70c7190301067b6e8438e27ed7fb8ae2f00de90638de3608b1391c098f6
ATTEMPT_GATE verdict=shipped attempt=1
```

Phase 49 positive check: `CHECK passed=11 failed=0`. Before the run, `git status --porcelain` was empty and no commit subject among the last 300 starts `<word>(50-19)`.

**Negative control:** the same `--attempt-gate` call on an empty evidence home printed `ATTEMPT_GATE verdict=not_ready attempt=ABSENT ...` and exited 1. So the gate does refuse missing evidence; exit 0 here is not the result of a gate that never fails. The scratch directory was removed afterwards.

## Performance

- **Duration:** about 35 min of live run (workflow_started 2026-10-20T18:51:41Z, workflow_finished 2026-10-20T19:26:30Z); the attempt was prepared at 14:50:42 EDT
- **Started:** 2026-10-20T18:51:41Z
- **Completed:** 2026-10-20T19:26:30Z
- **Tasks:** 1 (a `checkpoint:human-action` carried out by the operator)
- **Files modified:** 1 (this SUMMARY)

## Accomplishments

- **Attempt 1 shipped.** Gates fired in order Define, Code, Ship. The operator answered Define and Code. `--yes-ship` answered Ship. The project ended with `workflow_shipped`, a merge into `integration` and an open PR `feature/phase-01 -> main` on the private throwaway repository `denniyahh/p50-live-attempt-1`.
- **Both launch shapes were used.** There were 6 `stage_launched` events: 5 stream launches and 1 capture launch. Code auto-decided the seeded `blocking-human` decision (`checkpoint_auto_decided`) and was then relaunched on the capture arm. Every launch names its socket, label and id, no event carries a `monitor_pid` key, and no supervisor sidecars were left over (`sidecars_left=0`).
- **The fault arm ran on the attempt's own binary.** `fault-5` (bin_sha256 `9b0ec70c7190…`, `src_matches_checkout=true`) passed all six cases under `--require-all`. `fault-1` to `fault-4` are earlier passing runs on older binaries. The gate selected `fault-5` because its binary matches the attempt's (D-17).

## Task Commits

1. **Task 1: Operator runs one live attempt to Ship from the runbook and collects its evidence**: no commit, by design (the task writes nothing; a commit before the reply is an illegal partial state).

**Plan metadata:** the `docs(50-19)` commit that adds this SUMMARY is the plan's only commit.

## Files Created/Modified

- `.planning/phases/50-addressable-monitor-liveness/50-19-SUMMARY.md`: this summary

The evidence is outside the repository, under `$HOME/.cache/devflow-phase50/attempt-1/` and `$HOME/.cache/devflow-phase50/fault-5/`, until 50-20 commits a sanitized copy.

## Decisions Made

- Closed on the gate's exit code, not on the operator's "done" or on re-reading the evidence (R6-G6).
- SUPV-01 is listed in `requirements-completed` because the template copies the plan's requirement IDs, but it should not be checked off in REQUIREMENTS.md until 50-20 has judged the evidence (same handling as Phase 49's 49-11).

## Deviations from Plan

Operator-side deviations during the attempt. None changed the verdict, and none produced a gate answer or a stop:

1. **Define was first declined in the guided script.** The operator declined Define at the script's prompt because the script did not explain that the D-07 refusal text was the expected seeded gate. The script was fixed and resumed at the gate step. No gate response was written in between: the events contain exactly one `gate_resolved` for Define (`approved:true`, `responded_by: denniyahh`, 14:58:44 EDT, 7 min after it fired).
2. **The script's watcher was stopped once during Code.** The run was not affected. `events.jsonl` contains no stop or abort events. Its only event kinds are launch, gate, hook, capture, transition, notify, merge, canary, auto-decide and workflow start/ship/finish.
3. **Harmless `reset-failed` message.** The script's `systemctl --user reset-failed p49-a1-start` printed "Unit not loaded" because the unit had exited normally, so there was nothing to reset. This is cosmetic.

No deviations.txt was written under `attempt-1/`. Every gate fired was in the runbook's expected set.

**Total deviations:** 0 auto-fixed; 3 operator-side notes, all non-affecting.

## Issues Encountered

- **Two meanings of "unexpected".** The collector prints `NOTE gate define unexpected=false`, meaning the gate is in the runbook's expected set. DevFlow's own `gate_fired` events carry `"unexpected":true` for Define and Code, meaning a never-silent gate that auto mode would not normally fire. These do not contradict each other, but 50-20 should not read one as the other.
- **The throwaway repository was not deleted.** Runbook lines 34-35 say to delete it only after Phase 50 verification, so `denniyahh/p50-live-attempt-1` and its open PR #1 remain.

## User Setup Required

None now. Later, the operator deletes the private repository `denniyahh/p50-live-attempt-1` after Phase 50 verification, per the runbook.

## Next Phase Readiness

- 50-25 (wave 16) can build the four judges that 50-20 calls. The tool was not edited during the live attempt.
- 50-20 can judge and sanitize `$HOME/.cache/devflow-phase50/attempt-1/` and `fault-5/`.

## Self-Check: PASSED

- FOUND: `.planning/phases/50-addressable-monitor-liveness/50-19-SUMMARY.md` (this file)
- FOUND: `$HOME/.cache/devflow-phase50/attempt-1/verdict.json` (`outcome: shipped`) and `fault-5/verdict-require-all.txt` (`FAULT verdict=pass`)
- Verify exit 0 and negative control exit 1, both observed in this session
