---
phase: 48-survivable-state-writes-and-honest-gate-recovery
fixed_at: 2026-09-18T15:57:32-04:00
review_path: .planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-REVIEW.md
iteration: 1
findings_in_scope: 1
fixed: 1
skipped: 0
status: all_fixed
verification_environment: isolated worktree (/var/home/denniyahh/Github/devflow/.worktrees/phase-48)
---

# Phase 48: Code Review Fix Report

**Fixed at:** 2026-09-18T15:57:32-04:00
**Source review:** `.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-REVIEW.md`
**Iteration:** 1

**Summary:**

- Findings in scope: 1
- Fixed: 1
- Skipped: 0

## Fixed Issues

### CR-01: Escaped pipe-holder bypasses the parent-exit timeout

**Files modified:** `crates/devflow-core/src/agents/opencode.rs`
**Commit:** Not committed — independent orchestration requested no executor commit.
**Applied fix:**

- Linux probe children now enter a narrow pre-`exec` seccomp filter after Rust's `process_group(0)` setup. The filter returns `EPERM` for `setsid` and `setpgid` while allowing all other syscalls, so neither the probe nor its descendants can leave the dedicated kill group through either escape primitive.
- stdout and stderr begin draining on dedicated readers immediately. Completion waits for those readers only until the original probe deadline; it never calls synchronous `read_to_end` after observing parent exit. A retained descriptor therefore returns `TimedOut` rather than hanging `spawn_with_timeout`, `health`, or `capabilities`.
- Added Linux regressions for real `setsid` and `setpgid(0, 0)` escape attempts. Each has an unguarded negative control that must succeed, then verifies that the guarded `health` and `capabilities` paths fail closed promptly, the syscall receives a non-zero status, and the attempted child is no longer running.

## Verification

Ran in the isolated Phase 48 worktree:

- `cargo test -p devflow-core --lib agents::opencode::tests::probe_rejects_a_setsid_pipe_holder_before_parent_exit -- --exact` — passed: 1 passed, 817 filtered out.
- `cargo test -p devflow-core --lib agents::opencode::tests::probe_rejects_a_setpgid_pipe_holder_before_parent_exit -- --exact` — passed: 1 passed, 817 filtered out.
- `cargo test -p devflow-core --lib agents::opencode::tests:: -- --nocapture` — passed: 27 passed, 0 failed, 791 filtered out. This includes the existing normal parent-exit descendant cleanup test.
- `cargo clippy -p devflow-core --lib -- -D warnings` — passed.
- `rustfmt --check crates/devflow-core/src/agents/opencode.rs` and `git diff --check -- crates/devflow-core/src/agents/opencode.rs` — passed.

The two targeted regressions are discriminating controls: without the Linux pre-`exec` guard, their real escape utilities succeed, leave the group, retain the inherited pipes after the shell exits, and the prior synchronous parent-exit drain blocks. The passing Linux runs establish that those two tested escape routes are denied and the public probes return fail-closed. They do not establish behavior for every possible Linux namespace or privilege-escalation mechanism.

## Platform Guarantees and Limits

- **Linux:** when the filter installs, `setsid`/`setpgid` escape is denied before the target program runs, so process-group termination covers descendants using those mechanisms. If the kernel rejects filter installation, spawn itself fails closed; no uncontained probe is launched.
- **Other Unix:** ordinary process-group descendants are still terminated by `killpg`. A descendant that escapes that group cannot make the caller hang: concurrent readers time out at the probe deadline and the public probes fail closed. This crate has no native tree/job primitive there, so such an escaped descendant may outlive the probe and keep its reader thread blocked until it closes the inherited descriptor.
- **Non-Unix:** the direct child is killed at timeout and reader completion remains deadline-bounded. There is no descendant-tree termination claim for escaped descendants.
- Only `x86_64-unknown-linux-gnu` is installed in this worktree, so the non-Linux fallback was not cross-compiled or runtime-tested here. The existing test module already imports Unix-only test support, so that is a pre-existing test portability limit, not evidence of Windows/macOS runtime behavior.

---

_Fixed: 2026-09-18T15:57:32-04:00_
_Fixer: Codex (gsd-code-fixer)_
_Iteration: 1_
