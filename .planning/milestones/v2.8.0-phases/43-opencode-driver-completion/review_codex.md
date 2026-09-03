## Needs my decision

Recommendation: reject/retry this revision before acceptance. I found two High-severity correctness failures. No Critical finding.

## Needs my awareness

1. **High — an early success marker can silently advance an unfinished run.**

   [agent_result.rs:961](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-core/src/agent_result.rs:961) scans backward for the last *text event containing a marker*, not the final text event or final step.

   Failure input:

   ```text
   step_start
   text: DEVFLOW_RESULT success
   step_finish: tool-calls
   step_start
   tool_use
   text: "I could not complete the task"
   step_finish: stop
   ```

   The parser skips the later marker-less text and returns the earlier success at [agent_result.rs:969-973](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-core/src/agent_result.rs:969). `evaluate_layer1` then accepts it at [agent_result.rs:2261-2263](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-core/src/agent_result.rs:2261).

   The existing test only covers success-marker versus success-marker, not success-marker followed by a marker-less continuation.

2. **High — the health probe is not actually fail-closed.**

   [opencode.rs:85-92](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-core/src/agents/opencode.rs:85) never checks `output.status.success()`. A failed command returning stdout containing `└ 1 credentials` is reported healthy.

   Separately, [opencode.rs:145-155](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-core/src/agents/opencode.rs:145) accepts any line beginning with `1 credentials` or `1 environment variables`; it does not require the expected footer shape. Even malformed output such as:

   ```text
   1 credentials
   probe failed
   ```

   returns a positive count.

   This directly violates the stated fail-closed contract and is not covered by the tests, which only exercise exit-zero fixtures.

3. **Medium — the capability parser can turn malformed/noisy output into `true`.**

   [opencode.rs:197-200](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-core/src/agents/opencode.rs:197) uses an unrestricted substring search.

   Any successful command output containing `"(subagent)"` or `"(all)"`—for example `warning: fallback (subagent)`—sets `subagent_dispatch: true`. A primary agent whose name contains that text can do the same. This contradicts the comment that unparseable output fails closed.

   The official command currently emits `<name> (<mode>)` followed by permission JSON, as shown in [agent.ts](https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/cli/cmd/agent.ts), so this should parse the header structure rather than search arbitrary lines.

4. **Medium — legitimate credentialless OpenCode configurations are blocked.**

   [opencode.rs:89-92](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-core/src/agents/opencode.rs:89) treats zero listed credentials/environment variables as unusable. That blocks supported configurations such as local models and custom/config-only providers. OpenCode documents local-model support and custom providers; its provider-list implementation only reports `auth.json` entries and detected environment variables ([providers.ts](https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/cli/cmd/providers.ts), [provider docs](https://opencode.ai/v2/docs/providers/)).

   The failure blocks launch because `driver.health(state)` is part of preflight at [preflight.rs:1285-1287](/var/home/denniyahh/Github/devflow/.worktrees/phase-43/crates/devflow-cli/src/preflight.rs:1285).

## Handled

- **JSONL detector/dispatch integration:** OpenCode’s `step_*`/nested-error gate is distinct from Claude and Codex gates, and the parser is wired into `evaluate_layer1` in the correct position for normal JSONL captures.
- **Error precedence:** I found no concrete current-stream case where a recoverable retry emits a terminal `error` event and then succeeds. The current emitter’s JSON event/error handling is documented in [run.ts](https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/cli/cmd/run.ts). The “error anywhere” rule is conservative, though future emitter changes could make it over-fail.
- **Command injection:** probe arguments use `Command::new(...).args(...)`; launch arguments are passed through `"$@"` in the monitor wrapper, not interpolated into a shell command.
- **Health error leakage:** the returned health errors do not include raw provider output, provider names, auth paths, or environment-variable names.

## Detail

- `cargo test -p devflow-core --lib` passed: **718 tests**.
- The broader package test command was not usable in this checkout because integration tests fail to compile with the pre-existing `test_support` feature-gating issue. Passing tests do not cover the four adversarial inputs above.
