# Codebase Concerns

**Analysis Date:** 2026-07-17

## Tech Debt

### Large Monolithic Files

**Main CLI entry point:**
- Issue: `crates/devflow-cli/src/main.rs` is 3,334 lines with multiple responsibilities (CLI parsing, orchestration, gate handling, hook dispatch, workflow control)
- Files: `crates/devflow-cli/src/main.rs`
- Impact: Difficult to test orchestration paths in isolation; changes risk affecting unrelated workflows
- Fix approach: Extract orchestration concerns (`advance`, `transition`, `handle_validate_outcome`, `handle_ship_outcome`) into a separate module; split gate/hook logic into standalone functions testable without CLI context

**Agent result evaluation:**
- Issue: `crates/devflow-core/src/agent_result.rs` is 1,352 lines; the three-layer completion evaluation logic plus capture-file handling, exit-code parsing, and envelope parsing for multiple agent types are tightly coupled
- Files: `crates/devflow-core/src/agent_result.rs`
- Impact: Hard to reason about the multi-layer fallback chain; adding new agents or capture formats requires changes in multiple layers simultaneously
- Fix approach: Extract each layer into its own module; create trait-based envelope parsers per agent type (Claude, Codex, OpenCode) separate from the generic marker scan

### Monolithic Config Module

**Configuration handling:**
- Issue: No formal config file (documented as a deliberate design choice in Phase 11), but Phase 16 opens this decision by introducing `devflow.toml` for Phase 16's knobs (review angles, capture retention, verification settings)
- Files: `crates/devflow-core/src/config.rs`
- Impact: Contradictory documentation; new config support will conflict with the stated "no config file" principle if not carefully integrated
- Fix approach: Phase 16 plan (D-03) introduces minimal TOML with env-var override. Update `config.rs` docstring; design the loader (env > file > default) before implementation

---

## Known Bugs

### Terminal Ship Signal Failure (16k — CRITICAL)

**Ship completion false positive:**
- Symptoms: After operator approves final Ship gate, DevFlow reports `workflow_finished` with `hook_run VersionBump ok=true` and `hook_run BranchCleanup ok=true`, but the merge never actually occurred (PR remains open, feature branch not merged to develop)
- Files: `crates/devflow-core/src/ship.rs`, `crates/devflow-cli/src/main.rs` (`handle_ship_outcome`)
- Trigger: Observable in Phase 15 dogfood run; reproducer: complete a full phase cycle, approve Ship gate, verify PR status and develop branch history
- Root cause: VersionBump hook runs BEFORE branch integration and merge; if merge fails silently, hooks report success anyway. Also unclear: whether the merge is attempted via the `/gsd-ship` agent's GSD command or via DevFlow's own git primitives; current code suggests the former, but this is not explicit
- Workaround: Manually verify PR merge status and develop branch history after Ship gate approval
- Fix approach (16k scope): Reorder Ship path to ensure merge succeeds before terminal hooks fire; add explicit post-condition verification (16a in Phase 16 plan)

### Code-Stage False Positives (No Repo Diff)

**Agent success without commits:**
- Symptoms: Code stage reports `status: success` when agent made no commits (exits 0, runs define/plan idempotent logic, or completes without touching code)
- Files: `crates/devflow-core/src/agent_result.rs` (Layer 2 gate)
- Trigger: Phases where the agent runs Define or Plan (legitimately zero-commit tasks) and is mistakenly gated on commit count; also Code-stage false positives when the agent self-reports success without producing changes (e.g. a publish/push-only plan)
- Root cause: Layer 2's commit-count gate was scoped to all stages initially; Phase 13 fix (1.2.0) scoped it to `Code` and `Plan` only, excluding `Define` and `Validate`. However, `Code` stage itself legitimately produces zero commits if the agent only performs external operations (crates.io publish, pushing tags) without touching the repo
- Workaround: Ensure Code-stage agents produce at least one repo commit (e.g. a changelog or version bump) even for external-only work
- Fix approach (16a in Phase 16 plan): Introduce Layer-0 external post-condition verification (e.g. verify crates.io publish succeeded, verify PR was created) separate from commit-count heuristic; add verification contract to stages that perform external work

### Parallel Safety Flaw (CR-03 — DESIGN CRITICAL)

**Concurrent phases are unsafe by construction:**
- Symptoms: `devflow parallel 13 14` can result in wrong-phase state evaluation, duplicate or lost agent runs, interleaved version-bump commits
- Files: `crates/devflow-core/src/workflow.rs` (per-phase state), `crates/devflow-cli/src/main.rs` (`parallel` loop), `crates/devflow-core/src/monitor.rs` (advance invocation)
- Trigger: Running `devflow parallel` with 2+ phases; documented in Phase 13 post-review (13-DEFERRED-CR-03.md)
- Root cause: Phase-scoped locks (per-phase lock files `.devflow/lock-NN`) were introduced in Phase 13 to prevent one phase blocking at a multi-day gate from starving siblings. However, the resources those locks guard are still project-global:
  1. `.devflow/state.json` is a single file — the second `start` overwrites the first's state; each phase's monitor loads whichever phase was started *last*
  2. Main checkout git operations (version-bump commits/tags, branch cleanup) run unserialized when two phases finish concurrently
- Workaround: Run phases sequentially with `devflow start`, or use `devflow sequentagent` (which has its own issues — see below)
- Fix approach (Phase 14, already shipped): Per-phase state files `state-NN.json`, phase-threaded `devflow advance --phase N`, and a short project-wide checkout lock for git mutations. Acceptance criteria: two phases via `devflow parallel` each run independently without state clobbering; concurrent `finish_workflow`s serialize on the coarse lock

### Sequentagent's Unguarded State

**Rate-limit cron instructions are project-global:**
- Symptoms: Running `devflow sequentagent` with rate-limited agents can produce a stale `.devflow/cron-instructions-NN.json` that interferes with future runs of other phases
- Files: `crates/devflow-core/src/monitor.rs`, rate-limit detection in agent-result parsing
- Impact: Not documented as a test case; identified as a residual issue in CR-03 scope (Phase 14)
- Fix approach (planned Phase 14 but deferred): Adopt the per-phase state model for `cron-instructions-NN.json` as well; ensure it does not persist across phase boundaries

---

## Security Considerations

### Shell Command Injection (Mitigated)

**Notify hook injection risk:**
- Risk: `DEVFLOW_GATE_NOTIFY_CMD` is run via `sh -c` with gate metadata in environment variables. If the command string were interpolated (e.g. `sh -c "echo $DEVFLOW_GATE_PHASE"`) instead of passed via env, shell injection would be possible
- Files: `crates/devflow-core/src/gates.rs` (`fire_gate_notify`)
- Current mitigation: The command is invoked via `sh -c "command"` with env vars passed separately, never interpolated into the command string — the only way an attacker could inject is by controlling `DEVFLOW_GATE_NOTIFY_CMD` itself
- Recommendations: Document this pattern clearly; consider using `shlex` or similar to validate the command at startup if needed (Phase 15 docs already covered this)

### State File Exposure

**Risk:** `.devflow/state-NN.json` and `.devflow/events.jsonl` contain phase numbers, stage names, and agent exit status; if exposed to untrusted contexts, they reveal workflow internals
- Files: `crates/devflow-core/src/workflow.rs`, `crates/devflow-core/src/events.rs`
- Current mitigation: Marked in `.gitignore`; SECURITY.md advises not exposing these files
- Recommendations: No secrets are persisted in these files (all auth is via agent CLI), so exposure risk is low; document as operator guidance in OPERATIONS.md

### Agent Sandbox Scoping (Mitigated)

**Risk:** Agent sandboxes (Codex) need access to `.git` metadata and worktree admin directories without exposing the operator's signing/auth infrastructure
- Files: `crates/devflow-core/src/agents/codex.rs` (extra_writable_roots)
- Current mitigation (Phase 13): Codex agent gets explicit sandbox grants for worktree's git admin (`.git/worktrees/<name>`); commit/tag signing is disabled via `GIT_CONFIG_*` env scoped to the Codex process tree
- Status: Shipped in 1.2.0; no reported vulnerabilities

---

## Performance Bottlenecks

### Monitor Polling + Gate Blocking

**Slow gate response detection:**
- Problem: `Gates::poll_response()` polls the gate response file with exponential backoff (1s → 2s → … 60s), which means a human response can be delayed up to 60 seconds before DevFlow checks it
- Files: `crates/devflow-core/src/gates.rs` (`poll_response`)
- Current capacity: 60-second max backoff is acceptable for multi-hour gate waits (default timeout 7 days), but noticeable in interactive usage
- Scaling path: If gates become frequent (e.g. `--mode supervise` gates at every Validate), consider a notify/watch pattern or pushing gate decisions via a webhook instead of polling

### Capture File Accumulation

**No history retention by design:**
- Problem: Each stage launch overwrites `.devflow/phase-NN-stdout` and `.devflow/phase-NN-stderr.log`; multi-stage phases have only the final stage's captured output visible
- Files: `crates/devflow-core/src/agent_result.rs` (stdout_path/stderr_path)
- Impact: Debugging multi-loop phases (Code → Validate → Code → Validate) requires manual log tailing during execution; post-mortem debugging is impossible
- Scaling path (16b in Phase 16 plan): Retain per-stage capture history (e.g. `phase-NN-code-attempt-1-stdout`, `phase-NN-code-attempt-2-stdout`) instead of clobbering; add a retention policy (e.g. keep last N attempts or last 7 days)

---

## Fragile Areas

### Agent Result Evaluation Layer Ordering

**Files:** `crates/devflow-core/src/agent_result.rs` (`evaluate_agent_result`)
- Why fragile: Three-layer fallback chain (DEVFLOW_RESULT marker → exit code + commit count → process gone + commits heuristic) is tight coupling; small changes to one layer's assumptions can break another's fallback guarantee
- Example: Layer 2's commit-count gate initially applied to all stages; the "fix" that scoped it to Plan/Code only created a silent success case where Validate with zero commits proceeds instead of failing (T-13-14 in the code comments)
- Safe modification: Before changing any layer's logic, trace through all three paths for all stage types (Define, Plan, Code, Validate, Ship); add tests for the fallback path (Layer 3 heuristic) for each stage
- Test coverage: Unit tests cover Layer 1 (marker parsing) and Layer 2 (exit code), but Layer 3 (heuristic) and multi-stage combinations are under-tested

### Gate Write and Response TOCTOU

**Files:** `crates/devflow-core/src/gates.rs` (write/poll/respond flow)
- Why fragile: Gate request is written atomically, but there's a window between write and poll where a stale response from a previous gate (same phase, different stage) could be picked up
- Current safeguard: Gate files are stage-scoped (`.devflow/gates/NN-{stage}.json`), so cross-stage confusion is prevented; response format includes stage name for double-checking
- Safe modification: Add integration tests for concurrent gate firing (two gates at different stages); verify that old responses are never misattributed

### Worktree Cleanup on Crash

**Files:** `crates/devflow-core/src/worktree.rs`, `crates/devflow-cli/src/main.rs` (cleanup command)
- Why fragile: If the agent crashes hard (SIGKILL), the worktree cleanup hooks may not run; abandoned worktrees accumulate under `.worktrees/` and can interfere with subsequent `devflow start --force`
- Current safeguard: `devflow cleanup` and `devflow recover --clean` can remove stale worktrees; `devflow start --force` overwrites existing worktrees
- Safe modification: When `devflow start` detects an existing worktree for the same phase, verify it's associated with a live process before auto-cleaning; add a safety prompt or require `--force` to clean unknown stale worktrees
- Test coverage: E2E test for crash-recovery path (kill agent mid-run, verify `devflow status` still works, verify `devflow cleanup` removes the orphan worktree)

---

## Scaling Limits

### Per-Phase Lock Holding Across Gate Wait

**Resource:** Per-phase lock file `.devflow/lock-NN`
- Current capacity: Locked for the entire gate wait (default 7 days if gate is not answered)
- Limit: If a lock holder crashes and does not clean up, its stale lock wedges all future `devflow advance --phase N` calls for that phase. Mitigated (Phase 13, shipped) by stale-lock reclaim logic (`pid_is_alive` check), but still requires the stale process to be detectable
- Scaling path: Consider a TTL-based lock file (e.g. a lock is considered stale if older than N days regardless of process liveness); add a TTL parameter to `acquire()` and raise a warning if a lock is near expiry during gate polling

### Concurrent Phase State Enumeration

**Resource:** Enumerating active phases in `devflow status`/`devflow recover` requires scanning all state files
- Current capacity: `workflow::last_events_by_phase()` does one pass over `.devflow/events.jsonl`; state-file scan is O(phase_count)
- Limit: With many phases (e.g. 50+ concurrent phases in `devflow parallel`), file I/O for enumeration becomes noticeable
- Scaling path: Cache the phase list in memory per DevFlow invocation; consider an index file (e.g. `.devflow/.active_phases`) for faster enumeration (with atomic update discipline to avoid corruption)

### Checkpoint File Fragmentation

**Resource:** One file per artifact type, phase, stage
- Current: `.devflow/phase-NN-stdout`, `.devflow/phase-NN-stderr.log`, `.devflow/phase-NN-exit`, `.devflow/phase-NN-agent-pid`, `.devflow/state-NN.json`, `.devflow/lock-NN`, `.devflow/cron-instructions-NN.json` (at least 7 files per phase)
- Under Phase 16's capture history (16b), each phase could have 2-3 attempts per stage × 5 stages = 10-15 capture files, multiplied by phase count
- Scaling path: Consider a per-phase directory (`.devflow/phases/NN/`) to group related files; update `.gitignore` accordingly; refactor path helpers in `agent_result.rs` and `workflow.rs`

---

## Dependencies at Risk

### Tracing Ecosystem (Unlinked from Tests)

**Risk:** DevFlow uses `tracing` for structured logging, but test harness does not initialize a subscriber by default
- Files: All logging via `tracing::info!`, `warn!`, `debug!`; `crates/devflow-cli/tests/` do not initialize `tracing_subscriber`
- Impact: Test output is silent; if a test needs to debug log output, the developer must manually enable `RUST_LOG=debug` and run with `nocapture`, or add a tracing init to the test itself
- Migration plan: Add a test helper that initializes `tracing_subscriber` at the start of integration tests; document the pattern in CONTRIBUTING.md

### Serde JSON Round-Trip Leniency

**Risk:** `agent_result.rs` deserializes the `verdict` field leniently to avoid silently dropping valid fields if the verdict is malformed. However, other JSON deserialization in the codebase (state.rs, gates.rs) does not use lenient patterns
- Files: `crates/devflow-core/src/agent_result.rs` (`deserialize_verdict_lenient`); `crates/devflow-core/src/state.rs` (standard serde), `crates/devflow-core/src/gates.rs` (standard serde)
- Impact: A malformed state.json or gate response file will fail to parse and abort the operation, while a malformed DEVFLOW_RESULT verdict falls through to Layer 2 silently
- Migration plan: Document this asymmetry in the code; add a compatibility test that shows round-trip of edge-case JSON (unknown fields, wrong types, missing required fields) for all major types

---

## Missing Critical Features

### Merge Integration Verification

**Problem:** The Ship stage runs `/gsd-ship {phase}` (which is expected to create a PR and merge it), but DevFlow does not verify the merge actually succeeded before reporting `workflow_finished`
- Blocks: Full end-to-end automation of Ship; operator cannot trust the final gate approval
- Files: `crates/devflow-cli/src/main.rs` (`handle_ship_outcome`), `crates/devflow-core/src/ship.rs`
- Priority: Critical — identified as 16k in Phase 16 scope (external post-condition verification)
- Planned fix: Add a post-condition check after `/gsd-ship` that verifies the feature branch is an ancestor of develop (or main, depending on git flow config)

### Incremental Review for Long Phases

**Problem:** Ship's code review runs once at the end of the phase. If the phase has looped back multiple times (Code → Validate → Code → Ship), earlier Code stages are not re-reviewed
- Blocks: Catching bugs introduced in intermediate Code stages before final Ship approval; current model gates only on the final Code stage's review
- Files: `crates/devflow-core/src/prompt.rs` (Ship prompt generation), `crates/devflow-cli/src/main.rs` (no review orchestration between stages)
- Priority: Medium — identified as 16e in Phase 16 scope (incremental per-wave review)
- Planned fix: Add an optional Code-stage review (agent runs `/gsd-code-review` and adds findings to a running list); aggregate all reviews at Ship time

### Persistent Gate Notification

**Problem:** When a gate fires, a notify hook is run (if `DEVFLOW_GATE_NOTIFY_CMD` is set), but there's no persistent indicator in `devflow status` or the terminal that a gate is pending
- Blocks: Operator can miss a gate response (reported as 16j in Phase 16 scope)
- Files: `crates/devflow-cli/src/main.rs` (`run_gate`), `crates/devflow-core/src/gates.rs` (gate firing)
- Priority: High — observed as a gate-notification gap in Phase 15 dogfood
- Planned fix: Add a persistent banner to `devflow status` that lists all pending gates; consider a TUI indicator or a persistent background process that polls and alerts

---

## Test Coverage Gaps

### Cross-Phase Parallelism Integration Tests

**Untested area:** The CR-03 fix (per-phase state files, phase-threaded advance) was shipped in Phase 14, but the integration test for concurrent phases is minimal
- Files: `crates/devflow-cli/tests/phase7_cli.rs` has a dogfood test, but not a synthetic concurrent-phase test
- What's missing: Two fake agents running concurrently with interleaved exits, verifying that each phase's state machine advances independently and events.jsonl logs both phases correctly
- Risk: A subtle TOCTOU bug in state load/save under `devflow parallel` could go undetected until dogfooding
- Priority: High — parallelism is a core feature; the flaw (CR-03) was caught by code review, not tests
- Test approach: Minimal live test with two fake agents (shell scripts that exit cleanly), `devflow parallel`, and a post-run assertion on both phases' final state files

### Agent Envelope Parsing for Each Adapter

**Untested area:** Native envelope parsing (Layer 1) for Claude and Codex was added in Phase 13, but only the Claude envelope is tested in unit tests
- Files: `crates/devflow-core/src/agents/claude.rs`, `crates/devflow-core/src/agents/codex.rs`; agent-result parsing in `agent_result.rs`
- What's missing: End-to-end test for Codex JSONL envelope parsing (specifically, the `agent_message` item containing DEVFLOW_RESULT); OpenCode envelope parsing
- Risk: Codex envelope parsing could regress without notice; a false positive in Phase 15 dogfood (commit-count heuristic) was traced to Codex envelope parsing bugs
- Priority: High — Codex is a supported agent; false positives are critical
- Test approach: Add snapshot tests for expected Codex/OpenCode output samples (envelope format + expected parsed result) to `agent_result.rs`

### Gate TOCTOU Under Concurrent Fires

**Untested area:** Multiple gates firing at different stages concurrently (e.g. phase A fires Validate gate, phase B fires Ship gate at the same time)
- Files: `crates/devflow-core/src/gates.rs` (write/poll/respond flow)
- What's missing: Synthetic test with two phases firing gates simultaneously; verify response files are not cross-contaminated
- Risk: A gate response intended for one stage could be misrouted to another stage if response file lookup is not bulletproof
- Priority: Medium
- Test approach: Mock the filesystem; spawn two threads that fire gates concurrently; verify each thread reads its own response

### Stale Lock Reclaim Under Load

**Untested area:** The stale-lock reclaim logic (Phase 13) assumes `pid_is_alive` check is fast; under high process churn, the PID reuse window could allow a stale lock to be reclaimed by a different unrelated process
- Files: `crates/devflow-core/src/lock.rs` (stale-holder recovery)
- What's missing: Stress test with many phases and frequent crashes; verify that reclaim does not accidentally give a lock to the wrong holder
- Risk: Very low in practice (PID reuse is rare on modern systems), but theoretically possible
- Priority: Low
- Test approach: Document the PID-reuse assumption and the window size; add a comment linking to the theory (PID reuse windows in Linux are typically hours)

---

## Code Quality

### Unused or Minimal-Use Functions

**Issue:** A few functions are defined but have zero or one call site
- Examples: `GitFlow::release_start`, `GitFlow::release_finish` are defined but never called from production code (only exercised in tests)
- Files: `crates/devflow-core/src/git.rs` (release functions)
- Impact: Dead code adds to maintenance burden; unclear whether these are intentional stubs for future use or forgotten old APIs
- Recommendation: If not planned for use in Phase 16/17, mark as `#[deprecated]` or remove with a comment explaining why they were removed (e.g. "Release branching was deferred to Phase X")

### Inconsistent Error Handling Patterns

**Issue:** Some modules use `?` for error propagation; others use `match` or `.map_err`
- Files: Varies across `git.rs`, `worktree.rs`, `agent.rs`, `hooks.rs`
- Impact: No runtime bug, but inconsistency makes code harder to scan
- Recommendation: Enforce `?` propagation as the default pattern in CONTRIBUTING.md; use `match` only when error handling logic is non-trivial

### Large Match Statements Without Exhaustiveness Guards

**Issue:** `main.rs` has large match statements for stage handling, agent kinds, and commands that could become unmaintainable as new variants are added
- Files: `crates/devflow-cli/src/main.rs` (match on Stage, AgentKind, Command variants)
- Impact: If a new stage or agent is added, forgetting to handle it in a match statement could cause panics at runtime
- Recommendation: Use `#[non_exhaustive]` on public enums to force recompilation on variant changes; add a lint rule or CI check that catches unreachable patterns

---

## Architectural Anti-Patterns

### No Validation Layer Between CLI and Core

**What happens:** The CLI directly constructs `State`, `Mode`, and other core types from clap arguments without validation
- Files: `crates/devflow-cli/src/main.rs` (clap parsing → immediate State construction)
- Why it's wrong: If a clap argument parsing rule changes, the core layer doesn't know and could receive invalid inputs; no single place to verify that a combination of flags is valid
- Do this instead: Create a `validate_start_args()` function in `devflow-core` that takes the parsed arguments and returns an error if they're inconsistent (e.g. `--phase 0` is invalid, `--mode supervise` with `--no-worktree` has no practical benefit)

### State Loaded Outside the Advance Lock

**What happens:** Some code paths load state, check a condition, then acquire the lock — creating a window where state changes
- Files: `crates/devflow-core/src/workflow.rs`, `crates/devflow-cli/src/main.rs` (advance path)
- Why it's wrong: Phase 13's fix (re-load state under the phase lock) closed the double-advance TOCTOU, but other state reads are still vulnerable
- Do this instead: Restructure to acquire lock first, then load state; if the load is expensive, cache the result (with a comment explaining the cache is valid for the lock's duration)

### No Clear Separation Between Reading and Writing State

**What happens:** `workflow.rs` has `load_state`, `save_state`, and `clear_state`, but no invariants about when each is called or what state is valid before/after
- Files: `crates/devflow-core/src/workflow.rs`, `crates/devflow-core/src/state.rs`
- Why it's wrong: Easy to accidentally clear state before saving, or load state after it's been cleared; no transaction semantics
- Do this instead: Define a clear protocol (e.g. "state is loaded at start of `advance`, saved at end, cleared only on `finish_workflow`"); document it prominently in state.rs; consider a state-machine type (e.g. `StateHandle { phase, guard }`) that enforces lock/load/save/release in order

---

## Documentation vs. Implementation Gaps

### `.devflow.yaml` Decoy Removed, But Config Comment Still Says "No Config"

**Issue:** Phase 11's "no config file" stance is documented in code comments (`config.rs`), but Phase 16 deliberately opens this decision to add `devflow.toml`
- Files: `crates/devflow-core/src/config.rs` (doc comment claiming no config)
- Impact: Confusing for future maintainers; the docstring contradicts Phase 16's planned changes
- Fix approach: Update the docstring to: "Phase 11–15 used no config file, relying on CLI flags and env vars. Phase 16 introduces a minimal `devflow.toml` for review angles and capture retention settings; env vars override file settings."

### `ARCHITECTURE.md` Claims Idempotent Define/Plan, But Docs Don't Explain the Fallback

**Issue:** ARCHITECTURE.md says "if the stage's deliverable already exists, the agent reports success without re-running the GSD command" but doesn't explain what happens if the deliverable is malformed or incomplete
- Files: `ARCHITECTURE.md` (section on Define/Plan stages), `crates/devflow-core/src/prompt.rs` (actual idempotent logic)
- Impact: Operator confusion if a stale CONTEXT.md blocks a phase from being re-planned
- Fix approach: Add a note to ARCHITECTURE.md: "If the deliverable exists but is incomplete, the agent must be re-run (e.g. `devflow start --phase N --force`). Define/Plan idempotence is best-effort — a corrupt or partial file blocks re-runs."

### `OPERATIONS.md` Doesn't Mention Worktree Awareness Bug (16f)

**Issue:** `devflow gate approve 15` run from inside the worktree fails with "no project root found" because DevFlow defaults `--project` to `.` and doesn't walk up to find the primary checkout
- Files: `OPERATIONS.md` (doesn't mention this limitation), `crates/devflow-cli/src/main.rs` (no walk-up resolver)
- Impact: Operator must remember to run gate commands from the main checkout, not the worktree — surprise failure in automated workflows
- Fix approach (16f in Phase 16 scope): Implement a shared `resolve_project_root()` walk-up function; use it in all subcommands (status, gate, logs, recover). Document the change in OPERATIONS.md.

---

## Deferred Phase Issues

### Phase 16 Scope (Inserted 2026-07-17)

**Current:** Phase 16 is planned to address 11 reliability items (16a–16k, with 16k being critical). All are deferred to Phase 16 from Phase 15 dogfood findings.

**16a – External Post-Condition Verification:** No verification that crates.io publish, PR creation, or merge actually succeeded; relies on agent self-report (Layer 1) or heuristics (Layer 2/3)

**16b – Retained Capture History:** Current design wipes capture files on each stage launch; multi-loop phases have only the final stage's output visible

**16c – Deterministic Doc Claim Checker:** Claims in operator docs (env vars, CLI flags, defaults) must be verified against source to catch drift

**16d/16e – Ship Review Pipeline:** Current Ship prompt runs a single `/gsd-code-review` pass; Phase 15 dogfood had four Ship-stage loop-backs due to serial finding discovery. Phase 16 adds parallel/focused angles + incremental per-wave review

**16f/16g – Project Root Walk-Up + Gate CLI UX:** `devflow gate approve 15` fails when run from the worktree; Gate CLI has a positional-arg footgun (trailing project path swallows `--stage` value)

**16h – Cross-Attempt History View:** No visible log of which Code/Ship stages looped back and why; operator has no persistent context

**16i – `.gitignore` Invariant Checker:** Every `.devflow/`-writing path must be covered by `.gitignore`; no verification today

**16j – Persistent Gate Notification:** Gate fires, notify hook runs, but no persistent indicator that the gate is pending (16j was promoted from deferred list)

**16k – Terminal Ship Path Forensics + Fix:** VersionBump runs before merge; if merge fails, hooks still report success. Gate-approval advance path unclear. Merge hook missing from terminal Ship flow.

---

*Concerns audit: 2026-07-17*
