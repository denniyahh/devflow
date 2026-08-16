Here are the real adversarial defects found in the Phase 37 implementation:

### 1. Claude CLI Argument Shadowing
**File:** `crates/devflow-core/src/agents/claude.rs`
**Location:** Lines 28-36

```rust
    fn exec_command(
        &self,
        _phase: PhaseId,
        _prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "claude",
            vec![
                "-p".into(),
                "--input-format".into(),
                "stream-json".into(),
// ...
```
**Defect:** The prompt text is intentionally omitted from the arguments because `stream-json` takes it from `stdin`, but the `-p` flag itself was left in the vector. Since `-p` requires a value, the `claude` CLI will consume the immediately following argument (`--input-format`) as the value for `-p`. This means the agent's input format is never actually configured to `stream-json`, and the subsequent `stream-json` string is left dangling as a positional argument, breaking the launch. The unit test only passes because it relies on `args.windows(2).any(...)` which finds the sequence `["--input-format", "stream-json"]` without considering that `-p` preceding them alters the parsing.

### 2. Concurrency Race Clobbering `_auto_chain_active` Guard
**File:** `crates/devflow-cli/src/pipeline_launch.rs`
**Location:** Lines 876-903

```rust
    let _auto_chain_guard = match workflow::load_state(project_root, phase) {
        // ...
    };

    monitor::run_pipe_owning_monitor(/* ... */)?;

    advance(project_root, Some(phase))
}
```
**Defect:** `_auto_chain_guard` is held across the call to `advance()`. `advance()` evaluates the current stage, transitions the pipeline, and calls `launch_stage` which asynchronously spawns the *next* stage's monitor process. Only after `launch_stage` and `advance` return does the first stage's `run_monitor` scope end, dropping `_auto_chain_guard` and writing `false` to `gsd_config.json`. If the newly spawned monitor (for the next stage) reads or creates its own `AutoChainGuard` (e.g. setting it to `true` for `Code`) before the first stage's `run_monitor` finishes, the first stage's drop will blindly overwrite `gsd_config.json` back to `false` *after* the new stage has started, silently breaking auto-chaining for the new stage.

### 3. API Rate Limit JSON Suppression for Codex
**File:** `crates/devflow-core/src/agent_result.rs`
**Location:** Lines 228-234

```rust
    let stdout: String = stdout
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(strip_corruption_padding(line))
                .map(|v| !v.is_object())
                .unwrap_or(true)
        })
        .collect::<Vec<_>>()
// ...
```
**Defect:** `detect_codex_rate_limit` filters out any line that parses as a valid JSON object to avoid false positives from echoed text inside `item.completed` events. However, if Codex fails to start an event stream and instead outputs a top-level JSON API error (e.g., `{"error": {"message": "Rate limit exceeded. Try again at 17:00."}}`), `is_codex_event_stream()` (line 744) will return `false` because there is no `thread.started` or `turn.*` event. This causes `parse_codex_event_result` to ignore it entirely. The output then falls back to `detect_codex_rate_limit`, which filters the JSON error out completely because it parses as a valid JSON object (`!v.is_object()` is false). The true rate limit error is dropped, and DevFlow fails to initiate auto-resume.
