## CONFIRMED

### C-01: Stale `validate_pass_head` persists across stages and enables terminal `workflow_finished` from `Stage::Plan`
- **Severity**: Critical
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L30](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md#L30) and [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L34-L35](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L34-L35)
- **Quoted Text**:
  - `52-22-PLAN.md:30`: `"The field is durable run state (#[serde(default, skip_serializing_if = \"Option::is_none\")]) written only here: no stage transition, stop, resume or supervisor takeover clears it, so 52-29's can_finish reads it after any of them"`
  - `52-29-PLAN.md:34`: `"maps all nine verification kinds (clean_pass, ..., failure_cap, no_gap_closure_plan) to one Verification family. The routers D-39 names, approve_route... and reject_route... send that family to next_step and read gate_kind only through answer_family, so two verification gates of different kinds with the same evidence get the same route"`
- **Breaking Sequence**:
  1. `Stage::Validate` runs on commit `H1`. The Validate agent passes tests, writing `State.validate_pass_head = Some("H1")`.
  2. GSD status returns `gaps_found`. Per D-45 row 3, DevFlow transitions to `Stage::Plan` (`transition(Validate -> Plan)` with `gap_closure_pass = true`).
  3. `transition` deliberately does not clear `validate_pass_head` (`52-22-PLAN.md:30`).
  4. At `Stage::Plan`, the planner agent fails to write a gap-closure plan (`gap_closure: true`), opening a gate of kind `no_gap_closure_plan` ([52-27-PLAN.md#L145](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md#L145)). No new commits landed, so `HEAD` is still `H1`.
  5. `no_gap_closure_plan` is mapped into `AnswerFamily::Verification` ([52-29-PLAN.md#L34](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L34)).
  6. Operator answers `devflow gate approve` on `no_gap_closure_plan`.
  7. `approve_route` invokes `read_evidence(state)`. If GSD status is `passed` (e.g. manually verified or refreshed), `read_evidence` checks `current_head` (`H1`) against `state.validate_pass_head` (`H1`), evaluating `agent_pass_on_head` to `true`.
  8. `next_step` matches Row 1 of D-45 (`passed` + `true` -> `finish`), constructing `(Step::Finish, Some(FinishPermit))`.
  9. `approve_route` calls `finish_run(state, permit)`. DevFlow emits `workflow_finished` with reason `"finished"` directly while in `Stage::Plan`.
  10. `can_finish` held on stale evidence from an earlier stage; Validate never executed or passed on any gap-closure work.
- **Evidence Checked**:
  - `52-22-PLAN.md:30`, `52-29-PLAN.md:34, 125, 159`, `52-27-PLAN.md:145`.
  - [crates/devflow-core/src/state.rs#L36-L70](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/crates/devflow-core/src/state.rs#L36-L70).
  - Limits of check: Does not check whether an intermediate tool modifies `.planning/` files; relies on `git rev-parse HEAD` remaining `H1` when Plan makes no commit.
- **Suggested Fix**:
  `transition()` must clear `s.validate_pass_head = None` whenever leaving `Stage::Validate`. Additionally, `no_gap_closure_plan` must not be classified under `AnswerFamily::Verification`.

---

### C-02: Unbounded Auto gap loop on Validate agent PASS + GSD `gaps_found`
- **Severity**: Critical
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md#L26](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md#L26) and [.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md#L112](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md#L112)
- **Quoted Text**:
  - `52-28-PLAN.md:26`: `"a Validate whose fresh status is stale or missing counts one Validate failure through the streak update, whatever the agent's verdict and in either mode ... The streak update runs at most once per Validate outcome, so a round counted for two reasons (an agent FAIL over stale, missing or gaps_found: the existing update already counts the FAIL) adds exactly one to each counter."`
  - `52-27-PLAN.md:112`: `"Route gaps_found to transition(Validate → Plan) with the marker; launch the gaps Plan intent when the marker is set; after it, launch Code with FixType::GapsOnly..."`
- **Breaking Sequence**:
  1. In Auto mode, Validate runs. The Validate agent passes (unit tests exit 0), yielding `ValidateResult::Passed`.
  2. GSD status returns `gaps_found` (the exact 999.193 defect).
  3. In [crates/devflow-cli/src/pipeline_outcomes.rs#L581](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/crates/devflow-cli/src/pipeline_outcomes.rs#L581), streak update runs only `if result == ValidateResult::Failed`. Because the agent passed, this branch is skipped.
  4. `52-28-PLAN.md:26` adds streak accounting only for `stale` or `missing`, under the false assumption that any `gaps_found` would already fail the agent (`"an agent FAIL over stale, missing or gaps_found: the existing update already counts the FAIL"`).
  5. `52-27-PLAN.md:112` routes `gaps_found` directly to `transition(Validate → Plan)` with the gap marker without invoking the streak update.
  6. Neither `consecutive_failures` nor `phase_validate_failures` increments.
  7. Plan runs `--gaps`, Code runs `--gaps-only`, and Validate runs again. The agent passes tests, but GSD still reports `gaps_found`.
  8. `consecutive_failures` never reaches 3, and `phase_validate_failures` never reaches 10. The loop runs indefinitely in Auto mode.
- **Evidence Checked**:
  - `crates/devflow-cli/src/pipeline_outcomes.rs:581-651`.
  - `52-28-PLAN.md:26`, `52-27-PLAN.md:112-120`.
  - Limits of check: Confirmed against the text of 52-27 and 52-28; assumes no external script commits source changes that artificially trigger failure tracking.
- **Suggested Fix**:
  In `52-28-PLAN.md`, invoke the streak-update function for `gaps_found` whenever the Validate agent passed, identical to the handling for `stale` and `missing`.

---

### C-03: `can_finish` holds over dirty/uncommitted working tree
- **Severity**: High
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L144-L147](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L144-L147), [L369](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L369)
- **Quoted Text**:
  - `52-29-PLAN.md:144-146`: `"`read_evidence`, the fresh status read (`gsd_reader::verification_status` under the evidence root and the run's HOME, as 52-22 binds them) plus the evidence root's HEAD through `git::hermetic_command` compared with `State.validate_pass_head`"`
  - `52-29-PLAN.md:369`: `"The HEAD comparison sees commits, not uncommitted worktree changes."`
- **Breaking Sequence**:
  1. In Supervise mode, Validate passes on commit `H1`. `State.validate_pass_head` is stamped with `H1`. GSD status returns `passed`.
  2. The supervisor opens a `clean_pass` gate: `"Validation passed — approve to ship?"`.
  3. While the gate waits, an operator, agent tool, or script edits tracked source files in the worktree without committing.
  4. Operator runs `devflow gate approve`.
  5. `approve_route` calls `read_evidence`, which runs `git rev-parse HEAD`. Since no commit was created, HEAD remains `H1`.
  6. `agent_pass_on_head` evaluates to `true` (`H1 == H1`), and GSD returns `passed`.
  7. `can_finish` constructs a `FinishPermit`, and `finish_run` emits `workflow_finished` with reason `"finished"`.
  8. The run finishes on an uncommitted, unverified working tree.
- **Evidence Checked**:
  - `52-29-PLAN.md:144-147, 369`.
  - D-45 Rule 1 in [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L371-L373](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L371-L373).
  - Limits of check: Validated against git plumbing semantics; does not check whether untracked files are ignored.
- **Suggested Fix**:
  `read_evidence` must check for a clean worktree (`git status --porcelain` is empty) and set `agent_pass_on_head = false` if uncommitted modifications exist.

---

### C-04: `52-03-PLAN.md` Task 1 verify command violates CLAUDE.md Repo Trap
- **Severity**: High
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-03-PLAN.md#L109](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-03-PLAN.md#L109) and [CLAUDE.md#L20](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/CLAUDE.md#L20)
- **Quoted Text**:
  - `52-03-PLAN.md:109`: `"<automated>cargo nextest run -p devflow --test phase7_cli --no-tests=fail -E 'test(=removed_flags_are_unknown)' && cargo nextest run -p devflow --test gate_wedge_e2e --no-tests=fail && cargo nextest run -p devflow --test failover_e2e ...</automated>"`
  - `CLAUDE.md:20`: `"Gate on one named test: cargo nextest run -p <pkg> --lib --no-tests=fail -E 'test(=module::path::name)' (--bin devflow for the CLI). One test per command, chained with &&: -E 'test(=a) | test(=b)' skips a wrong name and exits 0."`
- **Breaking Sequence**:
  1. `52-03-PLAN.md:109` executes `cargo nextest run -p devflow --test gate_wedge_e2e --no-tests=fail` without `-E 'test(=...)'`.
  2. Running the entire test file without a named filter masks regressions if specific tests are renamed or deleted, allowing the suite to exit 0 on unrelated tests.
  3. This directly violates repository policy in `CLAUDE.md:20`.
- **Evidence Checked**:
  - `52-03-PLAN.md:109`, `CLAUDE.md:18-20`.
  - Limits of check: `scripts/lint-plan-bashisms.sh` permits `--test NAME` without `-E`, but `CLAUDE.md:20` explicitly forbids it as a repo trap.
- **Suggested Fix**:
  Specify the exact test name with `-E 'test(=...)'` for every invocation in `52-03-PLAN.md:109`.

---

### C-05: Conflicting prompt note carrier specifications between stage failure and rate limit at Code stage
- **Severity**: Medium
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L35](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md#L35) and [.planning/phases/52-subtract-and-stabilize/52-31-PLAN.md#L34](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-31-PLAN.md#L34), [L36](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-31-PLAN.md#L36)
- **Quoted Text**:
  - `52-29-PLAN.md:35`: `"at Code the note reaches the prompt through 52-26's code_fix_note, at Plan and Validate through 52-31's State.relaunch_note, R9"`
  - `52-31-PLAN.md:34`: `"P4 (review): a Code fix pass that is relaunched with a relaunch note carries two notes with different lifetimes (52-26's code_fix_note, cleared when the fix pass completes; relaunch_note, cleared when the pass it was given to ends). Both stay... the relaunch-note block says it is the operator's note for this relaunch, given when rejecting the rate-limit or stage-failure gate"`
  - `52-31-PLAN.md:36`: `"the note of a rejected rate-limit gate, and of a rejected stage-failure gate at Plan or Validate (52-29's relaunch with the note; at Code that note already travels in 52-26's code_fix_note)"`
- **Breaking Sequence**:
  1. A Code stage failure gate is rejected with a note.
  2. `52-29-PLAN.md:35` and `52-31-PLAN.md:36` specify writing to `code_fix_note`. This clobbers any prior fix note on the pass.
  3. Meanwhile, `52-31-PLAN.md:34` asserts that `relaunch_note` is written on a stage-failure rejection so that both `code_fix_note` and `relaunch_note` coexist in the Code prompt.
  4. The plans contradict each other regarding which field carries rejection notes at Code stage.
- **Evidence Checked**:
  - `52-29-PLAN.md:35`, `52-31-PLAN.md:34, 36`.
  - Limits of check: Identifies contradictory textual specifications across plans; runtime behavior depends on which plan an executor prioritizes.
- **Suggested Fix**:
  Explicitly mandate that all gate rejections (stage failure and rate limit across all stages) populate `State.relaunch_note`, reserving `code_fix_note` solely for the underlying fix request.

---

### C-06: Unterminated `<automated>` tags in planning documentation break bashism linter
- **Severity**: Medium
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-VALIDATION.md#L48](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-VALIDATION.md#L48) and [.planning/phases/52-subtract-and-stabilize/52-REVIEWS.md#L232](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-REVIEWS.md#L232)
- **Quoted Text**:
  - `52-VALIDATION.md:48`: `| 52-29 | Wave 26 | D-45 | 1 | <automated> |`
  - `scripts/lint-plan-bashisms.sh:52-53`: `"An opening tag with no closing tag is reported as a malformed block rather than ignored: leaving it unhandled would be a bypass by construction."`
- **Breaking Sequence**:
  1. Executing `scripts/lint-plan-bashisms.sh .planning/phases/52-subtract-and-stabilize/*.md` fails with exit code 1:
     `lint-plan-bashisms: .planning/phases/52-subtract-and-stabilize/52-VALIDATION.md: unterminated <automated> tag`
  2. While `--staged` currently filters for `*PLAN.md`, any test or hook that runs the linter across all markdown files fails immediately.
- **Evidence Checked**:
  - Verified by running `scripts/lint-plan-bashisms.sh .planning/phases/52-subtract-and-stabilize/*.md`.
  - Limits of check: Only affects tools that pass all markdown files to `lint-plan-bashisms.sh`.
- **Suggested Fix**:
  Wrap literal `<automated>` occurrences in backticks (`` `<automated>` ``) in `52-VALIDATION.md` and `52-REVIEWS.md`.

---

## SUSPECTED

### S-01: Redundant GSD status and git HEAD query during CLI `gate_respond` introduces latency and divergence risk
- **Severity**: Medium (Speculation)
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md#L27](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md#L27), [L133-L138](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md#L133-L138)
- **Quoted Text**:
  - `52-33-PLAN.md:27`: `"A verification approve prints the route approve_route gives over its own fresh read_evidence and says the supervisor reads GSD's status again when it picks the answer up"`
- **Breaking Sequence**:
  1. Operator runs `devflow gate approve --stage validate`.
  2. `gate_respond` spawns child processes for `gsd query verification status` and `git rev-parse HEAD` via `read_evidence`, then prints the projected route.
  3. `gate_respond` writes the answer file to disk.
  4. The supervisor wakes, claims the answer file, and executes `read_evidence` a second time.
  5. If repository state changes between steps 2 and 4, the printed CLI route contradicts the route taken by the supervisor. If GSD query fails during step 2, answering the gate fails before writing the answer.
- **Evidence Checked**: `52-33-PLAN.md:27, 133-140`.
- **Suggested Fix**: Have `gate_respond` print a static acknowledgment derived from `GateFile.gate_kind` without spawning external processes.

---

### S-02: State rollback race on rate-limit auto-relaunch
- **Severity**: Low (Speculation)
- **File & Line**: [.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md#L27](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/rev4/snap/.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md#L27)
- **Quoted Text**:
  - `52-32-PLAN.md:27`: `"it commits rate_limit_auto_relaunch_used = true durably first, then writes the timer approve; if that write returns AlreadyResponded because an operator answered first, it commits the flag back to false and takes the operator's answer"`
- **Breaking Sequence**:
  1. Supervisor timer wakes and durably commits `rate_limit_auto_relaunch_used = true`.
  2. An operator answers the gate concurrently via CLI.
  3. Timer write returns `AlreadyResponded`.
  4. If the supervisor process crashes before executing the compensating commit (`rate_limit_auto_relaunch_used = false`), the flag remains spent on disk permanently.
- **Evidence Checked**: `52-32-PLAN.md:27-28`, D-45 Rule 5.
- **Suggested Fix**: Do not commit `rate_limit_auto_relaunch_used = true` until the timer successfully claims the gate.

---

## SIMPLIFY

1. **Eliminate child process execution in `52-33` CLI `gate_respond`**:
   Invoking `read_evidence` from the CLI just to print a predicted route adds process latency and failure modes. Render the output purely from the gate file's metadata and notify the user that the supervisor evaluates live evidence.
2. **Unify `code_fix_note` and `relaunch_note` into a single field**:
   Maintaining two separate note fields on `State` with differing lifetimes creates conflicting prompt-formatting logic. Consolidate into `State.relaunch_note`.
3. **Remove `no_gap_closure_plan` from `AnswerFamily::Verification`**:
   `no_gap_closure_plan` belongs to `Stage::Plan`. Treating it as a verification gate creates the cross-stage terminal finish vulnerability identified in finding C-01.

---

## COVERAGE

- **Read in Full**:
  - `CLAUDE.md`
  - `.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md` (lines 360–517, D-45 and clarification)
  - `.planning/phases/52-subtract-and-stabilize/52-REVIEWS.md` (Round 3 and final section)
  - `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md` (lines 1–492)
  - `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md` (lines 1–292)
  - `.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md` (lines 1–353)
  - `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md` (lines 1–277)
  - `.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md` (lines 1–290)
  - `.planning/phases/52-subtract-and-stabilize/52-31-PLAN.md` (lines 1–378)
  - `.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md` (lines 1–326)
  - `.planning/phases/52-subtract-and-stabilize/52-03-PLAN.md` (lines 1–193)
  - `.planning/phases/52-subtract-and-stabilize/52-VALIDATION.md` (lines 1–126)
  - `scripts/lint-plan-bashisms.sh` (lines 1–125)
  - `crates/devflow-cli/src/pipeline_outcomes.rs` (lines 520–700)
  - `crates/devflow-core/src/state.rs` (lines 36–120)
- **Skimmed / Targeted Grep**:
  - `.planning/ROADMAP.md` (Phase 52 and Phase 999 entries)
  - `crates/devflow-cli/src/pipeline_gate.rs`
  - `crates/devflow-cli/src/commands.rs`
  - `crates/devflow-core/src/gates.rs`
