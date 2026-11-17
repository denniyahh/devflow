## Deferred Items

- A failed agent spawn in the runner reads "failed to spawn monitor"
  status: open
  **What:** `MonitorError::Io`'s text (`crates/devflow-core/src/monitor.rs:38`, `failed to spawn monitor: {0}`) is what `stage_launch_failed`'s `error` carries when `monitor::run_stage` cannot exec the agent inside the runner, although no monitor is spawned there. Seen in 52.2-02's `runner_e2e::a_stage_the_runner_cannot_start_opens_the_infra_gate_with_an_event` (`failed to spawn monitor: No such file or directory (os error 2)`).
  **Why deferred:** devflow-core's error text is outside 52.2-02's files; changing a `pub` error's Display is a devflow-core change. 52.2-11, which deletes the old monitor path, is the natural owner.

- A hermetic test child can reach a real agent CLI on the developer's PATH
  status: open
  **What:** `hermetic_devflow` sets PATH to the fake service-manager dir followed by the parent's PATH (`crates/devflow-cli/tests/common/mod.rs`, `path_after(&home.service_bin(), &parent)`). When a test's agent stub fails to exec, the PATH search falls through to the developer's real CLI. Seen in 52.2-02: the W-2 test's stub had a bad interpreter and `~/.local/bin/codex` ran for about 16 s on a Plan prompt in a temp repo. Whether it reached the API was not checked. 52.2-02 fixed that one test (stub-only PATH plus an assertion that no other codex resolves); the class is open.
  **Why deferred:** found by an executor outside its plan's files. 52.2-21's scan bans `.env("PATH", …)` overrides but not this fall-through. Fix shape: a shared scan or helper that fails when any agent binary name (claude, codex, gemini, agy, pi, hermes, opencode) resolves outside the test's own stub dir on a hermetic child's PATH. Owner: 52.2-21 if it fits its scan, else a 999.x entry at phase close.

- CONTRIBUTING.md names a test file that does not exist
  status: open
  **What:** CONTRIBUTING.md's "Required checks" paragraph says the `--all-targets` regression guards live in `crates/devflow-cli/tests/devcontainer_ci_failfast.rs`; no such file exists in the tree (checked by 52.2-03 with `ls`). The guards it describes are presumably in `ci_parity_guards.rs`, unconfirmed.
  **Why deferred:** pre-existing doc drift beside 52.2-03's CONTRIBUTING edit, not caused by it. Fix shape: point the sentence at the file that holds the guards. Owner: the phase's docs pass or any CONTRIBUTING change.
