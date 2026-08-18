> invocation: antigravity --model gemini-3.7-flash-high --print-timeout 15m --dangerously-skip-permissions -p "<prompt>" (MCP linear/vercel disabled during run, restored after)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-38 (feature/phase-38)

# Adversarial Review: Phase 38 Context & Planning Decisions

## Needs My Decision

### 1. Reconcile the Stdin Wire Protocol & Stream Transport Architecture (D-02)
* **The Problem:** Routing Pi through `MonitorLaunch::PipeOwning` as specified in D-02 causes an immediate **deadlock and guaranteed SIGKILL**:
  1. [`run_pipe_owning_monitor`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/monitor.rs#L820-L835) writes [`user_turn_line(prompt)`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/monitor.rs#L726-L732) (`{"type":"user","message":{"role":"user","content":...}}`) to stdin and holds stdin open. Verified via live execution: `pi --mode json` does not understand Claude's `stream-json` wire protocol and treats the JSON string as raw prompt text.
  2. `pi` running in JSON mode with an open stdin pipe **blocks on stdin and never exits** (verified live: `pi -p --mode json` with an open pipe stalls indefinitely, whereas with `DEVNULL` it completes in ~6s).
  3. [`CloseRule::observe`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/monitor.rs#L613-L638) waits for [`event_is_top_level_result_marker`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/agent_result.rs#L1188-L1196) (which requires `event["type"] == "result"`). Pi never emits `type: "result"`, so `CloseRule::should_close()` never fires, stdin is never closed, Pi never exits, and the monitor kills Pi with an [`IdleTimeout`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/monitor.rs#L1033-L1125) after 120s.
  4. There is no "event unwrapper" abstraction in `monitor.rs` — the reader thread pipes raw stdout directly to disk and `CloseRule`.
* **Options:**
  1. *(Recommended)* **Keep Pi on `-p` with `MonitorLaunch::Legacy` for process lifecycle**, but add a Pi JSONL completion parser in `agent_result.rs` (analogous to [`parse_codex_event_result`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/agent_result.rs#L740-L800)).
  2. **Refactor `MonitorLaunch::PipeOwning` to support driver-pluggable stdin serializers, close detectors, and unwrappers** so Pi closes stdin immediately or translates `turn_end` / `agent_end` into the close signal.
* **Open Decision:** Which transport model is authorized for Phase 38?

---

### 2. Define/Plan Stage Interactivity vs Full Parity Scope (D-01 vs D-04)
* **The Problem:** D-01 promises "Full Claude parity: Plan → Code → Validate → Ship (Define is a no-op)", while D-04 sets `interactivity_mode` for Define and Plan to `RequiresExistingArtifact`.
  - In [`crates/devflow-cli/src/commands.rs:289-303`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/commands.rs#L289-L303) and [`crates/devflow-cli/src/preflight.rs:613-625`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/preflight.rs#L613-L625), `RequiresExistingArtifact` means `devflow start --agent pi` will hard-refuse if `CONTEXT.md` is not already on `develop`.
  - Furthermore, if `Plan` also requires `RequiresExistingArtifact` (`-PLAN.md`), then running `Stage::Plan` in Auto mode either fails preflight (if `-PLAN.md` is absent) or is an idempotent no-op (if `-PLAN.md` is present). Pi will never generate a plan headlessly.
* **Options:**
  1. *(Recommended)* **Limit `RequiresExistingArtifact` strictly to `Stage::Define` for Pi**, allowing `Stage::Plan` to run headlessly as `HeadlessSafe` if Pi's non-interactive prompt supports plan generation.
  2. **Accept that Pi cannot plan headlessly**, mirror Codex completely (Define & Plan `RequiresExistingArtifact`), and explicitly downgrade D-01 from "Full Claude parity" to "Code → Validate → Ship only (Define & Plan require pre-authored artifacts)".
* **Open Decision:** Is Pi's `Stage::Plan` `HeadlessSafe` or `RequiresExistingArtifact`?

---

## Needs My Awareness

### 1. `canary_gate` Collateral Execution of Claude Code
* **Exact Site:** [`crates/devflow-cli/src/pipeline_launch.rs:143-150`](file:///home/devflow/.worktrees/phase-38/crates/devflow-cli/src/pipeline_launch.rs#L143-L150) & [`crates/devflow-cli/src/pipeline_launch.rs:707-713`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/pipeline_launch.rs#L707-L713).
* **Defect:** Widening `claude_stream_launch_enabled()` to Pi activates `canary_gate`, which hardcodes `canary::ClaudeCanaryLauncher`. Running `devflow start --agent pi` will spawn `claude` to check background task delivery tokens. If Claude is uninstalled, disabled, or unauthenticated, `canary_gate` will refuse the Pi launch with `CanaryOutcome::Absent` or `CanaryOutcome::Unverified`.

### 2. Preflight Unattended Launch Refusal in Auto Mode
* **Exact Site:** [`crates/devflow-cli/src/preflight.rs:964-990`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/preflight.rs#L964-L990) & [`crates/devflow-cli/src/preflight.rs:1131-1157`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/preflight.rs#L1131-L1157).
* **Defect:** In `Mode::Auto`, `preflight_unattended_launch_check` evaluates condition C2 (`unattended_launch_shape_condition`), which explicitly checks `if state.agent != AgentKind::Claude { causes.push("the agent is `{}`, not claude") }`. Without modifying this check, `devflow start --agent pi --mode auto` is guaranteed to be refused at preflight.

### 3. Idle Timeout Floor & Cadence Assumption Mismatch
* **Exact Site:** [`crates/devflow-core/src/monitor.rs:54-98`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/monitor.rs#L54-L98) & [`crates/devflow-core/src/monitor.rs:1024-1028`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/monitor.rs#L1024-L1028).
* **Defect:** The 120s idle timeout floor was empirically derived from Claude's 30.00s `tool_progress` keepalive cadence. Pi has no background keepalive heartbeat during long foreground tool runs (e.g. `cargo test`). Applying the pipe-owning monitor to Pi without validating Pi's silence duration during long builds risks killing healthy Pi runs.

### 4. Capture Classification Collision in Layer 1
* **Exact Site:** [`crates/devflow-core/src/agent_result.rs:1019-1066`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/agent_result.rs#L1019-L1066) & [`crates/devflow-core/src/agent_result.rs:1824-1842`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/agent_result.rs#L1824-L1842).
* **Defect:** [`classify()`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/agent_result.rs#L1019) scans top-level event types for `"system" | "user" | "assistant"` or dotted names (`"turn.*"`). Pi's JSON event stream emits top-level events like `"turn_start"`, `"message_start"`, `"agent_end"`. `classify()` classifies Pi captures as `CaptureKind::PlainText`, skipping stream parsers and falling through to the 4000-character tail scan.

### 5. Document Contradictions & Omissions
* **D-03 Omission:** In `38-CONTEXT.md`, Decision D-03 is listed under "Deferred (explicitly not here)" but is completely absent from the "Implementation Decisions" numbered list (which jumps from D-02 to D-04).
* **ROADMAP Goal vs D-01:** ROADMAP states the goal is to "complete all five stages", while D-01 defines success as "Plan → Code → Validate → Ship (Define is a no-op)".

---

## Handled
* Checked live CLI capabilities of `pi` (`pi --help`, `pi --mode json`, stdin streaming, `-p` flag).
* Verified `PipeOwning` supervisor, `CloseRule`, and `agent_result` event parser logic against current worktree source.

---

## Detail (Defects by Review Criteria)

### A. Decisions Failing Contact with Code & Tool Surfaces

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│ DEADLOCK CYCLE: Pi on PipeOwning Monitor                                                        │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 1. PipeOwning launches: `pi --mode json` with piped stdin and holds stdin open.                 │
│ 2. PipeOwning writes: `{"type":"user","message":{"role":"user","content":"..."}}` to stdin.     │
│ 3. Pi treats this JSON as literal prompt text; runs turn, emits `agent_end`.                    │
│ 4. Pi holds process open waiting for stdin EOF (does not exit while stdin pipe is open).        │
│ 5. CloseRule waits for `{"type":"result"}` (Claude schema) to signal stdin close.              │
│ 6. Pi never emits `{"type":"result"}` -> CloseRule never signals close -> Stdin never closed.  │
│ 7. DEADLOCK -> Monitor hits 120s idle timeout -> SIGKILLs Pi child -> Records IdleTimeout Fail. │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

1. **Pi Stdin Wire Format (`monitor.rs:726-732`):**
   - Quote from `38-CONTEXT.md`: *"route Pi through the pipe-owning monitor arm + the `CloseRule` drain gate"*
   - Reality: [`monitor.rs:820`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/monitor.rs#L820) calls `user_turn_line(prompt)`. Pi CLI has no handler for Claude's stdin JSON schema.
2. **Missing Unwrapper Infrastructure in Core (`monitor.rs:838-875`):**
   - Quote from `38-CONTEXT.md`: *"add a Pi JSON-mode event unwrapper that translates Pi's event schema into the monitor's event vocabulary"*
   - Reality: `run_pipe_owning_monitor` reads raw bytes from stdout and passes them directly to `rule.observe(&line)`. No trait or translation layer exists between the child stdout pipe and `CloseRule`.
3. **Canary Gate Coupled to Claude (`pipeline_launch.rs:143-150`):**
   - Quote from `38-CONTEXT.md`: *"`claude_stream_launch_enabled()` (`pipeline_launch.rs`) — the predicate to widen so Pi routes through `PipeOwning`"*
   - Reality: Widening this predicate trips [`canary_gate`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/pipeline_launch.rs#L462), which executes Claude Code to verify delivery tokens before any Pi run can start.

### B. Unstated Assumptions
1. **Pi Background Tasks & Subagent Concurrency:** D-02 assumes Pi emits background task events (`background_tasks_changed`, `task_started`, `task_notification`). Pi's built-in toolset (`read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`) has no native subagent dispatch. The `CloseRule` drain gate will always evaluate to `NeverAnnounced`.
2. **Preflight Interactivity Check Split:** D-04 assumes a single check governs interactivity. In reality, [`commands.rs:289`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/commands.rs#L289) is a hard CLI rejection before worktree creation, while [`preflight.rs:613`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/preflight.rs#L613) is an in-pipeline gate.

---

### Verification Limits & Negative Controls (RULE ZERO Compliance)
* **What this check establishes:** Proves that `pi --mode json` does not parse Claude stdin turn envelopes, does not exit while stdin is open, does not emit `type: "result"`, and that current `PipeOwning` monitor wiring deadlocks with Pi.
* **Negative control executed:** Ran `pi -p --mode json` with `stdin=DEVNULL` (exited cleanly in 6s) vs `stdin=PIPE` left open (timed out / stalled indefinitely).
* **What this check does NOT establish:** Does not test whether a custom Node.js wrapper or extension around Pi could theoretically bridge the protocol.
