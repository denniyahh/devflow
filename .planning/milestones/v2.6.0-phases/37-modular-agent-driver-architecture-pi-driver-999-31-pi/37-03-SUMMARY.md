# 37-03 Summary — CodexDriver + PiDriver (all four agents on the contract)

**Plan:** 37-03 (wave 3) — migrate Codex and Pi onto `AgentDriver`; all four agents now resolve through it.
**Status:** complete — both tasks verified.

## What landed
- **`CodexDriver`** (`crates/devflow-core/src/agents/codex.rs`):
  - `build_command` == the prior argv **plus the verified non-interactive approval flag** — `-a never`
    placed BEFORE `exec` (the global form; spawn-tested against the installed CLI: `codex exec -a never`
    is rejected, `codex -a never exec` is accepted).
  - `parse_completion` == `parse_codex_event_result` (now `pub(crate)`), relocating the JSONL parsing
    under driver ownership without moving the fixture-heavy body out of `agent_result.rs`.
  - `environment` == the `GIT_CONFIG_*` signing-disable env.
  - `render_prompt` == `render_workflow_style` (extracted from the 37-01 Codex renderer into `prompt.rs`).
- **`PiDriver`** (`crates/devflow-core/src/agents/pi.rs`):
  - `build_command` == `pi -p --no-approve <prompt>` (Phase-36 argv, byte-identical).
  - `health` == the `pi auth check` predicate (reuses `classify_auth_check`).
  - `render_prompt` == `render_workflow_style` (the de-Claude-ified prompt; **no JSON unwrapper** — 37.1/38).
- **`render_workflow_style(intent, agent_label)`** added to `prompt.rs` — the shared workflow-reference
  renderer for agents that can't receive `/gsd-*`; fixes the `AuditFix` arm to point at `audit-fix.md`.
- `adapter_for` now routes **all four** agents through `DriverShim` (Claude/Codex/OpenCode/Pi →
  their drivers). `CodexAgent`/`PiAgent` remain as thin legacy delegates (D-11 removal point).

## Verification
- `cargo test -p devflow-core --lib`: **626 passed, 0 failed** (added `codex_and_pi_drivers_reproduce_legacy_behavior`).
- `cargo test -p devflow --bin devflow`: **322 passed, 0 failed**.
- `cargo clippy -p devflow-core -p devflow --all-targets -- -D warnings`: clean; `cargo fmt --check`: clean.

## Note
`multi_agent_v2` was NOT force-enabled (adversarial review: it is already `stable true` on the
installed CLI); pinning its typed-subagent tool schema in tests is folded into 37-04's conformance
suite rather than shipped as a dead flag.
