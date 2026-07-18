---
phase: 13-mvp-core-loop
fixed_at: 2026-07-16T00:04:35Z
review_path: .planning/phases/13-mvp-core-loop/13-REVIEW.md
iteration: 1
findings_in_scope: 15
fixed: 15
skipped: 0
status: all_fixed
---

# Phase 13: Code Review Fix Report

**Fixed at:** 2026-07-16T00:04:35Z
**Source review:** .planning/phases/13-mvp-core-loop/13-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 15 (3 critical, 12 warning — `fix_scope: critical_warning`, so the 3 Info findings IN-01/IN-02/IN-03 were intentionally left untouched)
- Fixed: 15
- Skipped: 0

All fixes were verified with `cargo check`/`cargo build` (Tier 2 syntax/type check) and the full `cargo test --workspace` suite (217 tests) after every commit. For each finding with an observable behavioral effect, a new regression test was added and its fail/pass state was manually verified against both the pre-fix and post-fix code before committing (temporarily reverting the fix, confirming the new test fails for the expected reason, then restoring the fix and confirming it passes) — this is stronger than the mandated 3-tier verification and is called out per-finding below.

## Fixed Issues

### CR-01: Agent stdout capture silently discards all output on invalid UTF-8

**Files modified:** `crates/devflow-core/src/agent.rs`
**Commit:** d0768d2
**Applied fix:** `capture_agent_output` now reads the agent's stdout pipe as raw bytes (`read_to_end`) and converts with `String::from_utf8_lossy` instead of `read_to_string`, which discarded the entire buffer (including a valid `DEVFLOW_RESULT` marker) on the first invalid UTF-8 byte. Applied exactly as suggested in the review.

### CR-02: `sequentagent` never parses a real Codex agent's self-reported result

**Files modified:** `crates/devflow-cli/src/main.rs`
**Commit:** 92b2877
**Applied fix:** Adapted the suggested fix — rather than duplicating `evaluate_layer1`'s precedence chain inline in `run_agent_blocking`, delegated to `agent_result::evaluate_layer1(project_root, phase)` directly. `capture_agent_output` already writes stdout to the exact file `evaluate_layer1` reads, so this is the "share one code path" alternative the review's Fix section flagged as "better still" — it also picks up Claude's `is_error` envelope check for free, not just the Codex JSONL path. Verified against the existing `phase7_cli.rs` integration suite (all 9/9 passed, then 10/10 after WR-10 added a test).

### CR-03: Project-wide lock held across multi-day gate waits breaks `devflow parallel`

**Files modified:** `crates/devflow-core/src/lock.rs`, `crates/devflow-cli/src/main.rs`, `crates/devflow-core/src/recover.rs`
**Commit:** 962e931
**Applied fix:** Scoped the lock to `.devflow/lock-{phase:02}` instead of a single project-wide `.devflow/lock`, per the review's suggested fix. `lock::acquire`/`lock::holder`/`lock_path` now take an explicit `phase: u32`. `advance()` loads state (a plain read) before acquiring the lock so it can key the lock on `state.phase`. `recover::clean()` now sweeps all `lock-*` files under `.devflow/` (an operator-driven broad reset, not scoped to one phase) instead of removing a single hardcoded path. Added a new lock.rs test (`different_phases_do_not_contend`) confirming two phases' locks no longer collide. All existing lock/recover tests updated to pass a phase and pass unchanged otherwise.

### WR-01: `doctor`'s `cmd_check` reports a failed command as "ok"

**Files modified:** `crates/devflow-cli/src/main.rs`
**Commit:** b49ffea
**Applied fix:** Applied exactly as suggested — the `Ok(out)` arm for a non-zero exit now reports `status: "warn"` with an `install_hint` naming the failing command, instead of hardcoding `"ok"`.

### WR-02: Stale `cron-instructions.json` survives a successful post-rate-limit `sequentagent` run

**Files modified:** `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/tests/phase7_cli.rs`
**Commit:** 3d88d7b
**Applied fix:** Applied exactly as suggested — `sequentagent` now calls `devflow_core::ship::delete_cron_instructions(project_root)` right before printing "sequentagent complete". The existing integration test `sequentagent_hands_off_after_rate_limit_and_writes_cron_instructions` asserted the cron file **survived** a successful run (i.e. it encoded the bug as expected behavior); updated its final assertion to check the file is written during the run (via the "wrote .devflow/cron-instructions.json" stdout line) and then correctly deleted once agent B completes.

### WR-03: `cleanup_merged` doesn't strip the `+` worktree prefix from `git branch --merged`

**Files modified:** `crates/devflow-core/src/git.rs`
**Commit:** efdeb60
**Applied fix:** Applied exactly as suggested — `trim_start_matches('*')` → `trim_start_matches(['*', '+'])`. Added `cleanup_merged_strips_worktree_plus_prefix_from_ref_name`, which checks a merged branch out in a real linked worktree and confirms the resulting git error names the branch cleanly (`"used by worktree"`) rather than a corrupted `"+ branch-name"` ref. Manually verified this test fails (with the pre-fix "invalid ref" error) when the fix is reverted, and passes with it restored.

### WR-04: `cleanup_merged` computes "merged" relative to the main checkout's current HEAD, not explicitly `develop`

**Files modified:** `crates/devflow-core/src/git.rs`
**Commit:** b771358
**Applied fix:** Applied exactly as suggested — `git_output(["branch", "--merged"])` → `git_output(["branch", "--merged", &self.config.develop])`. Added `cleanup_merged_is_relative_to_develop_not_current_head`, which leaves the main checkout on a divergent branch (`topic`) and constructs a branch (`premature`) merged into `topic` but not `develop`; confirmed the test fails (wrongly deletes `premature`) when the fix is reverted, and passes (branch survives) with it restored.

### WR-05: `agent_running(0)` can report a dead/corrupt PID as running

**Files modified:** `crates/devflow-core/src/agent.rs`
**Commit:** 5364fc0
**Applied fix:** Applied exactly as suggested — `agent_running` now short-circuits `pid != 0 && ...` before calling `libc::kill`.

### WR-06: `evaluate_layer2` mixes two different `project_root` sources within one function

**Files modified:** `crates/devflow-core/src/agent_result.rs`
**Commit:** df7caf3
**Applied fix:** Went slightly further than the literal suggestion: rather than just swapping `state.project_root` for the `project_root` parameter in the two git subprocess calls, removed the now-fully-unused `state: &State` parameter from `evaluate_layer2`'s signature entirely (it had no other use), eliminating the "latent trap" the review described rather than just papering over it. Updated the one production call site (`evaluate_agent_result`) and all 6 test call sites accordingly, including removing two now-vestigial `let mut state = state_in(...)` test bindings whose only purpose was feeding the removed parameter.

### WR-07: `parse_offset_minutes` doesn't validate the offset format, risking a badly wrong cron schedule

**Files modified:** `crates/devflow-core/src/ship.rs`
**Commit:** 5e1105d
**Applied fix:** Applied exactly as suggested — requires a colon via `split_once(':')` and bound-checks `hours` (0..=23) and `minutes` (0..=59), returning `None` (which `split_time_and_offset` already treats as UTC+0, fail-safe) instead of misparsing e.g. `+0530` as 530 hours. Added `cron_schedule_rejects_non_colon_offset_instead_of_misparsing_it`, comparing the non-colon-offset schedule against the same wall-clock time with no offset at all. Manually verified this test fails with a wildly wrong schedule (`"46 13 27 5 *"` vs. the expected `"46 15 18 6 *"`) when the fix is reverted.

### WR-08: Monitor's TERM/INT trap exits without terminating the orphaned agent, permanently stalling auto-advance

**Files modified:** `crates/devflow-core/src/monitor.rs`
**Commit:** d65c7ba
**Applied fix:** Applied as suggested — `cleanup()` now kills `$apid` before exiting, with `apid` initialized to empty before the trap is installed (guards a signal arriving before the agent is backgrounded). Added `sigterm_to_monitor_also_kills_the_agent`, which spawns a monitor running a real `sleep 30` agent, sends SIGTERM to the monitor PID, and polls for the agent process to actually die. Manually verified this test fails (agent stays alive) when the fix is reverted — cleaned up the resulting orphaned `sleep 30` process before restoring the fix and re-running the suite.

### WR-09: `deserialize_verdict_lenient` only tolerates malformed string *values*, not wrong JSON *types*

**Files modified:** `crates/devflow-core/src/agent_result.rs`
**Commit:** e62d9bf
**Applied fix:** Applied exactly as suggested — decodes as `Option<serde_json::Value>` first, then only pattern-matches the string case via `.as_str()`, so a non-string `verdict` (bool/number/object) falls through to `None` instead of erroring the whole `AgentResult` parse. Added `parse_devflow_result_non_string_verdict_type_is_none_not_parse_error` covering bool, numeric, and object verdict values. Manually verified the test fails (the whole marker parse returns `None`, not just the verdict) when the fix is reverted.

### WR-10: `start`'s pre-flight divergence check inspects the wrong branch for the (default) worktree path

**Files modified:** `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/tests/phase7_cli.rs`
**Commit:** 2cc6245
**Applied fix:** Took the first of the two suggested options — gated the divergence check on `!worktree`, so it only runs for the `--no-worktree` (branch-in-place) flow where it's meaningful; worktree mode (the default) always forks fresh from `develop` regardless of the main checkout's HEAD. Added `start_worktree_mode_ignores_main_checkout_divergence`, which leaves the main checkout on a branch 60 commits behind `develop` (past the hard-fail threshold) and confirms `start` still succeeds in worktree mode. Manually verified this test fails with the exact "develop is 60 commits ahead" error when the fix is reverted.

### WR-11: `start` persists in-progress state before the agent launch is confirmed to succeed

**Files modified:** `crates/devflow-cli/src/main.rs`
**Commit:** 144422f
**Applied fix:** Took the first of the two suggested options — swapped the order so `launch_stage(&state, None)?` runs before `workflow::save_state(&state)?`, rather than adding a rollback path. Traced `launch_stage`/`monitor::spawn_monitor` to confirm neither reads `state.json` from disk (they operate purely on the in-memory `&State`; only the monitor's *later*, asynchronous `devflow advance` call reads it, well after `start` returns), so the reorder is safe. No new regression test: reliably triggering a `launch_stage` failure through the black-box CLI harness (missing agent binary → still spawns `sh` successfully and fails only inside the backgrounded job, not synchronously; missing `sh` on `PATH` → also breaks `git`, which the worktree-creation path needs first) proved impractical without a deeper harness change; verified instead via full-suite regression (217/217 pass) plus the code trace above.

### WR-12: Unbounded recursion in JSON traversal helpers used on agent-controlled stdout

**Files modified:** `crates/devflow-core/src/agent_result.rs`
**Commit:** c2665c3
**Applied fix:** Took the "cap recursion depth" option (rather than a full iterative rewrite) — added a `MAX_JSON_TRAVERSAL_DEPTH = 64` constant and threaded a `depth` counter through `json_has_str`/`json_has_i64`/`json_find_key` via internal `_at_depth` variants, returning "no match" once the cap is hit, without changing the public call-site signatures. Added `detect_rate_limit_does_not_stack_overflow_on_deeply_nested_json` at 100 levels of nesting (chosen to exceed the 64-level cap while staying under serde_json's own ~100–1000 parse-time recursion limit, isolating the traversal fix as the thing under test). Manually verified the test fails (`Some("deep")` instead of `None`) when the fix is reverted, confirming unbounded recursion really did reach and misuse a marker buried past the intended cap.

## Skipped Issues

None — all 15 in-scope findings were fixed.

---

_Fixed: 2026-07-16T00:04:35Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
