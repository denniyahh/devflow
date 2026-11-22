## CONFIRMED

### 1. [critical] `53.1-02-PLAN.md:114` — `a_far_rate_limit_gate_routes_to_another_agent_by_stop_then_resume` is guaranteed to fail preflight, and exposes a broken `--mode auto` route

- **Quoted text** ([53.1-02-PLAN.md:114](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-02-PLAN.md#L114)):
  > `Rig::with_reset(PhaseId::new(5134), Some(now_epoch() + 2 * 86_400))` ... `rig.start(&[])` ... Then `devflow resume --phase 5134 --agent codex <root>` with the stub PATH ... succeeds; within `RUN_WAIT` an `agent_handoff` event reads `from_agent` "claude", `to_agent` "codex", `reason` "resume --agent"

- **Why it fails:**
  1. `Rig::with_reset` ([failover_e2e.rs:338-340](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/tests/failover_e2e.rs#L338-L340)) constructs a `CLAUDE_AUTO` rig (`mode: "auto"`, [failover_e2e.rs:277-282](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/tests/failover_e2e.rs#L277-L282)). The file header itself warns ([failover_e2e.rs:9-11](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/tests/failover_e2e.rs#L9-L11)): *"The handoff runs use `--mode supervise`, because in `--mode auto` a fallback must hold the unattended launch shape, which Codex does not."*
  2. `devflow resume --phase 5134 --agent codex` enters `resume_loaded` ([pipeline_launch.rs:2494-2511](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L2494-L2511)) → `hand_off_agent` ([pipeline_launch.rs:1968-1972](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L1968-L1972)) → `generic_preflight_checks` ([preflight.rs:1183-1206](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/preflight.rs#L1183-L1206)) → `unattended_launch_shape_condition` ([preflight.rs:891-916](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/preflight.rs#L891-L916)). Because `state.mode == Mode::Auto` and `candidate.agent == AgentKind::Codex` (not `Claude` or `Antigravity`), preflight deterministically refuses: `handoff to codex refused at saved code stage: unattended launch refused (D-07)`.
  3. Because [53.1-02-PLAN.md:123](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-02-PLAN.md#L123) instructs the executor to **stop and file a 999.x defect** if the stop/resume half fails, executing this plan as written will halt Wave 1.
  4. This also exposes a real operator-facing edge case in the gate text: in `--mode auto`, `devflow resume` has no `--mode` flag, so `devflow resume --phase N --agent codex` (or `pi`) fails on an auto-mode run; only `antigravity` is admitted.

- **Fix:** Give `failover_e2e.rs` a supervised reset constructor (e.g. `CLAUDE_SUPERVISE`, matching `CLAUDE_TO_CODEX` at [failover_e2e.rs:285-290](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/tests/failover_e2e.rs#L285-L290)) for `a_far_rate_limit_gate_routes_to_another_agent_by_stop_then_resume`. In `OPERATIONS.md` (same task), explicitly note that on a `--mode auto` run, `resume --agent` only accepts an unattended-capable agent (`claude` or `antigravity`).

---

### 2. [critical] `53.1-03-PLAN.md:81` — `resume_loaded` runs `hand_off_agent` BEFORE advancing `state.stage`, breaking `resume --agent <other>` after `--until plan`

- **Quoted text** ([53.1-03-PLAN.md:81](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-03-PLAN.md#L81)):
  > In pipeline_launch.rs `resume_loaded`, before the commit record whether `crate::pipeline_gate::until_cap_fired(&state)` holds and the halted stage; inside the commit, where it already clears `stop_until` for a fired cap, also call `crate::pipeline_gate::enter_stage(s, s.stage, next)` when `s.stage.next()` is `Some(next)` ... after the commit, when it advanced, emit the `transition` event

- **Why it fails:**
  In `resume_loaded` ([pipeline_launch.rs:2490-2563](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L2490-L2563)), `refuse_recorded_canary_for_resume` (`:2490`) and `hand_off_agent` (`:2499`) execute **before** line `2547`'s `commit`, while `state.stage` is still `Stage::Plan`! When an operator runs `devflow resume --phase N --agent <other>` after `--until plan`:
  1. `hand_off_agent` ([pipeline_launch.rs:1968-1972](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L1968-L1972)) runs `generic_preflight_checks` against `candidate.stage == Stage::Plan` instead of `Stage::Code`:
     - `preflight_interactivity_check` ([preflight.rs:740-751](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/preflight.rs#L740-L751)) checks for `-CONTEXT.md` on base (a Plan-only check) and can falsely refuse a handoff for Code.
     - `unattended_planned_checkpoint_condition` ([preflight.rs:1001-1009](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/preflight.rs#L1001-L1009)) sees `Stage::Plan` and returns `NotYetApplicable("re-evaluated at Code")` instead of validating that plans exist for Code.
     - Any preflight refusal prints `"handoff to {requested} refused at saved plan stage"` ([pipeline_launch.rs:2508](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L2508)).
  2. `hand_off_agent` ([pipeline_launch.rs:2012](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L2012)) emits `agent_handoff` with `"stage": "plan"` *before* `transition` `plan -> code` is emitted.
  3. On a plain resume with a recorded failing canary, `refuse_recorded_canary_for_resume` ([pipeline_launch.rs:2591-2597](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L2591-L2597)) misreports the refusal as `"at the plan stage"`.

- **Fix:** Compute the effective resume target stage (`let target_stage = if until_cap_fired(&state) { state.stage.next().unwrap_or(state.stage) } else { state.stage };`) at the top of `resume_loaded`. Evaluate `refuse_recorded_canary_for_resume` and `hand_off_agent`'s preflight candidate against `target_stage` (without mutating disk state on refusal), emit `transition` before `agent_handoff` (with `"stage": target_stage`), and add a test covering `resume --agent <other>` after an `--until plan` halt.

---

### 3. [major] `53.1-05-PLAN.md:94, 99` — `<verify>` gate `shellcheck scripts/hooks/post-commit` fails on 3 pre-existing SC2086 warnings the plan forbids touching

- **Quoted text** ([53.1-05-PLAN.md:94, 99](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-05-PLAN.md#L94-L99)):
  > in scripts/hooks/post-commit write the marker to `"$(git rev-parse --path-format=absolute --git-path devflow-stale-state.json)"` (no `mkdir` needed; the git dir exists) instead of `$root/.planning/user/errors/state-stale.json`, keep the JSON body as it is, ...
  > `<automated>... && bash -n scripts/hooks/post-commit && shellcheck scripts/hooks/post-commit && ...</automated>`

- **Why it fails:**
  Running `shellcheck scripts/hooks/post-commit` on the current snapshot exits **1** with three pre-existing SC2086 findings:
  - [scripts/hooks/post-commit:47](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/scripts/hooks/post-commit#L47): `printf '  %s\n' $summaries`
  - [scripts/hooks/post-commit:69](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/scripts/hooks/post-commit#L69): `printf '"%s",' $summaries | sed 's/,$//'` (inside the JSON body!)
  - [scripts/hooks/post-commit:113](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/scripts/hooks/post-commit#L113): `printf '  %s\n' $setup_changed`

  Following the plan's instruction to *"keep the JSON body as it is"* guarantees the `<verify>` gate fails. Worse, naively applying shellcheck's quick-fix (`"$summaries"`) breaks multi-summary commits (a wave-closing commit produces newline-separated paths in `$summaries`, turning line 69 into a single invalid JSON string containing raw newlines).

- **Fix:** Explicitly include fixing lines 47, 69, and 113 in Task 1 using `mapfile -t` arrays (`printf '  %s\n' "${summary_arr[@]}"`, etc., which is valid in `#!/usr/bin/env bash`), and add a two-summary commit assertion in `post_commit_marker.rs` parsing the marker as valid JSON (`serde_json::from_str`) so multi-summary splitting is verified.

---

### 4. [major] `53.1-05-PLAN.md:105, 192` — Acceptance criterion `scripts/check-workspace-divergence.sh` exits 0 on `HEAD` is impossible on `feature/phase-53.1`

- **Quoted text** ([53.1-05-PLAN.md:105](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-05-PLAN.md#L105) and [53.1-05-PLAN.md:192](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-05-PLAN.md#L192)):
  > `scripts/check-workspace-divergence.sh` exits 0 on this tree (the new test file is declared).

- **Why it fails:**
  With no arguments, [scripts/check-workspace-divergence.sh:26-27, 87-106](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/scripts/check-workspace-divergence.sh#L87-L106) diffs `origin/develop` against `HEAD` and exits **1** for *any* path outside `FORBIDDEN_PATHS` that is not declared `only` or `modified` in `.workspace-divergence`.
  On `feature/phase-53.1`:
  - Plan 53.1-05's own Tasks 2 and 3 add `crates/devflow-cli/tests/live_payload_fixture.rs` and modify `scripts/scratch-dogfood-repo.sh` and `crates/devflow-cli/tests/ask_e2e.rs` — develop-bound changes that must *not* be declared in `.workspace-divergence`.
  - Plans 53.1-01..04 run in parallel in Wave 1 and modify `crates/` and `OPERATIONS.md` on the same branch.
  Running `scripts/check-workspace-divergence.sh` at `<verification>` (after Tasks 1–3) — and even after Task 1 if any other phase commit is in `origin/develop..HEAD` — will unconditionally fail with `undeclared change to a develop file` / `undeclared workspace-only path`.

- **Fix:** Remove `scripts/check-workspace-divergence.sh` against `HEAD` from Plan 53.1-05's acceptance criteria and `<verification>`. Instead, assert the exact `only crates/devflow-cli/tests/post_commit_marker.rs` entry in `.workspace-divergence` (e.g., via `rg -qx 'only crates/devflow-cli/tests/post_commit_marker\.rs' .workspace-divergence` plus `pre_push_divergence_e2e`, which already validates the parser and rules).

---

### 5. [major] `53.1-02-PLAN.md:112, 119` — Far-reset gate text overflows the 411-char display cut on `Stage::Validate`

- **Quoted text** ([53.1-02-PLAN.md:119](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-02-PLAN.md#L119)):
  > render the far layout: `"no usable fallback; the limit resets at {utc}, {distance}; for another agent now: \`devflow stop --phase {phase}\`, then \`devflow resume --phase {phase} --agent {other}\`; once this waiting supervisor has ended, \`devflow resume --phase {phase}{agent}\` relaunches the stage on the same agent. Cause: "` (measured at plan time with Task 2's approve phrase: the clause through `--agent {other}`; ends at character 407 of the 411 that `render_gate_context` keeps before its 39-character suffix at the 450 cut, for a 7-character phase id and `antigravity` as `{other}`; do not lengthen it)

- **Why it fails:**
  The plan only measured `Stage::Code` (4 chars). A rate limit at `Stage::Validate` also opens `GateKind::RateLimit` (failover is Code-only, [OPERATIONS.md:477](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/OPERATIONS.md#L477)). Because `{stage}` appears **three times** before `--agent {other}` (twice in `way_on_sentence`, once in `stage {stage} failed: `), `validate` (8 chars) adds **+12 characters**:
  - `validate`, `phase = 53.1`, `other = antigravity`: ends at **413 > 411** (cuts `--agent antigravity`).
  - `validate`, `phase = 999.250`, `other = <other>`: ends at **415 > 411** (cuts `--agent <other>`).
  - `validate`, `phase = 999.250`, `other = antigravity`: ends at **419 > 411**.
  - Task 2's test ([53.1-02-PLAN.md:112](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-02-PLAN.md#L112)) only exercises `Stage::Code`, so it misses the worst-case stage completely (unlike `commands.rs:8468`, which tests `Stage::Validate` for display budgets).

- **Fix:** Shorten the far-reset lead by ≥15 chars (e.g., `"resets {utc} ({distance}); another agent: \`devflow stop --phase {phase}\`, then \`devflow resume --phase {phase} --agent {other}\`; ..."` — saves 34 chars, bringing worst-case `validate` + `999.250` + `antigravity` to ~385 ≤ 411), and assert the 450-char display cut on `Stage::Validate` with a 7-char phase id and `antigravity` in `a_rate_limit_gate_two_days_out_leads_with_another_agent`.

---

### 6. [major] `53.1-06-PLAN.md:48, 78, 139` — Contradicts locked decision D-05 (and weakens D-06), introducing an ambiguous `--note` instruction that matches `gate approve --note`

- **Quoted text:**
  - Locked decision **D-05** ([53.1-CONTEXT.md:55-56](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-CONTEXT.md#L55-L56)):
    > A stage-failure gate whose reason asks for operator decisions says to answer it with `gate reject --note "<decisions>"`, and that the note reaches the fix pass (999.189 is fixed). No new gate kind.
  - Plan ([53.1-06-PLAN.md:78](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-06-PLAN.md#L78)):
    > `" If the agent asks for operator decisions, write them in the note (\`--note \"<decisions>\"\`): the note reaches the relaunched {stage} agent's prompt."`
  - Plan ([53.1-06-PLAN.md:139](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-06-PLAN.md#L139)):
    > `then write the sign-offs done and their results in the note: \`/gsd-quick\` records them and Validate runs again.`

- **Why it fails:**
  1. [53.1-06-PLAN.md:48](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-06-PLAN.md#L48) drops `gate reject` to stay under C2's `ANSWER_WORDS` scan ([pipeline_outcomes.rs:8110-8135](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_outcomes.rs#L8110-L8135)) and labels this *"Claude's Discretion: exact gate wording"*. It is not wording discretion: the entire defect in [ROADMAP.md:996-998](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/ROADMAP.md#L996-L998) and D-05 is that the operator does not expect **`reject`** to be the verb that passes decisions.
  2. `devflow gate approve` **also accepts `--note`** ([main.rs:276-277](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/main.rs#L276-L277)). An operator following *"write them in the note (`--note "<decisions>"`)"* who runs `devflow gate approve N --note "<decisions>"` on a `stage_failure` gate hits `StatusLessStep::RetryStage` ([pipeline_outcomes.rs:708](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/pipeline_outcomes.rs#L708)), which **silently discards the note**. On a `human_needed` gate, `devflow gate approve N --note "<sign-offs>"` likewise ignores the note and reopens the gate.
  3. On `stage_failure`, appending the sentence *after* up to 4,096 characters of failure reason means the notify hook, status banner, and `devflow gate list` (450-char cut) truncate it away whenever the reason exceeds ~240 chars.

- **Fix:** Surface the C2 (`ANSWER_WORDS`) vs D-05/D-06 conflict explicitly rather than silently dropping `gate reject`. Two clean options: (a) embed the `reject with a note` clarification directly inside `way_on_sentence` (where `reject` already lives, keeping C2 intact — for `StageFailure`, tighten or adjust the idle-timeout budget if it exceeds the 23-char slack; for `HumanNeeded`, `way_on_sentence` is not on the idle-timeout path so there is no slack constraint), or (b) have C2's test permit the exact documented `devflow gate reject ... --note` command only when it matches the gate's own `reject_route_by_kind` (preserving C2's invariant that a gate never names an unsupported route).

---

### 7. [major] `53.1-01-PLAN.md:66-67` — `999.201` test lacks an `allowed` `rate_limit_event` row, so a partial fix passes the test and fails on real Claude streams

- **Quoted text** ([53.1-01-PLAN.md:66-67](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-01-PLAN.md#L66-L67)):
  > a table over `{single document = the result line alone, stream = V3_INIT_EVENT then the same line}` x `{subtype error_rate_limit, subtype error_max_turns}` ... Same test, provenance row (T-30-12): a stream of V3_INIT_EVENT, an `error_rate_limit` result line that carries `"parent_tool_use_id":"toolu_1"`, then a top-level `{"type":"result","subtype":"success","is_error":false,"result":"done"}` is not `RateLimited`.

- **Why it fails:**
  1. As `agent_result.rs` itself documents ([agent_result.rs:1366-1369](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-core/src/agent_result.rs#L1366-L1369)), real Claude streams routinely emit top-level `rate_limit_event` lines with `rate_limit_info.status: "allowed"` on healthy turns.
  2. In `detect_claude_stream_rate_limit` ([agent_result.rs:1435-1444](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-core/src/agent_result.rs#L1435-L1444)), there are **two** early exits before reaching a `result` check: `?` on `.find(...)` (`:1438`), and `if !CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES.contains(&status) { return None; }` (`:1442-1444`).
  3. Because none of the plan's stream test rows includes a routine `status: "allowed"` `rate_limit_event` before the `error_rate_limit` result, an implementation that replaces only the `?` on line 1438 (leaving line 1443's `return None` intact) passes every test in Plan 53.1-01 yet returns `Failed` on real Claude streams that emit `status: "allowed"` earlier in the turn and end with `subtype: "error_rate_limit"`.
  4. Additionally, no test row exercises the `"usage limit"` fallback when `retry_after` is absent ([53.1-01-PLAN.md:72](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-01-PLAN.md#L72)).

- **Fix:** Add two stream rows to `an_error_rate_limit_result_is_rate_limited_on_both_paths`: (1) `V3_INIT_EVENT` + a top-level `rate_limit_event` with `status: "allowed"` + `error_rate_limit` result → `RateLimited`; (2) `error_rate_limit` without `retry_after` → `RateLimited` with reason `"rate limited until usage limit"`.

---

### 8. [major] `53.1-03-PLAN.md:106` — `a_cancel_for_a_finished_id_records_nothing` never finishes a call, so breaking the `in_flight` clear passes the test

- **Quoted text** ([53.1-03-PLAN.md:106](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-03-PLAN.md#L106)):
  > `ask_server::tests::a_cancel_for_a_finished_id_records_nothing`: on `CallFlags::default()`, `cancel(&flags, &json!(7))` for an id never queued and not in flight leaves `flags.cancelled` empty ... Controls in the same test: a queued id (pushed as a `PendingCall`) is removed from the queue by its cancel and not recorded; an id taken by `next_call` (so in flight) is recorded by its cancel.

- **Why it fails:**
  Despite its name, the test never runs a call to completion through `run_worker` ([ask_server.rs:312-326](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/ask_server.rs#L312-L326)) and never exercises clearing `flags.in_flight` after `worker.call` returns. If an implementer omits or breaks the worker-loop clear (`in_flight` stays `Some("7")` forever after call `7` finishes), `a_cancel_for_a_finished_id_records_nothing` still passes 100%, while a real late cancel for call `7` still poisons a reused id `7`!

- **Fix:** Either add a `run_server` test (using `ping_is_answered_while_a_call_is_held`'s pipe pattern at [ask_server.rs:1390-1425](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-cli/src/ask_server.rs#L1390-L1425)) that completes a call with id `7`, sends `notifications/cancelled` for id `7`, then sends a second call with id `7` and asserts it receives a reply (and isn't seen as cancelled), or extract the worker's completion helper and assert `cancel` after it records nothing.

---

### 9. [major] `53.1-07-PLAN.md:90, 92, 122` — `commit_files` failure path is untested/ambiguous and omits `--json-errors`; event path redaction leaks paths on symlinked `$HOME` and `LauncherNotFound`

- **Quoted text** ([53.1-07-PLAN.md:90, 92, 122](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-07-PLAN.md#L90-L122)):
  > `pub fn commit_files(...)` with argv `query`, `commit`, the message, `--files`, then each file, refusing an `error` key and a reply whose `committed` is not a JSON boolean ...
  > emit `gsd_phase_complete` with `{"ok": true, "committed": <bool>, "commit_failure": <label or null>}` ...
  > bound the message with `render_gate_context(.., GATE_CONTEXT_DISPLAY_CHARS)` after replacing every occurrence of the evidence root's display text with `.` (property 3 of `gsd_reader`: no absolute path in an event)

- **Why it fails:**
  1. **Untested & contradictory `commit_files` error path:** In gsd-core 1.22.0 (`cmdCommit`, `~/.claude/gsd-core/bin/lib/commands.cjs:2261-2270`), a staging failure exits **0** with `{"committed": false, "reason": "staging_failed", "error": "..."}`, which `commit_files`'s `refuse_gsd_error` maps to `Err(ReaderFailure::GsdError)`. And `commitRefusal` (`commands.cjs:1901-1906`) exits **1** via `error()`, which only emits structured JSON if `--json-errors` is passed — yet `commit_files` omits `--json-errors`. Neither Task 1 nor Task 2 specifies a test for `commit_files` returning `Err` or `{"committed": false}` (e.g., `nothing_to_commit` / `skipped: true`), nor does Task 2 specify whether a `commit_files` error prints to stderr and still emits `ok: true, committed: false, commit_failure: <label>` instead of falling into the outer `Err` arm (which falsely prints `"GSD did not mark phase N complete"` after GSD already mutated `ROADMAP.md`/`STATE.md` on disk).
  2. **Path redaction bypass (`T-53.1-14`):** Replacing only `evidence_root.display()` fails in two real cases:
     - On Fedora Kinoite (this host), `/home` is a symlink to `/var/home`. `evidence_root` may hold `/home/<user>/...` while Node/GSD resolves `fs.realpathSync` to `/var/home/<user>/...` (or vice versa — the plan itself notes canonicalized paths differ at [53.1-07-PLAN.md:84](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-07-PLAN.md#L84)). Exact string replacement of `evidence_root.display()` leaves `/var/home/<user>/...` intact in `events.jsonl`.
     - For `ReaderFailure::LauncherNotFound { home }` ([gsd_reader.rs:105](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-core/src/gsd_reader.rs#L105)), the raw message embeds `$HOME`, not `evidence_root`, leaking `$HOME` straight into `events.jsonl`.

- **Fix:**
  1. Pass `--json-errors` in `commit_files` as well; explicitly handle `commit_files`'s `Err(err)` inside `phase_complete`'s `Ok` arm (printing stderr warning that GSD marked the phase complete but commit failed, and emitting `{"ok": true, "committed": false, "commit_failure": err.label()}`), and add a test for `phase.complete` succeeding + `commit_files` failing (and one for `committed: false`).
  2. Replace both `evidence_root.display()` and `evidence_root.canonicalize()` (plus `home.display()` / `home.canonicalize()` when available) before bounding the event message, and do not emit raw `failure.to_string()` for `LauncherNotFound` (which already has the path-free `failure.label()`).

---

### 10. [minor] `53.1-VALIDATION.md:43-62` — Validation map is stale against the 7 plans (`TBD` bindings, wrong waves, missing tests)

- **Quoted text** ([53.1-VALIDATION.md:43-61](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-VALIDATION.md#L43-L61)):
  > `| TBD | TBD | 2 | 999.213 | ...` / `| TBD | TBD | 2 | 999.258 | ...` / `| TBD | TBD | 2 | 999.251 | ...` / `| TBD | TBD | 2 | 999.259 | ...`

- **Why it fails:**
  Every row still reads `Task ID = TBD`, `Plan = TBD`; wave numbers disagree with the plans (`999.213`/`999.258`/`999.251` are Wave 1 in `53.1-03`/`53.1-04`; `999.249`/`999.206`/`999.254` are Wave 2 in `53.1-06`; `999.259` is Wave 3 in `53.1-07`); and six tests added by the plans (`a_rate_limit_gate_shows_its_reset_as_a_utc_time_and_a_distance`, `a_far_rate_limit_gate_routes_to_another_agent_by_stop_then_resume`, `cleanup_with_nothing_to_remove_says_there_was_nothing_to_clean`, `gsd_written_files_names_every_file_gsd_wrote`, `phase_complete_runs_gsd_s_writer_by_argv`, `a_commit_reply_without_committed_is_a_failure`) are absent.

- **Fix:** Bind every row's Task ID, Plan (`53.1-01`..`53.1-07`), and Wave (1–3) to match the plans, and add the six missing tests.

---

## SUSPECTED

### 1. [major] (speculation — not executed) `53.1-07-PLAN.md:90, 92` — `gsd_run query commit` under `gsd_reader::reader_command` runs git commit hooks and signing inside a 60 s timeout with `stdin(Stdio::null())`

- **Quoted text** ([53.1-07-PLAN.md:90, 92](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/.planning/phases/53.1-phase-53-remediation/53.1-07-PLAN.md#L90-L92)):
  > `pub fn commit_files(...)` with argv `query`, `commit`, the message, `--files`, then each file ... through `read_json` and `GSD_READER_TIMEOUT`

- **Why it may fail (untested live):**
  `gsd_reader::reader_command` ([gsd_reader.rs:258-276](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/rev531/snapshot/crates/devflow-core/src/gsd_reader.rs#L258-L276)) sets `stdin(Stdio::null())` and bounds the subprocess to `GSD_READER_TIMEOUT` (60 s), while running inside the detached runner unit where no TTY exists. Unlike the read-only GSD queries (`verification status`, `frontmatter get`, `runtime-identity`), `query commit` invokes `git commit`, which triggers:
  - repo/global pre-commit hooks (e.g. `gitleaks`, `lint-plan-bashisms.sh`), and
  - GPG/SSH commit signing if `commit.gpgsign = true` is configured in the user's git config (note `git.rs:233-237` specifically had to pass `-c tag.gpgSign=false` on `tag()` because signing hung headless runs!).
  If a signing agent prompts or a pre-commit hook blocks/fails under `Stdio::null()`, `commit_files` will either fail or stall `finish_run` for 60 seconds before `notify_run_end` runs — leaving GSD's `ROADMAP.md`/`STATE.md` edits uncommitted in the worktree.

- **Fix:** Verify once live (or against a scratch repo with `pre-commit` + signing configured) during Phase 53.1 close-out, and ensure Finding 9's `commit_files` error-handling path records the failure cleanly without misreporting `phase.complete` as failed or blocking `notify_run_end` unexpectedly.
