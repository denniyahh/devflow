> invocation: claude -p --model opus --settings '{"effortLevel":"high"}' --dangerously-skip-permissions "<prompt>" (adversarial lens + 3 docs inlined)

I ran the checks against the actual tree (`develop` @ `a3a4871`) and the installed `pi` 0.84.1 rather than reading the docs alone. Seven findings, ranked by how much work they invalidate.

---

## 1. 999.67 is already fixed and tested on `develop`. Scope item B and AC #2 are no-ops.

**SPEC §B:** *"`parse_devflow_result` overwrites an agent-planted `decided_by_layer` exactly as `parse_claude_event_result` already does on the stream path, with a mirror test."*
**AC #2:** *"the planted-`decided_by_layer` fixture on the single-document path now returns the overwritten (non-planted) value, pinned by a regression test with a passing counterpart."*

Both are already true. `agent_result.rs:175-180` calls `normalise_stream_marker_provenance` on **both** arms of `parse_devflow_result`, and the code comment at line 172-174 names the item by number: *"this generic path — the one production hits today — did not (fourth adversarial pass, Medium 1; the class 999.67 tracks)."* The mirror test exists at `agent_result.rs:4342` (`generic_marker_cannot_forge_layer0_provenance`) **with the passing counterpart AC #2 asks for**, at line 4354.

```
test agent_result::tests::generic_marker_cannot_forge_layer0_provenance ... ok
test result: ok. 1 passed; 0 failed; 631 filtered out
```

The ROADMAP entry (`999.67`, line 3285) is stale — it still says "shortlisted for Phase 31" and describes the fix as pending. Neither the SPEC nor the CONTEXT re-verified it; CONTEXT D-05 sizes it as "XS (one-line overwrite + mirror test)" on the strength of the backlog prose. **What this does not establish:** I verified the generic and codex arms. I did not audit every marker path for the same class.

## 2. 999.96's "ready-made positive fixture" does not exist. AC #3 is unfalsifiable as written.

**SPEC §C:** *"Must carry a negative control — the current tree is itself a ready-made positive fixture (changelog `2.5.0` vs. workspace)."*
**AC #3:** *"`release --check` reports NOT viable on the current skew."*

There is no skew. `Cargo.toml:9` is `version = "2.5.0"`; `CHANGELOG.md:3` is `## 2.5.0 — 2026-08-15`. They agree, and they agree *because* commit `dbc3b11` ("release: v2.5.0") moved both together.

This is a transcription error with a traceable origin: ROADMAP 999.96 measured the skew on **2026-08-07** (`Cargo.toml` at `2.4.0`, changelog heading `2.5.0`) and wrote *"the current tree is itself a ready-made positive fixture."* The SPEC copied that sentence and bumped the number by one without re-measuring. The v2.5.0 cut consumed the fixture eight days later. A planner who takes AC #3 literally will write a test that cannot fail, which is the exact defect the ROADMAP entry itself warns about two lines lower (*"A version check that cannot fail is the same defect as the signing predictor 999.86 replaced"*).

## 3. AC #1 requires the thing the phase defers to Phase 37. This is circular.

**AC #1:** *"A `devflow` run driven by Pi reaches a terminal completion... on a real (not mocked) Pi session."*
**SPEC boundary:** *"The full `AgentDriver` contract... is **Phase 37**, not here."*
**SPEC §Deferred:** *"999.31 — Phase 37."*

`Stage::Code.gsd_command()` (`stage.rs:63`) returns the literal string `"/gsd-execute-phase {N}"`, and `prompt.rs:274` interpolates it into the Code-stage prompt — pinned by `each_stage_prompt_carries_its_gsd_command_and_marker` (`prompt.rs:371`). So the prompt DevFlow will hand Pi opens by invoking a Claude Code plugin slash command.

ROADMAP line 1547 records this as 999.31's *still-open* evidence, verified: *"`Stage::gsd_command()` (`stage.rs:52`) still returns literal `/gsd-*` strings from core"* — with the justification *"Onboarding any agent beyond Claude; **a confirmed Codex dogfood failure**."* The three adapters CONTEXT tells the planner to mirror have already failed this way in production for a non-Claude agent.

The SPEC's own sequencing argument inverts here: it lands Pi first so Phase 37's contract has a live second consumer, but Pi cannot reach terminal completion until Phase 37 de-Claude-ifies the prompt. The SPEC's boundary — *"Pi's prompt text may initially render the same stage prompt as the other agents (behavior-preserving)"* — is precisely the assumption that breaks. Pi does have `--skill <path>` and its own slash-command system, so a route may exist; **speculation:** I did not verify whether GSD's Claude-plugin skills load under pi. What is verified is that no document names this as work, and that the prompt is Claude-specific today.

## 4. CONTEXT's `monitor.rs` integration point is wrong, and it hides a capability downgrade.

**CONTEXT §Integration Points:** *"`crates/devflow-core/src/monitor.rs` — the pipe-owning monitor; its drain gate (`CloseRule`) reads agent-specific events, so Pi's completion detection (`agent_end`) needs a Pi-aware arm."*

`claude_stream_launch_enabled` (`pipeline_launch.rs:703-709`) is `!legacy_opt_out && agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)`. `resolve_launch_shape` (`pipeline_launch.rs:198-208`) routes every non-Claude agent to `MonitorLaunch::Legacy`. `MonitorLaunch`'s own doc comment (`monitor.rs:255-259`) states it outright: *"Every non-Claude adapter... run through here, unchanged."* `CloseRule` (`monitor.rs:582`, constructed at `:882`) exists only on the `PipeOwning` arm.

So a Pi adapter built "mirroring the existing three" never reaches `CloseRule`, and the stated work item does not exist. The inverse is worse: routing Pi onto `PipeOwning` means widening a predicate hard-gated on `AgentKind::Claude` plus the `stream-json` stdin wire (`user_turn_line`, `monitor.rs:726`) — not "mirror a small struct."

Nobody decides which. And the default (Legacy) silently gives Pi the pre-31 supervision: stdin at `/dev/null`, no task-notification turn, no constraint-4 drain gate. That is a real capability asymmetry with Claude, and **no acceptance criterion would detect it** — AC #1 only asks that a run reach terminal completion.

## 5. `--mode json` breaks `DEVFLOW_RESULT` parsing, and the required unwrapper is in nobody's scope.

**CONTEXT §Established Patterns:** *"A Pi adapter fits this: `exec_command` returns `("pi", vec![...])` with `--mode json` and the prompt positional."*
**CONTEXT §Reusable Assets:** *"The `{COMPLETION_PROTOCOL}` prompt fragment... the Pi prompt reuses it."*

`parse_marker_lines` (`agent_result.rs:1590-1618`) requires `DEVFLOW_RESULT:` at the **start** of a line: `line.get(..PREFIX.len())` after edge-stripping. Under `pi --mode json`, per pi's own `docs/json.md`, assistant text arrives as `{"type":"message_update","assistantMessageEvent":{"type":"text_delta",...,"delta":"Hello"}}` — delta-only, JSON-escaped, never at line start — with the authoritative text in `message_end` / `agent_end`, also escaped inside JSON.

DevFlow solves this per-agent today: `extract_json_result_text` for Claude's envelope, `parse_codex_event_result` for Codex events. There is no Pi equivalent, and CONTEXT D-05 asserts the **only** `agent_result.rs` change this phase is 999.67's "one-line overwrite." A Pi event unwrapper plus golden fixtures is a distinct, non-XS piece of work that appears in no scope list, no sizing, and no acceptance criterion.

Secondary: `agent_end` carries `messages: AgentMessage[]` — the entire conversation on one line. `parse_marker_lines` runs a 4000-char whole-line tail budget. Worth a fixture; the interaction is not obviously safe.

## 6. AC #4's negative control cannot be produced by the approach the SPEC defaults to — and SPEC and CONTEXT disagree on what that approach even is.

**AC #4:** *"`release --check`'s signing probe reports viable only for the maintainer key, with a negative control (the wrong key reported NOT viable)."*
**SPEC §D:** *"Default decision: repoint `release --check`'s tag-signing probe at `devflow.releaseSigningKey`... **Open decision**."*
**CONTEXT D-02:** *"One-line probe fix **plus** surfacing the fingerprint check at preflight."*

Three problems, in order of severity.

**(a) The negative control is unobtainable from a repoint.** `check_signing_viability` (`git.rs:1099-1150`) establishes viability by *performing the operation* — `run_ssh_sign_probe(key_path)`. It answers "can this key sign," not "is this the right key." On this machine both keys exist and are probeable:

```
user.signingkey            = ~/.ssh/devflow_signing_ed25519.pub
devflow.releaseSigningKey  = ~/.ssh/github_ed25519.pub
gpg.format                 = ssh
```

Point the probe at the wrong key and it reports **viable**. Only an identity comparison yields NOT-viable — which is CONTEXT's D-02 addition, not the SPEC's default. AC #4 is written against a fix that cannot satisfy it.

**(b) The fingerprint check is tautological where it matters.** `scripts/hooks/pre-push:58` sources the expected fingerprint from `devflow.releaseSigningKey` itself — local git config — and line 50 documents it as *"Opt-in by design: unset `devflow.releaseSigningKey` means no enforcement."* Surfacing that at preflight inherits both weaknesses: a misconfigured value compares against itself, and an unset one silently passes. That unset case is the literal failure 999.104 catalogues (*"the override forgotten"*). Neither document names a source of truth outside local config.

**(c) SPEC and CONTEXT contradict, and the CONTEXT misattributes.** CONTEXT's §Requirements says *"**In scope (from SPEC.md):** ... 999.104 (one-line signing probe + preflight fingerprint check)"* — but the SPEC contains no preflight fingerprint check; it lists only the repoint, flagged as open. CONTEXT also asserts *"Requirements (locked via SPEC.md). **4 requirements are locked.**"* while the SPEC's frontmatter still reads `status: draft (spec-phase, pre-discuss)` and carries four live "Open decisions," three of which the discussion already settled. CONTEXT then instructs: *"Downstream agents MUST read `36-SPEC.md` before planning or implementing."* A planner obeying that reads the superseded, narrower 999.104 scope and the settled questions as open.

## 7. Pi's default provider is `google`, and `AgentAdapter` has nowhere to say otherwise.

`pi --help`: `--provider <name>  Provider name (default: google)`.

Claude, Codex, and OpenCode carry model selection in their own account/config state, so `exec_command(&self, phase, prompt, extra_writable_roots) -> (&'static str, Vec<String>)` never needed a model surface. Pi does. With no `--provider`/`--model`, DevFlow drives Gemini via whatever `GEMINI_API_KEY` is in the environment — or fails on a missing key with no adapter-level diagnosis. Neither document mentions provider, model, or API-key selection anywhere.

This lands directly on SPEC §A requirement 3 (*"A discovery/health check distinguishes 'Pi binary installed' from 'Pi can execute headless'"*) — "can execute headless" for Pi means "has a working provider credential," a check with no analogue in the three existing adapters that requirement says to mirror.

Related and also unaddressed: pi's `docs/security.md:33` states *"Pi does not include a built-in sandbox,"* and `:41` recommends containerization specifically for *"unattended automation."* `AgentAdapter::exec_command` takes `extra_writable_roots` precisely for sandbox grants; Pi ignores them by construction. Separately, project trust is saved per canonical directory in `~/.pi/agent/trust.json` — and DevFlow creates a **fresh worktree path per phase**, so no saved decision ever applies and `defaultProjectTrust: "ask"` silently ignores project resources on every run. CONTEXT D-04 records the trust mechanics correctly but leaves `--approve` to "the agent's Discretion"; under DevFlow's worktree model it is load-bearing, not discretionary.

---

## Still open

Nothing here is mine to decide. Two items need your answer before planning starts:

1. **999.67 and 999.96 are both wholly or partly dead** (findings 1 and 2). Drop 999.67 from the phase and close the ROADMAP entry as already-fixed? And for 999.96 — the check is still worth building, but it needs a synthetic fixture, not "the current tree."
2. **Finding 3 is the one that decides the phase's shape.** If the Code-stage prompt's `/gsd-execute-phase` cannot run under Pi, then either the prompt work moves from Phase 37 into Phase 36 (breaking the stated boundary), or the phase's deliverable shrinks to "adapter plumbing plus a health check" and AC #1 is struck. My recommendation is to settle this before planning, since it changes the plan set, not just a plan.
