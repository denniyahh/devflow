## CONFIRMED

### 1. Gap loop target change to Plan (D-09) renders `MAX_CONSECUTIVE_FAILURES` ceiling permanently unreachable (contradicting D-12)
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L80-L83](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L80-L83) (D-09): `"The loop-back target for gaps therefore changes from Code to Plan."`
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L92-L94](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L92-L94) (D-12): `"No new cap. Each gaps_found round counts as a Validate failure, so MAX_CONSECUTIVE_FAILURES (3) and MAX_PHASE_VALIDATE_FAILURES (10) in crates/devflow-core/src/mode.rs apply unchanged."`
  * [`crates/devflow-core/src/mode.rs#L142-L144`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/mode.rs#L142-L144):
    ```rust
    pub fn transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool {
        !matches!((from, to), (Stage::Code, Stage::Validate))
    }
    ```
  * [`crates/devflow-cli/src/pipeline_gate.rs#L108-L111`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/pipeline_gate.rs#L108-L111):
    ```rust
    commit(state, |s| {
        s.stage = to;
        if mode::transition_resets_consecutive_failures(from, to) {
            s.consecutive_failures = 0;
        }
    ```
* **Contradiction & Mechanism:**
  When Validate finds gaps, it increments `consecutive_failures` and loops back to Plan. When Plan finishes its `--gaps` run and advances to Code via `transition(project_root, state, Stage::Code)`, `transition_resets_consecutive_failures(Stage::Plan, Stage::Code)` evaluates to `true` (it only returns `false` for `(Code, Validate)`). This resets `s.consecutive_failures` to `0` on every single gap cycle. As a result, `consecutive_failures` cannot accumulate across cycles and will never reach `MAX_CONSECUTIVE_FAILURES` (3). It will loop unchecked until the cumulative backstop `phase_validate_failures` (10), defeating D-12's assertion that `MAX_CONSECUTIVE_FAILURES` applies unchanged.
* **Limits:** What this check does not establish is whether a new `StageIntent` or variant of `transition_resets_consecutive_failures` could fix it; it proves that applying the existing code "unchanged" as D-12 demands breaks the failure ceiling.

---

### 2. Rate-limit gate context instructs operator to run `devflow resume`, which is hard-refused by the live supervisor (D-15 vs Liveness Architecture)
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L108-L112](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L108-L112) (D-15): `"its context names the reset time and the exact devflow resume --phase N … command; ... The supervisor also relaunches by itself once resetsAt has passed"`
  * [.planning/ROADMAP.md#L96-L98](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/ROADMAP.md#L96-L98) (Criterion 3): `"waits at a gate whose context names the reset time and the exact devflow resume --phase N ... command to run after it. Once the reset time passes, the supervisor relaunches the stage exactly once by itself."`
  * [`crates/devflow-cli/src/pipeline_launch.rs#L3391-L3398`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L3391-L3398): `resume_with_fallback` invokes `reserve_launch`.
  * [`crates/devflow-cli/src/commands.rs#L1009-L1010`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/commands.rs#L1009-L1010) & [`#L4700-L4713`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/commands.rs#L4700-L4713):
    ```rust
    let sockets = possibly_live_sockets(project_root, phase, state, cache_dir)...;
    if sockets.is_empty() && agent.is_none() {
        return Ok(());
    }
    Err(CliError::Message(live_run_refusal(phase, verb, &sockets, agent)))
    ```
* **Contradiction & Mechanism:**
  For the supervisor to auto-relaunch itself once `resetsAt` has passed (D-15), the supervisor process must stay alive and keep its socket open. But while that socket is alive, running `devflow resume --phase N` (the exact command printed in the gate notification) calls `reserve_launch`, which invokes `refuse_launch_over_a_live_run`. Because the socket is alive, `reserve_launch` hard-refuses: `"phase {phase} has a live supervisor"`. Telling the operator to run `devflow resume` while keeping the supervisor alive gives them a command that fails every time. Furthermore, telling them to run it "after it" (Criterion 3) creates a collision with the supervisor's automatic relaunch at that same moment.
* **Limits:** Does not establish whether a flag could bypass `refuse_launch_over_a_live_run`; it proves the command as specified fails against the codebase today.

---

### 3. Direct contradiction on Validate rejection behavior between Criterion 4 / GATE-02 and D-09 / D-10
* **Citations:**
  * [.planning/ROADMAP.md#L105-L107](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/ROADMAP.md#L105-L107) (Criterion 4): `"A Validate rejection relaunches GSD's gaps route for the phase, not the whole phase; a verification that reports gaps_found is driven to passed inside the same run"`
  * [.planning/REQUIREMENTS.md#L33-L34](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/REQUIREMENTS.md#L33-L34) (GATE-02): `"A Validate rejection relaunches only the gap-closure work (GSD's gaps route), not the whole phase."`
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L54-L56](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L54-L56) (D-04): `"Supervise mode stays, kept minimal: one gate, at every Validate. Approve records workflow_finished with shipped: false... Reject (with a note) runs the in-run fix loop (D-09 to D-11)."`
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L72](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L72) (D-08): `"- gaps_found → the gap loop (D-09)."`
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L84-L86](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L84-L86) (D-10): `"Rejecting a Validate that passed cleanly (no gaps) relaunches Code with the operator's note verbatim as the task, run through /gsd-quick, then Validate."`
* **Contradiction & Mechanism:**
  * Under Criterion 4 and GATE-02, rejecting a Validate gate relaunches GSD's gaps route (`plan-phase --gaps`).
  * But under D-08, `gaps_found` transitions automatically to Plan without gating.
  * If D-04 ("one gate, at every Validate") gates anyway on `gaps_found`, then an operator who runs `gate approve` records `workflow_finished` on broken code (violating Criterion 4: "the Validate gate never reads 'passed' while the report says otherwise").
  * If Validate only gates when it passes cleanly (as D-05 and D-08 imply), D-10 dictates that rejecting it relaunches Code with `/gsd-quick` and the operator's note. That is NOT GSD's gaps route.
  * Therefore, the scenario specified in Criterion 4 and GATE-02 ("A Validate rejection relaunches GSD's gaps route") does not exist anywhere in the D-08..D-10 state machine.
* **Limits:** What this check does not establish is operator preference between `/gsd-quick` and `/gsd-plan-phase --gaps`; it establishes that the requirements and decisions name conflicting routes for the same event.

---

### 4. Cutting `lock.rs` in D-01 breaks `recover.rs`
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L36](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L36) (D-01): lists `lock.rs` under confirmed cut rows.
  * [.planning/user/V4-CUT-INVENTORY.md#L126](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/user/V4-CUT-INVENTORY.md#L126): `"Shared checkout lock (lock.rs) ... Serves the hook batch only ... cut"`
  * [`crates/devflow-core/src/recover.rs#L85`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/recover.rs#L85), [`#L142`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/recover.rs#L142), [`#L805`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/recover.rs#L805):
    ```rust
    for outcome in crate::lock::remove_stale_locks_with_outcomes(project_root) {
    ```
  * [`crates/devflow-core/src/lock.rs#L374-L410`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/lock.rs#L374-L410): defines `remove_stale_locks` and `remove_stale_locks_with_outcomes`.
* **Cut-Scope Risk & Mechanism:**
  The premise in V4-CUT-INVENTORY that `lock.rs` "serves the hook batch only" is factually false. `lock.rs` also manages the sweeping of stale locks (`remove_stale_locks_with_outcomes`), which is actively called by `recover.rs`. `recover.rs` is a core module kept for `devflow doctor` and `cleanup`. Cutting `lock.rs` breaks the build of `devflow-core`.
* **Limits:** Does not establish whether `remove_stale_locks` could be moved to `recover.rs`; it establishes that deleting `lock.rs` as a whole cuts a live dependency.

---

### 5. Cutting `ship.rs` in D-01 breaks RFC 3339 timestamp generation in core supervisor daemon
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L36](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L36) (D-01): lists `version.rs / ship.rs / ship_evidence.rs` under confirmed cut rows.
  * [`crates/devflow-core/src/supervisor.rs#L509`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/supervisor.rs#L509):
    ```rust
    let (year, month, day) = crate::ship::civil_from_days(days);
    ```
  * [`crates/devflow-core/src/ship.rs#L57-L78`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/ship.rs#L57-L78): defines `pub(crate) fn civil_from_days(days: i64) -> (i32, u32, u32)`.
* **Cut-Scope Risk & Mechanism:**
  `supervisor.rs` is a core file for supervisor socket management and liveness, which is explicitly retained. Its `rfc3339_from_unix` helper directly calls `crate::ship::civil_from_days(days)` to format UTC timestamps for `Sidecar::created_at`. Deleting `ship.rs` causes a compilation error in `supervisor.rs`.
* **Limits:** Does not establish whether the helper can be relocated; it establishes that `ship.rs` contains live code used outside of shipping.

---

### 6. Code anchor error in D-13: `gate abort` declared as `Command` variant instead of `GateCmd`
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L96-L100](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L96-L100) (D-13): `"A new verb, devflow gate abort <phase> [--gate-id <id>] [--note <text>]... The new verb is a Command variant, so it needs docs or an allowlist entry for doc_check."`
  * [`crates/devflow-cli/src/main.rs#L236-L239`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/main.rs#L236-L239) & [`#L435-L525`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/main.rs#L435-L525): `Command` is the top-level enum (`Start`, `Gate`, `Logs`...). Subcommands under `devflow gate` belong to `GateCmd` (`List`, `Approve`, `Reject`, `Show`, `Sweep`).
  * [`crates/devflow-core/src/doc_check.rs#L425-L432`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/doc_check.rs#L425-L432):
    ```rust
    for command in enum_variants(&source, "Command") {
        let token = format!("devflow {command}");
        assert!(
            docs.to_ascii_lowercase().contains(&token)
                || is_allowlisted(&allowlist, "source_to_docs", &token),
            "CLI subcommand `{token}` is missing from scoped operator docs"
        );
    }
    ```
* **Code Anchor Error & Mechanism:**
  Adding `Abort` to `Command` creates `devflow abort`, not `devflow gate abort`. To create `devflow gate abort`, `Abort` must be added to `GateCmd`. Furthermore, `doc_check.rs#L425` verifies only `Command` variants against docs/allowlist, while lines 372-378 verify that documented subcommands exist in either `Command` or `GateCmd`. Calling `gate abort` a `Command` variant is an incorrect code anchor that misguides implementation.
* **Limits:** Does not establish how Clap generates help; it verifies the Rust AST type names and `doc_check` assertions in the snapshot.

---

### 7. Non-Claude agents (Codex, Pi) and `StageIntent` omitted from rejection note delivery (D-14)
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L101-L103](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L101-L103) (D-14): `"The rejection note reaches the relaunched agent's prompt verbatim, and that is asserted on the launch record or the captured prompt (ROADMAP criterion 4). Today fix_prompt(fix_type, phase) (prompt.rs L713) takes no note, so it gains one."`
  * [.planning/ROADMAP.md#L103-L104`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/ROADMAP.md#L103-L104) (Criterion 4): `"devflow gate reject <phase> --note "<text>" relaunches the agent with <text> verbatim in its prompt"`
  * [`crates/devflow-core/src/prompt.rs#L173-L176`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/prompt.rs#L173-L176):
    ```rust
    Code {
        phase: PhaseId,
        fix: Option<FixType>,
    },
    ```
  * [`crates/devflow-core/src/prompt.rs#L506-L518`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/prompt.rs#L506-L518) & [`#L558-L585`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/prompt.rs#L558-L585): `render_workflow_style` and `workflow_code_prompt(phase: PhaseId, fix: Option<FixType>, workflow_root: &str)`.
* **Incompleteness & Mechanism:**
  D-14 singles out `fix_prompt` as the only function gaining the note. But `fix_prompt` is exclusively called by `render_claude_style` (L487). Non-Claude agents (Codex, Pi) are dispatched via `render_workflow_style`, which calls `workflow_code_prompt`. Furthermore, the carrier enum `StageIntent::Code` does not hold any note. If only `fix_prompt` gains the note, non-Claude agents will silently never receive the rejection note in their prompt, failing Criterion 4.
* **Limits:** Does not establish how an agent executes the prompt; it proves the prompt generation code path for Codex and Pi does not receive the note under D-14's specification.

---

### 8. Cutting `version.rs` in D-01 breaks release checks retained by `scripts/cut-release.sh`
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L36](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L36) (D-01): lists `version.rs` under confirmed cut rows.
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L43-L46](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L43-L46) (D-02): `"release --check/--verify is cut together with moving its pin checks into scripts/cut-release.sh... The release script must not break."`
  * [`crates/devflow-cli/src/commands.rs#L5945`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/commands.rs#L5945), [`#L6063`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/commands.rs#L6063), [`#L6242`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/commands.rs#L6242):
    ```rust
    let (workspace_version, pins) = version::read_workspace_self_pins(&contents);
    ```
* **Cut-Scope Risk & Mechanism:**
  D-02 dictates moving `release --check` pin checks into a selftested Rust tool or `scripts/cut-release.sh`. These pin checks (`check_self_pin`, `check_changelog_sync`, `check_release_tag`) depend directly on `version::read_workspace_self_pins` and semver parsing in `crates/devflow-core/src/version.rs`. If `version.rs` is cut as a Ship-dependent in D-01, the underlying parsing logic for self-pin verification is destroyed.
* **Limits:** Does not establish whether a shell equivalent exists in Bash; it establishes that the Rust implementation in the snapshot depends on `version.rs`.

---

## ## SUSPECTED

### 9. Unhandled failure and missing-executable modes when Validate invokes external `gsd query` (D-08)
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L68-L76](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L68-L76) (D-08): `"The Validate verdict is pass only if the validate-phase agent says pass and GSD's own reader, gsd query verification status <phase_dir>, returns passed... DevFlow follows the route GSD returns: gaps_found... stale or missing... human_needed..."`
  * `~/.claude/gsd-core/bin/lib/verification.cjs#L302-L315`: defines `unparseable` (syntax error in YAML frontmatter) and `phase_dir_not_found` routes in GSD.
* **Suspected Failure Mode:**
  `gsd` is an external Node.js CLI tool. In execution environments without `gsd` on PATH, with broken shims (verified in current terminal session: `mise ERROR No version is set for shim: gsd` on `gsd query verification status`), or in hermetic CI containers (`check-in-container.sh`), executing `gsd query` will fail to spawn or exit non-zero. D-08 provides zero failure handling for execution errors or non-zero exits. Additionally, if GSD returns `unparseable`, treating it as `missing` would re-run `execute-phase`, which GSD's own documentation warns cannot fix a YAML typo.
* **Limits:** Unverified how the CLI runner handles process spawn errors for external tools until `pipeline_outcomes.rs` is modified.

---

### 10. `gate sweep` can abort unattended rate-limit waits before reset time expires (D-15 / D-16)
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L111-L120](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L111-L120) (D-15/D-16): rate limit gate waits for `resetsAt` to pass, bounded by `DEVFLOW_GATE_TIMEOUT_SECS`.
  * [`crates/devflow-cli/src/commands.rs#L2512-L2525`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/commands.rs#L2512-L2525) & [`#L2684-L2686`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-cli/src/commands.rs#L2684-L2686): `gate_sweep` reaps gates older than `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` when a waiter is live.
  * [`crates/devflow-core/src/gates.rs#L692-L703`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/crates/devflow-core/src/gates.rs#L692-L703): `Gates::reap_answer` writes an abort response.
* **Suspected Failure Mode:**
  When a rate limit wait extends for multiple hours, the supervisor sits polling the gate file with a live waiter. If `devflow gate sweep` runs periodically (e.g., via background cron or operator command) with an age threshold shorter than `resetsAt`, it observes an aged gate with a live waiter and reaps it by writing an abort response. When the parked supervisor picks up this response, it executes `GateAction::Abort` and tears down the workflow, completely nullifying the unattended auto-relaunch feature.
* **Limits:** Unverified whether the operator runs `gate sweep` on a schedule; verified that `gate_sweep` code unconditionally aborts gates with live waiters when the age threshold is crossed.

---

### 11. Gap-loop verification evidence is significantly narrower than the multi-agent contract relying on it (D-08..D-11 vs 52-GAP-LOOP-VERIFICATION.md)
* **Citations:**
  * [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L131-L140](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L131-L140) (D-19): claims recorded check shows what GSD actually does before building D-08..D-11.
  * [.planning/phases/52-subtract-and-stabilize/52-GAP-LOOP-VERIFICATION.md#L54-L68](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/a1867289-5fbb-4490-a4b8-2fea952372af/scratchpad/xreview/snapshot/.planning/phases/52-subtract-and-stabilize/52-GAP-LOOP-VERIFICATION.md#L54-L68):
    * `"Validate stage not run. /gsd-validate-phase (DevFlow's Validate) never ran. Re-verification happened inside execute-phase... but the Validate stage's verdict pairing (agent AND status) is untested."`
    * `"Clean reject not run. D-10's /gsd-quick route was not exercised."`
    * `"Stale-gaps gate not run live. D-11's case was only checked through the status read (check 1)."`
    * `"Claude only. No codex or other agent ran the loop."`
    * `"Not DevFlow's full prompts or config. The prompts were the bare GSD commands, without DevFlow's DEVFLOW_RESULT contract or the code policy text."`
* **Evidence Gap:**
  D-19 unlocked implementation of D-08..D-11 on the basis that the probe proved GSD behavior. However, the probe was run once ($20.66, n=1) on a toy 2-function Python repo without DevFlow's required completion contracts, without running `/gsd-validate-phase`, without exercising non-Claude agents, and without running the clean reject route. Relying on this probe as proof that the entire multi-stage loop converges across supported agents overstates the strength of the evidence.
* **Limits:** Does not prove that the loop will fail on a real phase; it proves that the live evidence cited cannot carry the weight of the broad stability guarantee claimed in CONTEXT.
