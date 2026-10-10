## CONFIRMED

### 1. `merge_result` event does NOT carry the Ship merge target (Criterion 2 unobservable from events)
- **Claim:** CONTEXT D-23 (`.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-CONTEXT.md:98-99`) and §Integration Points (`:240`) assert:
  > "criterion 2's merge target is observed from the run's events (`merge_result`)."
- **Evidence:** `crates/devflow-core/src/hooks.rs:228-234` (and lines `201-203`, `213-215`):
  ```rust
  info!(
      "Merge: merged {branch} into {target}",
      target = ctx.git_flow.develop
  );
  crate::events::emit(
      &ctx.project_root,
      ctx.phase,
      "merge_result",
      serde_json::json!({"merged": true, "branch": branch}),
  );
  ```
  `target` is interpolated into the `tracing::info!` log line, but the durable event payload emitted to `.devflow/events/` carries **only** `{"merged": true, "branch": branch}`. There is no `target` or `base_branch` key anywhere in the event stream.
- **Why it matters:** An evidence-checker or plan asserting on `merge_result.target` will read absent/null. If parsed via `.get("target")`, it silently proxies as `None`.
- **Recommendation:** Either patch `crates/devflow-core/src/hooks.rs:232` to include `"target": ctx.git_flow.develop` in the `merge_result` JSON, or amend Criterion 2 verification to observe the merge target directly from git (`git merge-base` or `git rev-parse refs/heads/integration`) and GitHub PR API (`gh pr view --json baseRefName`).

---

### 2. `devflow start` hard-refuses if `base_branch` is not already a local branch
- **Claim:** D-06/D-24 specifies `base_branch = "integration"` on a fresh GitHub repo whose default branch is `main`.
- **Evidence:** `crates/devflow-cli/src/commands.rs:180-210`:
  ```rust
  let qualified = format!("refs/heads/{base}");
  let exists = git_command(project_root)
      .args(["show-ref", "--verify", "--quiet", qualified.as_str()])
      .output()
      .map(|out| out.status.success())
      .unwrap_or(false);
  if exists {
      return Ok(());
  }
  ...
  return Err(CliError::Message(format!(
      "configured base branch `{base}` is not a local branch in this repository. \
       DevFlow forks phase worktrees from it and merges phase work back into it... \
       Create it locally (e.g. `git branch {base} origin/{base}`) and re-run."
  )));
  ```
- **Why it matters:** `scripts/scratch-dogfood-repo.sh:195-196` creates only `main` and `develop`. If the extended script or runbook writes `base_branch = "integration"` to `devflow.toml` without explicitly running `git branch integration`, `devflow start` aborts before preflight.
- **Recommendation:** The scaffold script or runbook must explicitly run `git branch <base_branch>` (and `git push origin <base_branch>`) when provisioning the disposable project.

---

### 3. GSD plan resumption restarts at Task 1, not at the checkpoint
- **Claim:** D-27 (`49-CONTEXT.md:129-132`) asserts:
  > "The operator replies `stopped: <what happened>`; the checkpoint plan fails its gate and writes no SUMMARY; a plain `/gsd-execute-phase 49` re-presents the checkpoint for the next attempt (GSD resume, `execute-phase.md:1453-1457`)."
- **Evidence:** `_external/gsd-core/workflows/execute-phase.md:327`, `:631-649`, and `execute-plan.md:223-238`:
  `execute-phase.md` lines 327 filter plans: `Skip plans where has_summary: true`. For an incomplete plan (no SUMMARY), it dispatches an executor agent with the prompt:
  ```text
  Execute plan {plan_number} of phase {phase_number}-{phase_name}.
  ```
  The executor agent reads `execute-plan.md` and starts execution from **Task 1**. It does not jump to the checkpoint task. GSD tracks completed plans, not completed tasks within an incomplete plan.
- **Why it matters:** If Phase 49's checkpoint plan has tasks before the checkpoint (e.g., scaffolding, environment prep, binary builds), those tasks **re-run on every attempt**. If they are not idempotent, resume fails or corrupts state.
- **Recommendation:** The checkpoint plan in Phase 49 must be a dedicated single-task plan whose sole task is `type="checkpoint:human-action"`, or preceding setup tasks must live in an earlier completed plan that wrote a SUMMARY.

---

### 4. Pre-commit worktree guard blocks `/gsd-quick` fixes to `crates/*`
- **Claim:** D-28 (`49-CONTEXT.md:133-137`) specifies:
  > "The fix vehicle for a blocking DevFlow defect. `/gsd-quick` for a contained fix; an inserted decimal phase (`/gsd-phase --insert 49 …`) when the fix needs a design decision or crosses crates."
- **Evidence:** `scripts/lint-phase-worktree.sh:47-98`:
  ```bash
  phase="$(awk '/^current_phase:[[:space:]]*/ ...' "$state_file")"
  phase_branch="feature/phase-${phase}"
  ...
  while read -r line; do
      case "$line" in
          "branch refs/heads/${phase_branch}") worktree_exists=1; break ;;
      esac
  done < <(git worktree list --porcelain 2>/dev/null || true)

  [ "$worktree_exists" -eq 1 ] || exit 0
  ...
  [ "$current_branch" != "$phase_branch" ] || exit 0 # already in the right place
  ...
  if [ -n "$staged_source" ]; then
      echo "ERROR: source changes committed outside active phase worktree." >&2
      exit 1
  fi
  ```
- **Why it matters:** While Phase 49 is active and `.worktrees/phase-49` exists, `/gsd-quick` creates a detached branch (`quick/...`) off the default branch (`~/.claude/gsd-core/workflows/quick.md:238`). Any commit touching `crates/**` on that branch is **hard-refused by `pre-commit`** unless `DEVFLOW_ALLOW_OFF_WORKTREE=1` is exported.
- **Recommendation:** Explicitly document in D-28 that `/gsd-quick` requires `DEVFLOW_ALLOW_OFF_WORKTREE=1` in the operator's environment, or require fixes to be developed inside a dedicated worktree.

---

### 5. Contradiction: ROADMAP Criterion 2 requires `unattended_config_condition = Holds`, but D-03/D-22 ensures it `DoesNotHold`
- **Claim:** ROADMAP Phase 49 Criterion 2 (`.planning/ROADMAP.md:264-266`) requires:
  > "an observed fork point on the configured base, `unattended_config_condition = Holds`, unattended stage progression, and an observed Ship merge target."
- **Evidence:** `crates/devflow-cli/src/preflight.rs:1076-1080` and `49-CONTEXT.md:75-78`:
  ```rust
  if verify::phase_has_human_only_checkpoint(launch_root, state.phase) {
      return ConditionState::DoesNotHold(
          "a plan declares a `blocking-human` gate or a `human-action` checkpoint, which \
           no mode auto-approves (checkpoints.md rule 6)"
              .to_string(),
      );
  }
  ```
  49-CONTEXT line 78 explicitly states: *"No fully unattended run can reach DECN-03's path."*
  Because D-03 seeds a `blocking-human` checkpoint in the pre-authored plan, C3 evaluates to `DoesNotHold` at both Define and Code preflight.
- **Why it matters:** Criterion 2 as written is structurally unsatisfiable by the D-03 payload design. A verification plan asserting `unattended_config_condition = Holds` will fail.
- **Recommendation:** Amend ROADMAP Criterion 2 to distinguish C1/C2 (which hold) from C3 (which `DoesNotHold` by design), and explicitly state that unattended launch prerequisites refuse with human gates at Define and Code.

---

### 6. DECN-03 resumed session output cannot be captured during a gate park
- **Claim:** 49-CONTEXT §Claude's Discretion (`:173-174`) states:
  > "The gate watcher's snapshot at each gate open should capture everything DECN-03 needs; research confirms, because a capture that must happen while the run is parked is otherwise a timed runbook step."
- **Evidence:** `crates/devflow-cli/src/pipeline_launch.rs:1888-1890` and `:1236-1282`:
  ```rust
  } else if state.mode == Mode::Auto {
      return relaunch_checkpoint_session(&mut state, &session_id);
  }
  ```
  When the agent hits the `blocking-human` checkpoint, DevFlow detects that the checkpoint was already approved at Code preflight (`unapproved.is_empty()`). In `Mode::Auto`, it **does not open a gate**. It calls `relaunch_checkpoint_session` immediately.
  Once the resumed turn finishes, Code transitions to Validate, Validate passes to Ship, and Ship auto-approves via `--yes-ship`. **No gate opens after Code preflight.**
- **Why it matters:** A gate watcher that takes snapshots only when gates open will take snapshots at Define preflight and Code preflight, but will capture **nothing** from the resumed session or the DECN-03 resolution turn.
- **Recommendation:** Do not tie DECN-03 capture to gate-open events. The evidence collector must harvest `.devflow/history/phase-01/*/stdout` and `.devflow/phase-01-stdout` after stage completion.

---

### 7. D-14 negative control refusal text has missing backticks
- **Claim:** D-14 (`49-CONTEXT.md:145-146`) states:
  > "asserts on the refusal text ("no `.planning/config.json` under the launch root")"
- **Evidence:** `crates/devflow-cli/src/preflight.rs:993-997`:
  ```rust
  Err(GsdConfigError::Missing(_)) => ConditionState::DoesNotHold(
      "no `.planning/config.json` under the launch root — nowhere for the chain flag \
       to live"
          .to_string(),
  ),
  ```
- **Why it matters:** The source string has literal markdown backticks around `` `.planning/config.json` ``. In `49-CONTEXT.md`, the backticks are inside markdown prose quotes, which renders ambiguously. If an assertion in a test script greps for `"no .planning/config.json under the launch root"` without backticks, it will fail to match.
- **Recommendation:** Pin the exact grep pattern to ``no `\.planning/config\.json` under the launch root`` in the plan.

---

### 8. Hardcoded Cargo assumption in Validate-to-Ship transition
- **Claim:** D-20 (`49-CONTEXT.md:69-70`) proposes using "the smallest toolchain already on the host" for the disposable project.
- **Evidence:** `crates/devflow-core/src/hooks.rs:100-104`, `:237-257`:
  ```rust
  pub fn hooks_for_transition(from: Stage, to: Stage) -> Vec<Hook> {
      match (from, to) {
          (Stage::Validate, Stage::Ship) => vec![Hook::DocsUpdate],
          _ => Vec::new(),
      }
  }

  fn docs_update(ctx: &HookContext) -> Result<(), HookError> {
      let output = Command::new("sh")
          .arg("-c")
          .arg("cargo doc --no-deps 2>&1")
          .current_dir(&ctx.project_root)
          .output();
  ```
- **Why it matters:** On any non-Rust project, `cargo doc` fails with exit code 101. While `docs_update` fails soft (`warn!("DocsUpdate: cargo doc reported a failure...")`) and returns `Ok(())`, it pollutes logs with warnings. Additionally, GSD's `/gsd-validate-phase` scans for pytest/jest/vitest test files (`~/.claude/gsd-core/workflows/validate-phase.md:68-70`). If none exist, it treats requirements as MISSING and triggers interactive prompts (`AskUserQuestion`), breaking headless runs.
- **Recommendation:** The synthetic project should either be a minimal Cargo package (e.g. `cargo init --lib`) with one passing test, or supply a pre-authored `01-VALIDATION.md` stamped `status: validated` so Nyquist audit skips gap prompts.

---

## SUSPECTED

### 1. `capture_retention = 5` may rotate out the DECN-03 resumed session capture
- **Checked:** `crates/devflow-core/src/config.rs:146` sets `DEFAULT_CAPTURE_RETENTION = 5`.
  `crates/devflow-core/src/agent_result.rs:3168-3183` prunes `.devflow/history/` to `retain` generations.
  In Phase 23a (`.planning/milestones/v2.0.0-phases/23-end-to-end-dogfood/23-PROBE-FINDINGS.md:177-193`), the critical Ship capture was rotated out and permanently lost due to 5 subsequent loop-back cycles.
- **Remains unverified:** In a nominal Phase 49 run without loop-backs:
  1. Define (1 archive)
  2. Plan (1 archive)
  3. Code initial turn (1 archive)
  4. Code resume turn (1 archive)
  5. Validate (1 archive)
  Total is exactly 5. If Validate loops back even once, or if an extra preflight evaluation archives, the Code initial or resume capture will be deleted before the run finishes.
- **Recommendation:** Set `capture_retention = 20` in the disposable project's `devflow.toml`.

### 2. Inserting a decimal phase mid-Phase 49 alters GSD roadmap state irreversibly
- **Checked:** `~/.claude/gsd-core/workflows/insert-phase.md:77` runs:
  `gsd_run query state.patch '{"Current Phase":"49.1",...}'`
  If a blocking bug is encountered during Phase 49 and the operator runs `/gsd-phase --insert 49 ...`, GSD advances `Current Phase` to `49.1`.
- **Remains unverified:** When Phase 49.1 completes, GSD's `complete-milestone` or `next` workflow normally advances to the next phase on the roadmap (Phase 50). GSD has no native concept of "return to the parent phase whose plan was unfinished".
- **Recommendation:** Prefer `/gsd-quick` for defect fixes over decimal phase insertion to prevent corrupting GSD's active phase pointer.

---

## CHECKED AND FINE

- **`preflight.rs:1076-1080` (C3 condition):** Verified at `crates/devflow-cli/src/preflight.rs:1076-1080`.
  ```rust
  if verify::phase_has_human_only_checkpoint(launch_root, state.phase) {
      return ConditionState::DoesNotHold(
  ```
- **`preflight.rs:971-972` (Applicable stages for unattended check):** Verified at `crates/devflow-cli/src/preflight.rs:971-972`.
  ```rust
  fn unattended_launch_check_applies(stage: Stage) -> bool {
      matches!(stage, Stage::Define | Stage::Code)
  }
  ```
- **`preflight.rs:1432-1434` (Recording checkpoint set at Code preflight approval):** Verified at `crates/devflow-cli/src/preflight.rs:1432-1434`.
  ```rust
  if stage == Stage::Code {
      record_checkpoint_set_for_code_evaluation(state)?;
  }
  ```
- **`prompt.rs:374-380` (Define is a no-op prompt):** Verified at `crates/devflow-core/src/prompt.rs:373-385`. `define_stage_prompt` explicitly forbids interactive commands and planning changes.
- **`prompt.rs:341` (Plan idempotency):** Verified at `crates/devflow-core/src/prompt.rs:340-356`. If `PLAN.md` exists, Plan returns `DEVFLOW_RESULT: {"status": "success"}` without running GSD commands.
- **`preflight.rs:724-760` (MAJOR-bump gate cannot be auto-approved):** Verified at `crates/devflow-cli/src/preflight.rs:741-744, 757-760`. Takes no `yes_ship` handling.
- **`preflight.rs:1068-1072` (No plans at Define is deferred):** Verified at `crates/devflow-cli/src/preflight.rs:1068-1072`. Returns `ConditionState::NotYetApplicable` at Define.
- **`pipeline_outcomes.rs:839` (`--yes-ship` auto-approval):** Verified at `crates/devflow-cli/src/pipeline_outcomes.rs:839-843`. Sets `GateResponse` with `"pre-authorized by --yes-ship"`.
- **`config.rs:351` (`validate_base_branch` refuses `main`):** Verified at `crates/devflow-core/src/config.rs:351-356`.
- **`pipeline_launch.rs:1777` (DECN-03 applies to Claude driver only):** Verified at `crates/devflow-cli/src/pipeline_launch.rs:1776-1778`. Checks `state.agent == AgentKind::Claude`.
- **`gates.rs:388-394` (`DEVFLOW_GATE_NOTIFY_CMD` no-op unless set):** Verified at `crates/devflow-core/src/gates.rs:388-394`.
- **`scripts/scratch-dogfood-repo.sh:187-191`:** Verified writes `.planning/config.json`.
- **Bump classification in fresh repository:** Verified in `crates/devflow-core/src/version.rs:385-444`. In a repository with no prior tags and non-breaking commits, `version::classify_range_bump` yields `Bump::Minor` (if `feat`) or `Bump::Patch`, never `Bump::Major`. Preflight major bump check (`preflight.rs:818-820`) passes without opening a gate.
