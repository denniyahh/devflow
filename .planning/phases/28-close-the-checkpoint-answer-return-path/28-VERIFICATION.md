---
phase: 28-close-the-checkpoint-answer-return-path
verified: 2026-07-31T00:00:00Z
status: human_needed
score: 8/9 must-haves verified
behavior_unverified: 1
overrides_applied: 0
human_verification:
  - test: "Run one real headless phase whose plan carries a genuine `gate=\"blocking-human\"` task via `devflow start`, from a context not subject to a Claude Code agent-session permission classifier (i.e. DevFlow's own actual `monitor` process, not a worktree sub-agent), and capture `.devflow/phase-NN-stdout`."
    expected: "The captured stdout contains the literal substring `**Gate:** blocking-human` (case-insensitive label, exact value), so `agent_result::blocking_human_checkpoint_reported` returns `true` and the checkpoint is auto-resolved via `relaunch_checkpoint_session`, recorded by exactly one `checkpoint_auto_decided` event in `.devflow/events.jsonl`."
    why_human: "RESEARCH.md's Assumption A1 / Pitfall 2: the `Gate:` field crosses two indirections (gsd-executor subagent emission → execute-phase.md orchestrator relay → DevFlow's captured top-level stdout) that no unit test can exercise, because it depends on Claude Code's own orchestrator-session rendering under a real headless run with no human present. 28-PROBE.md's own live-probe attempt was denied at the probing executor's Bash-tool permission classifier before the `claude` subprocess ever spawned (verdict: DIVERGENT, not CONFIRMED or HUNG). Every unit test in this phase proves the READER logic is correct against the *documented/predicted* literal, not that the literal is what a real run actually produces. WINDOWS.md entry #6 tracks this open."
---

# Phase 28: Close the Checkpoint Answer Return Path Verification Report

**Phase Goal (as narrowed by 28-CONTEXT.md):** Recognize a `gate="blocking-human"` checkpoint correctly, and let DevFlow resolve it unattended, so a checkpoint stops being a dead end for a headless DevFlow-driven run. Building a human-answer relay or notification interface is explicitly out of scope (D-08–D-11).

**Verified:** 2026-07-31
**Status:** human_needed
**Re-verification:** No — initial verification

## Central Finding First

**The phase's headline capability — a real `gate="blocking-human"` checkpoint fired by a live headless DevFlow run actually being recognized and auto-resolved — has never been observed working end-to-end.** This is not a hidden gap the executors tried to obscure; it is the single most heavily and honestly flagged fact in this phase's own artifacts, and every place a future reader would form a belief about this working correctly propagates the caveat faithfully:

1. **28-PROBE.md** records the A1 probe's own invocation was denied at the probing executor's Bash-tool permission classifier before the `claude` subprocess ever spawned. Verdict recorded honestly as `DIVERGENT` (not CONFIRMED, not HUNG) — the task's own explicitly anticipated third case, followed to the letter.
2. **`agent_result.rs`'s `HUMAN_GATE_VALUE` doc comment** (source code, not just a SUMMARY) states in ~15 lines, in bold: *"This literal was NEVER confirmed against a live end-to-end run"* and names exactly what would confirm it. This is the artifact a future engineer actually reads while working in this file — the caveat lives where it will be seen, not just in a planning document.
3. **`blocking_human_checkpoint_reported`'s doc comment** cross-references the same caveat and states plainly that a false negative (checkpoint present but reader misses it) is the safe direction.
4. **28-02-SUMMARY.md** documents it in both `key-decisions` and a dedicated `human_judgment: true` coverage entry with an explicit rationale.
5. **28-03-SUMMARY.md leads with it** — literally the first paragraph, in bold, exactly as its own PLAN.md instructed ("If `28-PROBE.md` recorded a HUNG verdict, lead the summary with that finding" — DIVERGENT is not HUNG, but the executor correctly generalized the instruction's intent and led with the caveat anyway).
6. **`.planning/WINDOWS.md` entry #6** — a formal, machine-tracked, project-wide open-item ledger, `kind: unmet-truth`, `status: open`. This is the strongest form of propagation: it survives outside this phase's own directory and will surface in any future `devflow doctor`/WINDOWS-sweep.

**I found no place in the codebase or its documentation where this is overstated as "confirmed working."** Every doc comment, every SUMMARY, and the formal WINDOWS.md ledger consistently use "unconfirmed default," "predicted," or "DIVERGENT" — never "confirmed" or "verified end-to-end." This is unusually disciplined self-reporting and I want to say so plainly rather than treat honesty as suspicious.

**What this means for phase status:** because the central deliverable (D-01's confirmation half + D-03/D-04's resume mechanism, working against a *real* checkpoint) is a state-transition/behavioral claim that no test — unit or otherwise — exercises, it cannot be marked VERIFIED on code presence alone, per this verification's own methodology. It is `⚠️ PRESENT_BEHAVIOR_UNVERIFIED` and routes to human verification (see frontmatter). This is not a code defect; it is an environmental limitation of the executing sandbox that the phase's own artifacts already correctly identified and cannot self-resolve.

**Concrete minimal action to close A1** (per 28-PROBE.md's own stated remaining path, which I confirm is the correct and only remaining step): run one real `devflow start --phase N --agent claude --mode auto` against a synthetic phase containing exactly one `checkpoint:human-verify` task with `gate="blocking-human"`, launched from DevFlow's own actual `monitor` process (not a Claude Code agent/worktree sub-session, which is what blocked 28-01's own probe attempt), and inspect the resulting `.devflow/phase-NN-stdout` for the literal `**Gate:** blocking-human`. If found, flip WINDOWS.md entry #6 to resolved and this phase's headline claim becomes CONFIRMED.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | DevFlow can statically determine whether a phase declares a `gate="blocking-human"` checkpoint, without launching anything (D-01 static half) | ✓ VERIFIED | `verify::phase_has_blocking_human_checkpoint` (`crates/devflow-core/src/verify.rs:104-`), 6 named unit tests covering positive/negative/multi-plan/non-plan-file/missing-directory cases, all green. Discriminates `blocking` from `blocking-human` correctly (Phase 26 near-miss class). |
| 2 | DevFlow can confirm (from captured stdout) that a declared checkpoint was actually reported, distinct from an unrelated ordinary failure (D-01 confirmation half) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `agent_result::blocking_human_checkpoint_reported` fully implemented and unit-tested against the RESEARCH-predicted/documented literal `**Gate:** blocking-human` — but that literal itself was never observed against a live headless run (A1/DIVERGENT). See Central Finding. |
| 3 | A confirmed checkpoint is resolved by resuming the exact exited Claude session (not a fresh spawn), unconditionally, with no flag/config toggle (D-03/D-04) | ✓ VERIFIED (mechanism) / see Truth 2 for end-to-end | `ClaudeAgent::exec_resume_command` (`agents/claude.rs:64-77`) builds the correct argv (print flag, instruction, `--resume` immediately followed by session id, `--output-format json`, `--dangerously-skip-permissions`). `resume_command_includes_permission_bypass` asserts both re-passed flags by name. No flag/config gate exists anywhere in the dispatch — confirmed via source read of the `Action::GateReview` arm. |
| 4 | The resume relaunch re-passes both the permission-bypass and JSON-output flags (Pitfall 1 regression guard) | ✓ VERIFIED | Source at `agents/claude.rs:64-77` shows both flags present; `resume_command_includes_permission_bypass` test is a dedicated, named regression guard with an explanatory panic message; doc comment explicitly instructs a future reader not to delete it as "obviously redundant." |
| 5 | A non-Claude agent never takes the resume path; `AgentAdapter` trait is untouched (D-05) | ✓ VERIFIED | `git log --oneline -- crates/devflow-core/src/agents/mod.rs` shows the trait's last touch predates phase 28 entirely (commit `3225fd1`, Phase 17). `exec_resume_command` is an inherent `impl ClaudeAgent` method, confirmed by direct source read. `advance_with_non_claude_agent_never_resumes` test exists and passes. |
| 6 | `session_id` cannot be forged by the agent to redirect which session DevFlow resumes into (T-28-04) | ✓ VERIFIED | `pub struct AgentResult` (source-confirmed, `agent_result.rs`) carries no `session_id` field. `claude_session_id` reads only the envelope's top-level key via direct `Value::get`, never the module's nested-traversal helpers. `session_id_in_devflow_result_marker_is_not_returned` constructs an envelope with differing top-level/embedded-marker ids and asserts the top-level one wins — confirmed present and passing in source. |
| 7 | Every auto-decided checkpoint leaves a durable, unconditional audit record before the relaunch spawns (D-07) | ✓ VERIFIED | `relaunch_checkpoint_session` (`pipeline_launch.rs:205-232`) emits `checkpoint_auto_decided` via `events::emit` BEFORE calling `spawn_agent_and_record` — confirmed by direct source read of the ordering. Carries stage, session_id, capped instruction, attempt number, and a policy field naming D-03. |
| 8 | A stuck checkpoint loop is bounded; exhaustion falls through to the never-silent gate with a reason naming the exhaustion | ✓ VERIFIED | `mode::MAX_CHECKPOINT_RESUMES` (value 3) checked in the `Action::GateReview` arm; `augment_unresolved_checkpoint_reason` names the exhaustion in the gate context. Two named tests (`advance_at_checkpoint_resume_ceiling_falls_through_to_generic_gate`, and the counter-reset test) confirmed present. |
| 9 | The D-01 static scan is strictly evaluated before the agent-controlled confirmation (T-28-01 mitigation) | ✓ VERIFIED | Source at `pipeline_launch.rs:489-491` shows the conjunction ordered: `agent == Claude && phase_has_blocking_human_checkpoint(...) && checkpoint_reported_in_capture(...)` — the static scan is the second-evaluated, first-agent-uncontrolled condition, strictly before the capture read due to Rust's short-circuit `&&` evaluation order and the source line ordering itself. |

**Score:** 8/9 truths verified by direct source inspection (1 present-and-wired-but-behavior-unverified, per methodology — see frontmatter `human_verification`).

### D-14 (Define headless safety) and D-15 (`--until` cap preservation) — separately verified, both clean

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| D-14a | A headless Define stage with no CONTEXT.md never invokes the discuss-phase command | ✓ VERIFIED | `define_stage_prompt` (`prompt.rs:181-194`) source-confirmed to contain no discuss-phase command string in either branch; explicitly instructs the agent not to run one. `define_prompt_never_invokes_discuss_phase` test present and green. |
| D-14b | The Plan stage's identical-looking idempotency branch is untouched | ✓ VERIFIED | `idempotent_stage_prompt` narrowed to `(phase: u32)`, Plan-only; `plan_prompt_is_idempotent` test present and green; `Stage::Define.gsd_command()`'s returned string confirmed byte-identical pre/post (doc-comment-only diff). |
| D-15 | An unfired `--until` cap survives `devflow resume` | ✓ VERIFIED | Source at `pipeline_launch.rs:305-323` shows the three-field clear (`stopped`/`stop_reason`/`stop_until`) wrapped in `if state.stopped { ... }`. `resume_preserves_unfired_until_cap`, `resume_clears_stop_marker_and_advances_past_stop_point`, and `resume_without_a_cap_is_unchanged` all present. |

### D-12/D-13 (`yes_ship` config persistence) — verified, clean

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| D-12a | `yes_ship` resolvable from `devflow.toml` / `DEVFLOW_YES_SHIP`, CLI flag still wins | ✓ VERIFIED | `config::yes_ship(project_root)` mirrors `external_verify_enabled`'s exact resolver shape; `commands::start` combines via logical OR. |
| D-12b | A config-sourced authorization is never silent | ✓ VERIFIED | `commands::start` prints a one-line notice naming `devflow.toml` when config alone supplied the authorization; `print_dry_run` reports the resolved state. Both covered by `crates/devflow-cli/tests/yes_ship_config.rs`'s 5 CLI-boundary tests. |
| D-13 | The Ship gate still fires and records explicit attribution; `yes_ship` never bypasses it | ✓ VERIFIED | `handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution` and `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` confirmed byte-for-byte untouched by every phase-28 commit (their continued green status IS the D-13 evidence, per the plan's own framing). |

## D-03 unconditional-decide fidelity (no flag/config gate)

Verified by direct source read: no environment variable, config key, or CLI flag anywhere gates entry into the `Action::GateReview` checkpoint-resolution arm. The five-condition guard in `pipeline_launch.rs:489-519` consists entirely of: agent kind, the static plan-declared scan, the capture confirmation, session-id presence, and the resume ceiling — none of which is an operator opt-in/opt-out toggle. D-06 (rejected: opt-in flag / umbrella merge with `yes_ship`) holds — `yes_ship` and the checkpoint-decide path share no code, no config key, and no dispatch condition.

## D-04 forgery-guard fidelity (security constraint)

Verified directly in source, not merely asserted by the SUMMARY: `pub struct AgentResult { ... }` (full struct read, `agent_result.rs`) contains no `session_id` field. `claude_session_id` performs a direct `value.get("session_id")` on the top-level envelope `Value`, never the module's `json_find_key`/`json_scan` nested-traversal helpers (confirmed absent from the function body by source read). The regression test `session_id_in_devflow_result_marker_is_not_returned` constructs an envelope whose top-level id and whose embedded `DEVFLOW_RESULT` marker id differ, and the test is present and passing.

## Self-referential hazard — confirmed still holds

`rg -l 'gate="blocking[-]human"' .planning/phases/28-*/28-*-PLAN.md` returns no matches (exit code 1), confirmed live during this verification. Every fixture in `verify.rs`, `agent_result.rs`, and `pipeline_launch.rs` that needs the literal constructs it via a `const HUMAN_GATE_VALUE` / `format!`, never as a bare source literal — confirmed by direct source read.

## Full Workspace Green — confirmed live, not trusted from SUMMARY

Ran `scripts/check.sh all` directly during this verification (not re-reading a prior claim): **`==> check.sh: all OK`** — 452 devflow-cli unit tests + integration test binaries all passing, plus the devflow-core suite, fmt clean, clippy clean.

## Executor-Reported Deviations — Adjudicated

1. **28-02 modified `crates/devflow-cli/tests/log_format_env.rs` outside declared scope, and wrote a WINDOWS.md entry.** Confirmed correct: this file constructs `State` via a manual struct literal (a legacy-state-json fixture) that would not compile once the two new `#[serde(default)]` fields were added — a genuine, necessary compile-fix, not scope creep. Verified live: the file does contain `session_id: None, checkpoint_resumes: 0,` at the expected location. The WINDOWS.md entry (#6) referenced in this task description turned out to be the A1-unconfirmed-default tracking entry, not a separate "wrote an entry" claim about a different file — confirmed it exists, is well-formed JSON, `status: open`, correctly describing the unconfirmed literal. **Adjudication: correct, appropriately recorded, non-blocking.**
2. **28-04 dropped the `Stage::Define` case from `each_stage_prompt_carries_its_gsd_command_and_marker` and restructured `idempotent_stage_prompt` to `(phase)`-only.** Confirmed necessary: Define's prompt must no longer contain the discuss-phase command by design (D-14), so a test asserting every stage's prompt contains its own GSD command would fail post-fix unless Define's case is removed — this is a required consequence of the locked decision, not an undisclosed regression. Both split tests (`plan_prompt_is_idempotent`, `define_prompt_never_invokes_discuss_phase`) confirmed present and passing. **Adjudication: correct, properly disclosed as a deviation, non-blocking.**
3. **28-06 renamed `config_file_with_yes_ship_key_loads_but_never_sets_the_flag` → `state_new_alone_never_derives_yes_ship_from_config`, preserving the assertion.** Confirmed correct via source read of `commands::start`: `State::new` genuinely takes no project-config input, so its assertion (`State::new` alone never derives `yes_ship` from config) remains true after D-12 — only the surrounding doc-comment premise ("`DevflowConfig` has no field of that name") became false. The pattern-mapper's correction (documented in 28-PATTERNS.md and cross-referenced in the plan) was the right call; inverting the assertion would have been the actual defect. **Adjudication: correct, non-blocking.**
4. **28-03 ran `git stash` once during setup, self-recovered by popping immediately.** Verified live: `git stash list` on the current `develop` HEAD is empty — no orphaned stash exists. Combined with the orchestrator's own recorded verification, nothing was lost. This is a genuine prohibited-operation violation in worktree mode that self-corrected before causing damage; it is disclosed, not hidden. **Adjudication: correct self-recovery confirmed independently; worth a process note for future worktree-mode executors to avoid `git stash` entirely (worktrees share `refs/stash`), but not a phase-blocking finding.**

## Scope Discipline — confirmed clean

- All four units (28a/28b/28c/28d) plus D-12 genuinely landed with real, tested code — none was hollowed out. Verified: `verify::phase_has_blocking_human_checkpoint` (28a static), `blocking_human_checkpoint_reported` + resume mechanism (28a/28b dynamic + resolve), `define_stage_prompt` (28c), the `if state.stopped` guard (28d), and the `yes_ship` config resolver (D-12) are all real, non-stub implementations backed by passing tests, confirmed by direct source read of each.
- No out-of-scope work landed: `rg -n "gate answer|GateAnswer"` across `crates/` returns nothing (no human-answer verb); `exec_resume_command` exists only on `ClaudeAgent`, confirmed no Codex/OpenCode accommodation; no `CHECKPOINT-ANSWERS.json`/Part-B fallback file or reference exists anywhere in `crates/` or the phase directory; `git diff` history shows no Ship-gate redundancy cleanup landed (D-13's untouched tests confirm this).

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `verify::phase_plan_files` / `phase_has_blocking_human_checkpoint` | D-01 static scan | ✓ VERIFIED | `crates/devflow-core/src/verify.rs:34-`, source-confirmed, tested, wired into `pipeline_launch.rs`'s dispatch. |
| `agent_result::claude_session_id` / `session_id_from_capture` | D-04 session capture | ✓ VERIFIED | Source-confirmed, forgery-guarded, tested. |
| `agent_result::blocking_human_checkpoint_reported` / `checkpoint_reported_in_capture` | D-01 confirmation | ⚠️ IMPLEMENTED, UNCONFIRMED LITERAL | Fully implemented, wired, tested against a documented prediction — see Central Finding. |
| `state.session_id` / `state.checkpoint_resumes` | D-04 persistence | ✓ VERIFIED | Source-confirmed `#[serde(default)]`, round-trip tests present. |
| `ClaudeAgent::exec_resume_command` | D-04/D-05 resume primitive | ✓ VERIFIED | Source-confirmed inherent method, correct argv, Pitfall-1-guarded. |
| `mode::MAX_CHECKPOINT_RESUMES` | Bounded loop | ✓ VERIFIED | Source-confirmed, `= 3`, enforced in dispatch. |
| `pipeline_launch::relaunch_checkpoint_session` | D-04/D-07 relaunch + audit | ✓ VERIFIED | Source-confirmed, audit-before-spawn ordering verified. |
| `prompt::define_stage_prompt` | D-14 | ✓ VERIFIED | Source-confirmed no discuss-phase invocation. |
| `resume()`'s guarded clear | D-15 | ✓ VERIFIED | Source-confirmed `if state.stopped`. |
| `config::DevflowConfig::yes_ship` / resolver | D-12 | ✓ VERIFIED | Source-confirmed, mirrors `external_verify_enabled`. |

### Key Link Verification

| From | To | Via | Status |
|------|-----|-----|--------|
| `verify::phase_has_blocking_human_checkpoint` | `pipeline_launch::advance`'s `Action::GateReview` | Static scan evaluated before capture confirmation | ✓ WIRED (source-confirmed order) |
| `agent_result::checkpoint_reported_in_capture` | Same dispatch arm | Confirmation, second-evaluated | ✓ WIRED |
| `agent_result::session_id_from_capture` | `state.session_id` | Persisted for every evaluated stage, unconditionally | ✓ WIRED (source-confirmed placement immediately after `advance_evaluated`) |
| `state.session_id` | `ClaudeAgent::exec_resume_command` | Only invoked via `relaunch_checkpoint_session` when `Some` | ✓ WIRED |
| `28-PROBE.md`'s reader contract | `HUMAN_GATE_VALUE` constant | Literal matches predicted rendering | ✓ WIRED to the documented prediction, ⚠️ not to an observed fact |

### Anti-Patterns Found

None. No `TBD`/`FIXME`/`XXX`/`TODO`/placeholder markers found in any file touched by this phase's six plans. No stub implementations (`return null`, empty handlers, hardcoded empty data flowing to production paths). All "Known Stubs" sections in every SUMMARY correctly report "None."

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace suite green | `scripts/check.sh all` | `==> check.sh: all OK` | ✓ PASS (run live during this verification) |
| Self-referential hazard clean | `rg -l 'gate="blocking[-]human"' .planning/phases/28-*/28-*-PLAN.md` | exit 1, no matches | ✓ PASS |
| `AgentAdapter` trait untouched | `git log -- crates/devflow-core/src/agents/mod.rs` | last touch predates phase 28 (commit `3225fd1`) | ✓ PASS |
| `git stash list` empty | direct check | empty | ✓ PASS |
| Real end-to-end checkpoint recognition against a live run | — | not runnable in this sandbox (same classifier restriction 28-PROBE.md hit) | ? SKIP — routes to human verification |

### Requirements Coverage

No REQUIREMENTS.md / REQ-IDs exist for this project (tracked by backlog identifier and CONTEXT.md decision IDs, consistent with prior phases — not a finding).

| Backlog ID | Description | Status | Evidence |
|------------|-------------|--------|----------|
| 999.57 / DEN-82 (parts A+C) | Checkpoint recognition + unconditional resolve + audit | ✓ SATISFIED (mechanism) / ⚠️ end-to-end unconfirmed | See truths 1-9 above and Central Finding. |
| 999.59 / DEN-84 | Define headless safety | ✓ SATISFIED | See D-14 table. |
| 999.60 / DEN-85 | `--until` cap preservation | ✓ SATISFIED | See D-15 table. |
| D-12/D-13 | `yes_ship` config persistence | ✓ SATISFIED | See D-12/D-13 table. |

No orphaned requirements found — WINDOWS.md entry #6 correctly maps to phase 28.

## Deferred Items (out of scope, correctly not built)

Confirmed absent from the codebase, matching `<deferred>` in 28-CONTEXT.md: human-answer path for checkpoints, notification/response interface, Ship-gate redundancy cleanup, cross-agent (Codex/OpenCode) checkpoint resolution. None were fabricated or partially scaffolded.

## Gaps Summary

There is exactly one substantive gap, and it is the phase's own headline claim: **the end-to-end recognition of a real `blocking-human` checkpoint has never been observed working.** Every downstream mechanism (resume argv, audit trail, dispatch guard ordering, bounding, D-14, D-15, D-12/D-13) is genuinely implemented, source-verified, and test-covered — this is not a case of tasks completing while the goal is missed by omission. It is a case where the phase's own artifacts identified, at Wave 1, a single genuine unknown (RESEARCH's Assumption A1), attempted to resolve it empirically, hit an environmental wall outside DevFlow's own control, recorded that honestly at every downstream layer including a formal WINDOWS.md ledger entry, and built everything else against the documented prediction rather than stalling the phase. That is the correct response to an unconfirmable assumption under a sandbox restriction — but it does mean a human (or a probe run from an unrestricted context) must still close A1 before this phase's stated goal can be called fully achieved, not merely fully implemented.

---

_Verified: 2026-07-31_
_Verifier: Claude (gsd-verifier)_
