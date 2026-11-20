I have enough evidence. Writing the review.

## CONFIRMED

### F-1 CRITICAL — Two "operator answers" are cited verbatim in the plans and exist nowhere

- `.planning/phases/53-the-question-channel/53-06-PLAN.md:26` — *"Operator answer (1), 2026-11-18, verbatim: \"YES: the pinned package-verification/blocking-human gate rule may be replaced …\""* (also :35, :48, :82).
- `.planning/phases/53-the-question-channel/53-08-PLAN.md:30` — *"Operator answer (2), 2026-11-18, verbatim: \"REFUSE: opencode/pi/hermes/antigravity launches are refused …\""* (also :32, :33, and `53-12-PLAN.md:21,114`).

The record holds exactly two operator answers:
- `53-CONTEXT.md:210-214` — "**Operator answers (2026-11-18)** … D-01: '1 spike now' … D-17: '2 a'". Nothing numbered (1) or (2).
- `53-RESEARCH.md:10` — "Operator answers recorded 2026-11-18: D-01 '1 spike now'…; D-17 '2 a'".
- `53-DISCUSSION-LOG.md:8` — "**Mode:** `--auto` — every selection below is the recommended option taken automatically, **not an operator answer**."

Both plans then act on the fabricated answer. 53-06 replaces text that the tree marks as an operator decision: `crates/devflow-core/src/prompt.rs:88` — *"Package-verification checkpoints keep an unconditional prohibition in `CODE_STAGE_POLICY` (D-02, settled by the operator 2026-09-10; the section text is pinned by `package_verification_prohibition_is_unconditional`, operator decision 2026-09-11). **Do not widen it without a new decision.**"* 53-06 rewrites that pin test to the new text on the strength of answer (1). 53-08 deletes/retargets nine e2e tests on the strength of answer (2). Provenance is the one thing these plans cannot invent.

### F-2 CRITICAL — 53-08 abandons a ROADMAP success criterion and an ASK-01 clause, and edits ROADMAP only to file the shortfall as backlog

- `.planning/ROADMAP.md` Phase 53 criterion 1: "proven live for the two agents already proven live (claude, codex) **and by an e2e test that drives the real binary with a stub agent for the other launch shapes**".
- `.planning/REQUIREMENTS.md:64` ASK-01: "…that **every supported agent** can call…".
- `53-CONTEXT.md` D-02 sequencing note: "The other drivers … are covered by the driver contract's conformance tests **and an e2e test that drives the real binary with a stub agent (roadmap criterion 1)**, not live."

`53-08-PLAN.md:30` refuses four of six drivers at launch, and `53-08-PLAN.md:32` files the missing registrations as a `999.N` BACKLOG entry. No plan provides the criterion-1 stub e2e for those four shapes, and no plan amends criterion 1 or ASK-01. `53-12` then deletes the nine `phase7_cli.rs` tests (`53-12-PLAN.md:67`; names verified present at `crates/devflow-cli/tests/phase7_cli.rs:1823,1858,1894,2024,2062,2103,2158,2195,2233`), so the phase would close green with less per-driver coverage than it opened with.

### F-3 CRITICAL — 53-08's codex wiring contradicts the spike on the operator's global MCP servers

`53-08-PLAN.md:28` enumerates the codex `-c` keys as `mcp_servers.devflow.command`, `.args`, `.tool_timeout_sec`, `.default_tools_approval_mode`, `.enabled` — and nothing else.

`53-MCP-SPIKE.md:36-38` — *"Codex has no `--strict-mcp-config` equivalent: the operator's four global MCP servers would also start, so each launch turned them off with `-c …enabled=false` (scoped to the launch)"* — with the exact keys at `:29-30` (`node_repl`, `cua_repl`, `github`, `mempalace`). `53-MCP-SPIKE.md:110`: *"Codex **additionally needs** per-launch `enabled=false` for each global server to get an isolated set."*

`crates/devflow-core/src/agents/codex.rs:26-66` adds no such disables, so every DevFlow codex launch will spawn the operator's global MCP servers — including MemPalace, which the operator's global rules require be launched only with an explicit ask ("never `--daemon`: the daemon holds the write lock and blocks MCP writes"). The spike is the authority CONTEXT names for D-01 behaviour.

### F-4 MAJOR — 53-13's "bound" is the 3-day gate timeout, so the idle-window hold is unbounded in production

`53-13-PLAN.md:73` sets `question_wait = gate_timeout_secs() + ASK_CLIENT_TIMEOUT_MARGIN_SECS`. `crates/devflow-cli/src/config_parse.rs:23-30`: `const THREE_DAYS: u64 = 3 * 24 * 60 * 60;` is the default. So `question_wait` = 259 260 s.

`53-13-PLAN.md:22` claims "**Bound:** … the idle timeout fires (never an unbounded hold)". The only test of that bound (`53-13-PLAN.md:69`, `a_question_gate_held_past_its_bound_still_trips_the_idle_timeout`) uses a **2 s** bound, so the production value is never exercised; the truth is nominally true and operationally empty. The same file's own rationale names the exact failure this recreates: `config_parse.rs:44-49` — *"capping the 'immortal parked monitor' case that wedged phase 35.1 for nine hours"*. Any leftover `question` gate file at the stage (53-09's backstop only runs *after* the agent exits) holds the monitor for three days.

### F-5 MAJOR — The codex cost of the chosen long block is known, quantified, and unaddressed

`53-MCP-SPIKE.md:79-82` — *"the model polls it with a `wait` function … about every 21 s … the 900 s run made 39 `wait` calls and used **1,346,920 input tokens** (1,306,880 cached) against 63,587 for the instant call … **this bears on whether codex should use the pending/re-call fallback rather than a long block (a choice for planning or the operator)**."*

`53-08-PLAN.md:28` takes the block path with `tool_timeout_sec = T + 60`, where `T` defaults to three days. Extrapolating the spike's own rate (≈1.5k tokens/s of held call) gives ~3.9×10⁸ input tokens for one unanswered codex question. The plan records this as "planner decision per the orchestrator remit" — but the spike routed that specific choice to the operator, and D-06's `pending`/re-call fallback exists precisely for it. No plan caps a hold or budgets the tokens.

### F-6 MAJOR — `.devflow/ask-<NN>.mcp.json` is a new untracked runtime path with no `.gitignore` entry and no coverage guard

`53-08-PLAN.md:113` writes `.devflow/ask-<NN>.mcp.json`. `.gitignore:49-57` covers `state.json`, `state-*.json`, `phase-*-{stdout,stderr.log,exit,agent-pid,monitor-stop}`, `events.jsonl`, `gates/`, `history/` — **not** `ask-*.mcp.json`. `crates/devflow-cli/tests/gitignore_coverage.rs:29-40` is the repo's guard for exactly this class (its doc: *"asserts `git check-ignore` actually matches these paths, so a future `.gitignore` rewrite can't silently regress"*); 53-08's `files_modified` does not include `.gitignore` or that test.

Consequence beyond hygiene: the repo's "clean" verdict is `git status --porcelain`, which counts untracked files — the same mechanism `53-VALIDATION.md` M1 relies on for `devflow --version` reading `(HEAD, clean)` — and a stale registration carrying an old `--run-started-at` survives across runs.

### F-7 MAJOR — `devflow status` reports `gate: none` while an agent is blocked at a question; 53-04 removes the only check that would say so

The runner's gate path commits `gate_pending = true` before opening (`crates/devflow-cli/src/pipeline_gate.rs:750`). The `__ask` server opens the gate directly (`53-01-PLAN.md:82`, `open_question_registered`) and no plan sets `gate_pending`.

- `crates/devflow-cli/src/commands.rs:3066-3070`: `let gate = if state.gate_pending { "pending" } else { "none" };` → the operator's default view prints `gate: none`.
- `crates/devflow-cli/src/commands.rs:4934-4944`: `check_orphan_gate` fires exactly on `!gate_pending && open_gate_stages non-empty && waiter Live` — i.e. the honest diagnostic. `53-04-PLAN.md:104` "fixes" it by exempting `GateKind::Question` instead of making the server set the flag.

So the plan silences the one signal that reports the inconsistency, and the human-facing status line stays wrong.

### F-8 MAJOR — 53-09's `State::abort_requested` is sticky; every later `resume` aborts instantly

`53-09-PLAN.md:23` adds `abort_requested` and sets it in the server ("`workflow::update_state` setting `abort_requested = Some(...)`"); `advance_with` reads it and calls `abort()`. Nothing in any plan clears it — not `abort()`, not `resume`, not the reader. `advance_with` reloads state at entry (`crates/devflow-cli/src/pipeline_launch.rs:2688`), so a resumed run immediately re-enters the `abort_requested` branch and ends with no gate and no message.

The repo's established one-shot pattern is explicit: `relaunch_note` is consumed and cleared (`record_relaunch_note(state, Some(note))` then `after.relaunch_note == None`, `pipeline_outcomes.rs:1707,9310,9370`). 53-09 cites `relaunch_note` at `state.rs:404` as "the field pattern" but copies none of its consumption. `state::tests::abort_requested_round_trips_and_defaults_to_none` cannot catch this.

### F-9 MAJOR — `devflow gate sweep` on an open question aborts the whole run, and races the server's own timeout

`crates/devflow-core/src/gates.rs:935-956`: `reap_answer` *"hard-codes `approved: false` and `abort: true` … the reap always resolves to `GateAction::Abort`"*, and unlike `rate_limit_timer_answer` (`gates.rs:958-975`, which "refuses, before anything is written, any gate whose file does not record `GateKind::RateLimit`") it applies **no kind guard**. Sweep's age threshold is `gate_max_unattended_age`, documented as *"Held EQUAL to `gate_timeout_secs`"* (`config_parse.rs:61-90`), i.e. three days.

A question held past three days is therefore reachable by both the server's `--timeout-secs` (`timed_out` — resumable) and a sweep's reap (abort — not resumable). Whichever writes first wins; if the reap wins, 53-09's abort branch fires and the run dies, recording `question_closed{reason: aborted_by_operator}` for a machine action. No plan mentions `gate sweep`, and no test covers the ordering.

### F-10 MAJOR — 53-03's D-13 truth is false for Validate, and no test can fail on it

`53-03-PLAN.md:22`: *"a failed stage whose final result declares `checkpoint: blocking-human` (**any agent**) opens the never-silent per-stage failure gate, and the gate's reason says …"* — while `53-03-PLAN.md:82` sets the reason "for every agent and stage, then fall through to the existing per-stage dispatch unchanged".

`crates/devflow-cli/src/pipeline_launch.rs:2970-2976`: `Stage::Validate` failures never reach `handle_stage_failure(...reason)`; they route to `handle_validate_outcome(..., ValidateOutcome::Failed)`, which discards `reason`. So on a Validate-stage declaration the sentence never surfaces. Both tests in Task 1 are Code-stage only, so the overbroad "any agent"/"every stage" clause is unfalsifiable as specified.

### F-11 MAJOR — 53-07's `--timeout-secs` does not cover the queue/ask-lock wait, so D-06's "never left blocked" contract has a hole

`53-07-PLAN.md:73` (Task 3): on `AlreadyExists` the worker "probes: `Abandoned` -> remove the file and retry, `Live` -> **wait in cancel-checked slices and retry**". The timeout is only handed to `await_answer` *after* the lock is taken (`53-01-PLAN.md:82`). `53-07-PLAN.md:24` nevertheless states: *"a call ends only by an answer, an abort, a client cancel, stdin EOF, SIGTERM, the run stopped, the runner lock no longer live, or `--timeout-secs`"*.

A second call queued behind a live ask lock therefore has **no** timeout path: it can wait indefinitely and never produce the typed `devflow_ask closed: <reason>` result the contract promises. The tests (`53-07-PLAN.md:150` `a_second_server_on_the_same_stage_waits_for_the_ask_lock`) pass no `--timeout-secs` and assert only that the gate eventually opens.

### F-12 MINOR — 53-03 must edit a file it does not declare, or leave a dangling intra-doc link

`53-03-PLAN.md:120` deletes `verify::phase_has_blocking_human_checkpoint` and says "Fix doc comments that named a deleted item so none links to it" — but `crates/devflow-core/src/prompt.rs:757` contains `/// the only trigger for this resume, \`verify::phase_has_blocking_human_checkpoint\`,` and `crates/devflow-core/src/agent_result.rs:666` contains a `[\`crate::verify::phase_has_blocking_human_checkpoint\`]` link. `prompt.rs` is 53-06's file (`53-06-PLAN.md:9`), not in 53-03's `files_modified` (`53-03-PLAN.md:3-9`). Harmless under `scripts/check.sh all` (fmt + clippy + test only — `scripts/check.sh:11`, no `cargo doc`), so this is a declared-file-set leak, not a red gate.

### F-13 MINOR — 53-10's deletion inventory misses a test it must delete

`53-10-PLAN.md:73` lists `recorded_approval_for test 6180` and "`checkpoint_resumes` reset" but not `pipeline_launch::tests::launch_stage_inner_resets_checkpoint_resumes_counter` (`crates/devflow-cli/src/pipeline_launch.rs:5510`), which asserts on the field 53-10 removes. It would be caught only indirectly, by 53-11's `checkpoint_resumes` removed-surface token at wave 6.

---

## SUSPECTED

### S-1 MAJOR (speculation about the intended design, verified gap) — 53-07's run-binding contradicts 53-01's fixture, and the fixture file is not in 53-07's file list

`53-07-PLAN.md:143` asserts "the same refusal for a `--run-started-at` that differs from the state's; **control: the normal fixture opens a gate**". For that control to hold, `AskFixture`/`AskClient` must already pass the saved state's `started_at` at 53-01, where the flag is "accepted here and checked from 53-07" (`53-01-PLAN.md:28`) — no 53-01 task says so, and `crates/devflow-cli/tests/common/ask_client.rs` is in `53-01-PLAN.md:15` but **not** in `53-07-PLAN.md:5-9`, while every 53-01/53-05/53-07 e2e test depends on it. Either the binding check is untestable as written or a wave-1 file must change in wave 3 without being declared.

### S-2 MAJOR (speculation: depends on how DevFlow launches the legacy claude path) — 53-08 exercises claude's wiring only in a unit test

Roadmap criterion 1's automated half is satisfied for codex only: `53-08-PLAN.md:141` drives a stub **codex**. Claude's `--mcp-config` splice is pinned by `agents::claude::tests::launch_args_name_the_ask_registration` (`53-08-PLAN.md:104`) and never by a real argv through `prepare_stage_launch`. Claude has two launch shapes in the tree (`crates/devflow-core/src/agents/claude.rs:41-58` stream-json with no prompt, and the pre-31 positional `-p <prompt>` shape selected by `claude_stream_launch_enabled`), and `Append` puts `--mcp-config` *after* the positional prompt on the second shape — untested for either CLI's parser. I did not run either CLI.

### S-3 MINOR — 53-12's "same capture shape" justification for deleting the hermes copies is asserted, not shown

`53-12-PLAN.md:21` deletes three `hermes_*` tests as "duplicates of the same properties **on the same capture shape**". Hermes and codex are separate drivers with separate `parse_completion` implementations (`crates/devflow-core/src/agents/hermes.rs` vs `codex.rs:67`); the phase7_cli tests are that capture-shape pinning. Before refusing a launch, at minimum the deleted assertions should be shown to be driver-independent. 53-12 deletes test coverage for four drivers in the same phase that removes those drivers from the product, with the justification supplied by F-1's fabricated answer (2).

### S-4 MINOR — `53-VALIDATION.md:34` "Estimated runtime ~300 seconds (full suite, warm)" and the "Feedback latency < 300s" checklist item are unverified

`scripts/check.sh all` runs `cargo test --workspace --no-fail-fast` (`scripts/check.sh:57-62`) over a workspace with 6 454-line `gates.rs` and 11 k-line `pipeline_launch.rs`, plus process-spawning e2e suites (`failover_e2e`, `phase7_cli`, `runner_e2e`). I did not run it, so I cannot contradict the number — but nothing in the plan set measures it, and an unmet 300 s target is not falsifiable by any task.

---

**Wave/`depends_on` check: no finding.** Levels are consistent: 53-01/02/03/12 = 1 (no deps); 53-04/05 (←01) and 53-06 (←03) = 2; 53-07 (←05,02) and 53-08 (←01,03,04,12) = 3; 53-09 (←07,08) = 4; 53-10 (←09,04,08) = 5; 53-11 (←10,06) and 53-13 (←08,10) = 6. Every declared `wave` equals its DAG level.

**Line-reference spot check: no finding.** The references that matter for `read_first` are accurate against the checkout (`open_registered` 683, `await_answer` 859, `claim` 1471, `close` 1022, `render_gate_context` 1813, `fire_gate_notify_with_fallback` 1778; `relaunch_checkpoint_session` 1823, `resume_launch_shape` 276, `advance_with` 2679, `close_gates_for_resume` 2419, `runner_stage_spec` 1555, `a_handoff_reruns_the_delivery_canary_for_the_new_agent` 8415; `AnswerFamily` 545, `screen_answer` 966, `way_on_sentence` 884; `check_orphan_gate` 4934; `validate_fallback_agent` 941; all six `ensure_agent_binary(` production sites match `53-08-PLAN.md:113`; `REMOVED_SURFACE_TOKENS` really is 27 + 2 = 29 with the four cited indices intact).

**One thing I could not verify and did not assume:** whether `--mode auto` runs can arm *any* fallback after 53-08. `53-08-PLAN.md:141`'s own backlog text says no ("no `--mode auto` run can arm a fallback until antigravity has a registration"), which contradicts `crates/devflow-cli/src/main.rs:80-81` and `:127` ("in `--mode auto` it must be `claude` or `antigravity`") and silently removes supervisor-mode failover. If that is intended, it is an operator decision, not a `999.x` entry.
