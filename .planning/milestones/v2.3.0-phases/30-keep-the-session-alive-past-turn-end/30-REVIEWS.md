---
phase: 30
reviewers: [codex]
reviewer_note: "Ad-hoc single-reviewer pass — codex invoked directly with --model gpt-5.6-sol -c model_reasoning_effort=high (overrides this project's configured review.default_reviewers: codex/cursor/opencode/antigravity, which normally resolve codex to a lower effort tier). Not run through the standard /gsd-review reviewer-instance mechanism."
reviewed_at: "2026-08-02T14:36:55Z"
plans_reviewed:
  - 30-01-PLAN.md
  - 30-02-PLAN.md
  - 30-03-PLAN.md
  - 30-04-PLAN.md
  - 30-05-PLAN.md
---

# Cross-AI Plan Review — Phase 30

> Single-reviewer pass (codex / gpt-5.6-sol / high effort). No consensus synthesis across
> reviewers is possible — treat every finding below as one independent, source-grounded
> verdict, not a cross-model agreement.

## Codex Review

# Cross-AI Plan Review — Phase 30

## Overall assessment

**Verdict: revisions required before execution. Overall risk: HIGH.**

The parser architecture is mostly well grounded, and the scoped baseline test is currently green: I verified `cargo test -p devflow-core --lib agent_result::` reports **89 passed, 0 failed**. I did not run the full workspace gate.

Three blockers remain:

- Plan 30-03 treats every `rate_limit_event` as a rate limit, but the real archived event says `status: "allowed"`; the proposed test would codify a healthy-stream false positive.
- Plan 30-02 redirects raw output directly into the committed evidence directory while requiring redaction before any write. The existing capture proves those streams contain home paths and usernames.
- Phase 30 cannot satisfy its formal goal. The goal says a multi-plan wave completes, but Phase 30 explicitly forbids the monitor/adapter changes required to keep the process alive.

There is also a conditional-flow problem: if 30c refutes delivery, Plans 30-03 and 30-05 still land even though Claude remains on the single-document format and the new stream parser has no operational producer.

---

## Plan 30-01 — Claude stream tracer

### Summary

The core design is sound: it mirrors the existing Codex JSONL parser, uses a mutually exclusive format gate, and protects the shipped single-document path. Two internal plan contradictions need correction before execution.

### Strengths

- Mirroring the established line-split, parse, gate, and reverse-iteration structure is appropriate. The Codex precedent is visible at [agent_result.rs:520](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:520) through [agent_result.rs:612](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:612).
- Gating only on `system/init` correctly avoids consuming the current Claude envelope, which is a single `type:"result"` document. The adapter still emits that format at [claude.rs:15](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agents/claude.rs:15), and the existing cross-adapter test confirms the shape at [agent_result.rs:1820](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:1820).
- Using the decoded final `result` string with `parse_marker_lines` avoids scanning JSON-escaped raw output. The existing parser only recognizes literal marker lines in the final 4,000 characters at [agent_result.rs:619](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:619).

### Concerns

- **MEDIUM — `decided_by_layer` will not become `Some(1)` as specified.** The plan says to return whatever `parse_marker_lines` returns, while its fixture marker contains only `status` ([30-01-PLAN.md:183](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-01-PLAN.md:183)). `parse_marker_lines` directly deserializes the agent-authored JSON without normalizing `decided_by_layer` ([agent_result.rs:641](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:641)); the field defaults to `None` ([agent_result.rs:37](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:37)). This conflicts with the acceptance criterion at [30-01-PLAN.md:238](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-01-PLAN.md:238).
- **LOW — the fixture instructions provide two result events, while behavior and acceptance demand three.** The fixture names lines 19 and 54 only ([30-01-PLAN.md:216](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-01-PLAN.md:216)), but the required regression explicitly says three results ([30-01-PLAN.md:236](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-01-PLAN.md:236)).

### Suggestions

- Require `parse_claude_event_result` to overwrite `result.decided_by_layer = Some(1)` after marker parsing, even if the agent supplied another value.
- Include archived result envelopes 19, 37, and 54 in the last-wins test.
- Add a test where the marker tries to set `decided_by_layer: 0`; assert the parser normalizes it to 1.

### Risk assessment

**MEDIUM.** The architecture is good, but the current action cannot satisfy its own Layer-1 provenance criterion.

---

## Plan 30-02 — Production-environment experiment

### Summary

The decision-gate structure and operator recount are strong, but the experiment is not presently safe or decisive enough to gate Phase 31.

### Strengths

- The plan correctly traces production through `hermetic_command("sh", ...)`, `sh -c`, shell redirection, and null outer stdio at [monitor.rs:134](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/monitor.rs:134) through [monitor.rs:174](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/monitor.rs:174).
- Reading the scrub lists from source avoids drift. The actual lists and removal loop are at [git.rs:27](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/git.rs:27), [git.rs:55](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/git.rs:55), and [git.rs:87](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/git.rs:87).
- The human checkpoint independently recounts raw events instead of trusting harness interpretation ([30-02-PLAN.md:254](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-02-PLAN.md:254)).

### Concerns

- **HIGH — redaction cannot occur before the proposed evidence write.** The plan directs child stdout straight into `30c-evidence/raw_output.jsonl` ([30-02-PLAN.md:143](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-02-PLAN.md:143)), then requires redaction before anything reaches that directory ([30-02-PLAN.md:153](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-02-PLAN.md:153)). Direct OS redirection bypasses the redactor. The baseline stream already contains absolute home paths, usernames, plugin paths, session IDs, and task output paths at [raw_output_v3.jsonl:5](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl:5).
- **HIGH — "at least one" notification is too weak for a two-child reliability gate.** Confirmation requires only one later task-notification result ([30-02-PLAN.md:199](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-02-PLAN.md:199)). The baseline produced two independent notification-origin results and a drained task set at [raw_output_v3.jsonl:37](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl:37), [raw_output_v3.jsonl:44](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl:44), and [raw_output_v3.jsonl:54](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl:54). One delivered child and one lost child could currently yield `confirmed`.
- **MEDIUM — "exactly one controlled variable" is unproven.** The harness only scrubs Git variables ([30-02-PLAN.md:121](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-02-PLAN.md:121)). In the current execution environment, Claude-session marker variables are present; `start_new_session=True` does not remove inherited environment or ancestry. The result may still be a descendant of an interactive agent environment.
- **MEDIUM — credential validation is narrower than the stated contract.** Acceptance checks only home paths and username ([30-02-PLAN.md:239](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-02-PLAN.md:239)), while the must-have also promises no credentials.

### Suggestions

- Redirect raw stdout/stderr into a run-scoped temporary directory outside `.planning/`; parse, structurally redact JSON fields, validate every line, secret-scan, then atomically publish sanitized copies.
- Define `confirmed` as both children completing plus either:
  - two correlated task-notification-origin results, or
  - a notification result combined with verified `background_tasks_changed.tasks == []`.
- Record relevant environment-variable names and parent/session metadata without values, then explain how they compare with the intended operator-launched DevFlow environment.
- Add an explicit secret scan covering tokens, API-key patterns, session/output paths, and private environment values.

### Risk assessment

**HIGH.** It can leak sensitive data and can green-light Phase 31 after only partial delivery.

---

## Plan 30-03 — Rate limit, failure, and session ID

### Summary

The session-ID and envelope-failure portions are well designed. The rate-limit interpretation is incorrect against the plan's own primary evidence and is a blocking defect.

### Strengths

- Direct top-level `session_id` access preserves the existing D-04 boundary at [agent_result.rs:241](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:241). The persisted consumer is real and operational at [pipeline_launch.rs:436](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_launch.rs:436).
- The proposed `is_error` precedence is grounded in the existing envelope detector at [agent_result.rs:351](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:351).
- Stream-first fallback in `session_id_from_capture` preserves the current lossy-read behavior at [agent_result.rs:275](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:275).

### Concerns

- **HIGH — every healthy v3 stream would become `RateLimited`.** The plan classifies the presence of any `rate_limit_event` as rate-limited ([30-03-PLAN.md:90](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-03-PLAN.md:90)). The real event it mandates as the positive fixture says `rate_limit_info.status: "allowed"` at [raw_output_v3.jsonl:15](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl:15). It also contains `overageStatus:"rejected"`, showing that arbitrary nested "rejected" fields are not safe classifiers.
- **HIGH — historical rate events outrank the final outcome.** The detector searches the whole stream for any rate event before examining the last result ([30-03-PLAN.md:98](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-03-PLAN.md:98)). A transient or informational earlier event would override a later successful turn.
- **MEDIUM — the exact session-ID verification can fail without failing the task.** The verify command separates the exact test and module test with `;`, not `&&` ([30-03-PLAN.md:230](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-03-PLAN.md:230)).
- **LOW — `pub` appears broader than needed.** The real consumer can remain `session_id_from_capture`; exposing another public `devflow-core` API should have an identified external caller.

### Suggestions

- Treat the real `status:"allowed"` event as a mandatory negative regression.
- Classify only explicitly documented or observed terminal statuses that mean quota denial. Unknown statuses should defer.
- Scope rate-limit interpretation to the last turn/result interval, not any event in process history.
- Use a synthetic, honestly labelled blocked-event fixture until a real blocked capture exists.
- Replace `;` with `&&` in the verification command.
- Prefer private or `pub(crate)` visibility unless Phase 31 has a concrete cross-crate caller.

### Risk assessment

**HIGH.** As written, Phase 31's normal Claude streams would be routed into the rate-limit recovery path.

---

## Plan 30-04 — Exit timing and pending-close behavior

### Summary

The multi-trial and monotonic-clock design is valuable, but the experiment's environment and outcome schema are not sufficient for reliable Phase 31 conclusions.

### Strengths

- Replacing a remembered 0.38-second sample with at least five archived trials is appropriate.
- Monotonic timing is the correct clock for sub-second exit latency ([30-04-PLAN.md:120](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-04-PLAN.md:120)).
- Recording `local_bash` events while gating only on the observed `background_tasks_changed` set is a reasonable evidence-building approach ([30-04-PLAN.md:124](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-04-PLAN.md:124)).

### Concerns

- **MEDIUM — 30d does not reuse the production-replica launch path.** It extends v3's direct launch and reads the 30c harness only for redaction ([30-04-PLAN.md:90](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-04-PLAN.md:90)). V3 launches `claude` directly and merges stderr at [run_experiment_v3.py:67](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/run_experiment_v3.py:67), unlike production.
- **MEDIUM — `mode_b_outcome` is not a mutually exclusive state.** A trial can exit cleanly and still lose child work, or hang while the child completes. The single-token alternatives at [30-04-PLAN.md:176](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-04-PLAN.md:176) collapse independent observations.
- **MEDIUM — the observation window is unspecified.** It must remain open beyond the 22-second child deadline plus delivery slack before "child work lost" is valid.
- **MEDIUM — cleanup is not operationally specified.** The threat register requires finally-block cleanup, but the action and acceptance criteria do not require a dedicated process group or verified descendant reaping. Killing only `proc` can leave delegated children behind.

### Suggestions

- Factor and reuse the exact 30c launcher for 30d.
- Record Mode B as separate fields: process exited, exit code, post-close result count, drained event observed, child A/B signals, stderr nonempty, cleanup action.
- Set the observation bound relative to the slowest child deadline plus a documented buffer.
- Launch each trial in its own process group and terminate that validated group only after the observation window.

### Risk assessment

**MEDIUM-HIGH.** The measurements may be internally reproducible yet externally unrepresentative or ambiguously classified.

---

## Plan 30-05 — Checkpoint prompt-echo hardening

### Summary

The prompt-echo diagnosis and real-path wiring are strong. Scanning every top-level assistant event, however, broadens the trusted control-signal surface beyond today's final-result semantics.

### Strengths

- The existing function does scan raw stdout before the decoded envelope result at [agent_result.rs:459](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:459), so prompt echo is a real mechanism rather than speculation.
- Excluding `user` events and non-top-level events is supported by the archived provenance fields.
- Wiring through `blocking_human_checkpoint_reported` reaches the actual file wrapper at [agent_result.rs:509](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:509) and its pipeline consumer at [pipeline_launch.rs:489](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_launch.rs:489).
- Positive, negative, co-occurrence, and file-level tests provide good anti-overcorrection coverage.

### Concerns

- **MEDIUM — all top-level assistant narration becomes trusted gate text.** The plan keeps both `assistant` and `result` events ([30-05-PLAN.md:125](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30-05-PLAN.md:125)). Today, the known live checkpoint was captured in the final envelope `result`, not arbitrary intermediate narration ([agent_result.rs:407](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:407)). The v3 stream shows top-level assistant narration duplicated into corresponding result events at [raw_output_v3.jsonl:17](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl:17) and [raw_output_v3.jsonl:19](/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl:19). An assistant discussing a plan's gate syntax could therefore recreate the false positive.
- **LOW — repeated full-stream parsing and concatenation is avoidable.** This adds another complete JSON parse and a potentially large text allocation for a long-running stage.

### Suggestions

- Prefer top-level `result.result` text only, ideally under the same last-result semantics as verdict parsing.
- If assistant-only detection is retained, require archived evidence showing a genuine checkpoint can appear there without a corresponding result.
- Scan selected strings incrementally instead of concatenating all eligible text.

### Risk assessment

**MEDIUM.** It closes direct prompt echo but may replace it with an intermediate-narration false-positive surface.

---

## Cross-plan corrections required

1. Fix Plan 30-03's rate-limit semantics before any Rust implementation.
2. Stage raw experiment output outside committed paths, then sanitize and validate before publishing.
3. Strengthen 30c confirmation to account for both delegated children and task-set drain.
4. Reconcile the formal phase goal. The roadmap promises multi-plan completion at [ROADMAP.md:2271](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:2271), while Phase 30 explicitly has no launch-path change at [ROADMAP.md:2368](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:2368). Either redefine Phase 30 as "parser and feasibility gate" or verification must fail it by construction.
5. Add a conditional stop: if 30c refutes delivery, do not automatically land Plans 30-03/30-05 without identifying an actual stream-output producer. The current adapter remains single-document JSON at [claude.rs:26](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agents/claude.rs:26).

---

## Consensus Summary

Single reviewer — no cross-model consensus to synthesize. See `## Codex Review` above for the full, source-grounded findings. Three blocking (HIGH) issues stand out for immediate attention before execution: the Plan 30-03 rate-limit false-positive, the Plan 30-02 redaction-order/evidence-leak gap, and the Phase 30 goal-vs-scope contradiction flagged in "Cross-plan corrections required" item 4.
