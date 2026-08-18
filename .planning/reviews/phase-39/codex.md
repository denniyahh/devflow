> invocation: codex exec -m gpt-5.6-terra -c model_reasoning_effort=high --cd /var/home/denniyahh/Github/devflow/.worktrees/phase-39 "$(cat /tmp/p39-review-prompt.txt)"
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-39 (feature/phase-39 @ e4f0bb6)
> DEDUPLICATED — codex emitted findings twice; the detailed copy is kept.

## BLOCKER (0)

None.

## HIGH

1. [crates/devflow-core/src/agents/pi.rs:73](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-core/src/agents/pi.rs:73)

   > `if providers.is_empty() { return Err("no provider configured in Pi's models.json ...") }`

   Pi 0.84.1 supports a default `google` provider and environment credentials, but this refuses before `pi auth check` whenever `models.json` is absent/empty. The old hardcoded Google probe would work for that ordinary setup.

   **What breaks:** a working default-provider/API-key Pi installation is now blocked by DevFlow preflight.

2. [crates/devflow-core/src/agents/pi.rs:80](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-core/src/agents/pi.rs:80) and [pi.rs:57](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-core/src/agents/pi.rs:57)

   > `for provider in providers { ... Ok(()) => return Ok(()) }`  
   > `vec!["-p".into(), "--no-approve".into(), prompt.to_string()]`

   Health accepts any ready `models.json` provider, while the launch supplies no provider or model. It therefore does not test the provider Pi will actually select.

   **What breaks:** a multi-provider profile can pass preflight on provider B and then fail its Pi run when the implicit/default provider A has no credential.

3. [crates/devflow-cli/src/commands.rs:2330](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-cli/src/commands.rs:2330) and [crates/devflow-cli/src/pipeline_launch.rs:200](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-cli/src/pipeline_launch.rs:200)

   > `let dispatch = agents::driver_for(AgentKind::Pi).capabilities().subagent_dispatch;`  
   > `if stream_launch { ... } else if agent == AgentKind::Claude { ... } else { ... }`

   `subagent_dispatch` is consumed only by `devflow doctor`. Launch/stage selection never reads it. Yet [the guide:40](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/docs/guides/pi-subagent-dispatch.md:40) claims two arms and that the capability “decides” dispatch expectations.

   **What breaks:** Stage 2 detection is diagnostic-only; install detection cannot alter routing, require dispatch, or prevent an undetected extension from loading under Pi.

4. [.planning/phases/39-pi-end-to-end/39-E2E-SMOKE.md:12](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/.planning/phases/39-pi-end-to-end/39-E2E-SMOKE.md:12)

   > `subagent task \`echo subagent-ran > /tmp/p39-subagent-proof.txt\``  
   > `The model invokes the \`subagent\` tool`

   The smoke invokes `pi -p` directly, not DevFlow/`MonitorLaunch::Legacy`; Pi’s built-in `bash` tool could create the identical proof file without calling `subagent`. No tool trace, parent/subagent PID evidence, or monitor capture is recorded.

   **What breaks:** the artifact does not establish either subagent invocation or DevFlow’s claimed process-exit observation.

## MEDIUM

5. [crates/devflow-cli/src/pipeline_launch.rs:3283](/var/home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-cli/src/pipeline_launch.rs:3283)

   > `resolve_launch_shape(..., false)`  
   > `assert!(matches!(launch, monitor::MonitorLaunch::Legacy));`

   The Legacy regression test injects `false`; it never checks that production `claude_stream_launch_enabled(AgentKind::Pi, ...)` remains false.

   **What breaks:** a future change that enables Pi stream launch can route to `PipeOwning` while this test still passes.

## LOW (0)

None.

VERDICT: FIX-FIRST — preflight can false-reject or false-green Pi credentials, and the Stage 2/E2E claims are not backed by a runtime routing path or discriminating evidence.
hook: Stop
hook: Stop Completed
