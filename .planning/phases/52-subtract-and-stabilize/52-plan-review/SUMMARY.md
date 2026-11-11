# Phase 52 plan review — adversarial lens (2026-11-09)

Snapshot: `git archive` of `feature/phase-52` @ e6b3343. Lanes: codex (gpt-6.1-sol, high), agy
(gemini-3.8-flash-high, high), pi (deepseek-flash). All three exited 0 with substantive output; none
dropped. Every finding below was re-checked against the snapshot by the host (Claude) before being
listed. "Verified" means the plan text and current source were read and the contradiction holds; no
future implementation was run.

## Confirmed — a plan's own gate fails, or plans contradict each other

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

## Plausible design gaps (reasoned, not reproduced)

- **P1 (codex F2, High):** approving a clean-pass gate finishes on the status recorded when the gate opened. Evidence can change during a wait of up to 3 days. No plan re-reads verification when an approval is picked up.
- **P2 (codex F1, High):** an uncovered-file quick fix whose re-verify agent skips the verifier and leaves `passed` unchanged would pass the post-check. `52-GAP-LOOP-VERIFICATION.md:92-100` records both ingredients separately.
- **P3 (codex F9, Low):** 52-32's flag-then-answer order is at-most-once. The plan admits that a crash between the two writes loses the relaunch. That fails safe (the gate stays open), but the "exactly once" wording overclaims.
- **P4 (pi F8, Low):** `code_fix_note` and `relaunch_note` have different lifetimes, so a rate-limit reject during a fix pass shows both notes.
- **P5 (pi F9, Low):** 52-23's audit of about 94 `warn!`/`error!` sites is verified only by judgment.
- **P6 (pi F5, Low):** 52-18 leaves `CHANGELOG.md` out of the scan, while ROADMAP criterion 2 says "the top-level docs". The plan records this as an interpretation. Amending the criterion text would remove the ambiguity for the verifier.

## Refuted

- agy F-04: `FixType::AuditFix` is not in `state.rs`. Every use is in files 52-15 already lists.
- agy F-05: 52-21 being a leaf is fine. Waves follow the DAG levels, and 52-21 shares no file with 52-19, 52-20 or 52-22.
- agy F-08: 52-02's gate failing when the 3.0.0 control is unavailable is the intended fail-closed behavior. The installed binary prints `devflow 3.0.0`.
- pi F7: the mermaid node *labels* (`hooks`, `prompt`, …) are lowercase. Only the node ids are uppercase, so the rule does match them.
- agy S-01: speculation about operator UX, not a plan defect.
