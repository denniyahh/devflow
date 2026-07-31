# Phase 28: Close the Checkpoint Answer Return Path - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-30
**Phase:** 28-close-the-checkpoint-answer-return-path
**Areas discussed:** Gate classification, the answer/resolution channel, resume mechanism design, auto-decide vs. human-answer, notification reality, `yes_ship` config persistence, Define's headless interview bug

---

## Gate classification — how does DevFlow know a checkpoint fired?

| Option | Description | Selected |
|--------|-------------|----------|
| Agent declares it in the envelope | New `status: checkpoint` field in `DEVFLOW_RESULT` | |
| Infer from exit shape | Guess from `is_error`/`num_turns`/marker presence | |
| Pattern-match reason text | Grep output for checkpoint-shaped content | |
| Static PLAN.md scan + existing `Gate:` field | Scan the stage's plan for `gate="blocking-human"` ahead of launch; confirm via the executor's already-emitted `checkpoint_return_format` | ✓ |

**User's choice:** static scan, confirmed via the field the executor already produces.
**Notes:** User's own framing broke this open: "aren't all the instances where an operator's input is required known ahead of time?" — `gate="blocking-human"` is a static plan attribute, not a runtime guess. Pattern-matching reason text was rejected as structurally identical to the tag-signing predictor killed twice in Phase 26 (999.50/999.54).

---

## The resume mechanism — literal `--resume` vs. fresh agent + explicit state

| Option | Description | Selected |
|--------|-------------|----------|
| Fresh relaunch + answer file (GSD-core's own pattern) | Mirrors `execute-phase.md`'s "why fresh agent, not resume" continuation model | |
| `claude -p --resume <session_id>` | Relaunch the exact exited session | ✓ |

**User's choice:** literal `--resume`, once verified real and Claude-only concerns were set aside ("if there is a clear solution for claude, then choose that option").
**Notes:** Verified via `claude-code-guide` agent: `--resume` is documented, supported headlessly, requires same-directory invocation (already satisfied by DevFlow's worktree-consistent launches). GSD-core's own avoidance of resume (parallel Agent-tool subagent spawns) doesn't transfer to DevFlow's one-agent-per-stage shape. `session_id` capture turned out to be nearly free — already present in the JSON envelope DevFlow already parses (`--output-format json`), just previously discarded.

---

## The answer channel — how does a human's response reach the agent?

| Option | Description | Selected |
|--------|-------------|----------|
| Dedicated verb (`devflow gate answer`) | New CLI command, bypasses approve/reject | |
| Stretch `approve --note` | Reuse existing verb, dispatch on gate class | |
| File-drop only | Reuse existing response-file protocol, no new verb | |
| Both — CLI verb wrapping the same file protocol | | (superseded, see below) |

**User's choice:** superseded — no human-answer path is being built this phase (see below).
**Notes:** This thread was worked through in detail (including correcting an initial "note/abort collision" claim that turned out to be wrong — the abort substring-match only fires on `approved=false`, not on approve) before the broader notification-reality discussion made the whole question moot.

---

## Opting out of human intervention entirely

| Option | Description | Selected |
|--------|-------------|----------|
| No mechanism — always wait for a human | | |
| Single consolidated flag covering Ship + checkpoints | | |
| Separate flags: `yes_ship` stays as-is, new flag for checkpoints | | |
| Opt-in flag/config for checkpoint auto-decide | | |
| Unconditional default — no flag at all | | ✓ |

**User's choice:** unconditional default, no flag.
**Notes:** This moved in stages. First: "we should allow the user to either set a config or add a flag ... that forces the agent to make decisions ... on behalf of the human" — raised against `checkpoints.md` rule 6 and the Phase 26 near-miss (a mistagged `gate="blocking"` that would have silently authorized `cargo publish`). User heard the concern, reaffirmed: "that's not a strong enough basis... in my opinion." Consolidating into one flag with `yes_ship` was then considered and rejected (different risk shapes — Ship is a coarse proceed/don't-proceed, checkpoints are arbitrary/planner-judged, e.g. package-legitimacy). Final landing, after the notification-reality discussion below: no flag at all — unconditional default, since a "human answers instead" fallback doesn't actually work today.

---

## Notification reality check

**User's questions, worked through directly rather than as a menu:**
- "What is the notification channel?" → Only `DEVFLOW_GATE_NOTIFY_CMD`, operator-supplied, silent no-op if unset. No default push notification of any kind.
- "Isn't this just human-in-the-loop with extra steps?" (re: a proposed stop-and-exit, pull-based design) → Confirmed: yes, still required a human to notice and act; didn't meet the actual goal.
- "Do we have any notification/response interface at all?" → Corrected: partial infrastructure exists (gate file protocol, `approve`/`reject`/`list`/`show`, used successfully for Ship/stage-failure) but nothing wired to checkpoints, and the existing blocking-poll mechanism has a documented 7-day-timeout / leaked-process history.
- "Given that's terrible UX, we effectively have no solution?" → Agreed, with precision: no *usable* one, not no plumbing at all.

**Conclusion:** no human-answer path this phase; checkpoints resolve unconditionally via the auto-decide mechanism instead.

---

## `yes_ship` config persistence

| Option | Description | Selected |
|--------|-------------|----------|
| Stay CLI-flag-only (Phase 23's D-05) | | |
| Add a config option too | | ✓ (after reversal) |

**User's choice:** add the config option, deliberately reversing Phase 23's D-05.
**Notes:** This flipped twice. First pass: "i've changed my mind so move forward with it" — read (incorrectly) as declining the config option. User corrected: "you misunderstood me, i'm saying add the yes-ship config option." Phase 23's own D-05 text was surfaced in full, including its stated reversibility cost ("relaxing this later is easy, but tightening it after operators depend on a persisted setting is not") before the final confirmation.

---

## Define's headless interview bug (28c / 999.59)

**User's framing:** "if a user wants a gsd discussion interview, it's up to them to decide to do that themselves beforehand. devflow doesn't have to have any accommodation for that."
**Verified:** `Stage::Define.gsd_command()` returns `/gsd-discuss-phase {N}` unconditionally when `CONTEXT.md` is missing — the exact interactive command this discussion is running under, invoked headlessly.
**Resolution:** not a flag to disambiguate two arms (999.59's original framing) — delete the arm. Proceed without `CONTEXT.md` when absent, same as any other early phase.

---

## Claude's Discretion

- Exact wording of the synthesized "use your own judgment" instruction relayed via `--resume`.
- Exact shape/location of the auto-decide audit record.
- Exact config key name/shape for `yes_ship`'s new config option.
- Where the static PLAN.md scan lives and how it locates the relevant plan file(s).
- Mechanical shape of `session_id` persistence on `State`.

## Deferred Ideas

- Human-answer path for checkpoints (dedicated verb or resume-with-human-text) — needs a real notification/response interface first.
- The notification/response interface itself.
- Ship-gate redundancy (yes_ship already expresses intent; DevFlow still always writes a gate and blocks on poll when false) — flagged, deliberately left unbuilt.
- Cross-agent (Codex/OpenCode) checkpoint resolution — Claude only this phase.
