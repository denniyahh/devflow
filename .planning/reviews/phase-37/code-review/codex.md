> invocation: codex exec -m gpt-5.6-luna -c model_reasoning_effort=high --cd <worktree> "<prompt>"
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-37 (feature/phase-37)
> DEDUPLICATED — codex emitted the findings twice and echoed prior-plan review files; the clean final report (findings 1-9) is kept.

CODE REVIEW REPORT

Recommendation: REQUEST CHANGES  
Architectural status: BLOCK

1. **HIGH — Codex/Pi Validate prompts omit the required verdict**

`crates/devflow-core/src/prompt.rs:394-402`

`render_workflow_style()` renders Validate as only:

> `"Read ... validate-phase.md ... {COMPLETION_PROTOCOL}"`

The Claude-style Validate prompt explicitly requires `"verdict": "pass"` or `"gaps"` at `prompt.rs:220-245`. The classifier treats `Success + verdict: None` as failed in `crates/devflow-cli/src/pipeline_outcomes.rs:230-245`.

Failure: a compliant Codex/Pi response with `DEVFLOW_RESULT: {"status":"success"}` loops back or gates instead of advancing to Ship.

2. **HIGH — Codex/Pi Ship bypasses the mandatory review gate**

`crates/devflow-core/src/prompt.rs:395-402`

`StageIntent::Ship { phase, .. }` discards `review_angles` and points directly at `ship.md`. It does not instruct the agent to run code review first, inspect `REVIEW.md`, block Critical findings, or emit a `review:` failure.

The legacy renderer implements that gate at `prompt.rs:167-208`.

Failure: native-agent Ship runs can skip the DevFlow review gate and do not preserve configured review angles or the Code loop-back contract.

3. **HIGH — Native Define/Plan rendering reintroduces interactive workflows**

`crates/devflow-core/src/prompt.rs:382-383`

Define maps directly to `discuss-phase.md`; Plan maps directly to `plan-phase.md`.

The existing Claude-style prompts deliberately avoid this:

- Define is an explicit no-op at `prompt.rs:288-309`.
- Plan checks for an existing artifact and avoids interactive overwrite decisions at `prompt.rs:248-284`.

Failure: headless Codex/Pi runs can enter interactive Define/Plan flows, hang, or repeatedly fail on existing artifacts.

4. **MEDIUM — `InteractivityMode` is dead metadata**

`crates/devflow-core/src/agents/mod.rs:175-177`  
`crates/devflow-core/src/agents/codex.rs:87-95`  
`crates/devflow-cli/src/preflight.rs:612-624`  
`crates/devflow-cli/src/commands.rs:289-303`

Codex declares Define and Plan as `RequiresExistingArtifact`, but runtime checks still hardcode `AgentKind::Codex`, only reject missing Define context, and merely warn for missing Plan artifacts.

Failure: Codex Plan proceeds despite its declared requirement; future drivers’ interactivity declarations are ignored entirely.

5. **MEDIUM — “Driver-owned parsing” is not wired into evaluation**

`crates/devflow-core/src/agents/codex.rs:66-71` defines `CodexDriver::parse_completion()`, but the production cascade directly calls the old free function at `crates/devflow-core/src/agent_result.rs:1834-1841`.

Failure: changes to `AgentDriver::parse_completion()` have no runtime effect. The new driver contract is nominal while completion behavior remains globally hardcoded.

6. **HIGH — Codex parser lets an earlier success marker beat a later terminal failure**

`crates/devflow-core/src/agent_result.rs:764-781`

The parser scans for any last `agent_message` marker and returns it before examining `turn.failed` at lines 784-812.

A valid sequence such as:

```text
thread.started
item.completed(agent_message: DEVFLOW_RESULT success)
turn.failed(error: ...)
```

returns Success. If the process exits zero or no exit record exists, the stage can advance despite the terminal failure. The existing test only covers success followed by `turn.completed` (`agent_result.rs:4490-4498`), not success followed by `turn.failed`.

7. **MEDIUM — `test_contract()` is far too weak**

`crates/devflow-core/src/agents/mod.rs:193-217`

The suite checks only:

- non-empty driver name;
- `DEVFLOW_RESULT` appears in rendered prompts;
- command program is non-empty.

It does not test `parse_completion`, health, environment, sandbox requirements, discovery, interactivity, argument validity, or stage-specific prompt semantics.

Failure: a driver with broken parsing, health, sandbox, or Validate/Ship behavior still passes every conformance assertion.

8. **MEDIUM — Pi health check can block indefinitely**

`crates/devflow-core/src/agents/pi.rs:52-59`

`Command::output()` runs `pi auth check` synchronously with no timeout. The installed Pi CLI documents that auth checks refresh expired OAuth credentials by default.

Failure: a stalled provider/network/credential refresh blocks DevFlow preflight indefinitely, with no cancellation or timeout path.

9. **MEDIUM — Codex writable-root serialization mishandles hostile paths**

`crates/devflow-core/src/agents/codex.rs:47-60`

Paths are converted with:

> `root.display().to_string()`

and only backslashes and quotes are escaped. Non-UTF-8 paths become `�`; newline-containing paths produce invalid TOML.

Failure: a valid worktree path can yield a nonexistent or malformed `sandbox_workspace_write.writable_roots` override, causing Codex launches or commits to fail.

No confirmed shell command injection was found in the prompt/argv construction itself; arguments are passed as literal argv values. The path serialization defect above is an encoding/configuration failure.

Validation performed: `cargo test -p devflow-core --lib` passed 628 tests; the focused pipeline-gate suite passed 17 tests. Those tests do not exercise native Codex/Pi Validate or Ship prompts, driver-dispatched parsing, hostile path encoding, or Pi network hangs.
