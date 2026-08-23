# 37-01 Summary — StageIntent + driver-owned prompt rendering (tracer)

**Plan:** 37-01 (tracer, wave 1) — `StageIntent` enum + per-driver `render_prompt` for all five stages + `fix_prompt`.
**Status:** complete — both tasks verified.

## What landed
- `StageIntent` enum in `prompt.rs` — `Define`/`Plan`/`Code { fix }`/`Validate`/`Ship { review_angles }`,
  carrying NO agent-specific syntax. `Stage::gsd_command()` is retained for the human-facing dry-run
  preview (`pipeline_gate.rs` `print_dry_run`) but no longer drives any agent prompt.
- `render_prompt(intent)` added to the `AgentAdapter` trait; `render_claude_style(intent)` in `prompt.rs`
  is the byte-identical legacy renderer.
  - **Claude / OpenCode / Pi** → `render_claude_style` (byte-for-byte identical to pre-migration text;
    Pi's native migration is 37-03).
  - **Codex** → a Codex-native instruction: references the workflow file path
    (`$HOME/.codex/gsd-core/workflows/<stage>.md`), carries the `--auto` token on Code, states the
    completion protocol, and contains no GSD slash command.
- The prompt API now routes through the adapter: `pipeline_launch.rs` (2 sites) and `pipeline_gate.rs`
  (the Code↔Validate loop-back) resolve `adapter_for(state.agent).render_prompt(&intent)`.
- `every_adapter_receives_identical_prompt_text` is **deleted** — replaced by
  `claude_and_opencode_stay_identical_but_codex_renders_native`.

## Verification
- `cargo test -p devflow-core --lib`: **624 passed, 0 failed**.
- `cargo test -p devflow --bin devflow`: **322 passed, 0 failed**.
- `cargo clippy -p devflow-core -p devflow --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.

## The dogfood bug is now closed for every stage
Before this plan, Codex received the literal `/gsd-execute-phase {N}` slash command at Code (and
`/gsd-plan-phase` / `/gsd-validate-phase` / `/gsd-ship` / `/gsd-audit-fix` at the other stages and
loop-backs). The new Codex renderer references the workflow file instead — no slash command at any
stage. The negative control asserts none of the seven slash-command names appear in the Codex render;
the positive oracle asserts the workflow path + `--auto` + `DEVFLOW_RESULT` are present.

## Note on the Codex-native text
The Codex instruction is a first-draft renderer (the plan's P-02 leaves the exact wording to the
executor). It references the workflow files but does not yet carry the full per-stage structure that
Claude's renderer does (the Ship critical-gate and the Plan idempotency contract are not restated in
Codex-native form). That is a 37-02/37-03 refinement, not a tracer gap — the tracer's bar is "no slash
command + workflow reference + completion contract", which is met.
