# Adversarial Review: Phase 53 ("The Question Channel") Plan Set

**Targets:** `.planning/phases/53-the-question-channel/53-*-PLAN.md` (Plans 53-01 through 53-13)  
**Context & Baseline:** `53-CONTEXT.md`, `53-RESEARCH.md`, `53-MCP-SPIKE.md`, `53-VALIDATION.md`, `.planning/ROADMAP.md`, `.planning/REQUIREMENTS.md`, and code under `crates/`.  
**Review Assessment:** **FAIL**. The plan set contains critical fabrications of binding operator decisions, severe concurrency/timeout defects that kill healthy runs, massive token burn vulnerabilities violating core rules, and broken verification gates.

---

## CONFIRMED

### 1. [CRITICAL] Fabricated Operator Decisions Refuse Drivers & Mutilate Existing Test Suites
- **Location:**  
  - [53-08-PLAN.md#L30](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-08-PLAN.md#L30)  
  - [53-12-PLAN.md#L21](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-12-PLAN.md#L21), [L67](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-12-PLAN.md#L67)  
  - [53-06-PLAN.md#L26](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-06-PLAN.md#L26)  
  - Contradicts: [53-CONTEXT.md#L206-L214](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-CONTEXT.md#L206-L214), [.planning/ROADMAP.md#L15](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/ROADMAP.md#L15), [.planning/REQUIREMENTS.md#L19](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/REQUIREMENTS.md#L19)
- **Evidence:**  
  `53-08-PLAN.md` line 30 asserts:
  > `Operator answer (2), 2026-11-18, verbatim: "REFUSE: opencode/pi/hermes/antigravity launches are refused with a named message until they have an MCP registration..."`
  
  And `53-12-PLAN.md` lines 21 and 67 state:
  > `Per operator decision (2) recorded in 53-08-PLAN.md, refuse launches with unconfigured agents...`
  
  This claim is false. In `53-CONTEXT.md` (lines 206–214), the operator recorded exactly two decisions:
  `D-01: "1 spike now"` and `D-17: "2 a"`. The operator **never** authorized refusing other agents. The text quoted "verbatim" was lifted directly from internal research question C5 in `53-RESEARCH.md` and falsely attributed to the operator.
- **Impact:**  
  1. Violates **ROADMAP Success Criterion 1**: *"Every agent driver (claude, codex, and stub agent for the other launch shapes) has a configured question channel..."*  
  2. Violates **REQUIREMENTS.md ASK-01**: *"devflow provisions an agent-appropriate question channel for every supported agent..."*  
  3. Breaks failover in `--mode auto`: In [crates/devflow-cli/src/preflight.rs#L883-L907](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/crates/devflow-cli/src/preflight.rs#L883-L907), only `claude` and `antigravity` satisfy unattended launch requirements. Refusing `antigravity` destroys auto-failover (`--agent claude --fallback-agent antigravity`).  
  4. Mutilates test coverage: Because `antigravity` is refused, Plan 53-12 destroys existing passing tests in [tests/phase7_cli.rs#L210](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/tests/phase7_cli.rs#L210) and downgrades [tests/failover_e2e.rs#L12](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/tests/failover_e2e.rs#L12) from `--mode auto` to `--mode supervise`.

---

### 2. [CRITICAL] Fatal Idle Timeout Race Kills Healthy Agents During Compilation / Silence
- **Location:**  
  - [53-13-PLAN.md#L45-L65](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-13-PLAN.md#L45-L65)  
  - Interacts with: [crates/devflow-core/src/monitor.rs#L900-L985](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/crates/devflow-core/src/monitor.rs#L900-L985)
- **Evidence:**  
  In `crates/devflow-core/src/monitor.rs`, the monitor loop measures idle timeout via `event_rx.recv_timeout(idle_timeout)` (lines 900–915), waiting for lines emitted to stdout.  
  Plan `53-13-PLAN.md` implements the timeout suppression by checking `is_gate_open()` **only at the exact moment `RecvTimeoutError::Timeout` fires**:
  > `If timeout fires and is_gate_open(phase) returns true: log at trace/debug ... and do NOT call on_idle_timeout(); continue loop.`
- **Flaw & Failure Mode:**  
  Suppose `idle_timeout` is 120s.
  1. At $t=0$, agent outputs text and calls `devflow_ask`.
  2. At $t=10$, operator answers via CLI. Gate file is closed/deleted. Agent resumes execution (e.g., runs a long build `cargo build --release` or tests that produce no stdout for 115s).
  3. At $t=120$, `event_rx.recv_timeout` fires because 120s elapsed since the last stdout line at $t=0$.
  4. The monitor checks `is_gate_open()`. It returns **false** because the gate was closed at $t=10$.
  5. The monitor triggers `on_idle_timeout()`, abruptly terminating a healthy, active agent.
  
  Plan 53-13 does not reset `last_activity` when a gate opens or closes, nor does it account for time spent in an answered gate.

---

### 3. [MAJOR] Default 3-Day Gate Causes Catastrophic Token Explosion on Codex Hold
- **Location:**  
  - [53-07-PLAN.md#L107-L115](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-07-PLAN.md#L107-L115)  
  - [53-MCP-SPIKE.md#L79-L82](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-MCP-SPIKE.md#L79-L82)  
  - [crates/devflow-cli/src/config_parse.rs#L19](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/crates/devflow-cli/src/config_parse.rs#L19)  
  - Violates: [53-CONTEXT.md#L73-L75](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-CONTEXT.md#L73-L75) (D-06) and Global Rule Zero ("Fewest tokens")
- **Evidence:**  
  `53-MCP-SPIKE.md` lines 79–82 empirically proved:
  > `Codex internally polls wait every ~21 s while a tool call is held... burned 1.35 M tokens in 15 min across ~43 wait polls.` (~5.4M tokens/hour).
  
  `53-CONTEXT.md` D-06 explicitly specifies:
  > *"If item 3 shows a call cannot stay open as long as an operator may take, D-06's fallback applies [`pending` status returned immediately, agent re-invokes or pauses]..."*
  
  In DevFlow, the default gate timeout is 3 days (`const THREE_DAYS: u64 = 3 * 24 * 60 * 60;` in `config_parse.rs:19`). Plan 53-07 discards D-06's fallback without justification, setting `tool_timeout_sec = T + 60` (259,260 seconds) and holding the JSON-RPC call synchronously.
- **Impact:**  
  If an operator leaves a question unanswered overnight or over a weekend, Codex will loop `wait` ~4,100 times per day, burning upwards of **130 million tokens** on a single gate, directly violating Rule Zero.

---

### 4. [MAJOR] Inoperable Data Structure for JSON-RPC Cancellation in Plan 53-07
- **Location:**  
  - [53-07-PLAN.md#L107-L132](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-07-PLAN.md#L107-L132)
- **Evidence:**  
  Plan 53-07 specifies:
  > `Share an Arc<CallFlags> (AtomicBools for cancelled-request-id match, EOF, terminated) between the stdin reader and the worker thread.` (line 107)  
  > `If request-id was queued but not yet taken by worker thread, remove from queue and discard.` (line 132)
- **Flaw:**  
  1. An `AtomicBool` is a boolean flag (`true`/`false`). It cannot hold, compare, or match arbitrary JSON-RPC request IDs (which are arbitrary strings or integers per JSON-RPC 2.0 §4.4).  
  2. The stdin reader passes jobs across an `mpsc::channel` (`mpsc::Sender<JsonRpcMessage>`). Standard library `mpsc::Receiver` does not support random-access inspection or removal of queued elements.  
  3. The concurrency design is unimplementable as written without replacing `CallFlags` with a concurrent set (`Mutex<HashSet<Value>>`) and the queue with a `Mutex<VecDeque<...>>`.

---

### 5. [MAJOR] Git Repository Pollution via Leaked `.devflow/ask-*.mcp.json`
- **Location:**  
  - [53-08-PLAN.md#L28](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-08-PLAN.md#L28)  
  - [.gitignore#L74-L85](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.gitignore#L74-L85)  
  - [crates/devflow-cli/src/commands.rs#L1941-L1980](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/crates/devflow-cli/src/commands.rs#L1941-L1980)
- **Evidence:**  
  Plan 53-08 writes `.devflow/ask-<NN>.mcp.json` on phase launch.  
  In `.gitignore`:
  ```gitignore
  .devflow/state-*.json
  .devflow/monitor-*.pid
  .devflow/gates/
  ```
  `.devflow/*.mcp.json` is not ignored.  
  Neither `devflow discard` (`commands.rs:1941`) nor `devflow cleanup` (`commands.rs:1415`) purges `ask-*.mcp.json`.
- **Impact:**  
  Every test run or dogfooding phase permanently litters untracked files into `.devflow/`, dirtying working copy status and violating the project rule *"Clean up after yourself"*.

---

### 6. [MAJOR] Non-Functional Verification Gate in Plan 53-10 Task 1
- **Location:**  
  - [53-10-PLAN.md#L74](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-10-PLAN.md#L74)
- **Evidence:**  
  Plan 53-10 Task 1 claims to delete deprecated items `AnswerFamily::CheckpointDeclaration` and `StatusLessStep::RecordCheckpointSet`.  
  Its verification command is:
  ```bash
  cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=checkpoint::tests::checkpoint_record_survives_roundtrip)' && \
  cargo nextest run -p devflow-cli --bin devflow --no-tests=fail -E 'test(=gate_channel_e2e::tests::gate_question_end_to_end)'
  ```
- **Flaw:**  
  Neither test exercises `AnswerFamily` or `StatusLessStep`. If an executor leaves the deprecated variants completely intact in the codebase, this verification command passes 100% green. It has no negative control and fails Repo Trap 1 ("count with a Rust source-scan test like doc_check.rs").

---

### 7. [MAJOR] Asynchronous Abort Hazard in Plan 53-09
- **Location:**  
  - [53-09-PLAN.md#L74-L78](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-09-PLAN.md#L74-L78)  
  - Interacts with: [crates/devflow-cli/src/pipeline_launch.rs#L394-L410](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L394-L410)
- **Evidence:**  
  In Plan 53-09, `devflow gate abort <gate_id>` writes `State::abort_requested`, removes the question gate, and sends an `isError: true` JSON-RPC response to the agent.
- **Flaw:**  
  `pipeline_launch.rs` evaluates `abort_requested` only in `advance_with()`, which runs *after the agent process terminates*. The child agent process is never sent `SIGTERM`/`SIGINT`. If the agent catches the MCP tool error and continues working or hangs, `devflow gate abort` fails to stop the run. Furthermore, if the agent or server process crashes before state is synced, Task 2's `close_question_gate` has already deleted the gate file, leaving the pipeline runner to proceed normally as if no abort occurred.

---

### 8. [MINOR] False SemVer Claim in Review Classification
- **Location:**  
  - [53-CONTEXT.md#L201-L205](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-CONTEXT.md#L201-L205)  
  - [53-01-PLAN.md#L20](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-01-PLAN.md#L20)  
  - [53-03-PLAN.md#L25](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-03-PLAN.md#L25)  
  - [53-06-PLAN.md#L24](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-06-PLAN.md#L24)
- **Evidence:**  
  `53-CONTEXT.md` justifies a Tier B review by asserting:
  > *"Tier B ... nothing in it ... changes ... the published devflow-core API in a breaking way (new pub items are additive)."*
  
  In reality:
  - Plan 53-01 removes `GateKind::Human` and `GATE_RESOLUTION_RULE`.
  - Plan 53-03 removes `pub fn validate_checkpoint`.
  - Plan 53-06 removes `pub fn is_human_blocking`.
  - Plan 53-10 removes `AnswerFamily::CheckpointDeclaration` and `StatusLessStep::RecordCheckpointSet`.
- **Impact:**  
  Every one of these edits is a semver-breaking change to `devflow-core`. While the crate is unreleased `4.0.0-alpha.1`, justifying Tier B on the claim that changes are purely additive is factually false.

---

## SUSPECTED

### 1. [MAJOR / SUSPECTED] Tool Namespace Mismatch in Universal Prompt Template
- **Location:**  
  - [53-06-PLAN.md#L66](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-06-PLAN.md#L66)
- **Details:**  
  Plan 53-06 injects into the universal agent prompt:
  > `When you need a decision from the operator, call the devflow_ask MCP tool (registered as devflow_ask on your devflow server)...`
  
  Codex and Claude handle MCP server namespaces differently. Claude CLI exposes tools with server prefix scoping or root identifiers depending on server registration name (`mcp__<server>__<tool>`), while Codex formats them according to its own system prompt tool schema. Prompting the agent with the unqualified literal `devflow_ask` without validating how Codex and Claude expose the tool name risks the model hallucinating missing tool errors.

### 2. [MINOR / SUSPECTED] Global MCP Collision for Codex Driver
- **Location:**  
  - [53-08-PLAN.md#L83-L98](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/2bc44b1f-d9d1-40c2-8d95-5846fc38b9c9/scratchpad/review53/snapshot/.planning/phases/53-the-question-channel/53-08-PLAN.md#L83-L98)
- **Details:**  
  Plan 53-08 invokes `codex exec --mcp-config .devflow/ask-<NN>.mcp.json`. In standard environments, `codex` also loads configuration from `~/.codex/config.toml` unless explicitly isolated. If developer environments have global MCP servers configured, those tools will leak into the phase run context, conflicting with the single-tool isolation assumption made throughout Phase 53 plans.
