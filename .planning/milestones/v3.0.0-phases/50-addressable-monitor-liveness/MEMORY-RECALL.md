# Memory Recall (MemPalace)

_Wing: devflow · Mode: augment · Transport: mcp_

Recalled 2026-10-13 for Phase 50 planning. Phase 50's own CONTEXT is excluded here, because the planner reads it
directly. Each item is distilled from prior phases, and its provenance is given so it can be checked.

## Prior decisions
- **Group-kill needs a descendant walk.** `killpg` alone orphaned two descendants that had left the process group,
  as seen in a deliberate interrupt trial (`killpg(...)` plus `kill(...) [out of group]` × 2). Backlog 999.46 tracks
  this leak shape. Relevant to Phase 50's STALE backstop and group-teardown design. — drawer
  `drawer_devflow_milestones_3863df86a0d59545a2700b4a` (30-04-SUMMARY)
- **`agent::terminate_and_verify` rejects non-positive pids by design**, to guard against `kill(-1)` and `kill(0)`.
  Passing a negative pid to it is a silent no-op, not a group kill. Phase 31's resolution: send SIGTERM to the
  group, then call `terminate_and_verify(child_pid, …)` for the leader, then sweep the group with SIGKILL. Do not
  write a second escalation loop. — drawer `drawer_devflow_milestones_22ca352c7e27749a97d6891b` (31-02-SUMMARY),
  `drawer_devflow_planning_4bee2b559393b8e612e68767` (25-16-PLAN)
- **A bare SIGTERM does not clear orphaned monitors.** On 2026-07-27, all 15 orphaned monitor wrappers survived
  SIGTERM, and killing the wrappers orphaned their `devflow advance` children (30 processes, SIGKILL needed).
  `agent::terminate` sends a single SIGTERM with no verification. — drawer
  `drawer_devflow_planning_ae48a7c05f3e2af275be6a48` (25-02-PLAN)
- **Lock holder classification exists.** Phase 48-13 added `lock::HolderStatus {NoHolder, Live, Recycled,
  Unconfirmable}`, which uses start-time equality. A mismatched start time is `Recycled`, never `Live`. Only `Live`
  and `Unconfirmable` may be waiting. This is the primitive that 999.128 (lock reclaim by pid plus start time) builds
  on. — drawers `drawer_devflow_milestones_954de49408a5978201ab3f5a`,
  `drawer_devflow_milestones_4dbcd6ffd4a77700e2f87cad` (48-13-SUMMARY)
- **The live-run guard is pid liveness, not identity (operator Option A, 2026-09-22).** A recycled pid produces a
  fail-closed false refusal that points the operator to `ps -p`. Phase 48 explicitly deferred "lock reclaim by pid
  plus start time" to Phase 50. — drawers `drawer_devflow_milestones_9fcf1065cfeabd54f1b7526e` (48-19-SUMMARY),
  `drawer_devflow_decisions_57a2a3beecdd18ffe0fe21f2` (48-CONTEXT)
- **No-state carve-out (operator-approved narrowing, 2026-09-22).** The start guard applies only when a state file
  exists, because `recover --clean` removes the state but not the agent pid file. Test:
  `start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process`. Phase 50's D-09/R-12
  corrupt-state refusal changes this area. — drawer `drawer_devflow_milestones_fcb6f0c6c7b92e72c5518209`
  (48-19-SUMMARY)
- **Phase 49 proof-rig rules are still current in the knowledge graph.** The run uses a disposable GSD project on a
  private throwaway GitHub repo; each live attempt is one human-action checkpoint; there are at most 3 attempts;
  the run launches with `--yes-ship`; and the work stays within GSD's confines. Phase 50 D-16 carries all of these.
  — KG `DevFlow Phase 49 → decided → …` (valid from 2026-09-24/28)

## Patterns
- **Hermetic registry and cache tests.** `registry::load_roots()` reads the developer's real machine registry.
  Tests should use `registry::load_roots_in(cache_dir)` and `registry::register_in(cache_dir, ..)` with a tempdir,
  not mutate `DEVFLOW_CACHE_DIR`; mutating it would need `ENV_MUTEX` and re-enter the 999.38 env-race class.
  Directly relevant to D-07/999.141 and to sweep tests. — drawer `drawer_devflow_planning_3ee37b5ad3366908ee5327f7`
  (25-15-PLAN)
- **Cache directory permissions.** Phase 23 created `~/.cache/devflow/` and `roots/` with mode `0o700`, citing the
  same rationale as the earlier socket-directory decision. The socket directory and sidecars should match. — drawer
  `drawer_devflow_planning_056f4d640fb324a155d25a21` (23-03-PLAN, T-23-33)
- **Process-reap evidence is a listing, not a claim.** Phase 30's harness recorded the pgid, sent SIGTERM then
  SIGKILL to the group in a `finally` block, then polled for surviving members, and acceptance required a
  post-interrupt process listing. — drawer `drawer_devflow_planning_f94d163509de87ffdba1e8e5` (30-04-PLAN, T-30-17)

## Surprises / gotchas
- **A rebuilt binary mid-run bypasses the lock.** `monitor.rs` spawns `current_exe()`, so a binary rebuilt or
  installed mid-run runs new code against an in-flight state. Under D-20, this is the operational rule "install 3.x
  only while no 2.x run is live". — drawer `drawer_devflow_decisions_57a2a3beecdd18ffe0fe21f2` (48-CONTEXT)
- **Process identity is Linux-only.** `agent::process_start_time` reads `/proc/{pid}/stat` and works only within one
  PID namespace. — same drawer (48-CONTEXT)
- **Pid-to-another-user reads as dead.** `kill(0)` returns EPERM, so `agent_running` is false. Zombies are also
  treated as dead. — drawer `drawer_devflow_milestones_881c750b93f7a36b154c0762` (48-19-SUMMARY)
