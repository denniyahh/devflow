> invocation: claude -p --model opus --settings '{"effortLevel":"high"}' --dangerously-skip-permissions "<prompt>" (adversarial lens + 7 planning docs + 999.31 context inlined)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-37 (feature/phase-37)

I read the source. Seven of these are verified against the live tree and the installed CLIs; two are labeled speculative.

## 1. `--ask-for-approval never` is not a valid `codex exec` flag — the phase mandates an argv that cannot launch

37-03-PLAN.md must_haves: *"`capabilities`/`environment` capture the signing-disable env + `--ask-for-approval never` (31b)"*; T-37-08 mitigation: *"argv-shape test asserts the flag."* VALIDATION.md row 37-03-01 Secure Behavior: `--ask-for-approval never`.

Verified against codex-cli 0.147.0, with a positive control:

```
codex exec --sandbox workspace-write --help   → exit 0        (control: flag parsing works)
codex exec --ask-for-approval never --help    → error: unexpected argument '--ask-for-approval' found
codex exec -a never --help                    → error: unexpected argument '-a' found
```

`-a/--ask-for-approval` exists on top-level `codex` (the TUI), not on `codex exec`. The 999.31 audit's own evidence — *"`codex doctor` reported effective policy `OnRequest`"* — was a `doctor` reading, never a check that `exec` accepts the flag.

The failure mode is the one CLAUDE.md already names: an argv-shape unit test asserting `args.contains("--ask-for-approval")` passes green forever while every real Codex launch dies at argv parse. There is no acceptance criterion anywhere in the phase that spawns `codex`. The correct override is `-c approval_policy=never` or the existing `-c` mechanism, but nothing in CONTEXT, RESEARCH, or the plans considers that this needs checking.

## 2. "Codex is no longer broken" is false by the phase's own scoping — 4 of 5 stages still emit `/gsd-*`

37-03-PLAN.md truth: *"Codex is no longer broken: a Codex run's prompt carries no `/gsd-*` string (37-01)."* 37-01's P-01 explicitly scopes to *"the Code stage only"* and its behavior says *"Keep the other stages' rendering unchanged."*

Verified in `crates/devflow-core/src/prompt.rs` — every other stage bakes a slash command through the same shared path:

- `:147` `gsd_command_for(Stage::Validate, phase)` → `/gsd-validate-phase {N}`
- `:193` `gsd_command_for(Stage::Plan, phase)` → `/gsd-plan-phase {N}`
- `:105` `ship_stage_prompt` → `/gsd-code-review {N}` **and** `/gsd-ship {N}`
- `:353-359` `fix_prompt` → `/gsd-audit-fix {N}`, `/gsd-execute-phase {N} --gaps-only --auto`

A Codex run under this phase still receives literal slash commands at Plan, Validate, Ship, and on every Code↔Validate loop-back. `fix_prompt` is the worst of these: 37-RESEARCH.md flags it twice as a must-migrate pitfall (`:37-38`, `:91-92`) and **no task in any of the four plans touches it**. It appears only in 37-01 Task 1's `read_first` list, which is not an obligation.

The phase can honestly claim "the Code-stage dogfood defect is fixed." It cannot claim what 37-03 claims.

## 3. 37-03 declares `depends_on: [01]` but cannot compile without 37-02

Both plans are `wave: 2`. 37-03's first must-have is *"`CodexDriver` implements the full `AgentDriver` contract"* — a trait that 37-02 Task 1 creates. Both plans also list `crates/devflow-core/src/agents/mod.rs` in `files_modified`, and both write `adapter_for` (37-02 P-02 installs the shim; 37-03 Task 2 rewrites it to route all four).

Under wave-parallel execution this is a hard build break plus a write collision on one file. Either 37-03 becomes `depends_on: [01, 02]` at `wave: 3` (pushing 37-04 to wave 4), or the trait definition moves into 37-01.

## 4. The Codex negative control passes before any work is done

37-01 `<verify>`: *"`grep -n '/gsd-' crates/devflow-core/src/agents/codex.rs` returns nothing."*

Verified on the current tree, unmodified: exit 1, zero matches. `codex.rs` has never contained a `/gsd-` string — the literal lives in `stage.rs:60-68`. The check is satisfied by doing nothing.

Worse, it has no positive counterpart. Nothing in CONTEXT.md, RESEARCH.md, or any plan says what the "Codex-native instruction" must *contain*. 37-01's `<output>` defers it: *"Record: … the Codex native text"* — the executor invents it at runtime and it is never reviewed. A renderer returning `"do the work"` satisfies every acceptance criterion in the phase.

This matters more than it looks: `/gsd-execute-phase` is a workflow definition, not a sentence. Whatever replaces it must reference `$HOME/.codex/gsd-core/workflows/`, and it must carry the `--auto` token that `prompt.rs:40` and `:290` exist to preserve (35.1-01's `workflow._auto_chain_active` fix). No plan states how the de-Claude-ified Codex render keeps that contract, or whether it needs to.

## 5. `multi_agent_v2` is already enabled by default — the 31b work item is a no-op against the installed CLI

37-03 P-03: *"`--enable multi_agent_v2` (gated on `codex features list`) land."* Sourced from the 999.31 audit's *"confirmed disabled by default in a clean `CODEX_HOME`"* (2026-07-24), repeated by 37-RESEARCH.md `:50` on 2026-08-15 without re-checking.

Verified on codex-cli 0.147.0:

```
multi_agent          stable   true
multi_agent_v2       stable   true
multi_agent_mode     removed  false
```

Stable and effective-true. The 999.31 risk entry — *"Enabling `multi_agent_v2` explicitly can change Codex tool schema shape; pin expected typed-subagent behavior in tests"* — is carried into no plan's threat model. So the phase would spend budget adding a flag that changes nothing, while its one named risk stays unmitigated. (`--enable` *is* valid on `codex exec`, so this fails silently rather than loudly — unlike finding 1.)

## 6. CONTEXT.md's canonical line citation points at the Claude parser, not the Codex one

37-CONTEXT.md `:72` and `:89` both state `crates/devflow-core/src/agent_result.rs:361-453` is *"Codex JSONL completion parsing"* and *"the seed of `CodexDriver::parse_completion`."*

Verified: `:363` is `pub fn claude_stream_session_id`. The Codex functions are at `:712` (`is_codex_event_stream`) and `:740` (`parse_codex_event_result`).

37-RESEARCH.md `:97-98` warns the numbers shifted and says to locate by symbol — but CONTEXT.md is the document marked **"Downstream agents MUST read these before planning or implementing,"** and it states the stale range as fact without the caveat. An executor that jumps to 361-453 relocates `claude_stream_session_id` — the single most regression-sensitive function in a phase whose top priority is Claude zero-regression.

## 7. "Claude → PipeOwning, never Legacy" contradicts a shipped feature

37-02 must-have and T-37-05: *"Claude routed to `Legacy` instead of `PipeOwning`"* is registered as a high-severity **threat**, mitigated by *"routing assertion test."* VALIDATION.md 37-02-02: *"Claude → PipeOwning (never Legacy)."*

`crates/devflow-cli/src/pipeline_launch.rs:703-709`:

```rust
pub(crate) fn claude_stream_launch_enabled(agent, stage, legacy_opt_out) -> bool {
    !legacy_opt_out && agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)
}
```

Claude → `Legacy` is a supported, shipped path — the `--legacy-claude-launch` opt-out from v2.3.0/Phase 31, with a dedicated pre-31 builder at `:203` (`exec_command_single_document`). A test asserting "never Legacy" either encodes a false invariant (the exact class of defect this phase exists to remove) or silently ignores the opt-out, in which case it is not testing the routing.

Secondary: the routing decision lives in `resolve_launch_shape` (`pipeline_launch.rs:188`, crate `devflow-cli`). Both 37-02's `<verify>` and VALIDATION.md row 37-02-02 give the automated command as `cargo test -p devflow-core --lib`, which cannot reach it. Also, 37-02 lists `pipeline_launch.rs` in `files_modified` while its own artifacts table says *"read-only: confirm Claude routing unchanged."*

## 8. `Stage::gsd_command()` cannot be removed, and the removal has an uncounted call site

37-01 artifacts: *"`crates/devflow-core/src/stage.rs` — StageIntent enum + Stage→StageIntent mapping; **gsd_command removed**"* — while the same block's truth says only *"removed from the Code-stage path."* Those are different claims and the artifact line is the one an executor acts on.

Verified callers outside prompt.rs's Code arm: `prompt.rs:147` (Validate), `:193` (Plan), and `crates/devflow-cli/src/pipeline_gate.rs:591` (`print_dry_run`). That last file is in no plan's `files_modified`. `stage.rs:51-59` documents the mapping as deliberately retained *"for human-facing preview/documentation output (e.g. a dry-run pipeline listing) only"* — 37-01 proposes deleting the thing whose doc comment explains why it survives.

## 9. AgentAdapter removal has a blast radius outside every plan's file list

37-04 P-03 gates removal on *"grep the workspace for every reference."* Doing that now:

- `crates/devflow-core/src/canary.rs:40` — `use crate::agents::{AgentAdapter, ClaudeAgent};` (the Phase-31 nonce-canary guard)
- `crates/devflow-cli/src/test_support.rs:205, 244` — `AlwaysFailAdapter`, `FailOnceAdapter` test doubles
- `crates/devflow-cli/src/preflight.rs:1266`, `pipeline_launch.rs:190` — `&dyn agents::AgentAdapter` params

None of those files appear in 37-04's `files_modified` (it lists mod.rs, conformance.rs, preflight.rs, and four docs). The plan's stated gate will correctly report "not clean" and defer — which is the safe outcome, but means the phase's advertised cleanup is decided by files nobody scoped.

Separately: CONTEXT D-11 (*"Remove only if the migration requires it for Pi; otherwise defer — whatever's easiest"*) is a straight reversal of locked 999.31 **D-04** (*"Put a deprecation date on `AgentAdapter` … Do not let both paths persist across multiple phases"*). CONTEXT.md `:66` records that D-02 is superseded and says nothing about D-04. Per the operator's own no-silent-supersession habit, that inversion should be stated.

## 10. Pi is the one agent whose prompt changes with no coverage at all

- `agents/mod.rs:133` — the retiring invariant test iterates `[Claude, Codex, OpenCode]`. **Pi is absent.** Pi has never had a prompt-delivery invariant.
- 37-01 gives `render_prompt` to Claude/Codex/OpenCode only; `pi.rs` is not in its `files_modified`. Between 37-01 and 37-03, Pi either fails to compile (no default impl) or silently inherits whatever default the executor writes — most likely the un-migrated shared prompt, undetected.
- 37-03 P-02 then changes Pi's prompt (*"`render_prompt` == the de-Claude-ified intent"*) while the same plan's `<verification>` demands *"Pi `build_command`/`health` **byte-equal to Phase-36 behavior**."* Pi passes its prompt positionally (`pi.rs:36`), so the prompt *is* an argv element — those two requirements are mutually exclusive unless the test pins argv with a fixed prompt input, which makes it blind to the change that matters.
- `pi.rs:14-17` records a Phase-36 deferral verbatim: *"the leading-dash hazard (a markdown `- [ ]` list) is a Phase 37 concern."* Pi has no `--` end-of-options convention. Phase 37's CONTEXT, RESEARCH, and all four plans never mention it — and 37-03 is the plan that replaces Pi's prompt text.

Net: the phase changes Pi in the one way that could break it, with end-to-end verification explicitly deferred by D-04 and no snapshot standing in for it.

---

## Compressed — real but lower-yield

- **D-03 inverted in 37-02.** CONTEXT D-03: *"Pi is the second native driver … **Supersedes 999.31 D-02**, which named Claude/OpenCode."* 37-02 objective: *"D-02 satisfied by Claude vs OpenCode."* The plan re-instates the superseded decision, and lets Claude+OpenCode shape the trait in wave 2 before Pi touches it — the exact risk D-03's *"Reversibility: costly"* note was written to prevent.
- **`--add-dir` verify-first has no falsifiable outcome.** 37-03 P-03's fallback is *"record why"* in a SUMMARY. No test, no criterion, no way for validate-phase to tell "verified equivalent" from "didn't try."
- **ARCHITECTURE.md doesn't carry the claim it's slated to fix.** The stale wording exists at `README.md:89`, `docs/guides/adding-agent.md:48`, `docs/architecture/agent-model.md:42` — verified. `ARCHITECTURE.md` has no "same/identical prompt" string, so T-37-12's grep over it is vacuous. (Its real stale content is the `AgentAdapter` trait description at `:92-96` and *"prompt-sharing"* at `:409`, which the plan's grep target won't match.)
- **VALIDATION.md ships unapproved.** `status: draft`, `nyquist_compliant: false`, all six sign-off boxes unchecked, `Approval: pending`. Its "Manual-Only" note claims *"All in-scope behaviors … have automated verification"* — findings 1 and 4 are the counterexample: the phase's central deliverable is verified by the absence of a substring.
- **999.94 has drifted twice.** ROADMAP pencils it into 37; the phase slug literally contains `999-94`; 36-SPEC deferred it *to* 37; CONTEXT D-09 defers it again to 38/37.1. ROADMAP was not updated. Speculative consequence: a later `roadmap.analyze` or milestone audit reads the slug and reports 999.94 as covered.
- **999.101 dropped.** 36-SPEC `:89` routes it forward as *"observation for Phase 37's driver contract."* Zero mentions in any Phase 37 artifact (grep-verified).

**Speculative, flagged as such:** finding 4's *consequence* (that an executor will invent a semantically empty Codex instruction) is inference — the *absence* of any specification for that text is verified. Finding 10's compile-failure branch depends on whether the executor gives `render_prompt` a default body, which no plan specifies either way.
