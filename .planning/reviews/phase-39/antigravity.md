> invocation: antigravity --model gemini-3.7-flash-high --print-timeout 15m --dangerously-skip-permissions -p "$(cat /tmp/p39-review-prompt.txt)" (MCP linear/vercel disabled during run, restored after)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-39 (feature/phase-39 @ e4f0bb6)

I have launched the search to locate the repository workspace and will proceed as soon as the results are returned.
I am waiting for the background search to complete.
Waiting for directory search to complete.
Running `cargo test` on devflow-core and devflow binaries to verify current test state.
Waiting for cargo test completion.
Waiting for task-51 to complete.
Waiting for tests to finish compiling and running.
### BLOCKER

#### 1. `PiDriver::health` breaks standard Pi installations lacking a custom `models.json`
- **Location:** [`crates/devflow-core/src/agents/pi.rs:74-79`](file:///home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-core/src/agents/pi.rs#L74-L79), [`crates/devflow-core/src/agents/pi.rs:127-148`](file:///home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-core/src/agents/pi.rs#L127-L148)
- **Quoted Code:**
  ```rust
  let providers = configured_pi_providers();
  if providers.is_empty() {
      return Err(
          "no provider configured in Pi's models.json — run `pi auth check` for details"
              .to_string(),
      );
  }
  ```
- **What breaks:** Standard Pi installations using built-in providers (Anthropic, OpenAI, Google) configured via environment variables (e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) or OAuth in `auth.json` do not create or require `models.json` (`models.json` is strictly for custom endpoints like LiteLLM/vLLM). Because `configured_pi_providers()` reads exclusively from `models.json`, any standard Pi user is unconditionally blocked by `PiDriver::health` with `"no provider configured in Pi's models.json"`, even when `pi auth check --json --provider anthropic` succeeds with exit 0 (`ready`). Moreover, the suggested remediation `"run pi auth check for details"` fails on Pi (`Error: Auth checks require --provider <provider> or --model <model>`).

---

### HIGH

#### 2. `PiDriver::health` accepts ANY ready provider in `models.json`, masking broken default providers
- **Location:** [`crates/devflow-core/src/agents/pi.rs:81-101`](file:///home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-core/src/agents/pi.rs#L81-L101)
- **Quoted Code:**
  ```rust
  for provider in providers {
      let output = std::process::Command::new("pi")
          .args([
              "auth",
              "check",
              "--json",
              "--provider",
              &provider,
              "--no-refresh",
          ])
          .output()
          .map_err(|e| format!("could not run `pi auth check`: {e}"))?;
      match classify_auth_check(
          &String::from_utf8_lossy(&output.stdout),
          output.status.success(),
      ) {
          Ok(()) => return Ok(()),
          Err(e) => last_err = Some(e),
      }
  }
  ```
- **What breaks:** If `models.json` defines multiple providers (e.g., a local `ollama` and a remote `openrouter`), `health()` returns `Ok(())` on the first provider that succeeds. If Pi's active model configured in `~/.pi/agent/settings.json` (`defaultProvider` / `defaultModel`) uses a different provider with expired or missing credentials, preflight reports healthy, and the headless run fails mid-flight upon launching `pi -p`.

---

### MEDIUM

#### 3. `39-E2E-SMOKE.md` records an unverified proxy measurement for subagent execution
- **Location:** [`.planning/phases/39-pi-end-to-end/39-E2E-SMOKE.md:12-14`](file:///home/denniyahh/Github/devflow/.worktrees/phase-39/.planning/phases/39-pi-end-to-end/39-E2E-SMOKE.md#L12-L14)
- **Quoted Code:**
  ```markdown
  Run 2 (dispatch proof): same, with the subagent task `echo subagent-ran > /tmp/p39-subagent-proof.txt`
  → stdout `DEVFLOW_RESULT: {"status":"success"}`, exit 0, and `/tmp/p39-subagent-proof.txt` contains
  `subagent-ran`.
  ```
- **What breaks:** An artifact file created via bash side-effect is a proxy measurement (Rule Zero b): Pi's built-in `bash` tool can write `/tmp/p39-subagent-proof.txt` directly without calling `subagent`. The smoke documentation records no session ID, tool-call trace, or subagent log confirming the `subagent` tool was invoked. While local inspection of `/tmp/p39-e2e-profile/sessions/` confirms DeepSeek happened to call `toolCall: subagent`, the documented evidence itself does not substantiate the claim. Furthermore, the test used a synthetic prompt coercing subagent usage rather than executing an actual DevFlow stage prompt or pipeline workflow.

#### 4. Regression test `pi_resolves_to_legacy_launch` passes a hardcoded boolean, bypassing the predicate
- **Location:** [`crates/devflow-cli/src/pipeline_launch.rs:3283-3294`](file:///home/denniyahh/Github/devflow/.worktrees/phase-39/crates/devflow-cli/src/pipeline_launch.rs#L3283-L3294)
- **Quoted Code:**
  ```rust
  let (program, args, launch) = resolve_launch_shape(
      state.agent,
      driver.as_ref(),
      state.phase,
      "the stage prompt".to_string(),
      &[],
      false,
  );
  assert!(matches!(launch, monitor::MonitorLaunch::Legacy));
  ```
- **What breaks:** The test passes a literal `false` as the 6th argument (`stream_launch`) to `resolve_launch_shape` instead of calling `claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch)`. It only asserts the fallback branch of `resolve_launch_shape` given `stream_launch: false`; it would silently pass even if `claude_stream_launch_enabled` was broken to return `true` for Pi.

---

### LOW

#### 5. Over-claimed "dual-stage routing" in documentation vs doctor-only reporting
- **Location:** [`docs/guides/pi-subagent-dispatch.md:39-43`](file:///home/denniyahh/Github/devflow/.worktrees/phase-39/docs/guides/pi-subagent-dispatch.md#L39-L43), [`ARCHITECTURE.md:104-106`](file:///home/denniyahh/Github/devflow/.worktrees/phase-39/ARCHITECTURE.md#L104-L106)
- **Quoted Code:**
  ```markdown
  ## Routing

  The launch argv is identical in both arms (`pi -p --no-approve "<prompt>"`). The
  capability decides what DevFlow *expects and advertises* — subagent delegation
  is available to the run only when a subagent extension is installed. The
  capability is the hook any stage-routing logic keys on.
  ```
- **What breaks:** No execution logic in DevFlow (`start`, `pipeline_launch`, `prompt`, `advance`, `run_gate`) branches on `DriverCapabilities::subagent_dispatch`. The launch command, prompt, and supervision are identical in both cases. Stating that DevFlow has "two routing arms" or that capability detection is a "routing decision" is an over-claim; `subagent_dispatch` is currently an informational check displayed in `devflow doctor`.

---

**VERDICT: FIX-FIRST** — `PiDriver::health` introduces a breaking regression that locks out standard Pi users who do not maintain a custom `models.json` (such as those using API keys in environment variables or OAuth in `auth.json`), and short-circuits auth checks on unused providers.
