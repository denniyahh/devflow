# Phase 53 deferred items

Out-of-scope discoveries logged by executors; not fixed in the plan that found them.

## From 53-07

- **Flaky `monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter`.**
  One `scripts/check.sh all` run (2026-11-19) failed it with `Capture SigBlk: SigBlk leader
  fffffffffffbfeff, inherited 0000000000000000`; it passed alone twice under nextest, the
  whole `cargo test -p devflow-core --lib` passed on the next run, and a second
  `scripts/check.sh all` passed. Nearly every signal blocked on the leader looks like
  `/proc/<pid>/status` was read between fork and exec, when the child still has the mask
  that the spawn blocked; this is a guess that a read-after-exec check would confirm.
  `monitor.rs` is not touched by 53-07. Should be filed as a 999.x backlog entry.
