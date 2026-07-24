# Spike: socket-addressable supervisor

Throwaway proof-of-mechanism for the process-teardown design. **Not built,
not tested, not part of the workspace** — preserved because the design in
`.planning/audits/2026-07-24-socket-supervisor-spike.md` was validated with
exactly this code, and the next implementer should be able to re-run it.

To run it: copy into a scratch dir, `cargo build`, then see the audit doc's
Part 1 (C1–C6) and Part 2 (R-A..R-M) for the scenarios and expected results.

Proves: socket path as a durable cross-process handle; GONE/STALE/ALIVE
liveness with no PID; whole-tree teardown incl. severed-ppid orphans;
takeover safety; and full parity with the production `sh -c` monitor
(worktree cwd, split stdout/stderr capture, pidfile, exit code, advance
tail, env propagation to grandchildren).
