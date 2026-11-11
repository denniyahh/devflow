## CONFIRMED

### C1 (High) — Cross-stage persistence of `State.validate_pass_head` enables false finish from `Stage::Plan` via `no_gap_closure_plan` gate
- **Severity**: High
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L30](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L30)
  - [.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md#L32](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md#L32)
  - [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L32-L34](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L32-L34), [#L244-L248](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L244-L248)
- **Quoted Text**:
  - `52-22-PLAN.md:30`: `"An agent PASS writes State.validate_pass_head = the evidence root's HEAD ... The field is durable run state (#[serde(default, skip_serializing_if = \"Option::is_none\")]) written only here: no stage transition, stop, resume or supervisor takeover clears it, so 52-29's can_finish reads it after any of them"`
  - `52-27-PLAN.md:32`: `"Pitfall 5 post-check: after the gaps Plan pass, a plan with gap_closure: true (read with gsd_reader::frontmatter_field) newer than the report must exist, or the pass counts as a failure and the run gates ... The post-check gate records GateFile.gate_kind = no_gap_closure_plan and is opened with the gaps_found status in hand; its reader-failure gate records reader_failure (52-22's GateKind); both kinds are labels (D-45 rule 2), and 52-29 routes their answers on a fresh status read"`
  - `52-29-PLAN.md:34`: `"maps all ten verification kinds (clean_pass, gaps_found, reverify_needed, human_needed, disagreement_over_passed, disagreement_over_human_needed, reader_failure, ambiguous_verdict, failure_cap, no_gap_closure_plan ... to one Verification family. The routers D-39 names, approve_route(gate, canary_refusal, read) and reject_route(gate, canary_refusal, note), send that family to next_step and read gate_kind only through answer_family, so two verification gates of different kinds with the same evidence get the same route."`
  - `52-29-PLAN.md:246-248`: `"approve_route(gate: &GateFile, canary_refusal: Option<&str>, read: impl FnOnce() -> Evidence) -> ApproveRoute (calls read and next_step(Trigger::Approve(..)) only for the Verification family)"`
- **Concrete Sequence**:
  1. `Stage::Validate` runs on commit `C1`. The Validate agent reports `verdict: pass`, but GSD's report still has `status: gaps_found` (the exact U2 condition).
  2. Per `52-22-PLAN.md:30`, because the Validate agent passed over a clean tree, `handle_validate_outcome` records `State.validate_pass_head = Some("C1")`.
  3. GSD status `gaps_found` routes to the gap loop: DevFlow calls `transition(Validate, Plan)` with the gap marker set.
  4. Per `52-22-PLAN.md:30`, `transition` explicitly does NOT clear `validate_pass_head`. So `State.validate_pass_head` remains `Some("C1")`.
  5. The run is now in `Stage::Plan`. The gaps Plan agent runs `/gsd-plan-phase N --gaps`.
  6. The Plan agent produces no commits (or fails before writing a commit). Working tree is clean and HEAD is still `C1`.
  7. The post-check detects no gap closure plan newer than report and opens a gate with `GateFile.gate_kind = no_gap_closure_plan` while the run is at `Stage::Plan` (`52-27-PLAN.md:32`).
  8. Suppose GSD verification status is updated to `passed` (e.g. operator or background tool re-verified or refreshed the frontmatter, or a test stub answers `passed`).
  9. The operator runs `devflow gate approve`.
  10. `approve_route` checks `answer_family(Some(GateKind::NoGapClosurePlan))` which returns `AnswerFamily::Verification`.
  11. `approve_route` invokes `read_evidence()`. `read_evidence()` queries GSD verification status (`passed`), checks clean tree (clean), and checks `State.validate_pass_head` (`C1`) == current HEAD (`C1`). Thus, `agent_pass_on_head` evaluates to `true`!
  12. `next_step` matches row 1 of the D-45 table: `passed` + `agent_pass_on_head: true` -> `Step::Finish(FinishPermit)`.
  13. `approve_route` executes the finish step: calls `finish_run(state, permit)`.
  14. DevFlow emits `workflow_finished` with reason `"finished"` directly while the run is at `Stage::Plan`, finishing code that no Validate agent ever evaluated for gap closure.
- **Evidence Checked**:
  - `52-22-PLAN.md:30` confirms `validate_pass_head` is never cleared by stage transitions, stops, or resumes.
  - `52-27-PLAN.md:32` confirms `no_gap_closure_plan` is opened from the Plan stage post-check.
  - `52-29-PLAN.md:34` maps `no_gap_closure_plan` into `AnswerFamily::Verification`.
  - `52-29-PLAN.md:32` routes verification approves with zero gate kind discrimination ("takes no gate kind").
  - `crates/devflow-cli/src/pipeline_gate.rs` line 104 (`finish_run`) does not assert `state.stage == Stage::Validate`.
- **Suggested Fix**:
  `transition()` must clear `s.validate_pass_head = None` whenever leaving `Stage::Validate`. In addition, `finish_run` must assert `state.stage == Stage::Validate` before granting a terminal finish, and `no_gap_closure_plan` should not route to finish.

---

### C2 (High) — `pipeline_gate.rs:70` comment trips 52-26 source-scan test `state_stage_is_written_only_by_transition_and_the_code_loop_back`
- **Severity**: High
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-26-PLAN.md#L230-L235](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-26-PLAN.md#L230-L235)
  - [crates/devflow-cli/src/pipeline_gate.rs#L70](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/crates/devflow-cli/src/pipeline_gate.rs#L70)
  - [crates/devflow-cli/src/commands.rs#L1989](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/crates/devflow-cli/src/commands.rs#L1989)
- **Quoted Text**:
  - `52-26-PLAN.md:230-234`: `"Pin the sweep with a source-scan test, pipeline_gate::tests::state_stage_is_written_only_by_transition_and_the_code_loop_back: it walks the non-test text of every .rs file under crates/devflow-cli/src and crates/devflow-core/src (a file with no #[cfg(test)] test-module marker is all non-test text, as in 52-29's emission scan) and requires the assignment .stage =  on a State to occur exactly twice, once in transition and once in prepare_loop_back_to_code, so a later plan that adds a stage write fails it and must add its clear site. Controls: a planted third s.stage =  in a scratch copy of the scanned text fails the scan; spec.stage and a let stage local are not counted."`
  - `pipeline_gate.rs:70`: `"// hooks, `state.stage = to`, the normal \"transition\" event, and"`
  - `commands.rs:1989`: `"/// checkout hook, before `state.stage = to`, before the \"transition\" "`
- **Concrete Sequence**:
  1. Plan 52-26 Task 3 specifies the test `pipeline_gate::tests::state_stage_is_written_only_by_transition_and_the_code_loop_back`.
  2. The test scans all non-test text of `.rs` files under `crates/devflow-cli/src` and `crates/devflow-core/src`.
  3. The specification requires the assignment `.stage = ` on a `State` to occur exactly twice (once at `pipeline_gate.rs:108` and once at `pipeline_gate.rs:199`).
  4. In `crates/devflow-cli/src/pipeline_gate.rs:70` (in non-test code inside `transition`), a comment line contains:
     `// hooks, `state.stage = to`, the normal `"transition"` event, and`
  5. In `crates/devflow-cli/src/commands.rs:1989` (in non-test code), a doc comment contains:
     `/// checkout hook, before `state.stage = to`, before the `"transition"` `
  6. In `crates/devflow-core/src/ship_evidence.rs:74` (in non-test code), a doc comment contains:
     `/// and `return`s BEFORE any checkout hook, before `state.stage = to`,`
  7. Unless the scanner strips comments, `state.stage = to` matches as an assignment to `.stage = ` on a `State` variable. The scan counts 3 matches in `pipeline_gate.rs` alone (lines 70, 108, 199) and fails immediately on the correct codebase.
- **Evidence Checked**:
  - `rg -n "\.stage\s*=" crates/` confirms the presence of `state.stage = to` on line 70 of `crates/devflow-cli/src/pipeline_gate.rs`.
  - `52-26-PLAN.md:230-235` excludes `spec.stage` and `let stage`, but does not specify comment stripping.
- **Suggested Fix**:
  Explicitly specify in `52-26-PLAN.md` that the scanner strips line (`//`) and block (`/* ... */`) comments, or rephrase the comments in `pipeline_gate.rs:70`, `commands.rs:1989`, and `ship_evidence.rs:74`.

---

### C3 (High) — `AutoChainGuard` re-serialization dirties `.planning/config.json`, causing D-46 clean-tree check to fail at Validate outcome and permanently blocking finish
- **Severity**: High
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L178-L184](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L178-L184)
  - [crates/devflow-cli/src/pipeline_launch.rs#L1000-L1008](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/crates/devflow-cli/src/pipeline_launch.rs#L1000-L1008)
  - [crates/devflow-core/src/gsd_config.rs#L168-L178](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/crates/devflow-core/src/gsd_config.rs#L168-L178)
- **Quoted Text**:
  - `52-29-PLAN.md:178-181`: `"A DevFlow write the read lists is a defect to fix: if an_auto_code_stage_leaves_the_gsd_config_as_it_found_it fails, make AutoChainGuard leave a file it found without the key, or with it false, byte-identical (for example: the guard keeps the bytes it read before engage and, when engage changed the file, writes them back on drop by temp-write-then-rename in the same directory, as gsd_config writes; the fix stays in pipeline_launch.rs)..."`
  - `pipeline_launch.rs:1000-1008`:
    ```rust
    impl Drop for AutoChainGuard {
        fn drop(&mut self) {
            if let Err(err) = gsd_config::set_auto_chain_active(&self.config_root, false) { ... }
        }
    }
    ```
  - `gsd_config.rs:168-170`:
    ```rust
    value[WORKFLOW_KEY][AUTO_CHAIN_KEY] = Value::Bool(active);
    let mut contents = serde_json::to_string_pretty(&value)?;
    ```
- **Concrete Sequence**:
  1. In a normal repository, `.planning/config.json` is a tracked git file that initially does not contain `workflow._auto_chain_active`.
  2. An auto Code stage launches. `AutoChainGuard::engage` runs with `active = true`.
  3. `set_auto_chain_active` parses `.planning/config.json`, inserts `_auto_chain_active: true`, re-serializes with `serde_json::to_string_pretty`, and writes it to disk.
  4. Code stage runs and finishes.
  5. `AutoChainGuard` drops. `Drop` calls `set_auto_chain_active(&self.config_root, false)`.
  6. `set_auto_chain_active` sets `_auto_chain_active: false`, re-serializes with `serde_json::to_string_pretty`, and writes to disk.
  7. Because the original committed `.planning/config.json` did not contain `"workflow": { "_auto_chain_active": false }`, `.planning/config.json` is left dirty on disk (`git diff` shows modifications).
  8. Code transitions to Validate: `transition(Code, Validate)`. Validate runs `/gsd-validate-phase N`. Validate does not touch or commit `.planning/config.json`.
  9. Validate agent finishes with `verdict: pass`.
  10. `handle_validate_outcome` runs `read_evidence()`'s dirty-tree check: `git status --porcelain --untracked-files=all`.
  11. Because `.planning/` is included in the D-46 dirty-tree check (`52-CONTEXT.md:413`), Git reports: `M .planning/config.json`.
  12. Per the D-46 amendment (`52-CONTEXT.md:419-422`, `52-29-PLAN.md:170-172`), because the tree is dirty at Validate outcome, `handle_validate_outcome` writes `State.validate_pass_head = None` and opens a gate.
  13. In 52-29, the remediation is phrased conditionally ("if `an_auto_code_stage_leaves_the_gsd_config_as_it_found_it` fails..."). However, in the existing codebase, `AutoChainGuard` unconditionally dirties `.planning/config.json` on drop. Unless the fix is mandatory and implemented in 52-29 Task 1, every auto Code stage leaves `.planning/config.json` dirty, preventing any automated phase run from ever achieving `can_finish` or finishing.
- **Evidence Checked**:
  - `git status --porcelain --untracked-files=all` includes `.planning/config.json`.
  - `gsd_config.rs:168-177` rewrites `.planning/config.json` with added JSON fields.
  - `52-CONTEXT.md:419-422` confirms a dirty tree at Validate outcome records `validate_pass_head = None` permanently.
- **Suggested Fix**:
  In `52-29-PLAN.md`, change the wording from a conditional suggestion to a mandatory Task 1 requirement: `AutoChainGuard` must snapshot the exact pre-existing bytes of `.planning/config.json` (or record its nonexistence) and restore them verbatim on drop if `engage` mutated the file.

---

### C4 (High) — D-46 2026-11-10 amendment omitted from 52-22, creating contract drift with 52-29 and leaving 52-22 test `a_validate_pass_is_recorded_with_the_head_it_ran_on` without disposition
- **Severity**: High
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L30](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L30), [#L150](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L150)
  - [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L33](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L33), [#L170-L174](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L170-L174)
  - [.planning/phases/52-subtract-and-stabilize/52-VALIDATION.md#L74](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-VALIDATION.md#L74)
- **Quoted Text**:
  - `52-22-PLAN.md:30`: `"D-45 rule 1 input (D45-4): every classified Validate verdict is recorded against the commit it ran on. An agent PASS writes State.validate_pass_head = the evidence root's HEAD, read through git::hermetic_command when the outcome is classified ... in the same state commit as the route; a FAIL or an ambiguous verdict writes None"`
  - `52-22-PLAN.md:150`: `"pipeline_outcomes::tests::a_validate_pass_is_recorded_with_the_head_it_ran_on (D45-4): an agent PASS → the state file on disk holds validate_pass_head equal to the worktree evidence root's HEAD sha ... an agent FAIL on the same HEAD → None; an ambiguous verdict → None; a HEAD read that fails ... → None ... Control in the same test: two PASS rows on two different commits record two different shas"`
  - `52-29-PLAN.md:33`: `"agent_pass_on_head true exactly when 52-22 recorded this outcome's agent PASS in State.validate_pass_head with the HEAD it read, which it now does only over a clean tree (D-46 and its 2026-11-10 amendment: read_evidence's one dirty-tree read ... runs after 52-22's HEAD read and before the PASS is recorded, and an agent PASS whose read lists a path or fails records validate_pass_head = None in the same state commit, as a FAIL does, so validate_pass_head stays unset; no new state)."`
  - `52-29-PLAN.md:173-174`: `"52-22's site stays the only writer of the field; this changes what it writes, adds no state and makes no second porcelain call."`
- **Concrete Sequence**:
  1. Wave 19 implements Plan 52-22.
  2. In 52-22, `handle_validate_outcome` is implemented per D45-4: it records `State.validate_pass_head = Some(HEAD)` whenever the agent says PASS, regardless of dirty tree status.
  3. 52-22 adds test `a_validate_pass_is_recorded_with_the_head_it_ran_on`, which asserts that an agent PASS writes `validate_pass_head = Some(HEAD)`.
  4. In Wave 26, Plan 52-29 retroactively updates what 52-22's site in `handle_validate_outcome` writes ("this changes what it writes... an agent PASS whose dirty-tree read lists a path or fails writes `validate_pass_head = None`").
  5. However, `52-29-PLAN.md` does not list `pipeline_outcomes::tests::a_validate_pass_is_recorded_with_the_head_it_ran_on` in its modified tests or test dispositions (lines 263-280, lines 468-472).
  6. Furthermore, `52-VALIDATION.md` line 74 maps requirement GATE-03 directly to 52-22's `a_validate_pass_is_recorded_with_the_head_it_ran_on` without any dirty-tree assertion.
  7. When 52-29 changes `handle_validate_outcome` to require a clean tree, any test fixture or test that relied on 52-22's behavior without establishing a clean git repo will fail, and 52-22's test remains blind to the D-46 amendment.
- **Evidence Checked**:
  - `52-22-PLAN.md` contains 0 occurrences of `porcelain`, `clean tree`, or `untracked`.
  - `52-29-PLAN.md` lines 260-280 lists superseded tests; `a_validate_pass_is_recorded_with_the_head_it_ran_on` is absent.
- **Suggested Fix**:
  Update `52-22-PLAN.md` directly so that D-46 and the 2026-11-10 amendment are part of `52-22`'s `validate_pass_head` contract from the start (Wave 19), and add a dirty-tree row to `a_validate_pass_is_recorded_with_the_head_it_ran_on`.

---

### C5 (Medium) — `relaunch_note` leaks into subsequent verification gates and prompt renderers when a relaunched Validate stage fails normally
- **Severity**: Medium
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-26-PLAN.md#L34](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-26-PLAN.md#L34), [#L150-L153](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-26-PLAN.md#L150-L153)
  - [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L35](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L35), [#L258](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L258)
- **Quoted Text**:
  - `52-26-PLAN.md:34`: `"relaunch_note is None once the run leaves the stage the note was given for, on every way out, which are the production sites that write State.stage or end the run as they stand at this wave ... transition (pipeline_gate.rs) at both of its exits ... prepare_loop_back_to_code's stage-changing commit ... and 52-08's finish_run at the run's end ... A retry of the same stage, a stop and resume, and a failover replay keep it"`
  - `52-29-PLAN.md:257-258`: `"an approve that takes it writes no note."`
- **Concrete Sequence**:
  1. Validate stage fails with an infrastructure error or timeout, triggering `handle_stage_failure` and opening a `stage_failure` gate.
  2. The operator rejects the gate with `--note "Check test logs in /tmp"`.
  3. `State.relaunch_note` is set to `Some("Check test logs in /tmp")`.
  4. The Validate stage is relaunched. Its prompt includes the note.
  5. The relaunched Validate stage executes without crashing, but discovers validation gaps or a test failure: the agent reports `verdict: gaps`, and GSD status reports `gaps_found` (or `human_needed`).
  6. `handle_validate_outcome` handles the outcome and opens a `gaps_found` or `human_needed` gate in supervise mode.
  7. Neither `transition`, `prepare_loop_back_to_code`, nor `finish_run` has executed. The run remains in `Stage::Validate`.
  8. Therefore, `State.relaunch_note` is STILL `Some("Check test logs in /tmp")` on disk.
  9. If the relaunched Validate stage instead experienced a second `stage_failure` gate, and the operator answers `devflow gate approve` (to retry without a note), `approve_route` does not clear `relaunch_note` (52-29:258: "an approve that takes it writes no note"). The second retry's prompt still carries the stale note from the earlier reject.
- **Evidence Checked**:
  - `52-26-PLAN.md:34` enumerates the clear sites: `transition`, `prepare_loop_back_to_code`, `finish_run`.
  - `handle_validate_outcome` and `stage_failure` approve paths do not clear `relaunch_note`.
- **Suggested Fix**:
  Clear `State.relaunch_note = None` in `handle_validate_outcome` when entering any normal outcome gate, and clear `relaunch_note = None` when `stage_failure` gate is answered with `approve`.

---

### C6 (Medium) — TOCTOU race in `handle_validate_outcome`: reading HEAD before dirty tree risks recording PASS on an unexamined commit
- **Severity**: Medium
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L33](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L33), [#L168-L172](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L168-L172)
- **Quoted Text**:
  - `52-29-PLAN.md:33`: `"read_evidence's one dirty-tree read, git status --porcelain --untracked-files=all over the evidence root through git::hermetic_command, is the only I/O it makes at a Validate outcome; it runs after 52-22's HEAD read and before the PASS is recorded, and an agent PASS whose read lists a path or fails records validate_pass_head = None in the same state commit ... When an approve is picked up, read_evidence reads all three again, never what a gate recorded: the status under the evidence root (52-22's binding), then the dirty tree, then the root's HEAD through git::hermetic_command (dirty tree before HEAD, so a commit landing between the two reads shows as a HEAD change, not as a clean tree)"`
- **Concrete Sequence**:
  1. At approve pickup, 52-29 explicitly orders: read dirty tree FIRST, then read HEAD. It gives the exact rationale: "dirty tree before HEAD, so a commit landing between the two reads shows as a HEAD change, not as a clean tree".
  2. However, at Validate outcome, 52-29 specifies the opposite order:
     "it runs after 52-22's HEAD read and before the PASS is recorded".
  3. Sequence at Validate outcome:
     - `handle_validate_outcome` runs after the Validate agent finishes.
     - Step 1: Read HEAD sha -> returns `H0`.
     - An external commit lands (e.g. background git hook, developer commit, or concurrent tooling) creating `H1`.
     - Step 2: Read dirty tree (`git status --porcelain`) -> working tree is clean at commit `H1`.
     - Step 3: Record `State.validate_pass_head = Some("H0")`.
  4. Now `validate_pass_head` is stamped with `H0`, while the working tree is clean at `H1`.
  5. In supervise mode, `clean_pass` gate opens.
  6. When the operator runs `devflow gate approve`, `read_evidence` at pickup reads dirty tree (clean), then reads HEAD (`H1`). It compares current HEAD (`H1`) against `validate_pass_head` (`H0`).
  7. Because `H0 != H1`, `agent_pass_on_head` evaluates to `false`.
  8. The approve gate is unexpectedly refused or routes to re-run Validate again, surprising the operator with a spurious mismatch caused purely by read inversion during outcome handling.
- **Evidence Checked**:
  - Contrast between 52-29:33 line 16 ("runs after 52-22's HEAD read") and line 22 ("dirty tree before HEAD").
- **Suggested Fix**:
  Harmonize `handle_validate_outcome` with approve pickup: execute `git status --porcelain --untracked-files=all` BEFORE reading HEAD in all paths.

---

### C7 (Low) — Supervise `reverify_needed` gate lacks streak update accounting on approve in 52-22 before 52-28 lands
- **Severity**: Low
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L143-L145](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L143-L145), [#L180-L182](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L180-L182)
  - [.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md#L26](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md#L26), [#L102-L108](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md#L102-L108)
- **Quoted Text**:
  - `52-22-PLAN.md:143`: `"a_stale_report_routes_to_execute_phase ... a supervise row: with the supervise flag set, the same stale read opens a reverify_needed gate ... and approving it relaunches Code on the same route as the auto row ... and records no workflow_finished"`
  - `52-28-PLAN.md:26`: `"in supervise the Validate ends at its reverify_needed or gaps_found gate, 52-22, whose approve 52-29 routes on a fresh read ... The streak update runs at most once per Validate outcome"`
- **Concrete Sequence**:
  1. In Wave 19 (`52-22`), `reverify_needed` gate is introduced for supervise mode on `stale` or `missing` reports.
  2. In 52-22, approving `reverify_needed` relaunches Code directly on the execute route.
  3. However, in 52-22, `handle_validate_outcome` does not update the streak on a `stale` report if the agent said PASS (streak update in `pipeline_outcomes.rs` only runs on `ValidateResult::Failed`).
  4. The streak update for `stale`/`missing` is not added until Wave 25 (`52-28`).
  5. Thus, between Wave 19 and Wave 25, an agent PASS over `stale` in supervise mode can be repeatedly approved without ever incrementing `consecutive_failures` or `phase_validate_failures`, allowing an unbounded manual loop if intermediate plans run in this state.
- **Evidence Checked**:
  - `52-22-PLAN.md:143-145` and `52-28-PLAN.md:26, 102`.
- **Suggested Fix**:
  Note in 52-22 that streak accounting for `reverify_needed` is deferred to 52-28, or include the streak update call on `reverify_needed` gate creation in 52-22.

---

## SUSPECTED

### S1 (Low, Speculation) — `failover_e2e::a_reset_later_than_the_gate_timeout_relaunches_at_the_reset` has a timing-dependent 4-second race window
- **Severity**: Low
- **File & Line**:
  - [.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md#L147](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev5/snap/.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md#L147)
- **Quoted Text**:
  - `52-32-PLAN.md:147`: `"it reaches the exact equal-deadline case only when the gate opens within about 4 s of the test computing resetsAt (later, the 2-second lifetime from the open time falls after the wake), so it is not the equality pin: the pure row of the_timer_wakes_at_reset_plus_margin_whatever_the_gate_timeout (a now equal to both deadlines returns the wake) is, and a dispatcher that checks the lifetime first fails that row"`
- **Concrete Sequence**:
  1. The test computes `resetsAt = now - RATE_LIMIT_RELAUNCH_MARGIN_SECS + 6s`. Gate timeout is set to 2 seconds.
  2. In a slow test execution environment (such as a constrained CI runner, containerized `check-in-container.sh`, or under high load), if the test process takes > 4 seconds between computing `resetsAt` and actually launching the supervisor and opening the gate:
  3. The 2-second lifetime computed at gate opening time will be `gate_open_time + 2s`.
  4. If `gate_open_time > test_start + 4s`, then `gate_open_time + 2s > test_start + 6s`.
  5. In this case, `lifetime > wake`, so the test does not exercise the intended deadline equality at all.
- **Evidence Checked**:
  - Text of `52-32-PLAN.md:147`.
- **Suggested Fix**:
  Freeze or mock the clock in the test or use a relative delta from gate open time.

---

## SIMPLIFY

1. **Clear `validate_pass_head` in `transition`**: Rather than keeping `State.validate_pass_head` durable across stage transitions, clear `validate_pass_head = None` in `transition()` whenever exiting `Stage::Validate`. An agent PASS from Validate should never be readable by a gate opened during `Stage::Plan` or `Stage::Code`.
2. **Harmonize Read Order in `read_evidence`**: Always read the dirty tree (`git status --porcelain --untracked-files=all`) *before* reading git HEAD in both `handle_validate_outcome` and `approve_route`. Having two different orders invites subtle TOCTOU discrepancies.
3. **Snapshot Bytes in `AutoChainGuard`**: Instead of re-serializing `.planning/config.json` on drop via `set_auto_chain_active(..., false)`, `AutoChainGuard` should snapshot the file's raw bytes on creation and restore them verbatim on drop, preventing dirty-tree porcelain false positives.

---

## COVERAGE

- **Read in full**:
  - `.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-26-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-31-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md`
  - `.planning/phases/52-subtract-and-stabilize/52-VALIDATION.md`
  - `.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md` (D-45, D-46, and 2026-11-10 amendment)
  - `.planning/phases/52-subtract-and-stabilize/52-REVIEWS.md` (U1..U10)
  - `.planning/phases/52-subtract-and-stabilize/52-plan-review-4/SUMMARY.md`
  - `CLAUDE.md`
- **Codebase and runtime files inspected**:
  - `crates/devflow-core/src/workflow.rs` (`ensure_devflow_dir` and `.devflow/.gitignore` self-protection)
  - `crates/devflow-core/src/gsd_config.rs` (`set_auto_chain_active`, `read_flag`, and serialization)
  - `crates/devflow-cli/src/pipeline_launch.rs` (`AutoChainGuard::engage` and `Drop`)
  - `crates/devflow-cli/src/pipeline_gate.rs` (`transition`, `finish_run`, `prepare_loop_back_to_code`, lines 60–120)
  - `crates/devflow-cli/src/pipeline_outcomes.rs` (`handle_validate_outcome`, streak update)
  - `crates/devflow-core/src/prompt.rs` (`validate_stage_prompt`, `gsd_command_for`)
  - `~/.claude/gsd-core/bin/lib/verification.cjs`
  - `~/.claude/gsd-core/workflows/validate-phase.md`
  - `~/.claude/gsd-core/workflows/execute-phase/steps/verify-phase-goal.md`
  - `~/.claude/gsd-core/agents/gsd-verifier.md`
  - `.gitignore` (root gitignore)
- **Skimmed / Not read in full**:
  - `.planning/phases/52-subtract-and-stabilize/52-30-PLAN.md` (only frontmatter and dependency links checked; not targeted in this round).
