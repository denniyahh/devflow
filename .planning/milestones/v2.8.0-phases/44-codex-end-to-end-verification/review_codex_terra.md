## Findings

### 1. High — handoff cannot preserve the state fields the plan promises

**Location:** [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:25), [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:277), [pipeline_launch.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-cli/src/pipeline_launch.rs:170)

**Defect:** The plan requires successful handoff to change only `agent` and `monitor_pid`, and selects `Stage::Validate` for the whole-state preservation test. But every normal relaunch resets `checkpoint_resumes = 0`; a Validate relaunch also calls `stamp_validate_dispatch_window`, mutating the verification nonce, fingerprint, mtime, and baseline flag.

**Consequence & reproduction:** Save a Validate state with a nonzero `checkpoint_resumes` and distinguishable verification fields, then hand it off. The prescribed JSON comparison fails even with the proposed implementation. Passing it requires weakening D-06 coverage or changing resume/launch semantics.

---

### 2. High — the claimed missing-driver preflight does not exist for Codex

**Location:** [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:171), [agents/mod.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-core/src/agents/mod.rs:97), [codex.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-core/src/agents/codex.rs:17), [pipeline_launch.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-cli/src/pipeline_launch.rs:985)

**Defect:** The plan says `driver_for(requested).health(...)` makes a missing binary fail before mutation. `CodexDriver` does not override `health`; it inherits the trait’s unconditional `Ok(())`. The actual binary check is `ensure_agent_binary`, after `state.monitor_pid = None` has been persisted.

**Consequence & reproduction:** Remove `codex` from `PATH`, then run `resume --agent codex` from a Claude state. The handoff persists `agent: codex`, emits its audit event, clears the PID, and only then fails. This violates D-08’s pre-mutation refusal and strands the run on an unavailable driver.

---

### 3. High — handoff bypasses launch preflights, then mutates before they can refuse

**Location:** [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:167), [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:173), [preflight.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-cli/src/preflight.rs:1205)

**Defect:** The proposed precheck runs only `preflight_interactivity_check` and driver health. The real launch path also runs unattended-launch, major-bump, and GitHub-auth preflights. Those run only after the new driver has been written to state.

**Consequence & reproduction:** Hand off an Auto Code state when the unattended-launch prerequisites fail, or a Ship state with failed `gh auth`. The saved agent becomes Codex first; `launch_stage` then opens/refuses through a preflight gate. The plan’s “unsafe handoffs fail before state mutation” claim is therefore false outside its single Define fixture.

---

### 4. Medium — 44-01 specifies contradictory `resume` argument order

**Location:** [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:79), [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:147), [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:158)

**Defect:** The declared signature and dispatch are `(project_root, phase, agent, legacy_claude_launch)`, but the required existing-call-site rewrite is `resume(root, phase, false, None)`.

**Consequence & reproduction:** With the plan’s own signature, `false` is supplied where `Option<AgentKind>` is required and `None` where `bool` is required. The described implementation does not compile.

---

### 5. High — cron audit events can claim consumption when deletion failed

**Location:** [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:189), [44-02-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-02-PLAN.md:196)

**Defect:** Both deletion paths first test existence, call `delete_cron_instructions` fail-soft, and emit `cron_instructions_consumed` whenever the precheck was true. They emit even when deletion returned `Err` and the record remains.

**Consequence & reproduction:** Make `.devflow/` non-writable after a record is created. A resume/ship succeeds, removal fails, and the event log nevertheless states that the record was consumed. D-18’s audit trail becomes actively misleading, and a later scheduler invocation can still fire.

---

### 6. High — legacy cron handling is both uncompilable and semantically wrong

**Location:** [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:189), [44-02-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-02-PLAN.md:197), [ship.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-core/src/ship.rs:76), [ship.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-core/src/ship.rs:139)

**Defect:** `legacy_cron_instructions_path` is `pub(crate)` in `devflow-core`; `devflow-cli` cannot call it as 44-02 directs. Independently, 44-01 checks only the per-phase path before calling `delete_cron_instructions`, although that function also deletes a matching legacy record. Thus a legacy deletion produces no consumption event. A naïve visibility fix in 44-02 still emits a legacy event merely because the legacy path exists, even if it belongs to a different phase and is not deleted.

**Consequence & reproduction:** Create legacy instructions for phase 8, then ship phase 7. The proposed existence check reports legacy consumption for phase 7; `delete_cron_instructions(..., 7)` retains the phase-8 record. Conversely, a matching legacy record consumed via resume is deleted silently.

---

### 7. High — the proposed Hermes command does not compile and, if repaired, is split by the shell

**Location:** [44-03-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-03-PLAN.md:242), [44-03-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-03-PLAN.md:268), [ship.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-core/src/ship.rs:39)

**Defect:** `HermesCronJob` has `schedule`, `name`, `command`, and `once`; it has no `repeat` field, despite the plan rendering `instructions.hermes_cron.repeat`. The format string also inserts `command` unquoted. The stored command is `cd <quoted-project> && devflow resume --phase N`, so the outer shell treats `&&` as a command separator, not content of Hermes’s positional `prompt`.

**Consequence & reproduction:** The proposed code first fails to compile. Mapping `once` to `1` does not fix the shell break:

```text
hermes argv: <cron> <create> <ISO timestamp> <cd> <project>
devflow argv: <resume> <--phase> <N> <--repeat> <1> <--name> <job>
```

Hermes receives only `cd` as the prompt; the resume runs immediately in the operator shell, with Hermes options incorrectly passed to `devflow`.

---

### 8. High — malformed offset timestamps fail open, contrary to D-13

**Location:** [44-03-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-03-PLAN.md:133), [ship.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-core/src/ship.rs:300), [ship.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-core/src/ship.rs:321)

**Defect:** The plan says preserving `parse_retry_timestamp` preserves fail-closed behavior. It does not. `split_time_and_offset` uses `parse_offset_minutes(...).unwrap_or(0)`: a malformed offset is silently converted to UTC instead of making parsing fail.

**Consequence & reproduction:** `2026-06-18T15:45:30+25:00` is not a valid ISO offset, but it is parsed as 15:45:30 UTC and scheduled rather than yielding an empty schedule. The listed tests cover `"unknown"` and valid offsets only, so this violates D-13 without detection.

---

### 9. High — the dogfood evidence rules contradict the required artifacts and remain forgeable

**Location:** [44-04-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:40), [44-04-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:117), [44-04-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:138), [44-04-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:249)

**Defect:** P-01 forbids any hand-authored file under `44-evidence/`, yet Task 1 requires a prose `target-proposal.md` in that directory, and Task 2 requires manually recorded command/window/attempt metadata there. The only automated evidence check establishes that the directory contains at least one file; timestamps can be forged with copied output or `touch`.

**Consequence & reproduction:** An executor can satisfy the acceptance shape with synthetic JSON-looking content, a hand-authored proposal, and a current modification time. The human checkpoint confirms the operator’s statement, but the plan supplies no immutable link from the target run’s actual `.devflow` captures to the Phase 44 evidence record.

---

### 10. High — “start at a stage past Define/Plan” is not a supported launch path

**Location:** [44-04-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:135), [main.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-cli/src/main.rs:49), [commands.rs](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/crates/devflow-cli/src/commands.rs:213)

**Defect:** The plan permits a target to “start at a stage past” Define/Plan, but `devflow start` has no start-stage flag and initializes a fresh state with `State::new`, whose stage is Define. The plan provides neither a supported resume fixture nor a safe migration procedure for skipping stages.

**Consequence & reproduction:** A candidate without the required artifacts cannot follow the proposed alternative. An executor must either manually edit state, invalidate the “real phase” evidence, or silently run the forbidden headless stages.

---

### 11. Medium — multiple verification commands are invalid Cargo invocations; the zero-failure pipeline cannot pass

**Location:** [44-01-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:218), [44-02-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-02-PLAN.md:216), [44-03-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-03-PLAN.md:186), [44-04-PLAN.md](/var/home/denniyahh/Github/devflow/.worktrees/phase-44/.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:149)

**Defect:** `cargo test` accepts one optional `TESTNAME`; the plans pass multiple test names as positional arguments. Directly checked: Cargo rejects the second name as an unexpected argument. Separately, every `cargo test --workspace | rg -c '^test result: FAILED' | rg '^0$'` check is broken: with stdin, `rg -c` emits no `0` for zero matches, so the final `rg '^0$'` exits nonzero even for a clean test-result stream.

**Consequence & reproduction:** Several required verification gates cannot run or cannot pass. The plan needs one broad filter per command, separate commands, or a test-binary regex strategy, plus a real failure-count mechanism rather than the current `rg -c` pipeline.