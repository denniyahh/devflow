# Memory Recall (MemPalace)

_Wing: devflow · Mode: augment · Transport: mcp. Hit distances ~0.48–0.51 vs a nonsense-query control at ~0.80: relevant, but every hit is about the supervisor design this phase removes._

## Prior decisions
- The write lock is never `unlock()`ed and never taken on `state-NN.json` itself (rename replaces it); `clear_state` takes the same write lock — 50.1-04-PLAN prohibitions (drawer_devflow_planning_6e97d947dec5306ca836cc1d). Carries to D-07's two lock files.
- `update_state` never spawns a process, waits on a gate or blocks on another lock inside its closure — 50.1-04 (same drawer). The runner loop must keep gate waits outside state-lock closures.
- 52.1 exit criterion (operator-confirmed 2026-11-14): one cheap live supervise run on a scratch repo reaches `workflow_finished` with no hand steps — KG fact `Phase 52.1 proposed_exit_criterion`. Same proof as 52.2's D-13.

## Patterns
- A test that cannot survive a flag removal is handed off and renamed in place, not deleted (e.g. `start_proceeds_over_a_corrupt_state_with_no_live_supervisor` → `start_refuses_…`) — 52.1-09-PLAN (drawer_devflow_planning_790c1e1996ffd38b88312134).

## Surprises / gotchas
- A hand-run `advance` beside a live run re-opened a gate under a new id and launched the next stage twice (T-50.1-97) — 50.1-29-PLAN (drawer_devflow_planning_9612c2f1910d9ad474dc73a2). Removing hidden `advance` removes this class; the single-runner loop must not reintroduce a second path that launches a stage.
- A socket-stop path that waited on a release while the launcher still held `.holder-NN` misreported the launcher's pid as "signalled" — 50.1-29 (drawer_devflow_planning_2d5783421ba345854db52d8d). Relevant to D-07's split of runner lock vs phase holder.
