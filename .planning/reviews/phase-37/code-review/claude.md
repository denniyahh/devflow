Read every changed file, ran the suite (628 pass in `devflow-core`, matching the SUMMARY), and verified the Pi/Codex CLI surfaces against the installed binaries. The shim is fine; the new Codex/Pi renderer is where the damage is.

## Needs your decision

**The phase ships four stage-contract regressions for Codex and Pi.** All four come from one function, `render_workflow_style` (`crates/devflow-core/src/prompt.rs:380-404`), which replaced per-stage prompts with a single generic template. Fix in a 37.x follow-up, or revert Codex/Pi to `render_claude_style` until the per-stage contracts are ported? My recommendation: port the contracts — reverting re-breaks the `/gsd-*` problem the phase exists to solve.

---

## Findings

### 1. Codex/Pi can never pass Validate — deterministic, not flaky

`prompt.rs:394` renders Validate as the generic template, whose only completion contract is `COMPLETION_PROTOCOL` (`prompt.rs:43-55`) — `status` and `reason`, **no `verdict` field**. The legacy `validate_stage_prompt` (`prompt.rs:220-246`) demanded `"verdict": "pass"|"gaps"` precisely because DevFlow refuses to advance on a bare `status: success`.

`crates/devflow-cli/src/pipeline_outcomes.rs:235-245`:
```
(_, AgentStatus::Success, Some(Verdict::Pass)) => ValidateOutcome::Passed,
(true,  AgentStatus::Success, None) => ValidateOutcome::Ambiguous(...),
(false, AgentStatus::Success, Some(Verdict::Gaps) | None) => ValidateOutcome::Failed,
```
`Passed` requires `Some(Verdict::Pass)`. A Codex or Pi agent that validates perfectly emits `{"status":"success"}`, lands in `Failed` (or `Ambiguous` if an external probe ran) and loops back to Code — every time, until the consecutive-failure counter gates. Pre-Phase-37, Codex got the verdict-demanding prompt. This is a clean regression, and no test covers Validate rendering for these drivers.

### 2. Define now tells Codex/Pi to run the interview D-14 exists to forbid

`prompt.rs:382`: `StageIntent::Define { phase } => ("discuss-phase.md", ...)`.

`define_stage_prompt` (`prompt.rs:298-311`) says the opposite in as many words: *"There is no agent work to perform here… you must NOT run an interactive discuss-phase or interview command… Do NOT modify any existing planning artifacts."* The new render says "Read … `discuss-phase.md` and follow it for phase N." Codex's exec mode hits `request_user_input is unavailable`; worse, when CONTEXT.md already exists (the only case that gets past `preflight_interactivity_check`), the agent is now instructed to re-run the interview over it.

The negative control at `agents/mod.rs:432-445` only forbids the literal string `/gsd-discuss-phase`. Pointing at the same workflow's *file* sails through it.

### 3. Plan's idempotency guard dropped — for the exact agent it was written for

`prompt.rs:383` renders Plan as plain "follow `plan-phase.md`". `idempotent_stage_prompt` (`prompt.rs:265-286`) exists because GSD's plan-phase demands an interactive Overwrite/Append/Cancel when PLAN.md exists and *"headless Codex can never answer it — the stage would fail on every retry, forever."* That's the 13-06 Codex dogfood finding, now removed from Codex.

### 4. Ship: the Critical-severity gate and `review_angles` are silently gone for Codex/Pi

`prompt.rs:395`: `StageIntent::Ship { phase, .. }` — `review_angles` is destructured away. The workflow render loses all three of `ship_stage_prompt`'s contracts: run `/gsd-code-review` first, **do not ship if REVIEW.md has any Critical finding**, and prefix the failure reason with `review:` — the string `is_review_rejection` (`pipeline_outcomes.rs:962`) matches to loop back to Code.

Concrete: a Codex/Pi Ship runs `ship.md` directly, so the merge/version-bump/tag/publish hooks fire with no review gate in front of them. Separately, `for_stage_in_project` still reads `config::review_angles` at `prompt.rs:114-121` and throws the result away for these agents — a config key that silently does nothing.

### 5. Pi is pointed at Codex's install directory

`prompt.rs:399` hardcodes `$HOME/.codex/gsd-core/workflows/{workflow}`, and `pi.rs:34` sends Pi through it. Verified on this machine: Pi's workflows are at `~/.pi/agent/gsd-core/workflows/` (all six filenames present). It only "works" here because Codex happens to be installed. On a Pi-only host every Pi stage names a path that doesn't exist.

### 6. The interactivity/health/capability machinery is entirely dead

`interactivity_mode`, `health_classification`, `DriverHealth`, `capabilities`, `sandbox_requirements`, `discover`, and `parse_completion` have no caller outside `agents/mod.rs` and its tests. The hardcoded checks they were meant to replace are untouched at `preflight.rs:613` (`state.agent == AgentKind::Codex && … Stage::Define`) and `commands.rs:289`.

`37-04-PLAN.md:190` lists *"preflight consumes `InteractivityMode` generically (no hardcoded Codex-Define match)"* as a completion criterion; 37-04-SUMMARY defers it. That's an unmet acceptance criterion, not just a deferral — and `PiDriver` declares no `interactivity_mode` at all, so Pi gets neither the new mechanism nor the old Codex-only guard.

Same shape at `codex.rs:70-72`: 37-03-SUMMARY says `parse_completion` "relocat[es] the JSONL parsing under driver ownership," but the live caller is still `agent_result.rs:1840` calling the free function. `is_codex_event_stream` was widened to `pub(crate)` (`agent_result.rs:712`) with no cross-module caller whatsoever.

### 7. `test_contract()` is mostly unfalsifiable

`contract_checks` (`agents/mod.rs:193-218`):
- `"name is non-empty"` — every impl returns a `&'static str` literal. Cannot fail.
- `"build_command names a program"` — same, and `_args` is discarded, so the one thing that actually varies between drivers (argv) is never checked.
- The five stage checks all carry the identical `name`, so a failure message can't say which stage broke.
- The only real predicate is `contains("DEVFLOW_RESULT")`, and both renderers append `COMPLETION_PROTOCOL` unconditionally — so a driver rendering `"do nothing"` + the protocol passes the full suite. Finding #1 is the proof: the suite is green on a Validate prompt that structurally cannot succeed.
- `every_driver_passes_the_conformance_suite` (`mod.rs:358`) has no negative control — nothing deliberately broken to show the suite can fail.

Separately, `mod.rs:450`'s `assert!(codex.contains("--auto"))` is vacuous: the boilerplate at `prompt.rs:401` ("The `--auto` flag, where present…") contains that literal for **every** intent, including Define/Validate/Ship where no `--auto` reaches `args`.

### 8. `PiDriver::health` can block preflight indefinitely, and probes a hardcoded provider

`pi.rs:52-55` calls `.output()` with no timeout. Per `pi auth --help`, "Checks refresh expired OAuth credentials by default; `--no-refresh` prevents this" — so a stalled token refresh hangs preflight before any monitor exists to sweep it. `--no-refresh` is not passed.

`--provider google` is hardcoded while Pi accepts `--provider`/`--model`. An operator running Pi on anthropic or openai gets a false `"no provider credential resolves"` and cannot launch.

---

## Verified as sound (not padding — these were the hunt targets)

- **DriverShim zero-regression for Claude/OpenCode: holds.** `exec_command`→`build_command`, `extra_env`→`environment`, `render_prompt`→`render_claude_style`, `completion_signal_detected`→`false` all match `develop`'s adapters exactly, and `preflight`→`health` is `Ok(())` for both. The regressions above are in the Codex/Pi *renderer*, not the shim.
- **Slash-command elimination for Codex/Pi: complete.** All three prompt call sites are rewired (`pipeline_launch.rs:91`, `:1084`, `pipeline_gate.rs:198-205`); the only remaining `prompt_override` producer is `checkpoint_auto_decide_prompt`, which is Claude-resume-only. No path emits `/gsd-*` to Codex or Pi.
- **Flag construction: correct.** Verified against installed binaries — `codex --help` confirms `-a/--ask-for-approval` is global (so `["-a","never","exec",…]` is right), and `pi --help` confirms `-p`/`--no-approve`. `pi auth check --json --provider google` returns exactly `{"status":"not_ready",...}` with exit 1, which `classify_auth_check` handles correctly.
- **Security: nothing exploitable found.** All spawns use `Command::args` — no shell, so no injection. Codex's TOML `writable_roots` escaping (`codex.rs:50-55`) handles both `\` and `"`. `classify_auth_check` discards stdout from its error string, so credentials don't reach logs. One latent item only: `pi.rs:45` passes the prompt raw with no `--` guard (Pi rejects `--`), so a prompt beginning with `-` would parse as a flag — not reachable today since all prompts are internally generated and start with letters.

## Still open
The decision in the first section. No other items.
