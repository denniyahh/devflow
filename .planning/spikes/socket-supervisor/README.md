# Spike: socket-addressable supervisor

Throwaway proof-of-mechanism for the process-teardown design. **Not built,
not tested, not part of the workspace** — preserved because the design in
`.planning/audits/2026-07-24-socket-supervisor-spike.md` was validated with
exactly this code, and the next implementer should be able to re-run it.

To run it: copy into a scratch dir, `cargo build`, then see the audit doc's
Part 1 (C1–C6) and Part 2 (R-A..R-M) for the scenarios and expected results.
Verified to build standalone this way (2026-07-24, rustc 1.97.1) — the
manifest previously pointed `path` at `src/main.rs` while `main.rs` sits at
the root, so it would not have built as originally preserved.

Design and results live in Linear as **DEN-58** (999.33), which carries the
full claim/parity tables; the audit doc referenced above is on an unmerged
planning-docs branch and may not be present in your checkout.

Proves: socket path as a durable cross-process handle; GONE/STALE/ALIVE
liveness with no PID; whole-tree teardown incl. severed-ppid orphans;
takeover safety; and full parity with the production `sh -c` monitor
(worktree cwd, split stdout/stderr capture, pidfile, exit code, advance
tail, env propagation to grandchildren).
