## Deferred Items

- A failed agent spawn in the runner reads "failed to spawn monitor"
  status: open
  **What:** `MonitorError::Io`'s text (`crates/devflow-core/src/monitor.rs:38`, `failed to spawn monitor: {0}`) is what `stage_launch_failed`'s `error` carries when `monitor::run_stage` cannot exec the agent inside the runner, although no monitor is spawned there. Seen in 52.2-02's `runner_e2e::a_stage_the_runner_cannot_start_opens_the_infra_gate_with_an_event` (`failed to spawn monitor: No such file or directory (os error 2)`).
  **Why deferred:** devflow-core's error text is outside 52.2-02's files; changing a `pub` error's Display is a devflow-core change. 52.2-11, which deletes the old monitor path, is the natural owner.

- A hermetic test child can reach a real agent CLI on the developer's PATH
  status: resolved (52.2-21, commit 9ffbfdb)
  **What:** `hermetic_devflow` sets PATH to the fake service-manager dir followed by the parent's PATH (`crates/devflow-cli/tests/common/mod.rs`, `path_after(&home.service_bin(), &parent)`). When a test's agent stub fails to exec, the PATH search falls through to the developer's real CLI. Seen in 52.2-02: the W-2 test's stub had a bad interpreter and `~/.local/bin/codex` ran for about 16 s on a Plan prompt in a temp repo. Whether it reached the API was not checked. 52.2-02 fixed that one test (stub-only PATH plus an assertion that no other codex resolves); the class is open.
  **Why deferred:** found by an executor outside its plan's files. 52.2-21's scan bans `.env("PATH", …)` overrides but not this fall-through. Fix shape: a shared scan or helper that fails when any agent binary name (claude, codex, gemini, agy, pi, hermes, opencode) resolves outside the test's own stub dir on a hermetic child's PATH. Owner: 52.2-21 if it fits its scan, else a 999.x entry at phase close.
  **Resolution:** 52.2-21 (operator-approved scope addition, 2026-11-17) gives each `HermeticHome` an `agent-guard/` dir with one fake per agent program the registry launches (`driver_for(kind).build_command` for every `AgentKind`, plus `antigravity` and `gemini`), each exiting 127 with `fake <name>: no stub installed for this test`. `hermetic_devflow` and `hermetic_path` put it after the test's own dirs and before the first dir inherited from the parent PATH, so a missing or broken stub stops there. `test_home_isolation::a_hermetic_path_never_falls_through_to_a_real_agent_cli` proves it against a planted working agent on a synthetic parent PATH, with an unguarded control that reaches it. `service_e2e.rs` builds its own real-PATH command and is outside this guard by design (host-only, every test ignored).

- CONTRIBUTING.md names a test file that does not exist
  status: open
  **What:** CONTRIBUTING.md's "Required checks" paragraph says the `--all-targets` regression guards live in `crates/devflow-cli/tests/devcontainer_ci_failfast.rs`; no such file exists in the tree (checked by 52.2-03 with `ls`). The guards it describes are presumably in `ci_parity_guards.rs`, unconfirmed.
  **Why deferred:** pre-existing doc drift beside 52.2-03's CONTRIBUTING edit, not caused by it. Fix shape: point the sentence at the file that holds the guards. Owner: the phase's docs pass or any CONTRIBUTING change.
