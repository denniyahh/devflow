> invocation: codex exec -m gpt-5.6-terra -c model_reasoning_effort=high --cd <worktree> "<prompt>"
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-38 (feature/phase-38)
> DEDUPLICATED — codex emitted findings twice; the detailed copy is kept.

## Needs my awareness

1. **D-02 is built on the wrong transport contract.** [`38-CONTEXT.md:27-30`](.planning/phases/38-pi-end-to-end/38-CONTEXT.md:27) calls `--mode json` an event stream suitable for the pipe-owning `CloseRule`. Pi 0.84.1 implements it as **single-shot print mode**: “`pi --mode json "prompt"`” ([Pi `print-mode.js:2-7](/var/home/linuxbrew/.linuxbrew/Cellar/pi-coding-agent/0.84.1/libexec/lib/node_modules/@earendil-works/pi-coding-agent/dist/modes/print-mode.js:2)), then processes only positional messages ([`print-mode.js:103-109`](/var/home/linuxbrew/.linuxbrew/Cellar/pi-coding-agent/0.84.1/libexec/lib/node_modules/@earendil-works/pi-coding-agent/dist/modes/print-mode.js:103)). It does not consume the stdin protocol DevFlow injects.

   The existing pipe-owning monitor writes the Claude-only input object `{"type":"user","message":...}` ([`monitor.rs:715-731`](crates/devflow-core/src/monitor.rs:715)) and intentionally holds stdin open until the close rule fires ([`monitor.rs:819-835`](crates/devflow-core/src/monitor.rs:819)). Pi JSON mode ignores that pipe. Routing Pi through this arm therefore gives no bidirectional-session behavior and no meaningful drain-gate protection.

2. **D-02 promises task coverage from events Pi does not emit.** The decision names `task_started`, `task_notification`, and `background_tasks_changed` ([`38-CONTEXT.md:27-30`](.planning/phases/38-pi-end-to-end/38-CONTEXT.md:27)). Pi’s installed JSON event union contains `agent_*`, `turn_*`, `message_*`, and `tool_execution_*` only ([Pi `types.d.ts:374-412`](/var/home/linuxbrew/.linuxbrew/Cellar/pi-coding-agent/0.84.1/libexec/lib/node_modules/@earendil-works/pi-coding-agent/node_modules/@earendil-works/pi-agent-core/dist/types.d.ts:374)). There is no task ID, task status, background-task list, or agent-subtask lifecycle to translate.

   `CloseRule` only treats Claude-shaped `system/background_tasks_changed` and `system/task_*` events as outstanding work ([`monitor.rs:620-685`](crates/devflow-core/src/monitor.rs:620)). Mapping arbitrary `tool_execution_start/end` to those events would fabricate concurrency semantics rather than provide “real coverage.”

3. **The proposed Pi route cannot release stdin through `CloseRule`.** The rule requires a top-level Claude-shaped `type:"result"` event with a parsed marker ([`monitor.rs:500-516`](crates/devflow-core/src/monitor.rs:500); [`agent_result.rs:1188-1195`](crates/devflow-core/src/agent_result.rs:1188)). Pi emits `message_end` and `agent_end`, not `result` ([Pi `types.d.ts:377-395`](/var/home/linuxbrew/.linuxbrew/Cellar/pi-coding-agent/0.84.1/libexec/lib/node_modules/@earendil-works/pi-agent-core/dist/types.d.ts:377)). Thus the close rule will never fire for native Pi output. The monitor only escapes when stdout closes after Pi exits ([`monitor.rs:886-899`](crates/devflow-core/src/monitor.rs:886)), making the drain gate inert.

4. **D-01 has no completion-parser plan, so Pi can ignore its own failed result.** The document claims “Full Claude parity” ([`38-CONTEXT.md:19-24`](.planning/phases/38-pi-end-to-end/38-CONTEXT.md:19)), but its unwrapper is only described as monitor-vocabulary translation. Pi places the final text inside JSON `message_*` events; DevFlow’s Layer 1 recognizes only Claude envelopes/streams, raw marker lines, and Codex events ([`agent_result.rs:1806-1841`](crates/devflow-core/src/agent_result.rs:1806)). A marker embedded in Pi JSON is neither a raw line nor a recognized envelope.

   This is not cosmetic. Pi JSON mode checks `stopReason == "error"` only in text mode ([Pi `print-mode.js:110-128`](/var/home/linuxbrew/.linuxbrew/Cellar/pi-coding-agent/0.84.1/libexec/lib/node_modules/@earendil-works/pi-coding-agent/dist/modes/print-mode.js:110)); JSON mode can return exit 0 after an assistant error. DevFlow then falls to the coarse exit-code classifier, which makes zero-exit non-Plan/Code stages successful ([`agent_result.rs:2001-2053`](crates/devflow-core/src/agent_result.rs:2001)). A Pi-specific terminal-result/error parser is required; it is absent from D-01/D-02.

5. **D-04 contradicts the actual Define behavior and would block the advertised command.** D-01 says “Define is a no-op for every agent” ([`38-CONTEXT.md:19-20`](.planning/phases/38-pi-end-to-end/38-CONTEXT.md:19)); D-04 nevertheless assigns Pi Define `RequiresExistingArtifact` because it “cannot run the interactive discuss-phase interview” ([`38-CONTEXT.md:35-39`](.planning/phases/38-pi-end-to-end/38-CONTEXT.md:35)). Define never invokes that interview: it explicitly says “There is no agent work to perform” and “whether or not … CONTEXT.md already exists” ([`prompt.rs:293-315`](crates/devflow-core/src/prompt.rs:293)).

   If the planned generic gate honors Pi’s declaration, a fresh `devflow start --agent pi` will be refused for missing `CONTEXT.md`, directly defeating D-01 and the roadmap goal ([`ROADMAP.md:19-23`](.planning/ROADMAP.md:19)).

6. **“Mirror Codex” is not a coherent implementation target.** `CodexDriver` declares `RequiresExistingArtifact` for both Define and Plan ([`codex.rs:87-96`](crates/devflow-core/src/agents/codex.rs:87)), but the live enforcement is hardcoded to **Codex + Auto + Define** only ([`preflight.rs:612-625`](crates/devflow-cli/src/preflight.rs:612)); its own test asserts Plan passes without a context artifact ([`preflight.rs:1379-1392`](crates/devflow-cli/src/preflight.rs:1379)). `commands.rs` merely warns if Plan is missing ([`commands.rs:289-302`](crates/devflow-cli/src/commands.rs:289)).

   Therefore, making the gate genuinely driver-driven has two bad interpretations: honor `Plan => RequiresExistingArtifact` and regress Codex/Pi fresh planning, or preserve current behavior and leave the driver declaration unconsumed. D-04 supplies neither an artifact mapping nor stage-specific semantics.

7. **Widening `claude_stream_launch_enabled()` silently makes Pi depend on Claude.** The stated integration point is to widen that predicate ([`38-CONTEXT.md:73-78`](.planning/phases/38-pi-end-to-end/38-CONTEXT.md:73)). Any true result invokes `ClaudeCanaryLauncher` ([`pipeline_launch.rs:143-150`](crates/devflow-cli/src/pipeline_launch.rs:143)), which explicitly launches `ClaudeAgent` ([`canary.rs:261-285`](crates/devflow-core/src/canary.rs:261)). The canary then refuses the Pi launch if the **Claude** delivery probe is absent or unverified ([`pipeline_launch.rs:490-519`](crates/devflow-cli/src/pipeline_launch.rs:490)).

   That is an unstated Claude binary/auth dependency, a false proxy for Pi behavior, and a scope leak: Phase 38 needs a Pi-specific capability decision or must explicitly exempt Pi—not a renamed Claude predicate.

8. **The acceptance bar is non-falsifiable.** “Completing Plan → Code → Validate → Ship” ([`38-CONTEXT.md:19-24`](.planning/phases/38-pi-end-to-end/38-CONTEXT.md:19)) specifies no Pi version, fixture, expected JSONL records, completion/error mapping, or failure assertion. It has no negative controls for:

   - Pi `message_end` with `stopReason: "error"` and process exit 0;
   - embedded `DEVFLOW_RESULT: failed` not advancing;
   - a marker-like string in a user/tool message not becoming completion;
   - absent task lifecycle events not being mistaken for “drained”;
   - Pi installed without Claude, proving no Claude canary is invoked;
   - a long-running tool operation with no output, proving the timeout policy is deliberate rather than borrowed from Claude cadence.

   The existing monitor explicitly says its timeout cadence was measured only for Claude and must not be applied to an unmeasured agent ([`monitor.rs:1023-1027`](crates/devflow-core/src/monitor.rs:1023)). D-02 proposes exactly that without Pi measurements.

Pi CLI surface was verified against installed **0.84.1** (`pi --help`, `pi --mode json --help`, and `pi auth check --help`).
