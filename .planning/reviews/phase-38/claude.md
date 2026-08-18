> invocation: claude -p --model opus --settings '{"effortLevel":"high"}' --dangerously-skip-permissions "<prompt via stdin>"
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-38 (feature/phase-38)

Verification done. Findings below; every claim was checked against the live binary or the source in this worktree.

---

## The two that break the phase before any code is written

**A. `devflow start --agent pi` cannot reach stage one on this machine — D-01's acceptance bar is currently unrunnable, and CONTEXT never mentions health.**

`crates/devflow-core/src/agents/pi.rs:61-69` hardcodes the credential probe to one provider:

```rust
.args(["auth","check","--json","--provider","google","--no-refresh"])
```

Measured just now, same binary, same cwd, with the negative control alongside it:

| command | result |
|---|---|
| `pi auth check --json --provider google --no-refresh` | `{"status":"not_ready","provider":"google","reason":"credentials_not_configured"}` |
| `pi --mode json --no-approve -p "…"` | ran to completion, `provider":"litellm","model":"deepseek-v4-pro"`, exit 0 |

`classify_auth_check` (`pi.rs:117-126`) turns `not_ready` into `Err`, and `preflight.rs:1270` wires it in as blocking: `generic_preflight_checks(...).and_then(|()| adapter.preflight(state))`. So Pi works and DevFlow's preflight says it doesn't. `pi auth check` also refuses to answer without an explicit target (`Error: Auth checks require --provider <provider> or --model <model>`), so "just drop the hardcode" is not the fix — the provider has to come from wherever Pi's default resolution lives, and `pi.rs:10-12` records that `AgentAdapter` has no config surface to source it from. Phase 38 has no decision covering this and no owner for it.

**B. D-02's stated justification is false: Pi has no concurrent work for the drain gate to cover.**

D-02 (`38-CONTEXT.md`) reads:

> so the `CloseRule` drain gate gets real coverage of Pi's concurrent work — not post-hoc output parsing

Pi's toolset, from two independent sources: `pi --help` line 1 — *"AI coding assistant with read, bash, edit, write tools"* — and the agent's own enumeration under `--mode json`: `read, bash, edit, write, gsd_invoke` (`gsd_invoke` is a local extension, not a dispatcher). There is no Task/subagent/background-shell tool. `CloseRule` exists to track exactly two things: `background_tasks_changed` from backgrounded shells and `open_tasks` from sub-agent dispatch (`monitor.rs:585-608`). Pi produces neither, so `background_tasks` stays `NeverAnnounced` and `open_tasks` stays empty for every run, forever.

That makes D-02's phrase **"real coverage"** unfalsifiable in the category-5 sense: there is no negative control. You cannot write a test that fails if the drain gate is broken for Pi, because a permanently-vacuous drain arm and a correctly-working one emit identical observations. The phase's own justification for its one-way transport decision does not survive contact with the tool.

*Limit of this check:* I sampled a trivial prompt plus a self-report, on 0.84.1 with the operator's litellm config. It establishes the default toolset, not that no extension anywhere could ever background work.

---

## D-02: the rest of it

**C. `-p` and `--mode json` are not alternatives — this is a category error.** D-02: *"Pi's launch switches from `-p` (print, process-exit) to `--mode json` (event stream)."* `pi --help`: `--mode <mode>  Output mode: text (default), json, or rpc` and `--print, -p  Non-interactive mode: process prompt and exit`. One is output format, the other is interactivity. The correct change is *adding* `--mode json` to the existing `pi.rs:51` argv, keeping `-p`. Written as a switch, a planner will drop `-p` — and `MonitorLaunch::PipeOwning` hands the child an **open pipe on stdin**, which is the one condition under which Pi may decide it is interactive. My negative control (`--mode json` without `-p`, stdin `/dev/null`) exited 0, but `/dev/null` is not a pipe; the pipe case is unmeasured and is the case the phase actually builds. Also: `rpc` mode exists and is never considered anywhere in CONTEXT or the discussion log.

**D. `PipeOwning` is a Claude wire protocol, not a generic pipe — the "reuses the drain arm unchanged" claim covers the wrong arm.** `monitor.rs:246-254`: PipeOwning *"delivers `prompt` as a JSON user turn on the child's stdin."* `monitor.rs:715-719`: the shape is `{"type":"user","message":{"role":"user","content":<prompt>}}` for *"a `--input-format stream-json` child."* Pi takes its prompt **positionally** (`pi.rs:3-5`, argv at `:51`), and has no `--input-format`. Routing Pi through this arm feeds Pi a stream-json envelope it cannot parse while the real prompt arrives on argv. CONTEXT's "Reusable Assets" bullet is right that the *drain* arm is reusable and silently wrong that the *launch* arm is.

**E. `CloseRule::should_close()` can never become true for Pi — the run hangs to idle timeout.** `monitor.rs:705-712` ANDs `marker_seen` with the two drain conditions. `marker_seen` is set only by `event_is_top_level_result_marker` (`agent_result.rs:1188-1196`), which requires `type == "result"`. Pi's actual event vocabulary, captured verbatim: `session, agent_start, turn_start, message_start, message_update, message_end, turn_end, agent_end, agent_settled`. No `result`. Worse, every task branch is gated on `type == "system"` first (`monitor.rs:620-622`, `:648-650`) — Pi has no `system` type either. So the unwrapper must **synthesize Claude's `type:"result"` envelope and rewrite `type` to `"system"`**, not merely map subtypes. CONTEXT lists only `task_started / task_notification / background_tasks_changed / …` — the `…` is carrying the single hardest requirement in the phase.

**F. The idle timeout Pi would inherit is Claude-calibrated, and the repo already forbids what D-02 does.** `pipeline_launch.rs:1023-1027`, verbatim:

> Scoped to the `PipeOwning` arm alone … The 120-second floor was measured against Claude's stream cadence (a fixed 30.00s `tool_progress` keepalive), and applying it to an agent whose output cadence has never been measured would be a **behaviour prediction — the thing constraint 1 forbids**.

D-02 routes Pi into that arm. Pi's cadence is unmeasured, and Pi has no keepalive event in its vocabulary at all (`message_update` fires on token deltas, which stop during a long `bash` tool call). CONTEXT budgets no cadence measurement.

**G. The existing gate for widening the stream path is unsatisfiable by Pi.** `pipeline_launch.rs:688-693` states what is allowed to widen `STREAM_JSON_STAGES`:

> a passing acceptance run (D-16/D-18 — **a two-plan wave where both plans produce a `SUMMARY.md` and merge**) producing the first real production `stream-json` capture to verify the parser against. Not a green unit suite

A two-plan wave is concurrent dispatch. Per finding B, Pi cannot dispatch. Phase 38 therefore has to either satisfy a criterion its agent structurally cannot meet, or amend that criterion — and amending it is a decision about Claude's safety gate, which nobody has taken. `claude_stream_launch_enabled` at `pipeline_launch.rs:712` is `!legacy_opt_out && agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)`; CONTEXT says only "widen the predicate" and is silent on the documented precondition for widening.

**H. Stale invariant, minor but load-bearing.** `pipeline_launch.rs:832-836` and `monitor.rs:342-360` both assert *"the sole adapter routed through the pipe-owning arm (Claude) declares no extra env"*, backed by the test `codex_disables_signing_via_env_others_do_not`. `PiDriver` declares no `environment()` override, so the invariant survives on value — but both comments and that test become false as written the moment Pi routes through. Not a bug; a documented invariant Phase 38 must actively re-state.

---

## D-04: contradicts D-01, and is blocked by the Phase 39 deferral

**I. D-01 and D-04 assert opposite things about Define.** D-01: *"(Define is a no-op for every agent, D-14)"*. D-04: Pi declares *"Define/Plan → `RequiresExistingArtifact`"* because *"Pi cannot run the interactive discuss-phase interview headlessly"*. Both cannot hold. And D-01 is the correct one — `render_workflow_style` routes Define to `define_stage_prompt` verbatim (`prompt.rs:392-394`), which reads (`prompt.rs:303-316`):

> There is no agent work to perform here. **Whether or not this phase's CONTEXT.md already exists**, you must NOT run an interactive discuss-phase or interview command

So declaring Pi's Define `RequiresExistingArtifact` would block `devflow start --agent pi` on a missing CONTEXT.md for a stage that provably does nothing. D-04 propagates a guard that D-14 already obsoleted. (For the record: Phase 37's own code review flagged Define/Plan/Validate/Ship regressions for Pi — those are **fixed** in current source; `VALIDATE_VERDICT_CONTRACT` at `prompt.rs:457`, the Critical gate at `:479-482`, and Pi's own `workflow_root` at `pi.rs:39-41`. Don't re-litigate them.)

**J. `interactivity_mode` is unreachable, and reaching it is Phase 39's deferred work.** It exists at `agents/mod.rs:176-178` with `CodexDriver`'s override at `codex.rs:87-94`. But `DriverShim` (`mod.rs:232-259`) forwards `name`, `exec_command`, `extra_env`, `completion_signal_detected`, `preflight`, `render_prompt` — **not** `interactivity_mode` — and `adapter_for` (`mod.rs:262`) hands back `Box<dyn AgentAdapter>`. There is no `driver_for`. Every reference to it outside `codex.rs` is a test (`mod.rs:460-473`). To make the gate driver-driven you need driver access at the call sites, which is verbatim the *"`AgentAdapter`/`DriverShim` removal + call-site migration"* that CONTEXT's own Deferred section sends to Phase 39. Phase 38 must either do part of Phase 39 or add a throwaway `AgentAdapter::interactivity_mode` passthrough that Phase 39 immediately deletes. Unacknowledged — this is the scope leak.

**K. "the hardcoded `AgentKind::Codex` check" is two divergent checks, and the Plan half doesn't exist.** CONTEXT names *"`preflight.rs` + `commands.rs:289`"* as if genericizing one predicate. Actually:
- `preflight.rs:613` gates on **four** conjuncts — `agent == Codex && mode == Auto && stage == Define && !phase_artifact_on_develop(…"-CONTEXT.md")`. A driver's `interactivity_mode(stage)` returns one enum from one input; the `Mode::Auto` conjunct and the artifact-name lookup have nowhere to live in that signature.
- `commands.rs:289` is a *different* check with a different failure mode: hard error on missing CONTEXT.md, but only a `println!("warning: …")` on missing PLAN.md (`commands.rs:298-300`) — not a gate at all.
- The comment at `preflight.rs:609-612` records that these were deliberately left divergent: *"The pre-state Codex check itself is intentionally left unmigrated (Review dispositions, out of scope for this plan)."*
- **No Plan-stage interactivity gate exists at either site.** D-04's "Define/Plan gate" implies building the Plan half from scratch, not genericizing it.

---

## Missing failure modes

- **`willRetry`.** Pi's terminal event is `{"type":"agent_end", …, "willRetry":false}`. An unwrapper mapping `agent_end` → completion without reading `willRetry` closes the stage on a turn Pi is about to retry. Nothing in CONTEXT mentions it. `agent_settled` (the actual last line) is likely the correct terminal signal, and is not named either.
- **Checkpoint resume silently drops out of the new path.** `pipeline_launch.rs:703-706`: *"`relaunch_checkpoint_session` hardcodes `MonitorLaunch::Legacy` … so it never consults this predicate at all."* After Phase 38, a Pi run that hits a checkpoint resumes on Legacy — stdin at `/dev/null`, no unwrapper. A parity hole inside D-01's own Plan→Ship window.
- **`completion_signal_detected` returns `false`** (`pi.rs:97-100`) on the reasoning *"`pi -p` exits cleanly when done; the monitor detects exit via kill -0."* That premise is exactly what D-02 changes. Untouched by any decision.
- **Prompt delivery under `--mode json`.** Pi echoes the operator's prompt back as a `message_start`/`message_end` user event (observed). `monitor.rs:509-513` documents that prompt echo is precisely what caused false positive 30-05 and why the marker predicate is structural rather than a text search. Pi's unwrapper reintroduces the echo surface with none of the `parent_tool_use_id` provenance Claude's events carry (`agent_result.rs:1229-1233`) — there is no top-level/nested distinction available in Pi's schema to rebuild that filter from.

---

## Needs your decision

**One item, and it gates planning:** D-02's justification is void (finding B) and its documented widening gate is unsatisfiable by Pi (finding G). Three ways forward:

1. **Keep `--mode json`, restate the rationale.** Drop "drain-gate coverage" and justify the event stream on what it actually buys — structured completion detection instead of exit-code guessing, and a real capture to parse. Then explicitly record that `CloseRule` is vacuous for Pi and amend the `STREAM_JSON_STAGES` widening criterion for non-dispatching agents. **My recommendation** — it preserves the operator's choice and makes the acceptance bar falsifiable.
2. **Stay on `-p`.** The discussion log rejected this for a reason that turns out not to exist.
3. **Measure first.** Spend a plan proving whether Pi can background anything at all before committing the transport.

## Still open

- Who owns the Pi provider/credential mismatch (finding A)? It blocks D-01's acceptance run regardless of which option above you pick, and no decision in Phase 38 covers it.
