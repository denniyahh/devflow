## Deferred Items

- `phase7_cli::suite_reap_audit` always fails under `cargo nextest` (found during 50-03 Task 1)
  status: open
  **What:** nextest runs each test in its own process, so the audit's process-local `REAPED` counter is always 0 and the test fails after its 60 s deadline with `alive=[], in_flight=0, registered=0`. It passes under `cargo test` (one process per test binary), which is how `scripts/check.sh` runs the suite.
  **Evidence:** the pre-migration file from `HEAD` fails the same way (`cargo nextest run -p devflow --test phase7_cli --no-tests=fail -E 'test(=suite_reap_audit)'`, exit 100), so 50-03's migration did not cause it.
  **Impact:** 50-03's Task 1 gate runs the whole `phase7_cli` target under nextest, so it cannot pass as written. 50-03 ran the sentinel-HOME check with that one test excluded under nextest, and all 28 tests under `cargo test`.
  **Fix options (not taken, out of 50-03's scope):** skip the audit when `NEXTEST` is set, or move it to a nextest setup script / separate binary.
- `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates` leaves `sleep 30` children running after it passes (found during 50-04 verification)
  status: open
  **What:** under `cargo nextest run -p devflow --bin devflow`, nextest marks the test LEAK; four `sleep 30` processes reparented to `systemd --user` survived the run and exited on their own at 30 s. Only one process's environment was read; it carried this test's `NEXTEST_ATTEMPT_ID`. The other three are attributed to the test because they had the same parent and started within two seconds of it. Nextest also marked `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper` (devflow-core) and `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling` LEAK, with no survivor seen when checked.
  **Evidence:** 50-04 does not touch these tests. Whether they also leak at the plan's base commit was not checked.
  **Impact:** the processes expire after 30 s, but each run leaves them behind for that long. That works against the rule that a run leaves no process of its own behind.
- In-process `devflow` unit tests still write registry entries into the operator's real `~/.cache/devflow/roots/` (found during 50-21 verification; answers 50-03's open question)
  status: open
  **What:** one `scripts/check.sh all` run at 50-21's HEAD (every e2e suite on the hermetic builder) added 33 new `roots/*.json` files to the real cache. Each names a `tempfile` project root (`/tmp/.tmpXXXXXX`, not a `/tmp/dfh*` hermetic home). Their phase numbers (186, 191, 113, 620 to 631, ...) occur only in the `#[cfg(test)]` modules of `crates/devflow-cli/src/pipeline_launch.rs` and `crates/devflow-cli/src/preflight.rs`, so the writers are in-process unit tests that call the registry with the test process's own `HOME`, not any integration suite.
  **Evidence:** the five suites 50-21 migrated left no devflow file under a sentinel HOME even before migration (17 tests, `leaked=[]`), and every integration suite now spawns through `common::hermetic_devflow`. The attribution is by phase-number match only; no per-test run was made.
  **Impact:** the operator's real registry keeps growing with dead entries on every full check run, and once 50-11 binds sockets under the cache dir the same tests could bind there. Nothing was deleted from the real cache.
  **Fix options (not taken, out of 50-21's scope):** give those unit tests an explicit cache-dir parameter or a test-only registry root; process-global env mutation is not an option (CLAUDE.md test-isolation rules).
