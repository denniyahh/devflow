> invocation: antigravity --model gemini-3.1-pro-high --print-timeout 15m -p "<prompt>" (MCP servers linear+vercel temporarily disabled — they hang on connect and were blocking print mode)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-37 (feature/phase-37)

Here is the adversarial review of the Phase 37 planning artifacts.

### 1. Contradiction & Scope Leak: `stage_prompt_for_project` rewire vs. `pipeline_launch.rs` lock
**Claim (37-01-PLAN):** "Rewire the Code-stage caller in `prompt.rs` (and `stage_prompt_for_project`) to route through `render_prompt`" while strictly commanding "Do NOT touch `pipeline_launch.rs` / `MonitorLaunch` in this plan".
**Reality:** In `crates/devflow-cli/src/pipeline_launch.rs` lines 91 and 1081, `prompt::stage_prompt_for_project` is invoked with no `AgentKind` or driver argument (`prompt::stage_prompt_for_project(state.stage, state.phase, &state.project_root)`).
**Failure:** To route Code-stage prompts to driver-specific rendering, `stage_prompt_for_project` *must* be modified to accept the agent context. Modifying its signature guarantees compilation failure unless you also modify `pipeline_launch.rs` to pass that context. The plan demands a signature change but forbids touching the caller.

### 2. Contradiction & Scope Leak: Migrating the wrong Codex interactivity check
**Claim (37-04-PLAN):** "Replace the hardcoded Codex-Define check in `preflight.rs` with a generic `InteractivityMode` read." (Attempting to satisfy Backlog 999.31c which mandates replacing the check in `commands.rs`).
**Reality:** `crates/devflow-cli/src/preflight.rs` line 610 explicitly warns: `/// The pre-state Codex check itself is intentionally left unmigrated`. The actual pre-state check that Backlog 999.31c targets lives in `crates/devflow-cli/src/commands.rs` at line 289 (`if agent == AgentKind::Codex { if !phase_artifact_on_develop(...) }`).
**Failure:** 37-04-PLAN targets the wrong file. By migrating the secondary check in `preflight.rs` instead of `commands.rs`, it leaves the hardcoded `AgentKind::Codex` logic intact in the `start()` path, completely failing the backlog's decoupling objective while mutating a check explicitly marked out of scope.

### 3. Decision fails contact: `AgentAdapter` removal is mathematically impossible
**Claim (37-04-PLAN):** "Grep the workspace for every `AgentAdapter` reference; if (and only if) all four agents resolve through `AgentDriver`... delete the trait."
**Reality:** `crates/devflow-cli/src/pipeline_launch.rs` is excluded from modification across the entire phase (marked "read-only" in 37-02 and explicitly "Do NOT touch" in 37-01). It statically depends on the legacy trait at line 190 (`adapter: &dyn agents::AgentAdapter`) and directly invokes `agents::ClaudeAgent::exec_command_single_document` at line 204.
**Failure:** The conditional removal is theater. It is physically impossible for the workspace to be free of `AgentAdapter` references because the plans deliberately freeze `pipeline_launch.rs` on the legacy trait. 

### 4. Factual Error: `FixType` hallucination
**Claim (37-01-PLAN):** The `<read_first>` section lists: `crates/devflow-core/src/stage.rs — Stage, gsd_command() (~:60), FixType`, instructing the developer to embed it via `StageIntent::Code { fix: Option<FixType> }`.
**Reality:** `FixType` does not exist in `crates/devflow-core/src/stage.rs`. It is actually defined in `crates/devflow-core/src/prompt.rs` at line 73.
**Failure:** The plan hallucinates the location of a core type. Embedding `FixType` into `StageIntent` inside `stage.rs` requires either moving the enum or introducing an upward dependency/re-export from `prompt.rs`, neither of which is acknowledged or architected in the plan.

### 5. Scope Leak: `fix_prompt` callers ignored
**Claim (37-01-PLAN):** The plan and research document identify `fix_prompt()` as a second slash-command site that *must* migrate to `render_prompt` to prevent the loop-back from emitting `/gsd-execute-phase` to Codex.
**Reality:** `fix_prompt` is currently called by `crates/devflow-cli/src/pipeline_gate.rs` line 198 and `crates/devflow-cli/src/pipeline_outcomes.rs` line 4866. 
**Failure:** The plan's `files_modified` block and task instructions completely omit `pipeline_gate.rs` and `pipeline_outcomes.rs`. To migrate `fix_prompt`, its signature must change to accept the agent context. Attempting this migration will break compilation because upstream CLI callers are completely ignored by the plan.

### 6. Unstated Assumption & Validation Gap: `codex features list` shell-out
**Claim (37-03-PLAN):** `CodexDriver`'s `capabilities`/`environment` will "carry the signing-disable env + `multi_agent_v2` discovery" (which 31b defines as parsing `codex features list`).
**Reality (Speculation):** `capabilities` and `environment` are synchronous trait methods in `AgentDriver`. If they dynamically execute `codex features list` to discover this feature, they will introduce a synchronous blocking shell-out on every capability check across the pipeline. 
**Reality (Validation):** The 37-VALIDATION table for 37-03-01 explicitly checks `--ask-for-approval never` and relocated parsing, but has *no negative control or validation step* for `--enable multi_agent_v2` or the features list parsing. This acceptance criterion is unfalsifiable because the test plan dropped it entirely.
