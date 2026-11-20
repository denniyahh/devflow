# Phase 53 Proof Run (53-VALIDATION.md M1–M4)

Operator option 1 (2026-11-19): the phase head is installed to a separate root, not over the installed
`devflow`, because the phase 53 runner is live at its Validate gate (999.254). Every proof command runs that binary
by putting its directory first on PATH.

## M1 — installed head

installed-head: d74a788543289704cc512e701f6839ea3aa0478a
install root: `/tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/df53` (`cargo install --path crates/devflow-cli --locked --force --root <that>`, exit 0, run
before this file existed)
`devflow --version` (that binary): `devflow 4.0.0-alpha.1 (d74a788543289704cc512e701f6839ea3aa0478a, clean)` — equals the expected string.
`claude --version`: 2.1.328 (Claude Code)
`codex --version`: codex-cli 0.170.0
codex-config-before: d7a527ad0a960a6f491202107f24a751292a2ebd3391e48737731a17bcede634

semver-checks (`cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor`):
exit 100, "semver requires new major version: 8 major and 0 minor checks failed". Lints failed: enum_missing, enum_variant_added, enum_variant_missing, function_missing, inherent_method_missing, pub_module_level_const_missing, struct_missing, struct_pub_field_missing.
Full output:

```text
Cloning origin/develop
    Building devflow-core v4.0.0-alpha.1 (current)
       Built [   1.172s] (current)
     Parsing devflow-core v4.0.0-alpha.1 (current)
      Parsed [   0.025s] (current)
    Building devflow-core v4.0.0-alpha.1 (baseline)
       Built [   1.215s] (baseline)
     Parsing devflow-core v4.0.0-alpha.1 (baseline)
      Parsed [   0.023s] (baseline)
    Checking devflow-core v4.0.0-alpha.1 -> v4.0.0-alpha.1 (assume minor change)
     Checked [   0.083s] 202 checks: 194 pass, 8 fail, 0 warn, 58 skip

--- failure enum_missing: pub enum removed or renamed ---

Description:
A publicly-visible enum cannot be imported by its prior path. A `pub use` may have been removed, or the enum itself may have been renamed or removed entirely.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/enum_missing.ron

Failed in:
  enum devflow_core::state::CheckpointApproval, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/state.rs:460

--- failure enum_variant_added: enum variant added on exhaustive enum ---

Description:
A publicly-visible enum without #[non_exhaustive] has a new variant.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#enum-variant-new
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/enum_variant_added.ron

Failed in:
  variant GateKind:Question in /var/home/denniyahh/Github/devflow/.worktrees/phase-53/crates/devflow-core/src/gates.rs:162

--- failure enum_variant_missing: pub enum variant removed or renamed ---

Description:
A publicly-visible enum has at least one variant that is no longer available under its prior name. It may have been renamed or removed entirely.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/enum_variant_missing.ron

Failed in:
  variant GateKind::CheckpointDeclaration, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/gates.rs:160

--- failure function_missing: pub fn removed or renamed ---

Description:
A publicly-visible function cannot be imported by its prior path. A `pub use` may have been removed, or the function itself may have been renamed or removed entirely.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/function_missing.ron

Failed in:
  function devflow_core::verify::phase_has_blocking_human_checkpoint, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/verify.rs:131
  function devflow_core::prompt::checkpoint_auto_decide_prompt, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/prompt.rs:782
  function devflow_core::agent_result::blocking_human_checkpoint_reported, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/agent_result.rs:716
  function devflow_core::agent_result::checkpoint_reported_in_capture, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/agent_result.rs:892

--- failure inherent_method_missing: pub method removed or renamed ---

Description:
A publicly-visible method or associated fn is no longer available under its prior name. It may have been renamed or removed entirely.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/inherent_method_missing.ron

Failed in:
  ClaudeDriver::exec_resume_command, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/agents/claude.rs:123
  ClaudeDriver::exec_resume_command, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/agents/claude.rs:123

--- failure pub_module_level_const_missing: pub module-level const is missing ---

Description:
A public const is missing or renamed
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/pub_module_level_const_missing.ron

Failed in:
  GATE_RESOLUTION_RULE in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/prompt.rs:79
  MAX_CHECKPOINT_RESUMES in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/mode.rs:107

--- failure struct_missing: pub struct removed or renamed ---

Description:
A publicly-visible struct cannot be imported by its prior path. A `pub use` may have been removed, or the struct itself may have been renamed or removed entirely.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/struct_missing.ron

Failed in:
  struct devflow_core::state::ApprovedCheckpoint, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/state.rs:443

--- failure struct_pub_field_missing: pub struct's pub field removed or renamed ---

Description:
A publicly-visible struct has at least one public field that is no longer available under its prior name. It may have been renamed or removed entirely.
        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/struct_pub_field_missing.ron

Failed in:
  field checkpoint_resumes of struct State, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/state.rs:206
  field checkpoint_approval of struct State, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/state.rs:299
  field checkpoint_resumes of struct State, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/state.rs:206
  field checkpoint_approval of struct State, previously in file /var/home/denniyahh/Github/devflow/.worktrees/phase-53/target/semver-checks/git-origin_develop/2f83d40d7900c99f11e4bb27938c1b6272eeb8d0/crates/devflow-core/src/state.rs:299

     Summary semver requires new major version: 8 major and 0 minor checks failed
    Finished [   3.587s] devflow-core
[exit 100]
```

Major items against the plan SUMMARYs: every item named above (CheckpointApproval, GateKind::Question added /
GateKind::CheckpointDeclaration removed, phase_has_blocking_human_checkpoint, checkpoint_auto_decide_prompt,
blocking_human_checkpoint_reported, checkpoint_reported_in_capture, ClaudeDriver::exec_resume_command,
GATE_RESOLUTION_RULE, MAX_CHECKPOINT_RESUMES, ApprovedCheckpoint, State.checkpoint_resumes, State.checkpoint_approval)
appears in the 53-*-SUMMARY.md text (literal name match). No unrecorded item.

## M2 — live claude

Fixture: `scripts/scratch-dogfood-repo.sh --live-payload` at `/tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/fx-m2` (base `integration`, no origin remote).
Started 2026-11-19T20:54:24Z: `devflow start --phase 1 --agent claude --mode supervise`; `workflow_started` carries
commit d74a788543289704cc512e701f6839ea3aa0478a, dirty false.

Question asked: `question_asked` ts 1795121777 (2026-11-19T20:56:17Z), question_id 18e98dbef87e4e1c, gate 18e88dbef8838c98,
kind blocking-human, recommended budget-120.

`devflow gate list`:

```text
PHASE  STAGE     AGE       GATE ID          CONTEXT
1 code      19s ago   18e88dbef8838c98 Which value should probe.budget.DEFAULT_RETRY_BUDGET_SECS hold, in seconds? Options — budget-30: 30 seconds; budget-120: 120 seconds (recommended); budget-300: 300 seconds.

answer with: devflow gate approve <phase> [--gate-id <id>] [--note ...] | devflow gate reject <phase> [--gate-id <id>] --note ... (fix and relaunch) | devflow gate abort <phase> [--gate-id <id>] [--note ...] (ends the run) | a question gate: devflow gate answer <phase> --gate-id <id> (--option <id> | --text <answer>)
```

`devflow gate show 1` (lists the fixture's three budget options):

```text
phase 1 code (19s ago)
gate id: 18e88dbef8838c98
question: Which value should probe.budget.DEFAULT_RETRY_BUDGET_SECS hold, in seconds?
context: Phase 1, plan 01-01, Task 1 (checkpoint:decision, gate=blocking-human). The probe retries calls to an upstream service until the budget runs out. Upstream failover completes within 90 s; a budget shorter than that turns every failover into a hard failure. The on-call pager escalates a stalled job after 4 minutes (240 s), so a longer budget lets a dead upstream page someone before the probe reports its own failure. A longer budget also holds a worker slot longer.
options:
  budget-30: 30 seconds
    Fails fast, frees worker slots quickly; but shorter than the 90 s failover window, so every failover becomes a hard failure.
  budget-120: 120 seconds (recommended)
    Covers the 90 s failover window and reports before the 4-minute page; holds a worker slot up to two minutes per stalled call.
  budget-300: 300 seconds
    Survives even a slow failover with margin; but exceeds the 4-minute pager threshold, so a dead upstream pages someone first.
```

Answer: `devflow gate answer 1 --gate-id 18e88dbef8838c98 --option budget-120` at 1795138083 (2026-11-20T01:28:03Z), exit 0, run by the session
on the operator's instruction ("answer it for me with budget-120"). Seconds held from `question_asked`: 16306
(the operator answered later than the 180 s minimum; the hold far exceeds it).

Events (fixture `.devflow/events.jsonl`): exactly one Code `stage_launched` (1795121716); `question_asked` 1795121777;
`question_answered` 1795138100 `{"answer":{"option_id":"budget-120"},"policy":"operator_via_gate","answered_by":"denniyahh"}`;
no idle-timeout, hang or relaunch event for the stage; Code `advance_evaluated` success 1795138387, then `transition` to
validate. The answered value landed in the fixture code: `probe/budget.py:3: DEFAULT_RETRY_BUDGET_SECS: int = 120`.

`devflow history 1` excerpt:

```text
[1795121777] question_asked (code)
  question 18e98dbef87e4e1c: Which value should probe.budget.DEFAULT_RETRY_BUDGET_SECS hold, in seconds?
  options: budget-30 "30 seconds" | budget-120 "120 seconds" (recommended) | budget-300 "300 seconds"
[1795138100] gate_claimed_answer_removed (code)
[1795138100] question_answered (code)
  answer 18e98dbef87e4e1c: option budget-120 by denniyahh via operator_via_gate
```

Validate gate approved by the session (route finish; GSD verification passed, clean tree); the run ended
`finished: Validate passed`.

claude-live: PASS

## M3 — live codex

Fixture: `scripts/scratch-dogfood-repo.sh --live-payload` at `/tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/fx-m3`. Deviation: the first
`devflow start --phase 1 --agent codex --mode supervise` was refused for a missing CONTEXT.md (999.255); the session
committed a short fixture `01-CONTEXT.md` on `integration` (no `/gsd-*` command) and started again at 2026-11-20T01:34:28Z.

Question asked: `question_asked` ts 1795138509, question_id 18e99cf6883d3666, gate 18e89cf68833186e, options budget-30, budget-120, budget-300.

Answer: `devflow gate answer 1 --gate-id 18e89cf68833186e --option budget-120` at 1795139587, exit 0, run by the session on
the operator's instruction. Seconds held from `question_asked`: 1078 (minimum 1,020).

(1) Codex capture (`.devflow/phase-01-stdout`), `mcp_tool_call` items for `devflow_ask`:
- `item_4` (first call, no `question_id` argument) result: `{"status":"pending","question_id":"18e99cf6883d3666"}`, text
  "No answer yet. Call devflow_ask again with the same arguments and question_id 18e99cf6883d3666 …".
- `item_10` (re-call, argument `question_id: 18e99cf6883d3666`) result: `{"question_id":"18e99cf6883d3666","answer":{"option_id":"budget-120"},"answered_by":"denniyahh","answered_at":"1795139598"}`.

(2) Events for question 18e99cf6883d3666: question_asked 1, question gate_fired 1, question_answered 1 (ts 1795139598,
policy `operator_via_gate`), question_closed 0.
(3) The answer reached codex only as the re-call's `devflow_ask` result (item_10), after the first call returned `pending`.

## M4 — closing run (D-17)

Project: `/tmp/claude-1000/-var-home-denniyahh-Github-devflow/fb493e9a-239c-4af3-b592-80f2362c147e/scratchpad/fx-m4`, scaffolded by `scripts/scratch-dogfood-repo.sh` (default mode, not DevFlow's repo), then its phase
replaced with "Probe Status Report", whose output format and budget source are left undecided, no plan and no
CONTEXT.md. Started 2026-11-20T01:54:20Z: `devflow start --phase 1 --agent claude --mode supervise --until plan`.

Codex Code-stage usage (`turn.completed` in capture 1795139777698340011-2-stdout): input 3,188,203 tokens, of which
3,097,856 cached (97.2%); output 4,950 (reasoning 140). Plan-stage capture: input 71,263 (42,752 cached), output 71.
Against the plan's estimate (about 1.35 M input per 15 min held), this run used about 2.4× that for a 1,078 s hold plus the
plan's work; recorded, not judged.

Same session: exactly one Code `stage_launched`, no idle-timeout/hang event; answered value in code:
`probe/budget.py:3:DEFAULT_RETRY_BUDGET_SECS: int = 120`. Validate gate approved by the session (GSD verification passed, clean
tree); the run ended `finished: Validate passed`.
codex-config-after: d7a527ad0a960a6f491202107f24a751292a2ebd3391e48737731a17bcede634  (equals codex-config-before)

codex-live: PASS

Questions (both through `devflow_ask`, both answered at a question gate by the session on the operator's word):
1. question_id 18e99e141d3d4ad2, asked 1795139735: "No CONTEXT.md found for Phase 1. Plans will use research and
   requirements only — your design preferences won't be included. Continue or capture context first?" Options
   `continue` | `discuss` (recommended). Answered `continue` (operator: "proceed as recommended", the session's
   recommendation), question_answered 1795139978.
2. question_id 18eb9e4f22b860f2, asked 1795139989: "Research before planning Phase 1: Probe Status Report?" Options
   `research` | `skip` (recommended). Answered `skip` (operator: "skip research"), question_answered 1795144372.

Real GSD discuss question: none observed. Both are GSD plan-phase's own prompts (the no-CONTEXT prompt and the research
prompt), not discuss-phase gray-area questions. The operator's global PreToolUse auto-research hook did not answer
question 2: it reaches DevFlow through `devflow_ask`, not AskUserQuestion.

`devflow history 1` excerpt:

```text
[1795139735] question_asked (plan)
  question 18e99e141d3d4ad2: No CONTEXT.md found for Phase 1. Plans will use research and requirements only — your design preferences won't be included. Continue or capture context first?
  options: continue "Continue without context" | discuss "Run discuss-phase first" (recommended)
[1795139978] gate_claimed_answer_removed (plan)
[1795139978] question_answered (plan)
  answer 18e99e141d3d4ad2: option continue by denniyahh via operator_via_gate
[1795139989] question_asked (plan)
  question 18eb9e4f22b860f2: Research before planning Phase 1: Probe Status Report?
  options: research "Research first" | skip "Skip research" (recommended)
[1795144372] gate_claimed_answer_removed (plan)
[1795144372] question_answered (plan)
  answer 18eb9e4f22b860f2: option skip by denniyahh via operator_via_gate
```

Run end: `advance_evaluated` plan success 1795144905, `workflow_finished` stopped_at plan; plans written: 01-01-PLAN.md,
01-02-PLAN.md. Claude result usage (Plan stage): input 44, cache creation 166,440, cache read 1,446,600, output 7,940
(thinking 1,072); cost $2.92 list; duration 5,198 s (most of it the two question holds).

closing-run: PASS

## Run exceptions (dogfooding notes)

Steps taken outside the normal DevFlow path, each on the operator's word:

- **Proof binary.** M1–M4 ran the phase-head build from a separate install root, not the installed release binary,
  because the Phase 53 runner was live at its Validate gate and reinstalling would have replaced its binary under it
  (operator chose this, option 1, 2026-11-19; 999.254).
- **Plan replan.** The `/gsd-plan-phase 53 --reviews` replan ran by hand; recorded in 53-REVIEWS.md.
- **TDD review.** GSD's TDD review deviations were accepted by an operator note, because its override flag is not
  implemented (999.252).
- **53-10 override.** Accepted by the operator; recorded in 53-UAT.md item 3.
- **M3 fixture.** A `01-CONTEXT.md` was added to the `--live-payload` fixture so codex could start (999.255).

No stage was run by hand to clear Validate. The gate was rejected with a note that DevFlow's own quick-task route
carried out (`/gsd-quick` marked the UAT and verification passed). Validate then passed on that commit, and the
operator approved the final gate, 2026-11-20.
