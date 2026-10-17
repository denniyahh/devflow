## Deferred Items

- `phase7_cli::suite_reap_audit` always fails under `cargo nextest` (found during 50-03 Task 1)
  status: open
  **What:** nextest runs each test in its own process, so the audit's process-local `REAPED` counter is always 0 and the test fails after its 60 s deadline with `alive=[], in_flight=0, registered=0`. It passes under `cargo test` (one process per test binary), which is how `scripts/check.sh` runs the suite.
  **Evidence:** the pre-migration file from `HEAD` fails the same way (`cargo nextest run -p devflow --test phase7_cli --no-tests=fail -E 'test(=suite_reap_audit)'`, exit 100), so 50-03's migration did not cause it.
  **Impact:** 50-03's Task 1 gate runs the whole `phase7_cli` target under nextest, so it cannot pass as written. 50-03 ran the sentinel-HOME check with that one test excluded under nextest, and all 28 tests under `cargo test`.
  **Fix options (not taken, out of 50-03's scope):** skip the audit when `NEXTEST` is set, or move it to a nextest setup script / separate binary.
