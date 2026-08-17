# 37-02 Summary — AgentDriver trait + Claude/OpenCode drivers

**Plan:** 37-02 (wave 2) — `AgentDriver` contract + `ClaudeDriver`/`OpenCodeDriver` with zero regression.
**Status:** complete — both tasks verified.

## What landed
- **`AgentDriver` trait** (`crates/devflow-core/src/agents/mod.rs`) with the full contract surface:
  `name`, `capabilities` (`DriverCapabilities`, `#[non_exhaustive]` + `Default`), `render_prompt`,
  `build_command`, `parse_completion`, `health`, `environment`, `sandbox_requirements`, `discover`,
  `test_contract`. The non-core methods carry sensible defaults (empty capabilities, `Ok` health,
  no-op discovery, empty conformance) so a driver implements only what it needs today.
- **`ClaudeDriver`** — owns the `stream-json` launch argv and the legacy `render_claude_style`
  prompt. **`OpenCodeDriver`** — owns the positional `opencode run <prompt>` argv + the same renderer.
  Both reproduce the pre-migration behavior byte-for-byte.
- **`DriverShim<D>`** — the compatibility shim: exposes an `AgentDriver` through the legacy
  `AgentAdapter` surface (`exec_command` → `build_command`, `extra_env` → `environment`,
  `preflight` → `health`, `render_prompt` → `render_prompt`). This is the D-11 removal point.
- **`adapter_for`** now routes Claude → `DriverShim(ClaudeDriver)`, OpenCode → `DriverShim(OpenCodeDriver)`,
  while Codex and Pi remain on `AgentAdapter` (37-03 migrates them).
- `ClaudeAgent`/`OpenCodeAgent` still exist (their `exec_command` now delegates to the drivers);
  `ClaudeAgent::exec_command_single_document` (the pre-31 legacy builder) is untouched.

## Verification
- `cargo test -p devflow-core --lib`: **625 passed, 0 failed** (added `drivers_reproduce_legacy_adapter_behavior`).
- `cargo test -p devflow --bin devflow`: **322 passed, 0 failed**.
- `cargo clippy -p devflow-core -p devflow --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.

## Zero-regression evidence
- `drivers_reproduce_legacy_adapter_behavior` asserts `ClaudeDriver.build_command` emits the
  `stream-json` argv and both drivers' `render_prompt` equals `render_claude_style` byte-for-byte.
- The pre-existing `claude_launches_headless_stream_json_without_positional_prompt` and
  `opencode_wraps_prompt_in_run` tests still pass through the shim (they now exercise the driver
  path), so the launch argv shapes are pinned.

## Not done here (37-03/37-04)
- Codex and Pi remain on `AgentAdapter` (37-03 migrates them to `AgentDriver`).
- `parse_completion`/`health`/`capabilities` are still default stubs for Claude/OpenCode — the
  conformance suite (37-04) will exercise them for real.
- `AgentAdapter` removal (D-11) is 37-04.
