# Phase 47 — Deferred Items

## Deferred Items

- `cargo clippy -p devflow-core --all-targets` does not compile two devflow-core integration tests
  status: open
  **What:** `tests/monitor_e2e.rs:30` and `tests/devflow_dir_gitignore.rs:231,286,303` call
  `devflow_core::test_support::git_command`, and rustc reports the module as "configured out" when only
  `-p devflow-core` is selected. The feature that enables it is only active under workspace feature
  unification, so `cargo clippy --workspace --all-targets -- -D warnings` (the CLAUDE.md / CI form) exits 0.
  **Found during:** 47-02 Task 1, 2026-09-11. Out of scope for 47-02: neither file was touched, and the
  errors do not involve the `#[cfg(test)]` code the plan changed.
  **Not established:** whether it predates Phase 47. No negative control was run on the fork-point tree.
  **Impact:** a package-scoped `--all-targets` check, which a contributor might reasonably run, fails
  for reasons unrelated to their change.
