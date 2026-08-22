> invocation: antigravity --model gemini-3.7-flash-high --print-timeout 15m --dangerously-skip-permissions -p "$(cat /tmp/p38-code-review-prompt.txt)" (MCP linear/vercel disabled during run, restored after)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-38 (feature/phase-38 @ 0286f4c)

### HIGH

#### 1. Incomplete TOML string escaping for DEL (`U+007F`) produces malformed TOML
- **Location**: [`crates/devflow-core/src/agents/codex.rs:113-114`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/agents/codex.rs#L113-L114)
- **Code**:
  ```rust
  c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
  c => out.push(c),
  ```
- **What breaks**: The TOML v1.0.0 specification forbids unescaped control characters in basic strings: `U+0000 to U+0008, U+000A to U+001F, U+007F`. `(c as u32) < 0x20` misses `0x7F` (DEL), causing it to fall through to `out.push(c)` as a raw byte; any path containing `\x7f` produces malformed TOML that standard parsers reject (`TOMLDecodeError: Illegal character '\x7f'`), crashing Codex sandbox launch.

#### 2. Inconsistent pre-start vs. runtime gating and develop-only lookup for `Stage::Plan`
- **Location**: [`crates/devflow-cli/src/commands.rs:302-310`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/commands.rs#L302-L310) vs [`crates/devflow-cli/src/preflight.rs:614-630`](file:///home/devflow/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/preflight.rs#L614-L630)
- **Code in `commands.rs:302-310`**:
  ```rust
  if driver.interactivity_mode(Stage::Plan) == agents::InteractivityMode::RequiresExistingArtifact
      && !phase_artifact_on_develop(project_root, phase, "-PLAN.md")
  {
      println!(
          "warning: phase {phase} has no PLAN.md on develop — headless {} \
           planning is untested and may need input; pre-writing plans is safer",
          driver.name()
      );
  }
  ```
- **Code in `preflight.rs:614-630`**:
  ```rust
  InteractivityMode::RequiresExistingArtifact => {
      if state.mode == Mode::Auto {
          let artifact = match state.stage {
              Stage::Define => "-CONTEXT.md",
              _ => "-PLAN.md",
          };
          if !phase_artifact_on_develop(project_root, state.phase, artifact) {
              return Err(format!(
                  "phase {} has no {artifact} on develop — {} cannot run the {} \
                   stage headlessly in auto mode",
                  state.phase,
                  driver.name(),
                  state.stage,
              ));
          }
      }
      Ok(())
  }
  ```
- **What breaks**: `devflow start` treats a missing `-PLAN.md` on `develop` as a non-blocking warning and launches Define, but as soon as the pipeline transitions from Define to Plan in Auto mode, `preflight_interactivity_check` fails with a blocking `Err`. Furthermore, in ordinary workflows where planning artifacts are generated during Define on the worktree branch (and not merged to `develop`), the gate unconditionally fails because it checks `develop` (`phase_artifact_on_develop`) instead of the worktree/feature branch.

---

### MEDIUM

#### 3. Wildcard stage-to-artifact mapping in `preflight_interactivity_check`
- **Location**: [`crates/devflow-cli/src/preflight.rs:616-620`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-cli/src/preflight.rs#L616-L620)
- **Code**:
  ```rust
  let artifact = match state.stage {
      Stage::Define => "-CONTEXT.md",
      _ => "-PLAN.md",
  };
  ```
- **What breaks**: Any future driver declaring `RequiresExistingArtifact` for stages other than `Stage::Define` (e.g. `Stage::Research` or `Stage::Verify`) will incorrectly probe for `-PLAN.md` on `develop` rather than the corresponding artifact or rejecting unsupported stage configurations.

#### 4. Lossy non-UTF-8 path replacement breaks sandbox writable-roots whitelist
- **Location**: [`crates/devflow-core/src/agents/codex.rs:50-54`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/crates/devflow-core/src/agents/codex.rs#L50-L54)
- **Code**:
  ```rust
  .map(|root| {
      let path = root.to_string_lossy();
      format!("\"{}\"", escape_toml_basic_string(&path))
  })
  ```
- **What breaks**: `root.to_string_lossy()` substitutes invalid UTF-8 bytes with `U+FFFD` (`\u{FFFD}`). The resulting TOML string is valid, but the whitelist path received by Codex's sandbox (`[0xEF, 0xBF, 0xBD]`) does not match the actual filesystem bytes on POSIX systems, causing the sandbox to deny write permissions when git operations access the directory. *(Speculation: Affects only non-UTF-8 repository paths on Unix systems).*

---

### LOW

#### 5. Stale `adapter_for()` and missing Pi entry in architecture documentation
- **Location**: [`docs/architecture/agent-model.md:24-29, 38`](file:///home/denniyahh/Github/devflow/.worktrees/phase-38/docs/architecture/agent-model.md#L24-L29)
- **Code**:
  ```markdown
  - `mod.rs` — `AgentDriver` trait definition + `adapter_for()` factory + conformance suite
  ```
- **What breaks**: Documentation refers to the deleted `adapter_for()` symbol instead of `driver_for()`, and the "Supported Agents" table at lines 24–29 omits Pi despite Pi driver implementation being active.

---

### VERIFICATION EVIDENCE & CONTROLS

- **`completion_signal_detected` removal**: Verified clean. Search across commit history confirms `completion_signal_detected` was an unused stub (`return false`) across all adapters and never consumed by `monitor`, `pipeline_launch`, or `agent_result`.
- **999.107 #1 Reorder**: Verified sound. `turn.failed` takes precedence over earlier `agent_message` markers (case a); `turn.completed` with marker yields marker verdict (case b); `turn.completed` without marker returns `None` for Layer 2 evaluation (case c); marker with no terminal returns marker (case d).
- **Relocated Claude Builders**: Verified that `ClaudeDriver::exec_command_single_document` and `ClaudeDriver::exec_resume_command` match the former `ClaudeAgent` methods byte-for-byte in argv ordering, flags (`-p`, `--output-format json`, `--dangerously-skip-permissions`, `--resume`), and arguments.
- **Negative Control on TOML DEL escape**: Tested `tomllib.loads("key = \"a\x7fb\"")` → fails with `TOMLDecodeError: Illegal character '\x7f'`. Tested `tomllib.loads("key = \"a\\u007Fb\"")` → succeeds with `{'key': 'a\x7fb'}`.

---

VERDICT: FIX-FIRST — Fix the unescaped `0x7F` TOML control character in `escape_toml_basic_string` and reconcile the Plan stage preflight check with worktree artifact lifecycles.
