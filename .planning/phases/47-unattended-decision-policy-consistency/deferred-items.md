# Phase 47 — Deferred Items

## Deferred Items

- `cargo clippy -p devflow-core --all-targets` does not compile two devflow-core integration tests
  status: resolved
  **What:** `tests/monitor_e2e.rs:30` and `tests/devflow_dir_gitignore.rs:231,286,303` call
  `devflow_core::test_support::git_command`, and rustc reports the module as "configured out" when only
  `-p devflow-core` is selected. The feature that enables it is only active under workspace feature
  unification, so `cargo clippy --workspace --all-targets -- -D warnings` (the CLAUDE.md / CI form) exits 0.
  **Found during:** 47-02 Task 1, 2026-09-11. Out of scope for 47-02: neither file was touched, and the
  errors do not involve the `#[cfg(test)]` code the plan changed.
  **Predates Phase 47** (orchestrator, 2026-09-11): on the fork-point tree `034f5b6`, whose `crates/`,
  `Cargo.toml` and `Cargo.lock` are identical to the phase base, the same command exits 101 with the same
  `E0433: cannot find test_support in devflow_core` errors, while `cargo clippy --workspace --all-targets`
  on that tree exits 0. Only the first two error sites were compared, not the full list of affected files.
  **Impact:** a package-scoped `--all-targets` check, which a contributor might reasonably run, fails
  for reasons unrelated to their change.
  **Resolution (2026-09-13):** devflow-core now dev-depends on itself with `test-support` — `fc40bd9` on
  branch `fix/test-support-and-dependency-checks`, merged to `develop` via #212 as `f6aad13` (merge
  `4a2bfcb`; identical patch-id). On that branch the command went from
  exit 101 to exit 0, both integration tests pass, workspace clippy stays green, and the `devflow`
  binary's normal dependency graph still builds devflow-core with no features.

- `an_empty_path_child_cannot_resolve_git_while_the_parent_still_can` races PATH-mutating sibling tests
  status: resolved
  **What:** `crates/devflow-cli/src/test_support.rs:749-799` reads process-global `PATH` before and after
  a child-process run and asserts it is byte-identical, deliberately without `env_lock()` (its doc
  comment: "nothing in this test mutates process-global state any more"). Sibling tests in the same
  `devflow` binary still set `PATH` to a `/tmp/.tmp…` stub directory while holding `env_lock()`, for
  example the `relaunch_checkpoint_session_*` tests via `prepend_path`. When one of them runs inside the
  window, the post-run read sees the stubbed value.
  **Observed:** 47-03 Task 3 gate, first `scripts/check.sh test` run, 2026-09-11 — `left:
  Some("/tmp/.tmpZLHb3f:/home/…")`, `right: Some("/home/…")`; the `devflow` bin reported 363 passed,
  1 failed. The test passed alone, and in 5 other full-bin runs on the same tree.
  **Why it is not 47-03's:** 0 of the 240 added lines in 47-03's diff touch `PATH`, `env_lock`,
  `set_var` or `prepend_path`; `crates/devflow-cli/src` has 117 PATH writers at `f98e404` and at HEAD;
  `test_support.rs` is unchanged since `f98e404`. **Not established:** a reproduction on the
  pre-47-03 tree, or a failure rate.
  **Impact:** an intermittent red in `scripts/check.sh test` (the required CI Test job) unrelated to
  the change under test.
  **Resolution (2026-09-13):** the test's parent half holds `env_lock()` — `82f081f` on branch
  `fix/test-support-and-dependency-checks`, merged to `develop` via #212 as `683f6dc` (merge `4a2bfcb`;
  identical patch-id). Every devflow-cli file that mutates `PATH`
  takes the same lock. The race was never reproduced on demand, so this rests on construction plus a
  passing run, not on an observed before and after.

- devflow-core `PathGuard` (`agents/pi.rs`, `agents/opencode.rs`) hides `git`/`sh` from concurrent tests
  status: open
  **What:** `PathGuard::set` replaces process-global `PATH` with a tempdir holding only a stub script
  (`pi.rs:284-307`, `opencode.rs:420-443`). Each file serializes its own callers with a module-local
  `static ENV_MUTEX` (`pi.rs:194`, `opencode.rs:355`), and the guard's SAFETY comment claims "no other
  thread reads/writes PATH". That claim is false: the `git.rs`, `hooks.rs`, `version.rs` and
  `worktree.rs` tests resolve `git`/`sh`/`sleep` through `PATH` and take no lock at all (0 lock
  references in each file; control: `config.rs` has 17).
  **Observed:** 47-03 Task 3 gate, second `scripts/check.sh test` run, 2026-09-11 — the `devflow_core`
  lib reported 668 passed, 101 failed in 2.35s, and every panic was a spawn `NotFound` (`spawn git: …
  NotFound` ×40, `unwrap()` on `Err(NotFound)` ×43, monitor stubs, `spawn sleep fixture`). The same lib
  binary passed 3 of 3 immediate re-runs (769 passed) and passed again inside the next `check.sh test`.
  **Why it is not 47-03's:** `pi.rs` and `opencode.rs` are unchanged since `f98e404`, and 47-03's
  devflow-core changes are string constants and tests only. **Not established:** a reproduction on
  the pre-47-03 tree, or a failure rate. Same hazard class as Phase 46 C-01; whether that sweep
  examined `devflow-core` was not checked.
  **Impact:** a burst of about 100 spurious failures in the required Test job, which reads as a
  broken build.
  **Tracked (2026-09-13):** added to ROADMAP 999.38 as its third site family and commented on
  devflow#181. Still open: its fix is 999.38's per-`Command` refactor, not a lock.
