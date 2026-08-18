> invocation: claude -p --model opus --settings {"effortLevel":"high"} --output-format json --dangerously-skip-permissions "$(cat /tmp/p38-code-review-prompt.txt)" (cd .worktrees/phase-38)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-38 (feature/phase-38 @ 0286f4c)
> model resolved: claude-opus-5 (verified green: cargo test --workspace + clippy -D warnings)

Verified against the working tree: `cargo test --workspace` is green (exit 0, all suites 0 failed) and `cargo clippy --workspace --all-targets -- -D warnings` is clean. Both facts are compatible with every finding below — the suite is green *over* the behavior changes because the phase's own tests were rewritten to assert the new behavior.

---

## HIGH

**1. The new Plan gate makes `commands.rs`'s pre-start check structurally unable to do its job.**

`crates/devflow-cli/src/commands.rs:302-310` — Plan is still only a `println!`:
```rust
if driver.interactivity_mode(Stage::Plan) == agents::InteractivityMode::RequiresExistingArtifact
    && !phase_artifact_on_develop(project_root, phase, "-PLAN.md")
{
    println!("warning: phase {phase} has no PLAN.md on develop — headless {} …");
```
…while `crates/devflow-cli/src/preflight.rs:614-628` turned the same condition into a hard preflight gate. **What breaks:** `devflow start --phase N --agent codex --mode auto` on a phase that has CONTEXT.md but no PLAN.md on develop prints a warning, forks a worktree, burns the Define stage, then wedges on a never-silent preflight gate at Plan — which is verbatim the outcome the pre-start check's own comment at `commands.rs:286-287` says it exists to prevent ("Fail in one second with instructions instead of after a burned agent run and a dead-end gate"). D-03 decided to extend the gate to Plan; it did not decide to leave the fast-fail leg at warn.

**2. The Plan gate checks `develop` for an artifact the Plan stage *produces*, so nothing the run does can ever clear it.**

`crates/devflow-cli/src/preflight.rs:616-628`:
```rust
let artifact = match state.stage { Stage::Define => "-CONTEXT.md", _ => "-PLAN.md" };
if !phase_artifact_on_develop(project_root, state.phase, artifact) {
```
Define's artifact is a genuine precondition; Plan's is an output. **What breaks:** an operator who clears the Plan gate with `Advance` (which skips the check via `launch_stage_inner`, `preflight.rs:1325`) gets a successful Plan that commits PLAN.md **to the feature branch**. Any subsequent Plan-stage relaunch — `devflow resume` (`pipeline_launch.rs:1193` → `launch_stage` → `run_preflight`) or a gate `LoopBack` (`preflight.rs:1331`) — re-reads `develop`, still finds nothing, and re-gates. That burns `preflight_retries` to `MAX_PREFLIGHT_RETRIES` and then `abort()`s the phase (`preflight.rs:1289-1306`), with the reason naming an artifact that demonstrably exists.

## MEDIUM

**3. `escape_toml_basic_string` misses U+007F — the emitted `-c` value is unparseable TOML.**

`crates/devflow-core/src/agents/codex.rs:112`:
```rust
c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
```
TOML requires escaping `"`, `\`, **and** the control characters U+0000–U+0008, U+000A–U+001F, **U+007F**. I confirmed this against the `toml` crate the workspace already depends on: a root of `/repo/a\u{7f}b` yields `TOML parse error at line 1, column 49: invalid basic string`. Negative control: plain, quote+backslash+newline, tab, NUL, U+0080, U+009F and U+FFFD all parse and round-trip exactly — so the parser is not just rejecting everything. **What breaks:** the exact 999.107 #2 defect (a hostile path corrupting `sandbox_workspace_write.writable_roots`) survives for one character class. Low real-world likelihood, same as the newline the phase did fix.

**4. The non-UTF-8 leg of 999.107 #2 is not fixed — and the new test codifies the broken behavior as correct.**

`crates/devflow-core/src/agents/codex.rs:51`: `let path = root.to_string_lossy();`, asserted at `codex.rs:158-170`:
```rust
assert!(flag.contains('\u{FFFD}'), "lossy replacement expected: {flag:?}");
```
**What breaks:** a repo under a non-UTF-8 path gets a `writable_roots` entry naming a *different, nonexistent* directory. Codex's sandbox then denies the linked-worktree git-metadata write and the 13-06 failure ("Code stage implemented and tested, then could not commit") returns with no diagnostic pointing at the cause. TOML is UTF-8-only, so there is no correct escaping here — the honest fix is to refuse the launch, not to lossily convert. The CONTEXT names this leg explicitly ("a non-UTF-8 path becomes `�`"); the phase tested it rather than fixing it.

**5. The `other` arm refuses regardless of mode, unlike the arm above it.**

`crates/devflow-cli/src/preflight.rs:632-637`:
```rust
other => Err(format!("{} declares {} as {:?} — that stage cannot run headless", …)),
```
`RequiresExistingArtifact` is correctly scoped to `if state.mode == Mode::Auto` (`:615`); `RequiresTypedSubagents`/`InteractiveOnly` are not. **What breaks:** the first driver to declare either one blocks Supervise-mode runs too, where an operator *is* present at the gates. Latent — no driver returns those variants today, so this is unreachable at HEAD.

**6. `_ => "-PLAN.md"` silently answers for stages that have no plan artifact.**

`crates/devflow-cli/src/preflight.rs:618`: `_ => "-PLAN.md",`. **What breaks:** a future driver declaring `RequiresExistingArtifact` for Code/Validate/Ship gets its launch gated on a PLAN.md, with a reason string naming the wrong artifact. Latent, same reachability caveat as #5. An exhaustive `match` with an explicit arm per stage would fail to compile instead.

**7. The reorder inverts a precedence a shipped test comment declares "by design", and the comment was left contradicting the code.**

`crates/devflow-core/src/agent_result.rs:4151-4153` still reads:
```
/// The resurrection shape here is a torn SUPERSEDING marker: codex verdict
/// precedence is marker-over-`turn.failed` by design (13-06 dogfood
/// finding), and last-marker-wins
```
The new terminal-first block at `agent_result.rs:758-780` makes that false. **What breaks:** nothing at runtime — that test's fixture has no terminal event, so it still passes for a different reason than its comment states. But the 13-06 rationale for the old precedence was never addressed, and the next reader gets two contradictory statements of the same rule. Also note the new path returns `commits: None, summary: None, verdict: None`, discarding the agent's own self-report whenever `turn.failed` is present — the failure direction is safe, but the gate context is thinner than before.

**8. Old-style bare `PLAN.md` phases now hard-block instead of spuriously warning.**

`crates/devflow-cli/src/commands.rs:106-109`:
```rust
path.strip_prefix(&prefix).is_some_and(|rest| rest.contains('/') && rest.ends_with(suffix))
```
`rest.ends_with("-PLAN.md")` never matches `.planning/phases/NN-name/PLAN.md` — a shape this repo's own CLAUDE.md records as live ("Old phases use a bare `PLAN.md`, not `NN-PLAN.md`"). **What breaks:** a Codex auto run against such a phase previously ate a false warning; it now wedges on a preflight gate claiming the plan is absent when it is present.

## LOW

**9.** `crates/devflow-cli/src/preflight.rs:599` — a truncated leftover doc line survived the edit: `/// D-14 (universal, generic layer): a headless/auto Codex run cannot pass` sits immediately above `/// The driver-driven interactivity gate (999.106): …`. Mid-sentence garbage in the doc for the function the phase rewrote.

**10.** `crates/devflow-core/src/agents/claude.rs:25-43` — `ClaudeDriver::build_command` is now the sole definition of the most regression-sensitive argv in the repo and carries **no doc comment**. The ~30 lines deleted with `ClaudeAgent::exec_command` (the Phase-30 `DEFAULT_CLI_ARGV` provenance, "the prompt is deliberately absent from the returned argv", and "`--verbose` is load-bearing, not decoration … Do not 'clean it up'") were not relocated. Doc-only: the flag itself and its guard test survive at `claude.rs:39` and `claude.rs:148-154`.

**11.** `docs/architecture/agent-model.md:38` still names a symbol this phase deleted: `` - `mod.rs` — `AgentDriver` trait definition + `adapter_for()` factory ``. That file was not touched by the phase (only ARCHITECTURE.md, CONTRIBUTING.md, docs/guides/adding-agent.md were). Adjacent: `ARCHITECTURE.md:96` still says three `AgentKind`s one line above the phase's own edit listing four drivers, and `agent-model.md:22-27`'s agent table omits Pi.

---

## Checked and clean (no finding)

- **`completion_signal_detected` was already dead.** At `ff0e0d6` every match is an impl or a planning doc — zero production callers, including the monitor/`agent_result` path. Its disappearance is dead-code removal, not silent behavior drift.
- **`extra_env`→`environment` and `preflight`→`health` are 1:1** at every migrated site; `run_preflight`'s single call became `driver.health(state)` with the same `Result<(), String>` aggregation. The test doubles' overrides are behavior-identical.
- **The relocated Claude builders are byte-identical.** I diffed both bodies against `ff0e0d6`: `exec_command_single_document` and `exec_resume_command` are unchanged line-for-line, argv order and flags included.
- **`driver_for` is exhaustive** over all four `AgentKind` variants with no wildcard, so a fifth variant is a compile error.
- **All four 999.107 #1 cases hold and are test-pinned:** `turn.failed`+marker→Failed (`agent_result.rs:4513`), `turn.completed`+success marker→Success (`:4497`), `turn.completed`+no marker→`None` (`:4467`), failure marker+`turn.completed`→marker (`:4482`). The no-terminal-at-all+marker case behaves identically to before but has no dedicated test.
- **No stage other than Codex's Plan changes behavior today.** Codex declares `HeadlessSafe` for Code/Validate/Ship; Claude/OpenCode/Pi all take the trait default. Claude's zero-regression baseline holds.

**What this review does not establish:** I ran no Codex agent, so findings 1, 2 and 8 are read off the control flow and the gate/retry machinery, not observed live. The TOML claim in #3 is proven against the `toml` crate as a proxy for codex's own `-c` parser, not against codex itself. Findings 5 and 6 are unreachable at HEAD by construction.

VERDICT: FIX-FIRST — the D-03 Plan gate ships as a half-wired behavior change whose fast-fail leg still only warns and whose artifact check reads a branch the artifact can never be on, and 999.107 #2's non-UTF-8 leg is tested rather than fixed.
