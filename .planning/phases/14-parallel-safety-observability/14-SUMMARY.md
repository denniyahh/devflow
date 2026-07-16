# Phase 14 Summary: Parallel Safety + Observability

**Completed:** 2026-07-16 | **Plans:** 4/4 | **Validation:** `cargo test
--workspace` 252 passed / 0 failed; `cargo clippy --workspace --all-targets
-- -D warnings` clean; `cargo fmt --check` clean; live two-phase e2e run
(below).

## What shipped

### 14a — CR-03 parallel safety (`13-DEFERRED-CR-03.md` closed)

- **Per-phase state files** (`workflow.rs`): `.devflow/state-{NN}.json`
  mirrors the lock naming; `load_state`/`clear_state` take the phase,
  `list_states()` enumerates active phases; a legacy single-slot
  `state.json` migrates on first read (per-phase file wins if both exist).
- **Phase-threaded advance** (`monitor.rs`, `main.rs`): the monitor script
  now ends with `devflow advance {root} --phase N`, recorded at spawn time.
  `advance()` keys the per-phase lock on the argument directly — the
  pre-lock state read (and its TOCTOU guard) is gone by construction. A
  bare `advance` (legacy monitor) falls back to the single active phase.
- **Coarse checkout lock** (`lock.rs`): second lock level
  `.devflow/lock-project` (`acquire_project` + bounded-backoff
  `acquire_project_blocking`) serializes all primary-checkout git mutation:
  Validate→Ship hook batch, after-ship hook batch, and sequentagent's
  ensure-branch/integrate/push sections. Held for seconds, never across a
  gate. Shares the `lock-` prefix so the existing stale-holder sweep covers
  it.
- **cron-instructions re-check**: per-phase
  `cron-instructions-{NN}.json` with legacy read/delete compat;
  `status` hints and `recover --clean` handle all of them.
- **Multi-phase status/recover**: `status` prints one block per active
  phase; `recover` uses `inspect_all` and cleans every phase.

### 14b — sequentagent behind the monitor (sync path deleted)

- `run_agent_blocking` = `spawn_monitor_no_advance` + `wait_for_agent_exit`
  (blocks on the exit file, errors if the monitor dies without writing it).
  Gains stderr/stdout capture separation for free; the exit-code-without-
  marker gate is preserved.
- sequentagent holds its phase's lock for the whole run (excludes a
  concurrent monitored run of the same phase — it took no lock before).
- Deleted: `agent::launch_agent`, `agent::capture_agent_output`,
  `AgentCapture`, and the `lib.rs` re-export. The monitor is now the single
  way an agent process is spawned.

### 14c — Observability

- **`events.rs` (new)**: append-only `.devflow/events.jsonl`, schema v1
  (`v`/`ts`/`phase`/`event` envelope + kind fields; envelope keys cannot be
  forged by payloads; emission fail-soft). Kinds: workflow_started,
  stage_launched, advance_evaluated, transition, loop_back, gate_fired,
  notify_fired, gate_resolved, gate_timeout, hook_run, workflow_finished,
  workflow_aborted.
- **`devflow logs [--follow] [--phase N] [--stderr]`**: prints the capture
  file; defaults to the single active phase, else the newest capture;
  `--follow` terminates once the exit file exists and output is quiescent.
- **Richer `devflow status`**: per-phase blocks with stage/mode/gate,
  agent + pid liveness, elapsed age, and the phase's last event
  ("last action: notify_fired (ship) (8s ago)").

## Acceptance evidence (live e2e, fake agent, real binary)

Two phases started back-to-back in one repo: each ran
Define→Plan→Code→Validate(pass)→Ship in its own worktree/monitor chain,
both blocked at their own Ship gates **concurrently** (two state files, two
locks, two gate files — previously the second start clobbered the first),
both approved, both `finish_workflow`s serialized on the checkout lock, and
the history recorded **both** version bumps as separate tags (v0.0.1,
v0.1.0). 46 events in events.jsonl; `logs`/`status` observed the run from a
second terminal.

## Notes for later phases

- Phase 16's Hermes gate watcher can consume `events.jsonl` as designed
  (gate_fired/gate_resolved carry stage + context + responded_by).
- The Hermes skill file (16b) must document `advance --phase`, per-phase
  state/cron file names, and `devflow logs`.
- Phase 15 docs rewrite should list the new `logs` command and the
  `.devflow/` file inventory.
