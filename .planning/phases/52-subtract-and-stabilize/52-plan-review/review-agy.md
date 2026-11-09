I am waiting for the background compilation of the test targets to finish before inspecting the test list.
I have launched the background compilation to inspect the test suite and will wait for it to complete.
I have launched the test run to verify the baseline status of the binary tests.
I will wait for the test run to finish.
## CONFIRMED

### F-01
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

### F-02
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

### F-03
- **Severity**: High
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-16-PLAN.md:104-110`
- **Quoted Text**:
  > "Add one helper, for example `fn run_never_launched(project_root, state) -> bool`, true when `state.supervisor` and `state.pending_supervisor` are `None` and no `workflow_started` event exists for the phase, and use it in `run_never_prepared` and `check_missing_branch`; point the interactivity probe at `Stage::Plan`."
- **Why It Breaks**: `check_missing_branch` has the signature `fn check_missing_branch(facts: &PhaseFacts) -> Option<PhaseFinding>` (`crates/devflow-cli/src/commands.rs:6680`). It takes only `&PhaseFacts`, not `project_root` or `state`. It is invoked inside `reconcile_phase(facts: &PhaseFacts)`, which is documented and enforced as a pure, zero-I/O reconciliation function taking no filesystem paths. `PhaseFacts` contains no event history or pending supervisor info. Passing `(project_root, state)` to `check_missing_branch` violates the purity invariant of `reconcile_phase`. If `run_never_launched` is instead evaluated in `build_phase_facts`, `PhaseFacts` requires a new field, but `PhaseFacts` and `build_phase_facts` are not listed in `52-16-PLAN.md` tasks or artifacts.
- **Evidence Checked in Source**: Inspected `crates/devflow-cli/src/commands.rs:6353-6400` (`struct PhaseFacts`) and line 6680 (`fn check_missing_branch`). Neither `project_root` nor `state` is accessible within `reconcile_phase`.
- **Limits of Check**: Verifies compiler and architectural incompatibility of the specified signature in `commands.rs`; does not evaluate whether an alternative boolean flag could be added to `PhaseFacts` without breaking downstream consumers.
- **Suggested Fix**: Specify adding a `run_never_launched: bool` field to `PhaseFacts`, compute it inside `build_phase_facts` (where `project_root` and `state` are available), and have `check_missing_branch` consume `facts.run_never_launched`.

---

### F-04
- **Severity**: High
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-15-PLAN.md:6-25, 36`
- **Quoted Text**:
  > "`FixType::AuditFix` (whose only producer was `handle_ship_failure`) is deleted with its prompt arms, unless `rg` finds a kept producer, which the SUMMARY names"
- **Why It Breaks**: Plan 52-15 deletes `FixType::AuditFix` from `crates/devflow-core/src/prompt.rs`, but omits `crates/devflow-core/src/state.rs` from `files_modified`. In `crates/devflow-core/src/state.rs:1106, 1115`, the unit tests for `code_fix` serde roundtripping explicitly reference `crate::prompt::FixType::AuditFix`. Deleting the variant from `prompt.rs` without modifying `state.rs` causes `devflow-core` compilation to break on `cargo check --workspace --tests` and `scripts/check.sh all`.
- **Evidence Checked in Source**: Verified `crates/devflow-core/src/state.rs:1100-1120`. `FixType::AuditFix` is explicitly constructed and asserted at lines 1106 and 1115.
- **Limits of Check**: Confirms compilation failure in `state.rs`; does not test whether any external integration tests outside `crates/devflow-core` also retain literals.
- **Suggested Fix**: Add `crates/devflow-core/src/state.rs` to `files_modified` in `52-15-PLAN.md` and switch the test in `state.rs` to use `FixType::Quick` or `FixType::GapsOnly`.

---

### F-05
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

### F-06
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

### F-07
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

### F-08
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

### F-09
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

## SUSPECTED

### S-01
- **Severity**: Medium
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:207, 280`
- **Quoted Text**:
  > "for a verification kind it records `code_fix = Quick`, the note and `code_fix_base`, launches Code, and clears the 52-27 gap marker where it was set"
- **Why It Breaks (Speculation)**: Under Plan 52-29, rejecting any verification gate (including uncovered files or unverified requirements) defaults to dispatching `/gsd-quick` with the operator's note. When Code runs under `/gsd-quick`, it executes quick fixes without re-planning gaps. If the agent's changes leave required test files uncovered or fail to satisfy GSD's verification criteria, the subsequent Validate stage will persistently return `gaps_found` or `uncovered_files`, cycling through `/gsd-quick` until `MAX_CONSECUTIVE_FAILURES` is reached. The plans do not specify how an operator can instruct DevFlow to switch from a quick fix back to the gap-closure planner if a quick fix proves insufficient.
- **Evidence Checked in Source**: Inspected `pipeline_outcomes.rs` handling of `code_fix` and `reject_route`. `reject_route` on verification kinds unconditionally routes to `/gsd-quick`.
- **Limits of Check**: Behavior depends on live interaction between external GSD CLI and operator prompts during execution; cannot be statically reproduced without a running GSD agent.
- **Suggested Fix**: Allow the operator's rejection note or an explicit CLI option on `devflow gate reject` to select between `/gsd-quick` and the gap-closure loop.

---

## COVERAGE

- **Read in Full**:
  All 32 phase plans were read in full and checked against the source code:
  - `52-01-PLAN.md` through `52-32-PLAN.md`
  - Supporting context: `52-CONTEXT.md` (decisions D-01 through D-39), `52-RESEARCH.md`, `52-VALIDATION.md`, `52-PATTERNS.md`, `ROADMAP.md` (Phase 52), `REQUIREMENTS.md`, and `CLAUDE.md`.
- **Skimmed**: None. Every plan was parsed for dependency topology, automated verification gates, file mutations, and cross-plan symbol contracts.
