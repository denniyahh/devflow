# Phase 7 — Claude's Phase 6 Testing Sprint Recommendations

From Claude's review of Codex's 16-test sprint (2026-06-18):

## Must fix (from sprint review)
- [ ] **spawn_monitor test is weak** — asserts `pid > 0` (tautological). Test launches detached shell script that re-invokes cargo test binary 5×, leaks background processes, races tempdir teardown. Fix: assert on observable output (pid file appears) or stub binary.
- [ ] **Layer 2 failure paths untested** — the decision matrix has 3 outcomes (exit=0+commits>0→Success, exit=0+commits=0→Failed "no work", exit≠0→Failed), but only Success got a test. The two failure branches with `reason` formatting are uncovered.
- [ ] **Lowercase-no-space marker variant** (`agent_result.rs:98`) still uncovered.

## Nice to have
- [ ] `capture_agent_output` doesn't cover stderr behavior (monitor discards stderr)
- [ ] Monitor daemon integration test (end-to-end: spawn monitor, agent writes DEVFLOW_RESULT, verify check() advances)
