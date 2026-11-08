# Live failover probe: merged Phase 51 build (2026-11-08)

**Build:** `devflow` installed from `develop` at `fad00ff` (PR #234 merge); `workflow_started` records that commit, clean.
**Project:** throwaway, `scripts/scratch-dogfood-repo.sh --live-payload` under `~/.cache/claude-scratch/p51-live/proj`,
no remote. Payload plan edited before the second launch: Task 1 (a `blocking-human` decision checkpoint) made an `auto`
task with the value settled in advance (120 s), because a checkpoint would have ended Code before any edit.
**Command:** `PATH=<wrapper>:$PATH devflow start --phase 1 --agent claude --mode supervise --fallback-agent codex`.

## What was real and what was not

| Seam | Real? |
|---|---|
| Define, Plan, delivery canary, health check | Real Claude 2.1.319 (wrapper passes through) |
| Code under Claude, up to its first file write | Real Claude (gsd-executor subagent committed the task, wrote SUMMARY, was mid-way through STATE/ROADMAP) |
| The quota denial | **Synthetic**: the wrapper stopped real Claude and printed a `rate_limit_event` in the live shape (copied from a real `allowed` event, `status` flipped to `rejected`) plus an `is_error` result, exit 1 |
| Classification, handoff, relaunch, fallback Code, Validate, gate, abort | Real DevFlow, real codex 0.167.0 |

## Result

- Classified `ratelimited` ("rate limited until …"), `agent_handoff` claude → codex with `tree_dirty: true`,
  `commit_count: 1`, relaunched Code in the same worktree on `feature/phase-01`.
- Codex finished the interrupted stage without redoing the task: it kept Claude's commit `90220eb`, committed the
  uncommitted SUMMARY/STATE/ROADMAP work, ran review and verification (`passed`), left a clean tree. Tests pass.
- Validate (on codex) ran the tests, a negative control and a direct value check; verdict pass; gate fired.
- Gate answered with an abort note; the supervisor picked it up 27 s later (poll backoff); state cleared; no process
  left behind.

**Not established:** a real provider denial (none has ever been captured, 999.212); a failover from a primary
other than Claude; repeated runs (this is one run).

## Defects and design problems seen

| Item | Backlog |
|---|---|
| Define launches a paid agent session that does nothing (12-15 s, ~$0.33, twice) | 999.214 |
| `cleanup` says "halted via --until" for a `stop`ped run, contradicts itself, exits 0; `--force` leaves the state, so `status` shows a removed run as active; `start` replaces it silently | 999.213 |
| Fallback keeps the whole run but every message says "takes over the Code stage"; fallback prompt says nothing about the handoff | 999.215 |
| `--version` hides the commit | 999.216 |
| Tracing lines in command output, red ✗ for unset `RUST_LOG`, warnings that do not apply to non-DevFlow projects | 999.217 |
| Denial fixtures lag the live event shape; no real denial archived | 999.212 |
| Plan accepted the pre-written plan without running plan-phase or a checker (8-9 s) | not filed: arguably correct for a pre-planned phase |
| Codex sandbox makes `~/.cache` read-only, so the operator's shell prompt tool prints an error in every codex command | not filed: host config, not DevFlow |

## Probe hygiene

- First launch stopped with `devflow stop` after Define/Plan because of the checkpoint. The wrapper had put real
  Claude in its own session, so `stop`'s group kill missed it (the 999.209 shape, made by the wrapper); DevFlow logged
  `stream_wait_bounded` for it. Ended by its recorded pgid.
- The second launch's wrapper ended its own group; no survivors (checked by pgid).
