# Adversarial Review — Phase 38 CONTEXT (D-01..D-04)

**Targets:** `38-CONTEXT.md` + `38-DISCUSSION-LOG.md` (committed `76d08a9` on `feature/phase-38`)
**Reviewers:** claude (opus, high) · antigravity (Gemini 3.7 Flash High) · codex (terra — **failed**, see below)
**Date:** 2026-08-17
**Review root:** `.worktrees/phase-38` (both reviewers read the live source + ran the installed `pi` 0.84.1 with negative controls)

## Verdict: REVISE — the CONTEXT as written would deadlock Pi on first launch

Both reviewers independently traced the same failure and confirmed it against the live binary. The
phase must be re-scoped before planning.

## Cross-review consensus (claude + antigravity agree, code-verified)

1. **BLOCKER — routing Pi through `MonitorLaunch::PipeOwning` deadlocks.** `run_pipe_owning_monitor`
   writes Claude's `stream-json` stdin turn envelope (`{"type":"user","message":…}`) and holds stdin
   open; Pi treats that JSON as raw prompt text and, in `--mode json`, **blocks on the open pipe**.
   `CloseRule::should_close()` waits for `type:"result"`, which Pi never emits (its vocabulary is
   `session/agent_start/turn_*/message_*/agent_end/agent_settled`). Net: stdin never closes, Pi never
   exits, the 120s idle timeout SIGKILLs it. (claude D/E, antigravity finding 1 + A)
2. **BLOCKER — D-02's justification is false: Pi has no concurrent work for the drain gate to cover.**
   Pi's toolset is `read/bash/edit/write/grep/find/ls` — no subagent dispatch, no background-shell
   tool. `CloseRule` watches exactly `background_tasks_changed` (backgrounded shells) and `open_tasks`
   (sub-agents); Pi emits neither, so the drain arm is **permanently vacuous** for Pi. D-02's "real
   coverage" is unfalsifiable (no negative control can fail a broken drain gate for Pi). (claude B,
   antigravity B.1)
3. **HIGH — D-01 and D-04 assert opposite things about Define.** D-01 says Define is a no-op for every
   agent (D-14); D-04 declares Pi's Define → `RequiresExistingArtifact`, which would block
   `devflow start --agent pi` on a missing CONTEXT.md for a stage that provably does nothing
   (`define_stage_prompt` says "you must NOT run an interactive discuss-phase … whether or not
   CONTEXT.md exists"). (claude I, antigravity 2)
4. **HIGH — the `STREAM_JSON_STAGES` widening gate is unsatisfiable by Pi.** Widening it requires "a
   two-plan wave where both plans produce a SUMMARY.md and merge" — i.e. concurrent dispatch, which
   Pi structurally cannot do (finding 2). The gate must be amended for non-dispatching agents — an
   unowned decision. (claude G)
5. **HIGH — `interactivity_mode` is unreachable, and reaching it is Phase 39's work.** `DriverShim`
   forwards six methods but not `interactivity_mode`; `adapter_for` returns `Box<dyn AgentAdapter>`.
   Making the gate driver-driven needs driver access at the call sites — verbatim the
   `AgentAdapter`/`DriverShim` removal deferred to Phase 39. (claude J)
6. **MEDIUM — `classify()` classifies Pi's event stream as `PlainText`.** Layer 1 scans for
   `"system"|"user"|"assistant"` or `turn.*` top-level types; Pi's `turn_start`/`message_start`/`agent_end`
   don't match the parser's expectations cleanly, so captures fall through to the tail scan.
   (antigravity 4)
7. **MEDIUM — unrelated gates will refuse Pi before it launches.** `preflight_unattended_launch_check`
   hardcodes "the agent is not claude" as a refusal cause in Auto mode; the `canary_gate` couples to
   `ClaudeCanaryLauncher` and would spawn `claude` to verify delivery tokens for a Pi run.
   (antigravity 2 "Needs My Awareness", claude H)
8. **MEDIUM — idle-timeout cadence.** The 120s floor is Claude-calibrated (30s `tool_progress`
   keepalive); Pi has no keepalive during long foreground tool runs. (claude F, antigravity 3)

## claude-only (code-verified; verify before acting)

- **HIGH — the provider/credential mismatch blocks D-01's acceptance run today.** `pi.rs` hardcodes
  `pi auth check --provider google`, which returns `not_ready` on this machine even though `pi` runs
  fine (operator's `litellm`/`deepseek` config). `classify_auth_check` turns that into a blocking
  preflight failure. No Phase 38 decision owns it. (claude A)
- **HIGH — `completion_signal_detected` returns `false`** on the premise "`pi -p` exits cleanly",
  which D-02's transport switch invalidates. (claude)
- **MEDIUM — `willRetry` / `agent_end`.** Pi's terminal event carries `willRetry`; the unwrapper must
  not treat a `willRetry:true` turn as completion. `agent_settled` is the likely terminal signal.
- **MEDIUM — checkpoint resume** hardcodes `MonitorLaunch::Legacy`, so a Pi run that hits a checkpoint
  resumes on the legacy path (no unwrapper) — a parity hole. (claude)
- **MEDIUM — prompt echo.** Pi echoes the prompt as `message_start`/`message_end`; this reintroduces
  the 30-05 false-positive surface with none of Claude's `parent_tool_use_id` provenance.

## Document bugs (minor)

- `D-03` appears only under "Deferred", not in the numbered `D-01..D-04` list (antigravity 5).
- ROADMAP says "complete all five stages"; D-01 says "Plan→Code→Validate→Ship (Define is a no-op)"
  — reconcile the wording (antigravity 5).

## Reviewer status

- **claude** — success (opus, high).
- **antigravity** — success after re-run with `--dangerously-skip-permissions` (Gemini 3.7 Flash High;
  the first attempt auto-denied a `command`-permission tool).
- **codex** — **FAILED**: `-m terra` → HTTP 400 *"The 'terra' model is not supported when using Codex
  with a ChatGPT account."* No findings. (Note: the user's correction named Terra as the model for
  codex "going forward"; it is not available on this ChatGPT account.)

## Recommended path

The four decisions need revision before planning, in this order of importance:

1. **Re-scope D-02's transport.** The "drain-gate coverage" rationale is void, and `PipeOwning`'s
   stdin wire protocol deadlocks Pi. Either (a) keep `--mode json` for *structured completion
   detection* and drop the drain-gate claim (restate the rationale + amend the `STREAM_JSON_STAGES`
   widening criterion for non-dispatching agents), or (b) stay on `-p` + `MonitorLaunch::Legacy` with
   a Pi JSONL completion parser in `agent_result.rs` (analogous to `parse_codex_event_result`).
2. **Fix D-04 vs D-01** — Define is a no-op; drop `RequiresExistingArtifact` for Define (Plan is the
   open question: can Pi plan headlessly?).
3. **Resolve the `interactivity_mode` reachability** — it is Phase 39's `AgentAdapter`-removal work;
   either pull that sliver into Phase 38 or reword D-04 to acknowledge the throwaway passthrough.
4. **Own the provider/credential mismatch** (claude A) — it blocks the acceptance run regardless of
   the transport choice.
