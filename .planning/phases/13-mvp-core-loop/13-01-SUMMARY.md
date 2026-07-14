---
phase: 13-mvp-core-loop
plan: 01
subsystem: reliability
tags: [rust, gate-protocol, notify-hook, shell-injection-safe, wr-11]

# Dependency graph
requires: []
provides:
  - "fire_gate_notify + run_notify_command in devflow-core/src/gates.rs (fail-soft, argv/env-based, never shell-interpolated notify hook)"
  - "handle_stage_failure / handle_ship_failure / gate_timeout_secs / prepare_loop_back_to_code in devflow-cli/src/main.rs"
  - "Every non-Validate stage failure (Define/Plan/Code/Ship-AgentFailed) now gates+notifies instead of returning a silent Err (WR-11 closed)"
  - "Ship ReviewFailed loops back to Code with FixType::AuditFix instead of GapsOnly"
  - "DEVFLOW_GATE_TIMEOUT_SECS env override for the 7-day gate poll timeout"
affects: [13-02, 13-03, 13-04, 13-05, 13-06]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pure-fn + env-reading-wrapper split for testability (parse_gate_timeout/gate_timeout_secs, run_notify_command/fire_gate_notify) — avoids process-global env mutation in unit tests wherever possible"
    - "Module-level ENV_MUTEX static to serialize the one unavoidable env-mutating test per crate"
    - "CR-01 stale-gate-cleanup-before-retry pattern extended to a generic handle_stage_failure (previously only loop_back_to_code/abort/finish_workflow did this)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/gates.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/Cargo.toml

key-decisions:
  - "Split loop_back_to_code into prepare_loop_back_to_code (pure state mutation) + the launch_stage call so ReviewFailed's dispatch/state-mutation logic is unit-testable without ever spawning the real configured agent CLI"
  - "non_validate_failure_fires_gate_and_hook asserts only that the notify hook fired (sentinel exists) plus a separate pure should_gate() check, not the exact DEVFLOW_NON_SILENT_GATE value read back through the process-global env var — that exact value is already covered contamination-free by gates.rs's notify_hook_sets_non_silent_flag test"
  - "Added tracing as a direct devflow-cli dependency (was previously only pulled in transitively via tracing-subscriber) to support the new never-silent-gate info! log"

requirements-completed: [13a, 13c, WR-11]

coverage:
  - id: D1
    description: "Every non-Validate stage failure (Define/Plan/Code, or a Ship agent crash) fires a gate + notify instead of returning a silent Err"
    requirement: "WR-11"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#ship_agent_failed_fires_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#non_validate_failure_fires_gate_and_hook"
        status: pass
    human_judgment: false
  - id: D2
    description: "Ship ReviewFailed (review: prefix) loops back to Code with the /gsd-audit-fix prompt, not GapsOnly"
    requirement: "13a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#ship_review_failed_loops_to_code"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#ship_review_failed_uses_audit_fix"
        status: pass
    human_judgment: false
  - id: D3
    description: "Notify hook is fail-soft and shell-injection-safe (context/phase/stage passed via env vars, never interpolated into the sh -c string)"
    requirement: "13c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#notify_hook_failure_is_fail_soft"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#notify_hook_runs_configured_command"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#notify_hook_sets_non_silent_flag"
        status: pass
    human_judgment: false
  - id: D4
    description: "Gate poll timeout is configurable via DEVFLOW_GATE_TIMEOUT_SECS, defaulting to 7 days"
    requirement: "13c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#parse_gate_timeout_env_override"
        status: pass
    human_judgment: false
  - id: D5
    description: "CR-01 closed: retrying a failed non-Validate stage cleans up the stale gate/response/ack before the retry, so it cannot silently consume the prior response"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#stage_failure_retry_cleans_stale_response"
        status: pass
    human_judgment: false

duration: 17min
completed: 2026-07-14
status: complete
---

# Phase 13 Plan 01: Never-Silent Gates + Ship Failure Split Summary

**WR-11's silent-halt bug closed: every non-Validate stage failure now writes a gate + fires a fail-soft, injection-safe notify hook, Ship AgentFailed/ReviewFailed are split (ReviewFailed loops to Code with `/gsd-audit-fix`), and the gate poll timeout is env-configurable.**

## Performance

- **Duration:** ~17 min (first commit 16:00:40 → last commit 16:16:42, plus SUMMARY authoring)
- **Started:** 2026-07-14T16:00Z (approx, first task commit)
- **Completed:** 2026-07-14T16:16Z
- **Tasks:** 3
- **Files modified:** 3 (`crates/devflow-core/src/gates.rs`, `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/Cargo.toml`)

## Accomplishments

- `fire_gate_notify`/`run_notify_command` in `gates.rs`: a fail-soft, shell-injection-safe notify hook. Reads `DEVFLOW_GATE_NOTIFY_CMD`; passes phase/stage/context/unexpected to the child via `.env()` — never interpolated into the `sh -c` string (WR-01/T-13-01 precedent).
- `advance()`'s failure-handling catch-all no longer returns a bare `CliError::Message` for Define/Plan/Code/Ship failures. `handle_stage_failure` unconditionally gates + notifies (independent of `Mode::should_gate`), and `handle_ship_failure` splits Ship's `AgentFailed` (gate+notify) from `ReviewFailed` (`review:`-prefixed reason, loops back to Code with `FixType::AuditFix`).
- `run_gate` computes an `unexpected` flag (`!should_gate(...)`) marking gates the active mode wouldn't normally fire, logs it via `info!`, and always calls `fire_gate_notify` after writing the gate.
- CR-01 (stale gate/response/ack reuse on retry) closed for the new `handle_stage_failure` path: `Gates::cleanup` runs before any Advance/LoopBack re-launch, mirroring the existing `loop_back_to_code`/`abort`/`finish_workflow` cleanup.
- Gate poll timeout is now `DEVFLOW_GATE_TIMEOUT_SECS`-configurable via a pure `parse_gate_timeout`/env-reading `gate_timeout_secs` split, replacing the hardcoded `GATE_TIMEOUT_SECS` const (still defaults to 7 days).
- `loop_back_to_code` gained a `FixType` parameter; existing Validate/Ship call sites pass `FixType::GapsOnly` unchanged.

## Task Commits

1. **Task 1: Add fail-soft gate notify hook to gates.rs** - `22ec1b7` (feat)
2. **Task 2: Add non-Validate + Ship failure branches and env-configurable gate timeout to advance()** - `548711c` (feat)
3. **Task 3: Unit tests for Ship/stage failure gating and env timeout** - `0126f20` (test)

**Plan metadata:** _(to be committed after this SUMMARY)_

## Files Created/Modified

- `crates/devflow-core/src/gates.rs` - `fire_gate_notify`/`run_notify_command` (fail-soft, argv/env-based notify hook) + 4 unit tests + module `ENV_MUTEX`
- `crates/devflow-cli/src/main.rs` - `parse_gate_timeout`/`gate_timeout_secs`, `handle_stage_failure`, `handle_ship_failure`, `is_ship_review_failure`, `prepare_loop_back_to_code`; `run_gate` notify wiring; `advance()`'s failed-match dispatch; 6 new unit tests + module `ENV_MUTEX`
- `crates/devflow-cli/Cargo.toml` - added `tracing` as a direct dependency (needed for the never-silent-gate `info!` log; was previously only a transitive dependency via `tracing-subscriber`)

## Decisions Made

- **Extracted `prepare_loop_back_to_code`** from `loop_back_to_code` so the state-mutation half (cleanup + `state.stage = Code` + persist) is unit-testable without ever invoking `launch_stage`. This mattered concretely: `launch_stage` spawns the *actual* configured agent CLI (`claude -p ... --dangerously-skip-permissions` for `AgentKind::Claude`), and the real `claude` binary is on `$PATH` in this dev environment. An earlier draft of `ship_review_failed_loops_to_code` called `handle_ship_failure` end-to-end and would have raced a real `TempDir` cleanup against a real headless Claude Code invocation with `--dangerously-skip-permissions` in the background — caught during self-review before committing, not shipped.
- **`non_validate_failure_fires_gate_and_hook` asserts hook-fired + a separate pure check, not the exact env value it wrote.** `DEVFLOW_GATE_NOTIFY_CMD` is a process-global env var; under `cargo test`'s default parallelism, any other concurrently-running test that reaches `run_gate` (most of the suite's gate-related tests do) reads whatever is currently set and can fire the same sentinel command with its own `unexpected` value. The `ENV_MUTEX` in this plan only serializes env *mutation* (matching the plan's own stated scope — "so it does not race with other env-mutating tests"), not reads by unrelated tests. First implementation asserted the exact `"1"`/`"0"` content and failed intermittently under the full-suite run (confirmed: failed once with `left: "0", right: "1"` when another concurrent test's gate resolution fired the hook mid-window). Fixed by weakening the assertion to "hook ran at all" (contamination-safe) plus a pure, race-free `!state.mode.should_gate(...)` check — the exact env-value propagation is already covered, contamination-free, by `gates.rs`'s `notify_hook_sets_non_silent_flag` (calls the pure `run_notify_command` directly with explicit args, no global env involved).
- Added `tracing` as a direct dependency of the `devflow-cli` crate rather than reaching into the transitive dependency graph, per normal Cargo extern-prelude rules (a crate can only `use` its own direct dependencies).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Unsafe test design risked spawning a real agent process**
- **Found during:** Task 3 (writing `ship_review_failed_loops_to_code`)
- **Issue:** The first draft called `handle_ship_failure` end-to-end for the ReviewFailed path, which unconditionally calls `loop_back_to_code` → `launch_stage`. `launch_stage` spawns the actual configured agent CLI via `monitor::spawn_monitor` (`claude -p "<prompt>" --output-format json --dangerously-skip-permissions` for `AgentKind::Claude`). Since the real `claude` binary is present on `$PATH` in this environment, this test would have raced a background subprocess spawn against the test's own `TempDir` cleanup — a live, credentialed agent invocation with `--dangerously-skip-permissions` triggered purely as a side effect of running `cargo test`.
- **Fix:** Split `loop_back_to_code` into `prepare_loop_back_to_code` (cleanup + state mutation + prompt selection — pure, no process spawn) and the `launch_stage` call. The test now calls `prepare_loop_back_to_code` directly, proving the exact same production logic (state moves to `Code`, gate files cleaned, workflow not finished) without ever reaching `launch_stage`. Verified after the fix: `ps auxww | grep dangerously-skip-permissions` shows nothing after 5 consecutive full-suite test runs.
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** `cargo test -p devflow` green; manual `ps` check across 5 repeated full-suite runs confirms no `claude --dangerously-skip-permissions` process is ever spawned by the test suite.
- **Committed in:** `0126f20` (Task 3 commit)

**2. [Rule 1 - Bug] Flaky env-based notify assertion under parallel test execution**
- **Found during:** Task 3 (`non_validate_failure_fires_gate_and_hook`), discovered via a real intermittent failure during full-suite verification
- **Issue:** The test set `DEVFLOW_GATE_NOTIFY_CMD` to a command asserting the exact `DEVFLOW_NON_SILENT_GATE` value written by the *own* call, but `cargo test` runs tests in parallel by default and the env var is process-global — another concurrently-running test's own `run_gate`/`fire_gate_notify` call (several exist in the same test binary) could read the same var and overwrite the sentinel with its own `unexpected` value. Observed failure: `assertion left == right failed ... left: "0", right: "1"`.
- **Fix:** Weakened the assertion to "the notify hook fired at all" (sentinel file exists, regardless of writer), and added a separate, purely computational assertion (`!state.mode.should_gate(Stage::Code, state.consecutive_failures)`) that needs no I/O and cannot race. The exact env-value propagation remains covered, contamination-free, by `gates.rs`'s `notify_hook_sets_non_silent_flag` test (calls the pure `run_notify_command` with explicit args, bypassing the global-env read entirely).
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** 5 consecutive full-suite `cargo test -p devflow` runs, all green (0 failures across `devflow` unit tests + `phase7_cli` integration tests).
- **Committed in:** `0126f20` (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 1 — bugs in the test design itself, not the production code, caught during self-review before committing)
**Impact on plan:** Both fixes tightened test safety/determinism; no scope creep, no change to the production behavior specified by the plan. The plan's own Task 3 action text already anticipated needing to avoid spawning a real monitor for terminal (Abort) paths — this extends the same discipline to the LoopBack path, which the plan hadn't explicitly called out as spawn-risky.

## Issues Encountered

None beyond the two documented above (caught and fixed during this plan's own execution, not carried forward as open issues).

## User Setup Required

None - no external service configuration required. `DEVFLOW_GATE_NOTIFY_CMD` and `DEVFLOW_GATE_TIMEOUT_SECS` are optional operator-configured env vars with safe defaults (no-op notify, 7-day timeout); nothing to set up for this plan to function.

## Next Phase Readiness

- WR-11 (silent halt on non-Validate stage failure) is closed — every stage failure in the pipeline now surfaces a gate, which is a hard prerequisite for the phase's dogfood-run acceptance criterion (13e/13-06).
- Ship's `AgentFailed`/`ReviewFailed` split and the `review:`-prefix convention are in place and unit-tested; the convention's live-agent compliance is explicitly deferred to 13-06 (dogfood verification), per this plan's own threat model (T-13-04).
- `DEVFLOW_GATE_NOTIFY_CMD`/`DEVFLOW_GATE_TIMEOUT_SECS` are available for 13-04 (worktree-by-default) and 14 (observability/Hermes) to build on — the Hermes plugin's gate watcher can now also drive off `DEVFLOW_GATE_NOTIFY_CMD` if desired, though that wiring is out of scope here.
- No blockers for subsequent Wave-1/Wave-2 plans in 13-mvp-core-loop.

---
*Phase: 13-mvp-core-loop*
*Completed: 2026-07-14*

## Self-Check: PASSED

- FOUND: `.planning/phases/13-mvp-core-loop/13-01-SUMMARY.md`
- FOUND: `22ec1b7` (Task 1 commit)
- FOUND: `548711c` (Task 2 commit)
- FOUND: `0126f20` (Task 3 commit)
