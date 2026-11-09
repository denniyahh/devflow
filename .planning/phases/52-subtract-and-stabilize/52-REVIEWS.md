---
phase: 52
reviewers: [codex, antigravity, pi]
reviewed_at: 2026-11-09T20:54:42Z
plans_reviewed: [52-01-PLAN.md, 52-02-PLAN.md, 52-03-PLAN.md, 52-04-PLAN.md, 52-05-PLAN.md, 52-06-PLAN.md, 52-07-PLAN.md, 52-08-PLAN.md, 52-09-PLAN.md, 52-10-PLAN.md, 52-11-PLAN.md, 52-12-PLAN.md, 52-13-PLAN.md, 52-14-PLAN.md, 52-15-PLAN.md, 52-16-PLAN.md, 52-17-PLAN.md, 52-18-PLAN.md, 52-19-PLAN.md, 52-20-PLAN.md, 52-21-PLAN.md, 52-22-PLAN.md, 52-23-PLAN.md, 52-24-PLAN.md, 52-25-PLAN.md, 52-26-PLAN.md, 52-27-PLAN.md, 52-28-PLAN.md, 52-29-PLAN.md, 52-30-PLAN.md, 52-31-PLAN.md, 52-32-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  antigravity: "gemini-3.8-flash-high (effort=high)"
  pi: "deepseek-flash"
model_sources:
  codex: "cli-flag"
  antigravity: "cli-flag"
  pi: "cli-flag"
---

# Cross-AI Plan Review — Phase 52

> Provenance: these lanes were run by the host session with the `external-review` skill (adversarial lens) on a
> `git archive` snapshot of feature/phase-52 @ e6b3343, not by `/gsd-review`. Every finding was then checked by
> the host against the snapshot source. The Consensus Summary below is the outcome of that check and is the list to
> act on. The per-reviewer sections are the raw output. Findings listed under "Rejected by host verification" were
> disproved against source and need no plan change beyond a one-line rejection in the ledger.
> Replanning with `/gsd-plan-phase 52 --reviews` runs in an agent session, not through `devflow start`, because
> DevFlow's Plan stage prompt has no `--reviews` route (`crates/devflow-core/src/prompt.rs` builds a plain
> `/gsd-plan-phase N`). That makes this a hand-run stage, which is an exception under the dogfooding rule (the operator chose
> this replan route on 2026-11-09).

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## Codex Review

### CONFIRMED

**F1 — High — An uncovered quick fix can finish without being re-verified.**

- **Location:** `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:178`; `52-28-PLAN.md:24`.
- **Quoted text:** “a non-empty remainder routes to the … stale re-verify relaunch and post-check”; “if it is `passed` or `gaps_found`, Validate runs”.
- **Why it breaks:** Add an uncovered file, launch re-verification, let the agent skip the verifier, and return the unchanged report. GSD still reports `passed`; the post-check accepts it. The proposed negative control only covers an actually `stale` report.
- **Evidence checked:** `52-GAP-LOOP-VERIFICATION.md:92–100` records both ingredients: an agent skipped re-verification, and adding an uncovered file left status `passed`. The source’s report-location/fingerprint helpers are in `crates/devflow-core/src/agent_result.rs:3817` onward.
- **Suggested fix:** Persist the outstanding re-verification obligation and require fresh GSD evidence tied to the fix before clearing it. Add a control where the re-verification agent returns success while leaving the old `passed` report unchanged.

**F2 — High — Approve can finish against verification evidence that changed during the wait.**

- **Location:** `52-29-PLAN.md:34`.
- **Quoted text:** “`approve_route` … follows the recorded status … `passed` → finish”.
- **Why it breaks:** Open a clean-pass gate, change a covered file or replace the report while waiting, then approve. Both routers still consume the gate’s old `passed` value. The run finishes although a current GSD read would return `stale` or `gaps_found`. This defeats D-08’s verification guarantee.
- **Evidence checked:** `crates/devflow-cli/src/pipeline_gate.rs:633–692` waits, claims the answer, and maps it without refreshing verification. `config_parse.rs:16–43` allows a three-day default wait. No later plan requires a freshness check at approval pickup.
- **Suggested fix:** Refresh verification before consuming a finish approval. If the evidence changed, refuse that approval and reopen with the current basis. Test both unchanged and changed evidence during a pending gate.

**F3 — High — Plan 16 requires a green suite before Plan 17 repairs a test it necessarily breaks.**

- **Location:** `52-16-PLAN.md:82`, `52-04-PLAN.md:97`, `52-17-PLAN.md:160`.
- **Quoted text:** “Set `State::new`’s stage to `Stage::Plan`”; “If the witness keeps `--until define`, leave it”; the witness changes to `--until plan` only in Plan 17.
- **Why it breaks:** After Plan 16, no run reaches Define, so that witness’s stop condition cannot fire. Nevertheless, Plan 16’s final gate runs `scripts/check.sh all`.
- **Evidence checked:** `crates/devflow-cli/tests/supervisor_e2e.rs:439–451` passes `--until define` and waits for stopped state. `pipeline_gate.rs:73` halts only when `stop_until == Some(from)`.
- **Suggested fix:** Move the affected e2e migrations into Plan 16 alongside the start-stage change. Plan 17 should only remove the now-unused variant.

**F4 — High — The stage-removal tasks cannot pass their own gates in the specified order.**

- **Location:** `52-15-PLAN.md:88–89`, `:115`, `:176`; analogous split at `52-17-PLAN.md:83`.
- **Quoted text:** “until Task 2 lands the CLI crate does not build”; “commit Tasks 1 and 2 together **if** … [a] hook builds the workspace”.
- **Why it breaks:** Workspace compilation is a correctness requirement regardless of hook configuration. Additionally, Plan 15 Task 1 removes review-angle configuration and runs the entire core library suite, while the documentation removal is deferred to Task 3. Its `doc_check` gate therefore encounters documentation naming removed source identifiers.
- **Evidence checked:** CLI `Stage::Ship` consumers remain in `pipeline_launch.rs:4003` and elsewhere. `doc_check.rs:365–372` rejects documented environment variables absent from source. Review-angle mentions exist in `README.md:175`, `ARCHITECTURE.md:441`, and `docs/guides/configuration.md:50`.
- **Suggested fix:** Make variant removal, all consumers, affected tests, and corresponding documentation one unconditional atomic task/commit. Run workspace verification at that boundary.

**F5 — High — Historical events make a fresh run look prepared.**

- **Location:** `52-16-PLAN.md:112–114`.
- **Quoted text:** “true when … no `workflow_started` event exists **for the phase**”.
- **Why it breaks:** Run a phase once, then start it again and interrupt preparation. The earlier run’s event remains, so the fresh reservation is classified as already launched. The proposed tests only exercise a fresh history.
- **Evidence checked:** `crates/devflow-core/src/events.rs:1–17,63–67` defines an append-only project log. `commands.rs:1148–1169` emits `workflow_started` without a run-generation identity. The existing `run_never_prepared` at `commands.rs:888–890` instead uses persisted preparation evidence.
- **Suggested fix:** Use preparation state belonging to the current run generation. Do not infer it from any historical event for that phase. Add a previous-run-history control and an unavailable-event-log case.

**F6 — High — Stopping a gap pass destroys the information needed to resume it correctly.**

- **Location:** `52-27-PLAN.md:101–103`.
- **Quoted text:** “a persisted gap-closure marker … cleared when … the run stops”.
- **Why it breaks:** Stop while the gaps Plan or Code stage is pending, then resume. Clearing the marker selects the ordinary Plan/Code intent, losing `--gaps` or `--gaps-only`. Ordinary Plan can short-circuit because plans already exist—the exact trap this plan otherwise addresses.
- **Evidence checked:** `pipeline_launch.rs:3650–3669` clears the stopped condition and relaunches the saved stage. It cannot reconstruct a deleted gap intent. `pipeline_gate.rs:73–86` also supports intentional intermediate halts.
- **Suggested fix:** Preserve gap intent across resumable stops and interruptions. Clear it on successful completion, explicit abandonment, or replacement by another fix route. Add stop/resume controls at both gaps stages.

**F7 — High — Plans 29 and 30 prescribe mutually incompatible context tests.**

- **Location:** `52-29-PLAN.md:280`; `52-30-PLAN.md:86–87`; reinforced by `52-32-PLAN.md:95`.
- **Quoted text:** Plan 29 requires the context to “**end** with exactly” its answer sentence and forbids answer words before it. Plan 30 requires the answer sentence “**first and the cause last**”.
- **Why it breaks:** A nonempty failure context cannot satisfy both. Moving the sentence first makes Plan 29’s test fail; leaving it last loses the operator’s instructions under the 300-character display/notification cap.
- **Evidence checked:** `pipeline_outcomes.rs:920–945` contains the differing context builders; `commands.rs:2056` defines the display cap. Plan 30 runs the full suite without replacing Plan 29’s incompatible assertion.
- **Suggested fix:** Keep the shared router renderer, require its sentence first everywhere, and update all parity tests and Plan 32’s rate-limit row accordingly.

**F8 — Medium — The refused-answer recovery path must reopen a gate the open API forbids.**

- **Location:** `52-29-PLAN.md:245–246,277`; `52-22-PLAN.md:225–227`.
- **Quoted text:** Reopen “with the kind and status it was opened with”, including “a file with no kind”; the earlier plan makes the opening parameter “a `GateKind`, **not an `Option`**”.
- **Why it breaks:** The required kindless bypass test cannot reopen the same kind through the required API. No `Unclassified` gate kind is specified. Implementers must either weaken the opening invariant or invent an unplanned recovery representation.
- **Evidence checked:** `crates/devflow-core/src/gates.rs:468–483,1301` contains the opening/writing path that Plan 22 changes. Plan 29 explicitly includes missing-kind records in its supervisor bypass tests.
- **Suggested fix:** Specify a typed recovery kind with abort-only routing, or retain the original gate while rejecting its invalid answer. Test that chosen behavior through the required opening API.

**F9 — High — The timer design implements at-most-once entitlement consumption, not exactly one automatic relaunch.**

- **Location:** `52-32-PLAN.md:27,104–106`.
- **Quoted text:** “commits `rate_limit_auto_relaunch_used = true` first”; “A crash between the two writes can lose the automatic relaunch”.
- **Why it breaks:** Crash after persisting the flag but before publishing the timer answer. Recovery sees the entitlement as spent although no timer answer won and no automatic launch happened. This contradicts D-38’s meaning of the flag and CUT-03’s exactly-once claim. The listed takeover tests start from an already-used flag; they do not exercise this boundary.
- **Evidence checked:** `gates.rs:1188–1203` durably publishes the consumption claim separately from state. `pipeline_gate.rs:633–692` likewise separates answer pickup, claim, and state mutation.
- **Suggested fix:** Specify a recoverable pending timer action keyed by gate identity, with idempotent publication and recovery. Inject crashes between each durable step; do not label the current two-write protocol exactly-once.

### SUSPECTED

**S1 — High — Speculation: relocating the reset parser preserves an invalid-offset-to-UTC fallback.**

- **Location:** `52-01-PLAN.md:121–123`.
- **Quoted text:** “Move `parse_rfc3339ish`, `split_time_and_offset` and `parse_offset_minutes`”.
- **Why it may break:** The source helper converts an invalid explicit offset into zero. For example, `2026-08-02T00:00:00+99:00` can become a valid UTC deadline instead of the plain gate required by D-27.1. The planned invalid-input tests do not exercise malformed RFC 3339 through the complete parser.
- **Evidence checked:** `crates/devflow-core/src/ship.rs:418,426` uses `parse_offset_minutes(...).unwrap_or(0)`. Its calendar check at `:388` also accepts impossible dates such as February 31.
- **Suggested fix:** Propagate invalid offsets as failure, validate calendar dates, and add full-parser negative controls paired with valid offset/leap-day controls. This remains suspected because the new parser has not been implemented.

### COVERAGE

- **Read in full:** Plans 52-08–10, 52-15, 52-17–18, 52-20, 52-27–32.
- **Skimmed with targeted task, gate, dependency, and source checks:** Plans 52-01–07, 52-11–14, 52-16, 52-19, 52-21–26.
- Checked the supplied CONTEXT decisions, requirements, Phase 52 roadmap criteria, inventory, validation contract, research, patterns, and `CLAUDE.md` against relevant source. These findings are not an exhaustive certification of coverage or scope.
- No Rust suite, container check, or semver check was run. “Confirmed” denotes contradictions or failure paths established from the specified plans and inspected source, not executed future implementations.
- No repository files were edited or temporary files created; probe children were reaped. The snapshot lacks Git metadata, so commit `e6b3343` and branch identity could not be independently verified.

---

## Antigravity Review

### CONFIRMED

#### F-01
- **Severity**: Critical
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-07-PLAN.md:32, 112`
- **Quoted Text**:
  > "D-01 / CUT-04: `devflow parallel` is rejected by clap and absent from `--help`; `parallel.rs` is deleted; `phase7_cli::parallel_creates_two_worktrees_and_spawns_two_monitors` and `fallback_agent_config::parallel_ignores_the_config_key` are deleted; every scoped-doc mention goes"
  > "Delete `parallel.rs`; delete the two parallel tests."
- **Why It Breaks**: The plan assumes only two tests in the codebase exercise `devflow parallel`. That assumption is false. `crates/devflow-cli/tests/fallback_agent_config.rs:434-485` contains a third test: `parallel_refuses_a_config_key_that_names_no_agent`. This test executes `devflow parallel --phases ...`, strips `devflow.toml`, re-runs `devflow parallel`, and asserts `assert_success(&control)`. When `Command::Parallel` is removed from `main.rs`, clap rejects `devflow parallel` with exit code 2. Because Plan 52-07 fails to delete or adapt this test, `cargo test --workspace` and `scripts/check.sh all` fail immediately on `parallel_refuses_a_config_key_that_names_no_agent`.
- **Evidence Checked in Source**: Verified lines 434–485 in `crates/devflow-cli/tests/fallback_agent_config.rs`. The test explicitly spawns `devflow parallel` twice (lines 451, 477) and checks exit status `assert_success(&control)`.
- **Limits of Check**: Establishes that the third test exists and will fail when `parallel` is unmapped; does not evaluate whether additional tests outside `tests/` reference `parallel` via mock wrappers.
- **Suggested Fix**: Add `crates/devflow-cli/tests/fallback_agent_config.rs` to `files_modified` in `52-07-PLAN.md` and explicitly delete `parallel_refuses_a_config_key_that_names_no_agent` alongside `parallel_ignores_the_config_key`.

---

#### F-02
- **Severity**: Critical
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-11-PLAN.md:32, 92`
- **Quoted Text**:
  > "`stop_e2e::a_real_stop_lets_a_real_ship_finish_the_pass_it_started` and `pipeline_gate` `ship_override_*` tests are deleted; the other `stop_e2e` tests stay green"
  > `<automated>cargo nextest run -p devflow --test phase7_cli --no-tests=fail -E 'test(=removed_subcommands_are_unknown)' && cargo nextest run -p devflow --test help_snapshot --no-tests=fail -E 'test(=help_output_matches_committed_snapshot)' && cargo nextest run -p devflow --test stop_e2e --no-tests=fail && ...</automated>`
- **Why It Breaks**: Plan 52-11 deletes the Ship special case in the `stop` command and eliminates all operator-facing output containing `devflow ship`. However, `crates/devflow-cli/tests/stop_e2e.rs:508-555` contains `stop_at_a_ship_gate_stores_the_abort_and_signals_no_recycled_holder`, which asserts:
  ```rust
  assert!(stdout.contains("wrote a rejection") && stdout.contains("devflow ship --phase"));
  ```
  Plan 52-11 does not delete or update this test, yet its automated verify gate executes the entire test suite `cargo nextest run -p devflow --test stop_e2e --no-tests=fail`. That command executes `stop_at_a_ship_gate_stores_the_abort_and_signals_no_recycled_holder`, which fails immediately when the Ship message is stripped.
- **Evidence Checked in Source**: Verified `crates/devflow-cli/tests/stop_e2e.rs:508-555`. The test specifically checks for the `devflow ship` hint string that Plan 52-11 removes.
- **Limits of Check**: Proves that the test fails against the modified output specified in 52-11; does not verify whether other tests in `stop_e2e.rs` relying on `Stage::Ship` fail prior to Plan 52-15.
- **Suggested Fix**: Explicitly delete or rewrite `stop_at_a_ship_gate_stores_the_abort_and_signals_no_recycled_holder` in Plan 52-11 Task 1, or scope the verification gate in 52-11 to targeted tests using `-E` rather than the entire `stop_e2e` suite.

---

#### F-03
- **Severity**: High
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-16-PLAN.md:104-110`
- **Quoted Text**:
  > "Add one helper, for example `fn run_never_launched(project_root, state) -> bool`, true when `state.supervisor` and `state.pending_supervisor` are `None` and no `workflow_started` event exists for the phase, and use it in `run_never_prepared` and `check_missing_branch`; point the interactivity probe at `Stage::Plan`."
- **Why It Breaks**: `check_missing_branch` has the signature `fn check_missing_branch(facts: &PhaseFacts) -> Option<PhaseFinding>` (`crates/devflow-cli/src/commands.rs:6680`). It takes only `&PhaseFacts`, not `project_root` or `state`. It is invoked inside `reconcile_phase(facts: &PhaseFacts)`, which is documented and enforced as a pure, zero-I/O reconciliation function taking no filesystem paths. `PhaseFacts` contains no event history or pending supervisor info. Passing `(project_root, state)` to `check_missing_branch` violates the purity invariant of `reconcile_phase`. If `run_never_launched` is instead evaluated in `build_phase_facts`, `PhaseFacts` requires a new field, but `PhaseFacts` and `build_phase_facts` are not listed in `52-16-PLAN.md` tasks or artifacts.
- **Evidence Checked in Source**: Inspected `crates/devflow-cli/src/commands.rs:6353-6400` (`struct PhaseFacts`) and line 6680 (`fn check_missing_branch`). Neither `project_root` nor `state` is accessible within `reconcile_phase`.
- **Limits of Check**: Verifies compiler and architectural incompatibility of the specified signature in `commands.rs`; does not evaluate whether an alternative boolean flag could be added to `PhaseFacts` without breaking downstream consumers.
- **Suggested Fix**: Specify adding a `run_never_launched: bool` field to `PhaseFacts`, compute it inside `build_phase_facts` (where `project_root` and `state` are available), and have `check_missing_branch` consume `facts.run_never_launched`.

---

#### F-04
- **Severity**: High
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-15-PLAN.md:6-25, 36`
- **Quoted Text**:
  > "`FixType::AuditFix` (whose only producer was `handle_ship_failure`) is deleted with its prompt arms, unless `rg` finds a kept producer, which the SUMMARY names"
- **Why It Breaks**: Plan 52-15 deletes `FixType::AuditFix` from `crates/devflow-core/src/prompt.rs`, but omits `crates/devflow-core/src/state.rs` from `files_modified`. In `crates/devflow-core/src/state.rs:1106, 1115`, the unit tests for `code_fix` serde roundtripping explicitly reference `crate::prompt::FixType::AuditFix`. Deleting the variant from `prompt.rs` without modifying `state.rs` causes `devflow-core` compilation to break on `cargo check --workspace --tests` and `scripts/check.sh all`.
- **Evidence Checked in Source**: Verified `crates/devflow-core/src/state.rs:1100-1120`. `FixType::AuditFix` is explicitly constructed and asserted at lines 1106 and 1115.
- **Limits of Check**: Confirms compilation failure in `state.rs`; does not test whether any external integration tests outside `crates/devflow-core` also retain literals.
- **Suggested Fix**: Add `crates/devflow-core/src/state.rs` to `files_modified` in `52-15-PLAN.md` and switch the test in `state.rs` to use `FixType::Quick` or `FixType::GapsOnly`.

---

#### F-05
- **Severity**: High
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-21-PLAN.md:6`, `52-22-PLAN.md:6`, `52-32-PLAN.md:6`
- **Quoted Text**:
  > `52-21-PLAN.md:6`: `depends_on: ["52-18"]`
  > `52-22-PLAN.md:6`: `depends_on: ["52-19", "52-20"]`
  > `52-32-PLAN.md:6`: `depends_on: ["52-31"]`
- **Why It Breaks**: Plan 52-21 is assigned Wave 18. No plan in the rest of Phase 52 declares a dependency on `52-21` (topological sort leaves `52-21` and `52-32` as the only leaf nodes). In any worktree-based, branch-per-wave, or DAG-based execution harness, Plan 52-21 is never merged into the downstream mainline. Furthermore, Plan 52-21 modifies `crates/devflow-cli/src/preflight.rs`. Plan 52-22 Task 4, Plan 52-26, Plan 52-29, and Plan 52-30 all edit `crates/devflow-cli/src/preflight.rs` concurrently without depending on 52-21, creating merge conflicts and stranding the OPS-03 preflight fixes.
- **Evidence Checked in Source**: Executed transitive ancestor analysis across all 32 plan YAML frontmatters. `52-21` is absent from all `depends_on` lists and appears only as a threat ID `T-52-21` in `52-18-PLAN.md`.
- **Limits of Check**: Measures declared dependencies in plan frontmatter; if an executor executes all 32 plans strictly sequentially by file number ignoring declared DAGs, the disconnect is masked.
- **Suggested Fix**: Change `52-22-PLAN.md` `depends_on` to `["52-19", "52-20", "52-21"]` and bump subsequent plan waves accordingly if required.

---

#### F-06
- **Severity**: High
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-08-PLAN.md:25-32, 92-105`
- **Quoted Text**:
  > "D-04/D-05: a Validate that passes ends the run through a new `finish_run` in `pipeline_gate.rs`, not a transition to Ship ... the ambiguous-approve, gate-approve and auto-pass sites in `handle_validate_outcome` that called `transition(…, Stage::Ship)` all call `finish_run`"
  > "D-29: the terminal finish emits `workflow_finished` with payload exactly `{\"reason\":\"finished\",\"shipped\":false}`; the `--until` halt keeps `{\"reason\":\"stopped_at\",\"stage\":…}`"
- **Why It Breaks**: In `pipeline_gate.rs:73-98`, `transition` intercepts `--until <stage>` halts by checking `if state.stop_until == Some(from)` and emitting `workflow_finished` with `{"reason":"stopped_at","stage":...}`. When `handle_validate_outcome` transitions to `Stage::Ship`, `transition` detects `stop_until == Some(Stage::Validate)` and halts cleanly. In Plan 52-08, every passing Validate branch calls `finish_run` instead of `transition`. However, `finish_run` unconditionally sets `state.finished = true` and emits `{"reason":"finished","shipped":false}` without checking `state.stop_until == Some(Stage::Validate)`. Consequently, running `devflow start --until validate` will record a terminal finish (`finished: true`, `reason: "finished"`) instead of an `--until` halt (`reason: "stopped_at"`, `stage: "validate"`), violating D-29 and ROADMAP Success Criterion 1.
- **Evidence Checked in Source**: Inspected `crates/devflow-cli/src/pipeline_gate.rs:62-98` and `52-08-PLAN.md:25-105`. `stop_until` does not appear anywhere in `52-08-PLAN.md`.
- **Limits of Check**: Verifies the code paths in `pipeline_gate.rs` and the task specifications in 52-08; does not determine if `finish_run` could trivially incorporate the check.
- **Suggested Fix**: Explicitly instruct `finish_run` in `52-08-PLAN.md` to check `if state.stop_until == Some(Stage::Validate)` before setting `finished = true`, and branch to emit `stopped_at` instead of `finished`.

---

#### F-07
- **Severity**: Medium
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-31-PLAN.md:33, 149`
- **Quoted Text**:
  > "D-27.4 / D-37: answers to a rate-limit gate map through their own function: approve → `GateAction::Relaunch { note: None }`, reject → `GateAction::Relaunch { note: Some(note) }` with the note byte-identical, abort → `GateAction::Abort`; `Advance` keeps meaning advance"
  > "gates::tests::rate_limit_gate_answers_map_to_relaunch_or_abort: ... `abort: true` → `GateAction::Abort`; control in the same test: none of the three maps to `GateAction::Advance`"
- **Why It Breaks**: In `crates/devflow-core/src/gates.rs:91`, `GateAction::Abort(String)` is a tuple variant taking a reason `String`. It is not a unit variant. Referencing `GateAction::Abort` without a tuple argument fails compilation (`error[E0532]: expected tuple struct or tuple variant, found unit variant`). Furthermore, Plan 52-31 never specifies what reason string the rate-limit gate mapping passes into `GateAction::Abort(...)`.
- **Evidence Checked in Source**: Inspected `crates/devflow-core/src/gates.rs:85-92` and Plan 52-19 Task 2 (`52-19-PLAN.md:128`), which confirms `from_response` maps an abort to `GateAction::Abort(reason)`.
- **Limits of Check**: Verifies rustc variant requirements; does not test if an implementer would guess a fallback string such as `note.unwrap_or_default()`.
- **Suggested Fix**: Change the specification in `52-31-PLAN.md` to `GateAction::Abort(String)`, specifying the payload string (e.g. `GateAction::Abort(note.unwrap_or_else(|| "rate limit gate aborted".into()))`).

---

#### F-08
- **Severity**: Medium
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-02-PLAN.md:143-157`
- **Quoted Text**:
  > "Run `~/.cargo/bin/devflow --version` and stop if it does not print `devflow 3.0.0` (record what it printed instead and report it: the control is then unavailable). Clone this repository into a fresh scratch directory (`git clone --local`, outside the worktree, under `$TMPDIR` or the session scratchpad), run `~/.cargo/bin/devflow start --dry-run --phase 52` there..."
  > `<automated>test -s .planning/phases/52-subtract-and-stabilize/52-RUN-NOTES.md && grep -q 'devflow 3.0.0' .planning/phases/52-subtract-and-stabilize/52-RUN-NOTES.md && grep -q 'D-18' .planning/phases/52-subtract-and-stabilize/52-RUN-NOTES.md</automated>`
- **Why It Breaks**: The instructions tell the agent to stop if `~/.cargo/bin/devflow` does not print `devflow 3.0.0`, noting that the control is then unavailable. However, the `<automated>` verify gate unconditionally requires `grep -q 'devflow 3.0.0' .planning/phases/52-subtract-and-stabilize/52-RUN-NOTES.md`. In any environment where `~/.cargo/bin/devflow` is not installed, has a different binary name, or is not v3.0.0 (such as containerized CI environments, clean development containers, or automated evaluator harnesses), following the stop instruction causes the verify block to fail unconditionally. Furthermore, `git clone --local . /tmp/scratch` fails when executed inside a linked git worktree (where `.git` is a pointer file rather than a git directory).
- **Evidence Checked in Source**: Checked `52-02-PLAN.md:143-160`. The verify command is non-conditional and enforces content that the task explicitly allows skipping when the host environment lacks the binary.
- **Limits of Check**: Confirms verify gate failure if `~/.cargo/bin/devflow` is absent; does not assert that the binary is absent on the user's current host workstation.
- **Suggested Fix**: Adjust the `<automated>` block to allow an explicit "control unavailable" note in `52-RUN-NOTES.md` if `devflow 3.0.0` is absent from `~/.cargo/bin/`.

---

#### F-09
- **Severity**: Low
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-25-PLAN.md:6-12`
- **Quoted Text**:
  > `files_modified: [crates/devflow-cli/src/commands.rs, crates/devflow-cli/tests/phase7_cli.rs, OPERATIONS.md]`
- **Why It Breaks**: Plan 52-25 implements OPS-02 (`devflow cleanup --force` removes the state file together with the worktree). However, `crates/devflow-cli/src/main.rs` is omitted from `files_modified`. The doc comment on `Command::Cleanup::force` in `crates/devflow-cli/src/main.rs:299` currently reads:
  ```rust
  /// Also remove the reference worktree and force-remove dirty worktrees.
  #[arg(long)]
  force: bool,
  ```
  The reference worktree is removed in Plan 52-02. Without updating `main.rs`, `devflow cleanup --help` continues to document removing a nonexistent reference worktree and fails to document that `--force` removes state files.
- **Evidence Checked in Source**: Verified `crates/devflow-cli/src/main.rs:295-303`. The doc comment on `force` explicitly describes the reference worktree.
- **Limits of Check**: CLI help text drift does not fail unit tests unless a dedicated subcommand snapshot test exists; only top-level help is checked by `devflow-help.txt`.
- **Suggested Fix**: Add `crates/devflow-cli/src/main.rs` to `files_modified` in `52-25-PLAN.md` and update the doc comment on `Command::Cleanup::force`.

---

### SUSPECTED

#### S-01
- **Severity**: Medium
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:207, 280`
- **Quoted Text**:
  > "for a verification kind it records `code_fix = Quick`, the note and `code_fix_base`, launches Code, and clears the 52-27 gap marker where it was set"
- **Why It Breaks (Speculation)**: Under Plan 52-29, rejecting any verification gate (including uncovered files or unverified requirements) defaults to dispatching `/gsd-quick` with the operator's note. When Code runs under `/gsd-quick`, it executes quick fixes without re-planning gaps. If the agent's changes leave required test files uncovered or fail to satisfy GSD's verification criteria, the subsequent Validate stage will persistently return `gaps_found` or `uncovered_files`, cycling through `/gsd-quick` until `MAX_CONSECUTIVE_FAILURES` is reached. The plans do not specify how an operator can instruct DevFlow to switch from a quick fix back to the gap-closure planner if a quick fix proves insufficient.
- **Evidence Checked in Source**: Inspected `pipeline_outcomes.rs` handling of `code_fix` and `reject_route`. `reject_route` on verification kinds unconditionally routes to `/gsd-quick`.
- **Limits of Check**: Behavior depends on live interaction between external GSD CLI and operator prompts during execution; cannot be statically reproduced without a running GSD agent.
- **Suggested Fix**: Allow the operator's rejection note or an explicit CLI option on `devflow gate reject` to select between `/gsd-quick` and the gap-closure loop.

---

### COVERAGE

- **Read in Full**:
  All 32 phase plans were read in full and checked against the source code:
  - `52-01-PLAN.md` through `52-32-PLAN.md`
  - Supporting context: `52-CONTEXT.md` (decisions D-01 through D-39), `52-RESEARCH.md`, `52-VALIDATION.md`, `52-PATTERNS.md`, `ROADMAP.md` (Phase 52), `REQUIREMENTS.md`, and `CLAUDE.md`.
- **Skimmed**: None. Every plan was parsed for dependency topology, automated verification gates, file mutations, and cross-plan symbol contracts.

---

## Pi (DeepSeek Flash) Review

I read the phase artifacts and all 32 plans, and checked their claims against `crates/` and the host toolchain. Findings below; everything marked CONFIRMED was reproduced against the tree at this snapshot.

### CONFIRMED

**F1 — Critical — `52-32-PLAN.md` Task 1: the launch-count assertion is arithmetically impossible for the fixture it names**
Quoted (`52-32-PLAN.md`, Task 1 behaviour): "`rig.invocations("claude")` is exactly 2 after the second denial (one original, one automatic relaunch) and stays 2 over a bounded wait".
Evidence in source: `crates/devflow-cli/tests/failover_e2e.rs:314-331` — `Rig::start_with` always passes `--mode auto --until code`, so one run logs Define, Plan and Code turns; `:390-398` — `invocations()` filters only `"health"`, **not** `"canary"`; `:471-475` — the existing test on this exact rig asserts
`rig.invocations("claude")` == `["canary", "stage define", "stage plan", "stage code"]` (4 entries).
So one automatic Code relaunch yields 5 entries, never 2, and "stays 2" can never hold. Why it breaks: the gate `a_passed_reset_relaunches_once_and_a_second_denial_waits` is the phase's only proof of CUT-03's "exactly one automatic relaunch" (ROADMAP criterion 3). An executor either games the count (filters to `stage code` and asserts 2 — which counts Define/Plan/Code, not relaunches) or the test cannot be satisfied.
Fix: assert on relaunches, not invocations — count `stage_launched` events for stage `code`, or `invocations("claude").iter().filter(|l| *l == "stage code").count() == 2`, with the pre-relaunch count asserted first as the control.

**F2 — Critical — `52-32-PLAN.md` context names a constant that does not exist, and the work it implies is not planned**
Quoted: "`CLAUDE_STUB` and its `@CLAUDE_RATE_LIMIT@` substitution (~L66-99, ~L275), `RESETS_AT` constant (~L45, already past)".
Evidence: `rg 'RESETS_AT' crates/devflow-cli/tests/failover_e2e.rs` → **no match**. The reset value is inlined inside the JSON fixture: `failover_e2e.rs:45` `const CLAUDE_REJECTED_RATE_LIMIT_EVENT: &str = …"resetsAt":1785645600,…`, substituted once at `:275`. Task 1 requires "the claude stub denies twice with a computed past `resetsAt`" and Pitfall 10 requires "e2e reset times are computed from the current time". Both need a new `@RESETS_AT@` placeholder threaded through `CLAUDE_STUB` and `Rig::build` (two separate code points); nothing in the plan asks for it, and `files_modified` cannot express it.
Fix: add the placeholder explicitly in Task 1's action and to the anchors, and say which two literals change.

**F3 — High — `52-19-PLAN.md` removes note-substring abort but leaves an e2e that depends on it, outside `files_modified`**
Quoted (`52-19-PLAN.md` truth): "`gate reject` always means fix and relaunch"; `files_modified` lists `gate_wedge_e2e.rs`, `stop_e2e.rs`, `start_lock_e2e.rs`, `recover_clean_e2e.rs` — **not** `phase7_cli.rs`.
Evidence: `crates/devflow-cli/tests/phase7_cli.rs:912-917` — "…abort it (note containing \"abort\" — `gates.rs::GateAction::from_response`) so the monitor clears state"; `:932-949` — `gate reject 8 --stage validate --note "abort test teardown"` then `wait_for_state_cleared(root, PhaseId::new(8))`.
After 52-19 that reject loops back; the state is never cleared and the fixture blocks until its wait bound. 52-19 Task 3's gate is `scripts/check.sh all`, so the break lands unplanned and the executor must edit a file the plan does not authorise (which is also how `files_modified` drift starts).
Fix: add `crates/devflow-cli/tests/phase7_cli.rs` to 52-19 `files_modified`, change that teardown to `gate abort 8 --stage validate`, and list it with the other writers.

**F4 — High — `52-30-PLAN.md` names a cap in bytes and tests it against a cut that counts characters**
Quoted (must_haves): "bounded by a stored cap of 4096 **bytes**, `GATE_CONTEXT_STORED_MAX_BYTES`"; behaviour: "exactly 4096 **bytes** → unchanged; 4097 **bytes** with a multi-byte character at the boundary → cut before it".
Evidence: `crates/devflow-cli/src/pipeline_outcomes.rs:893-914` — `pub(crate) fn render_gate_context(context: &str, max_chars: usize)` and `if sanitized.chars().count() <= max_chars { return sanitized; }`. A 4097-byte string containing a 3-byte character is 4095–4096 **chars**, so it is returned unchanged and the named test `a_context_at_the_stored_cap_is_unchanged_and_one_over_is_cut_at_a_char_boundary` fails exactly as specified. The constant is also the artifact 52-30 promises (`contains: "GATE_CONTEXT_STORED_MAX_BYTES"`), so the name misstates the unit for every later reader.
Fix: rename to `…_MAX_CHARS` and state the count in chars, or implement a byte-bounded cut (and then the "cut at a char boundary" phrasing needs the truncation suffix to fit inside the byte budget).

**F5 — High — `52-18-PLAN.md` excludes `CHANGELOG.md` from a criterion that says "the top-level docs"**
Quoted (`52-18-PLAN.md`): "`CHANGELOG.md` and `.planning/` are out of scope"; `52-ROADMAP.md` criterion 2: "…remains under `crates/`, `docs/` or **the top-level docs**, comments stripped"; `REQUIREMENTS.md` CUT-03: "**nothing in the tree or docs refers to it**".
`CHANGELOG.md` is a top-level doc and, by 52-18's own reasoning, will name removed surface (it must record the removal). The plan records this only as an "Interpretation … in the SUMMARY so verification carries it" — a verifier checking criterion 2 literally fails the phase. The precedent for fixing the criterion text rather than the scan exists: D-17 amended CUT-03/ROADMAP criterion 3 for a different reason and the amendment is recorded in CONTEXT.
Fix: amend REQUIREMENTS CUT-03 and ROADMAP criterion 2 to exclude release history, in the same way D-17 amended criterion 3 — or get an explicit operator answer that the interpretation stands (this is a decision point, not a planner choice).

**F6 — High — "approve to ship" survives past 52-18's "cut complete" checkpoint and no gate in the phase can see it**
Quoted: `crates/devflow-cli/src/pipeline_outcomes.rs:729` —
`" The per-phase ceiling of {} is reached: this run is paused for a human, not aborted — approve to ship, reject to loop back for another pass, or abort."`
52-22 explicitly defers it: "Any sentence that tells the operator what to answer … is interim: 52-29 replaces … so add no new answer wording here". So from 52-15 (no `Stage::Ship`, no Ship prompt) to 52-29, the failure-cap gate tells the operator to approve a Ship that no longer exists — including across 52-18, whose Task 3 is the phase's nominal "cut tree is green / semver matches the plan" checkpoint.
Why no gate catches it: 52-18's token list matches identifier-bounded identifiers plus the whole string literals `"ship"`/`"define"`; this sentence contains neither form, and `help_names_no_removed_surface` only scans three `--help` outputs (52-15/52-17).
Fix: add the phrase `approve to ship` (and `loop back for another pass`) to 52-18's token list, or reword the sentence when 52-22/52-27 touch that gate.

**F7 — Medium — `52-18-PLAN.md`'s docs-site guard cannot police the diagram it claims to**
Quoted (must_haves): "fails while … `docs/diagrams/module-map.md` names a module that `crates/devflow-core/src/lib.rs` does not declare"; rule (3) in Task 2: "every backticked name in the table's Modules column and every mermaid node label **that is a single lowercase identifier**".
Evidence `docs/diagrams/module-map.md:6-20`: the mermaid nodes are `CLI[devflow-cli]`, `WF[workflow and state]`, `HOOKS[hooks]`, `VERSION[version]`, `LOCK[lock]`, `SHIP`… — all labels are **uppercase**, so no node matches "a single lowercase identifier" and none is ever checked. Only the table rows (`| Git lifecycle | … \`hooks\`, \`version\`, \`ship\` |`) can fail. The stated invariant is strictly broader than the mechanism; after 52-12/52-13 delete those modules, the diagram still draws them and the test stays green.
Fix: match the payload inside each label (`\[(.*?)\]`) or the node id against `lib.rs`, or narrow the truth to the table.

**F8 — Medium — two durable carriers with different lifetimes for the same operator note**
Quoted (`52-31-PLAN.md` Task 4, action): "a Code pass that already carries a `code_fix_note` shows **both** blocks, its own first"; 52-26 clears `code_fix_note` "when the fix pass completes"; 52-31 clears `relaunch_note` "when the pass it was given to ends, whatever the outcome".
Both are `State` fields on the same stage (`crates/devflow-core/src/state.rs` shape, 52-26 `code_fix_note`, 52-31 `relaunch_note`). A rate-limit reject that lands during a Code fix pass therefore hands the relaunched agent the previous rejection's note *and* the new one, with nothing distinguishing which pass each belongs to — and the byte-identity tests in 52-26/52-31 pass either way, because each asserts only its own block.
Fix: one carrier (`relaunch_note`) written by both reject paths, or tag each block with the gate/stage it came from and clear on the same event in both plans.

**F9 — Medium — `52-23-PLAN.md`'s never-silent audit is unbounded and verified by judgement only**
Quoted (must_haves): "Never-silent audit: every `warn!`/`error!` reachable from a foreground command that is the only operator-visible signal … also prints a plain stderr line (or is already backed by a gate or event); the SUMMARY lists each site and its disposition", backed by a `prohibitions` entry with `verification: judgment`. The plan's own context sizes the job: "About 94 `warn!`/`error!` sites exist across both crates (`monitor.rs` 23, `pipeline_launch.rs` 12, `config.rs` 10, `gates.rs` 9, `supervisor/runtime.rs` 9, …)".
OPS-03 asks only that "command output carries no `tracing` lines unless `RUST_LOG` is set". Turning the default filter to `off` (`EnvFilter::new("off")` in both branches of `main.rs`) turns 94 diagnostics off by default, and the mitigation is a 55k-token plan with no gate: nothing in `<automated>` can fail if the audit is done badly. The highest-risk sites (`monitor.rs`, detached `__monitor` with stderr at `/dev/null`) are also the ones the plan excludes by fiat.
Fix: bound it — enumerate the sites that are *the only* signal (the ones 52-30/52-08 already gate on), give each a named test that asserts the stderr line, and scope the audit claim to that list.

**F10 — Low — `52-16` leaves `--until define` accepted and unreachable, which is the exact class of bug D-07 exists to prevent**
Quoted (52-16 must_haves): "`Stage::Define` still compiles but no code path starts or transitions into it"; 52-16 Task 1 sets `State::new` to `Stage::Plan` and `print_dry_run` to start at Plan. `52-17` then makes `--until define` a clap error ("D-07: `devflow start --until define` is rejected by clap").
In between, `--until define` parses (`Stage::Define` exists, `crates/devflow-core/src/stage.rs`), no run can ever reach Define, and `until_rejects_removed_stages` (created in 52-15) does not cover `define` yet — so the CLI advertises a `--until` value that silently runs the whole pipeline. `--dry-run`'s annotation (`start_dry_run_annotates_until_stage`, rewritten in 52-16) will also produce a marker on a stage line that does not exist.
Fix: reject `define` in the same plan that makes it unreachable (a one-line guard on the `--until` arg), or state the window explicitly and test that `--until define --dry-run` fails.

---

### SUSPECTED (speculation — flagged as such)

**S1 — Medium — `52-20-PLAN.md` `phase_dir` returns an arbitrary directory on a prefix match**
Quoted: "`gsd_reader::phase_dir(evidence_root, phase)` returns the phase DIRECTORY found by the `{padded}-` prefix scan under `.planning/phases`".
The existing analogue it is modelled on requires a discriminator: `crates/devflow-core/src/agent_result.rs:3817-3838` (`phase_verification_path`) only accepts a directory that *contains* `{padded}-VERIFICATION.md`, and 52-20 deliberately drops that requirement so `missing` is reachable. The scan then returns the first `read_dir` entry whose name starts with `{padded}-`; for phase 52 in this repo the phase-dir subtree already contains sibling artifact directories named `52-context-review/` and `52-gap-loop-evidence/` (they sit inside it, not beside it — which is why I am marking this suspected rather than confirmed), and nothing in `gsd_reader` forbids a project from putting such a directory at `.planning/phases/`. If that happens, `phase_dir` returns a directory GSD does not recognise and D-24 converts it to a never-silent gate — a permanently wedged Validate.
Fix: accept a candidate only if it contains the phase's own artifacts (`{padded}-*-PLAN.md` or `{padded}-VERIFICATION.md`), falling back to a single prefix match, and test a two-candidate directory.

**S2 — Low — `52-30` silently changes preflight refusal text outside GATE-04**
Quoted (52-30 Task 1): "switch every write-time caller to the stored cap", with `crates/devflow-cli/src/preflight.rs` in `files_modified`. `truncate_reason` is the shared helper (`pipeline_outcomes.rs:888`), and `preflight.rs:919-922` documents a deliberate 300-character cap for preflight refusals ("a hard 300-character cap …"). Raising it to 4096 puts up to 13× more agent-derived text into preflight refusals. Nothing in 52-30 tests the preflight path, and `999.206` is about gate contexts only. I have not established that 4096-char preflight refusals are harmful — only that the change is untested and undocumented in the plan.

**S3 — Low — `52-04`'s fake `codex` may not survive the codex driver's preflight**
Quoted: "answer any probe arguments the preflight sends (for example a version or login query) with a success line". The witness must satisfy the delivery canary (`phase7_cli.rs:836-856` shows the canary is a *top-level* `result` event carrying a planted token) **and** the codex capture parser, in `sh`. The plan postpones the details to execution ("read `crates/devflow-core/src/agents/codex.rs` … and any codex preflight probe"). If the codex probe does not have a cheap argv-only success shape, the plan's "test-only plan" fails at its own tracer and the capture-shape coverage 52-05 depends on is lost.

---

### COVERAGE

- **Read in full** (every task, gate, truth): 52-01, 52-02, 52-03, 52-04, 52-05, 52-06, 52-07, 52-08, 52-09, 52-11, 52-12, 52-13, 52-14, 52-15, 52-16, 52-17, 52-18, 52-19, 52-20, 52-21, 52-22, 52-24, 52-25, 52-26, 52-27, 52-28, 52-29, 52-30, 52-31, 52-32. Plus 52-CONTEXT, 52-VALIDATION, ROADMAP Phase 52, REQUIREMENTS (CUT/GATE/OPS), CLAUDE.md, and 52-RESEARCH/52-PATTERNS only at the sections the plans cite.
- **Skimmed**: 52-10 (must_haves, anchors, actions, and both `<automated>` gates via targeted extraction — I did not read its threat model or full prose), 52-23 (Tasks 1–2 in full; the last ~86 lines, Tasks 3 and the closing sections, not read).
- **Checked against source and passing** (i.e. I tried to break these and could not): every `cargo nextest … -E 'test(=…)'` name that already exists (`doc_check::*`, `hermetic_spawn_sites`, `supervisor_e2e`, `phase7_cli`, `workspace_version_pin`, `build_provenance`, `log_format_env`, `gate_wedge_e2e` helpers, `parallel.rs` tests being moved); the `gates.rs`/`commands.rs`/`pipeline_outcomes.rs`/`mode.rs`/`agent_result.rs`/`prompt.rs`/`state.rs` line anchors I spot-checked (~20 of them, all within a few lines); `registry::{entry_path_in,roots_dir_in,deregister}`, `State::{stopped,stop_reason,fallback_consumed,new}`, `Cleanup{project,force}` positional, `gate_sweep_may_reap`, `git::publish_order`, `deny.toml`, `scripts/check.sh` using `--workspace`, `check-release --release-type`; `gsd_run` present in all three runtime homes and `query verification fingerprint` / `query frontmatter get --field` / `runtime-identity` all real in gsd-core 1.20.1. DAG levels for all 29 waves are consistent with `depends_on`, and no same-wave pair shares a file.
- **Not verified**: anything needing network or a live agent (crates.io baseline for `cargo semver-checks`, the GSD live routes), and 52-10/52-23's unread prose.

---

## Consensus Summary

### Agreed Concerns (host-verified against source; each MUST be addressed or dispositioned)

| # | Sev | Plans | Finding | Fix |
|---|-----|-------|---------|-----|
| C1 | High | 52-16, 52-17 | 52-16 sets `State::new` to Plan and gates on `scripts/check.sh all`, but `supervisor_e2e.rs:439` still runs `--until define`; that test is only migrated in 52-17. After 52-16 no run reaches Define, so the halt never fires. (codex F3) | Move the `--until define` e2e migrations into 52-16. |
| C2 | High | 52-29, 52-30, 52-32 | 52-29's `every_gate_names_only_the_routes_its_answers_run` needs every context to *end* with the way-on sentence, and 52-32:95 repeats that. 52-30:27/46 puts the way-on sentence *first* in failure contexts. 52-30 does not update 52-29's test. (codex F7) | Pick one order (first, so the 300-char cut keeps the answer) and update 52-29/52-32 assertions to match. |
| C3 | High | 52-32 | `a_passed_reset_relaunches_once…` asserts `rig.invocations("claude")` is exactly 2. The rig filters only `health` (`failover_e2e.rs:393`), the denial happens at Code, and after 52-17 the minimum sequence is `canary, stage plan, stage code` + relaunch, so 2 is unreachable. (pi F1) | Count `stage code` entries (1 before, 2 after) or `stage_launched` events for Code. |
| C4 | High | 52-19 | 52-19 makes `gate reject` always loop back, but `phase7_cli.rs:915-948` tears down with `gate reject --note "abort test teardown"` and waits for cleared state. `phase7_cli.rs` is not in 52-19's `files_modified`. (pi F3) | Add the file and switch that teardown to `gate abort`. |
| C5 | High | 52-16 | `run_never_launched` keys on "no `workflow_started` event exists for the phase". The event log is project-wide and append-only, so a re-run of a phase that ran before reads as already launched. Today `run_never_prepared` (`commands.rs:888`) uses `verification_baseline_captured`, which is per-state. (codex F5) | Keep the per-state flag. Drop the event-log test. Add a previous-run control. |
| C6 | High | 52-27 | The gap-closure marker is "cleared when … the run stops" (52-27:101). A stop/resume during the gaps Plan or Code stage relaunches ordinary Plan or Code without `--gaps`. (codex F6) | Clear only on `passed`, abort, or replacement by another fix route. Add stop/resume tests at both gaps stages. |
| C7 | Medium | 52-15 | Task 1 removes `DEVFLOW_REVIEW_ANGLES` from core and then runs the full core lib suite. The docs still name it (`ARCHITECTURE.md:441`, `docs/guides/configuration.md:50`) until Task 3, so `doc_check` (docs→source rule, `doc_check.rs:363-369`) fails. Task 1 also leaves the CLI unbuildable and makes the joint commit conditional. (codex F4) | Move the doc removal into Task 1. Make the Task 1+2 commit unconditional. |
| C8 | Medium | 52-08, 52-22, 52-29 | 52-08:145 requires `pipeline_outcomes.rs` to contain no `approve to ship`. The failure-cap text at `pipeline_outcomes.rs:729` contains it, but 52-22:152 says to leave that sentence until 52-29, and 52-29:42 removes it there. 52-08 fails its own criterion or does 52-29's work. (pi F6, reframed) | Narrow 52-08's criterion to the Validate-gate message (L697), or move the L729 rewrite into 52-08. |
| C9 | Medium | 52-08 | `--until validate` stays valid (52-15:136). 52-08 has every passing Validate call `finish_run`, which emits `finished`. 52-08 never says whether `--until validate` gives `stopped_at` or `finished`. Only `--until plan` is tested. (agy F-06) | State the intended outcome and test `--until validate`. |
| C10 | Medium | 52-30 | The cap is named and tested in bytes (`GATE_CONTEXT_STORED_MAX_BYTES`, "4097 bytes … cut"). The helper it reuses, `render_gate_context` (`pipeline_outcomes.rs:893-905`), counts chars, so the named boundary test fails as written. (pi F4) | Pick chars or bytes and align the name, the helper and the test. |
| C11 | Medium | 52-29, 52-22 | 52-29 requires a hand-written reject on a no-kind gate to reopen a gate "of the same kind". 52-22 makes the open path take `GateKind`, not `Option`, so a no-kind gate cannot be reopened. (codex F8) | Name the kind such a reopen uses (or keep the original gate and only refuse the answer). |
| C12 | Medium | 52-07 | 52-07 names `parallel_ignores_the_config_key` for deletion but not `parallel_refuses_a_config_key_that_names_no_agent` (`fallback_agent_config.rs:434`), which also runs `devflow parallel`. The file is in scope and the gate would catch it, but the must-have lists the wrong set. (agy F-01) | Name both. |
| C13 | Medium | 52-11 | The truth says "the other `stop_e2e` tests stay green", and Task 2 removes every `devflow ship` hint. `stop_at_a_ship_gate_stores_the_abort…` (`stop_e2e.rs:512-540`) asserts that hint. (agy F-02) | Name the test as deleted or rewritten in 52-11. |
| C14 | Medium | 52-01 | `parse_rfc3339ish` maps an invalid offset to UTC via `unwrap_or(0)` (`ship.rs:418,426`), and 52-01 moves it "unchanged". `+99:00` becomes a valid deadline instead of the plain gate that D-27.1 requires. (codex S1, verified in source) | Propagate the error. Add a malformed-offset negative test. |
| C15 | Low | 52-32 | The anchor `RESETS_AT` constant does not exist. The reset is inlined in `CLAUDE_REJECTED_RATE_LIMIT_EVENT` (`failover_e2e.rs:45`), and a computed reset needs a new placeholder. (pi F2) | Fix the anchor and name the placeholder. |
| C16 | Low | 52-16, 52-17 | Between 52-16 and 52-17, `--until define` parses but can never fire. (pi F10; overlaps C1) | Covered by C1's fix, or reject `define` in 52-16. |
| C17 | Low | 52-16 | `check_missing_branch(facts: &PhaseFacts)` (`commands.rs:6680`) is pure and cannot take `(project_root, state)`. The value has to become a `PhaseFacts` field. (agy F-03) | Say so in the plan. |
| C18 | Low | 52-02/52-25 | The `Cleanup --force` doc comment (`main.rs:299`) still says "reference worktree". (agy F-09) | Update it in 52-02. |
| C19 | Low | 52-31 | `GateAction::Abort` is a tuple variant `Abort(String)` (`gates.rs:91`). The plan gives no reason string. (agy F-07) | Name the payload. |

### Divergent Views — plausible design gaps (single reviewer, reasoned, not reproduced)

- **P1 (codex F2, High):** approving a clean-pass gate finishes on the status recorded when the gate opened. Evidence can change during a wait of up to 3 days. No plan re-reads verification when an approval is picked up.
- **P2 (codex F1, High):** an uncovered-file quick fix whose re-verify agent skips the verifier and leaves `passed` unchanged would pass the post-check. `52-GAP-LOOP-VERIFICATION.md:92-100` records both ingredients separately.
- **P3 (codex F9, Low):** 52-32's flag-then-answer order is at-most-once. The plan admits that a crash between the two writes loses the relaunch. That fails safe (the gate stays open), but the "exactly once" wording overclaims.
- **P4 (pi F8, Low):** `code_fix_note` and `relaunch_note` have different lifetimes, so a rate-limit reject during a fix pass shows both notes.
- **P5 (pi F9, Low):** 52-23's audit of about 94 `warn!`/`error!` sites is verified only by judgment.
- **P6 (pi F5, Low):** 52-18 leaves `CHANGELOG.md` out of the scan, while ROADMAP criterion 2 says "the top-level docs". The plan records this as an interpretation. Amending the criterion text would remove the ambiguity for the verifier.

### Rejected by host verification

- agy F-04: `FixType::AuditFix` is not in `state.rs`. Every use is in files 52-15 already lists.
- agy F-05: 52-21 being a leaf is fine. Waves follow the DAG levels, and 52-21 shares no file with 52-19, 52-20 or 52-22.
- agy F-08: 52-02's gate failing when the 3.0.0 control is unavailable is the intended fail-closed behavior. The installed binary prints `devflow 3.0.0`.
- pi F7: the mermaid node *labels* (`hooks`, `prompt`, …) are lowercase. Only the node ids are uppercase, so the rule does match them.
- agy S-01: speculation about operator UX, not a plan defect.
