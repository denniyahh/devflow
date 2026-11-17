## Deferred Items

- A failed agent spawn in the runner reads "failed to spawn monitor"
  status: open
  **What:** `MonitorError::Io`'s text (`crates/devflow-core/src/monitor.rs:38`, `failed to spawn monitor: {0}`) is what `stage_launch_failed`'s `error` carries when `monitor::run_stage` cannot exec the agent inside the runner, although no monitor is spawned there. Seen in 52.2-02's `runner_e2e::a_stage_the_runner_cannot_start_opens_the_infra_gate_with_an_event` (`failed to spawn monitor: No such file or directory (os error 2)`).
  **Why deferred:** devflow-core's error text is outside 52.2-02's files; changing a `pub` error's Display is a devflow-core change. 52.2-11, which deletes the old monitor path, is the natural owner.
