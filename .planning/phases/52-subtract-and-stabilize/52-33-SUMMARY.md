---
phase: 52-subtract-and-stabilize
plan: 33
subsystem: gate-answers
tags: [rust, devflow-cli, gates, routing, operator-surface, tdd]

requires:
  - phase: 52-29
    provides: "approve_route, reject_route, answer_family, status_less_step, screen_answer, read_evidence, next_step, recorded_canary_refusal as a router input"
provides:
  - "way_on_sentence: one sentence per gate, rendered from approve_route_by_kind / reject_route_by_kind (and next_step(Trigger::Abort)); a verification gate's approve clause states the finish rule (U10)"
  - "lead_with_way_on: the sentence first, after any [never-silent] marker, called by every listed gate writer"
  - "step_phrase / status_less_phrase: the one place a route's printed description lives (gate text and CLI print)"
  - "gate_respond refuses through the supervisor's own screen_answer before writing, and prints the route of every written answer (gate_respond_report)"
  - "approve_route_by_kind / reject_route_by_kind: the routers over a kind alone"
  - "OPERATIONS.md 'What each answer runs': the D-45 table, D-46 and amendments, status-less and refused answers, D-45 rule 4 limits, the chain-flag commit (999.219)"
affects: [52-30, 52-31, 52-32]

actuals:
  tokens: 25824
  tasks: 3
  commits: 8
plan_head_before: 9835cc9f2f31f7d9361ab93e4c21b4affa25c23b
plan_head_after: feef0dae915206be771f4c8ecbcf8fed54ce8244

tech-stack:
  added: []
  patterns:
    - "A route's description is rendered from its enum variant in one function, read by both the gate text and the CLI print"
    - "The CLI refuses an answer by calling the supervisor's own screen function, so both share one refusal predicate"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-33-SUMMARY.md
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - crates/devflow-core/src/agent_result.rs
    - OPERATIONS.md

key-decisions:
  - "The way-on helper is way_on_sentence(kind, phase, stage, canary_refusal); writers call lead_with_way_on, which puts it first after a leading [never-silent] marker"
  - "The pending-gate banner and gate list display bound (GATE_CONTEXT_DISPLAY_CHARS) grows from 300 to 450 so a gate shows its way-on sentence and its cause; the idle-timeout clause budget is re-derived (171-char prefix, 240 left for the reason)"
  - "GATE-03's never-lies checks read the cause after the way-on sentence: the sentence's finish rule names `passed` on every verification gate (U10), which is not a claim that GSD said passed"
  - "The rate-limit gate's own lead (52-09) is left for 52-31, which gives it its own kind; only the shared stage-failure tail '(retry, loop-to-code, or abort)' is gone from it"

patterns-established:
  - "Every gate context: [never-silent]? + way-on sentence + cause; nothing after the sentence names an answer"

requirements-completed: [GATE-02, GATE-04]

coverage:
  - id: D1
    description: "On a clean-pass Validate gate the context leads with the way-on sentence; a whitespace-only reject is refused before anything is written; a reject with a note prints the /gsd-quick route with the note, and the next Code prompt runs /gsd-quick with that note byte for byte"
    requirement: GATE-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_validate_gate_names_the_route_its_reject_prints_and_runs"
        status: pass
    human_judgment: false
  - id: D2
    description: "gate reject prints the routers' route for every GateKind and a kindless file, refuses preflight, checkpoint-declaration, kindless and whitespace-note rejects without writing, ignores decoy contexts, and prints the relaunch-refused route under a refusing canary"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_reject_prints_the_route_it_will_run"
        status: pass
    human_judgment: false
  - id: D3
    description: "gate approve prints approve_route's route: one row per D-45 table row from a fresh read, the pickup line, status-less routes with no GSD read, a refused kindless approve, a decoy label, the canary rows, and an unloadable state named instead of guessed"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_approve_prints_the_route_it_will_run"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every listed gate writer leads with the routers' sentence and names no answer after it; verification gates share the rule clause; canary rows at Plan, Code and Validate; planted controls fail"
    requirement: GATE-04
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::every_gate_names_only_the_routes_its_answers_run"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::rejecting_the_rescan_gate_is_refused_and_widens_nothing"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::approving_the_preflight_refusal_gate_records_the_set"
        status: pass
    human_judgment: false
  - id: D5
    description: "Under a refusing canary the stage-failure gate's way on survives the banner, gate list and all-roots row; the live failed-canary handoff gate leads with the way on and no longer carries the removed lead; approve and reject still stop the run"
    requirement: GATE-04
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::the_failure_gate_way_on_survives_the_banner_and_gate_list_for_a_long_reason"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#approving_the_failed_canary_gate_stops_the_run_and_names_the_way_on"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#rejecting_the_failed_canary_gate_stops_the_run_and_names_the_way_on"
        status: pass
    human_judgment: false
  - id: D6
    description: "OPERATIONS.md documents the D-45 table, the approve re-read, D-46 and both amendments, the status-less and refused answers, D-45 rule 4's limits, the refusing-canary way on and the chain-flag commit (999.219)"
    requirement: GATE-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
    human_judgment: true
    rationale: "doc_check proves the named identifiers exist; whether the prose reads correctly to an operator is a human judgment"
  - id: D7
    description: "A real operator answering a real gate through the installed binary"
    verification: []
    human_judgment: true
    rationale: "Every test uses stub GSD readers and fake agent drivers; no installed release binary has printed these routes over a live run"

duration: 46min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 33: The route an answer runs, printed and leading every gate Summary

**`devflow gate approve`, `gate reject` and `gate abort` now refuse, before writing anything, exactly what the supervisor's own `screen_answer` refuses, and print the route the written answer runs, from `approve_route` / `reject_route` over the gate file on disk and the run's recorded canary refusal. Every listed gate writer opens its context with one sentence, `way_on_sentence`, rendered from the same routers (a verification gate's approve clause states the finish rule, U10). The hand-written answer texts are gone, and OPERATIONS.md documents the D-45 table.**

## Performance

- **Duration:** about 46 min
- **Started:** 2026-11-11T13:11:50Z
- **Completed:** 2026-11-11T13:57:00Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments

- **The way-on sentence (Task 1, Task 3).** `way_on_sentence(kind, phase, stage, canary_refusal)` renders `Answers — approve: …; reject with a note: …; abort: ends the run.`
  - A status-less approve clause comes from `approve_route_by_kind`. Every reject clause comes from `reject_route_by_kind`. The abort clause comes from `next_step(Trigger::Abort)`. A refused answer reads "not offered".
  - A verification gate's approve clause is the rule (U10): it reads GSD's status again, finishes only on `passed` with a Validate pass on this commit over a clean tree, and otherwise takes the route the fresh read names.
  - Under a refusing canary, approve and reject on a stage-failure gate share one clause. It says the run stops, and that the way on is `devflow resume --phase N --agent <other>`.
  - `lead_with_way_on` puts the sentence first, after a leading `[never-silent]` marker, with the cause after it.
- **Every listed writer calls it (Task 3).**
  - Validate outcome, every kind, including both ceiling clauses.
  - The two reopens (`human_needed`, reader failure).
  - The gaps Plan `no_gap_closure_plan` and `reader_failure` gates.
  - The stage-failure gate, at any stage. `failure_gate_context`'s hand-written canary lead is replaced by the rendered relaunch-refused clause.
  - The preflight gate (`preflight_gate_context`, extracted) and the checkpoint-declaration gate (`rescan_gate_context`).
  - Removed: the 52-22/52-27 answer texts, the failure-cap ceiling sentence ("approve to ship, …"), the preflight and stage-failure "(retry, loop-to-code, or abort)", and the re-scan answer list.
- **The CLI refusal and print (Tasks 1, 2).** `gate_respond` reads the gate file back from disk and calls `screen_answer`. A refusal exits non-zero, names the answers that apply, and writes no response and no event. For a written answer, `answer_route_lines` prints:
  - `route: <name> — <description>` from `step_phrase` / `status_less_phrase`;
  - for a reject, the note;
  - for a verification approve, the read it made now (`read_evidence`) and that the supervisor reads again at pickup.

  A status-less approve reads no GSD status. When the run's state cannot be loaded and the route needs it, the print says `route: not known` and names the load failure.
- **OPERATIONS.md (Task 3).** The new `### What each answer runs` section under `## Answering gates` holds:
  - the D-45 table, and that an approve follows the Approve column in both modes, a failure-cap approve included;
  - D-46 and both amendments, and the ambiguous-verdict sentence;
  - the `.planning/config.json` chain-flag paragraph, covering the resume repair and the `--no-worktree` start repair (999.219);
  - the status-less table, the refused answers, and D-45 rule 4's limits (#6078, #6096).

  The `## Rate-limit failover` sentence now says the context leads with the routers' sentence. Two stale lines elsewhere were corrected; see Deviations.

## Task Commits

1. **Task 1 (tracer):** `214754c` test (RED), `5340813` feat (GREEN)
2. **Task 2:** `2bbb44e` test (RED, and `every_gate_kind` moved to `test_support`), `a945465` feat (GREEN), `517c792` style (fmt of the Task 1 e2e)
3. **Task 3:** `0ed1a25` test (RED, with `preflight_gate_context` and `rescan_gate_context(phase, stage, ..)` extracted unchanged), `bb8a0df` feat (GREEN), `feef0da` test (the `concat!` fix for the removed-surface scan)

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch `--profile red` config. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `supervisor_e2e::a_validate_gate_names_the_route_its_reject_prints_and_runs` | `RED_EVIDENCE_OK` | The `clean_pass` context opened "Validation passed — approve to finish the run?", not the sentence. The refusal half was not reached in RED. |
| `commands::tests::gate_reject_prints_the_route_it_will_run` | `RED_EVIDENCE_OK` | The verification rows passed (Task 1). The `stage_failure` reject printed `route: relaunch_with_note` with no description. |
| `commands::tests::gate_approve_prints_the_route_it_will_run` | `RED_EVIDENCE_OK` | The first table row printed no route at all. |
| `pipeline_outcomes::tests::every_gate_names_only_the_routes_its_answers_run` | `RED_EVIDENCE_OK` | `clean_pass` passed (Task 1). The `gaps_found` context opened with the agent line and ended in "Approving closes the gaps…". |
| `commands::tests::the_failure_gate_way_on_survives_the_banner_and_gate_list_for_a_long_reason` | `RED_EVIDENCE_OK` | The writer's context opened "stage code failed: a retry or loop-back will not relaunch…", not the sentence. |
| `failover_e2e::a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate` | `RED_EVIDENCE_OK` | The way on was already before the cause. The removed lead was still present, which is the control the plan names. |

Gate sequence:

- `test` 214754c → `feat` 5340813
- `test` 2bbb44e → `feat` a945465
- `test` 0ed1a25 → `feat` bb8a0df

There is no REFACTOR commit. Task 3's GREEN commit uses the plan's prescribed subject, scope `cli`, not `52-33` (see Deviations).

The tracer gate ran interactive, in `end-of-phase` mode, with an automated-only verify. Task 1's verify was re-run after the GREEN commit and passed, so expansion continued.

## Decisions Made

See `key-decisions`. One needs a note: GATE-04's sentence names `passed` on every verification gate, and two GATE-03 checks banned that substring outright: the e2e `never_lies` and the `approving_a_gaps_gate_runs_the_gap_loop` assertion. Both now check the text after the sentence, which is where the gate reports GSD's status. Each gained or kept a control: a cause saying `passed` after the sentence still fails. OPERATIONS.md's never-lies line says the same.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The display bound could not hold the sentence and the cause**
- **Found during:** Task 3
- **Issue:** The gate list and banner showed 300 characters. With a 132-character way on first, the idle-timeout cause clause no longer closed inside the bound, so `the_budgeted_idle_timeout_clause_closes_inside_the_gate_display_bound` failed. A verification gate would have shown almost nothing but its 287-character sentence.
- **Fix:** `GATE_CONTEXT_DISPLAY_CHARS` is now 450. The test's lead assertion was updated, and its 24-character control still discriminates (slack 23). `MAX_IDLE_TIMEOUT_CLAUSE_CHARS`'s derivation comment in devflow-core was re-derived; it is a comment-only change, with no API change.
- **Commit:** `bb8a0df`

**2. [Rule 1 - Bug] GATE-03 substring checks versus the U10 rule** — see Decisions Made. Files: `crates/devflow-cli/tests/supervisor_e2e.rs`, `crates/devflow-cli/src/pipeline_outcomes.rs`. Commit `bb8a0df`.

**3. [Rule 1 - Bug] Two OPERATIONS.md lines the new section contradicts**
- The `human_needed` row said approving runs Validate again. 52-29 reopens the gate.
- "An ambiguous verdict … always stops at a gate" was not true after 52-29: in auto mode over `gaps_found`, `stale` or `missing` it takes the table's route.

Both now match 52-29's routing.

**4. [Rule 3 - Blocking] The removed-surface scan read the test's `"ship"` answer word** — split with `concat!` (`feef0da`). `scripts/check.sh all` caught it on its first run.

**5. [Process] Two commits not gated on fmt; one subject over 72 characters**
- `214754c` was committed with an unformatted assertion; `517c792` formats it. The commit script now runs `cargo fmt --all -- --check` before clippy.
- `bb8a0df` uses the plan's prescribed subject, which is 76 characters against the 72-character rule (the commit-msg hook warns only). History was not rewritten.

**6. [Scope] Extra tests and files**
- Writer-driven lead assertions were added to `rejecting_the_rescan_gate_is_refused_and_widens_nothing` and `approving_the_preflight_refusal_gate_records_the_set`. The every-gate test calls the preflight and re-scan context functions directly, so these two pin that the live writers use them.
- Files beyond `files_modified`: `pipeline_gate.rs` (`read_gate_file` made `pub(crate)`), `test_support.rs` (`every_gate_kind` moved there), `crates/devflow-core/src/agent_result.rs` (doc comment).

**Total deviations:** 4 auto-fixed (2 blocking, 2 bugs), 1 process note, 1 scope note.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>` (named e2e), and re-run as the tracer gate | 0, 0 |
| Task 2 `<verify>` (2 named runs) | 0 |
| Task 3 `<verify>` chain without `check.sh` (6 named runs: every-gate, banner, two canary stop pins, the handoff e2e, `doc_check::doc_referenced_identifiers_exist_in_source`) | 0 |
| `cargo nextest run -p devflow` after Task 1 GREEN | 0 (922 passed) |
| `cargo nextest run -p devflow` after Task 3's first GREEN draft | 100: 2 failures (the two GATE-03 substring checks), fixed before the commit |
| `scripts/check.sh all` after `bb8a0df` | 101: `doc_check::no_removed_surface_literal_remains` |
| `scripts/check.sh all` after the last code change (`feef0da`) | 0 ("all OK"; 2032 passed, 0 failed across its `test result` lines) |
| `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` before each commit from `517c792` on (clippy alone before) | 0 |

The known flakes 999.221, 999.222 and 999.223 did not occur. No semver check was run: no `pub` item in devflow-core changed, only a doc comment.

**What these checks do not establish:**
- Only the reject on a clean pass is proven end to end (printed route = launched pass). The other kinds and answers are checked against the routers, not against a live supervisor.
- A verification approve's print comes from a read at answer time. The supervisor reads again at pickup and decides, and the print says so; a status change between the two reads routes differently from the print.
- The sentence is rendered from the canary fact when the gate opens. The plan assumes, unverified, that nothing records a new canary outcome during a gate wait. No test changes the canary mid-wait.
- The preflight and checkpoint-declaration rows of the every-gate test call the context functions their writers use. That the live writers use those functions is pinned by the two writer-driven tests named above, for one path each.
- No real GSD install, real agent or installed release binary was exercised. Every read came from the `gsd_run` stub, and every agent was a fake driver.
- The rate-limit gate still carries its 52-09 hand-written lead ("approving this gate relaunches it now, `devflow gate abort` ends the run"). It is a `stage_failure` gate today, but it is not in this plan's writer list; 52-31 gives it its own kind and its row.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: new-exec-site | crates/devflow-cli/src/commands.rs | `devflow gate approve` on a verification gate now runs GSD's `gsd_run` reader, `git status` and `git rev-parse` in the run's evidence root from the CLI process, through the same `read_evidence` the supervisor uses (hermetic git command, the reader's 60 s deadline). Before this, only the supervisor ran them. |

## Known Stubs

None.

## Issues Encountered

- Task 1's GREEN and Task 2's RED tests were written in the same tree. To commit them separately, the Task 2 parts were saved in the scratch directory, taken out of the Task 1 commit, and restored afterwards. The Task 1 commit's content was the content the full `-p devflow` run had tested.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-30, 52-31 and 52-32.
- 52-31 adds `GateKind::RateLimit`:
  - add it to `test_support::every_gate_kind`;
  - add its row to `way_on_sentence`'s callers, and give the rate-limit writer a `lead_with_way_on` call in place of its hand-written lead;
  - the exhaustive matches in `answer_family`, `status_less_step` and `print_stage` (commands tests) will flag it.
- `status_less_phrase` needs an arm for any new `StatusLessStep`.

## Self-Check: PASSED

- Files exist: `crates/devflow-cli/tests/supervisor_e2e.rs` (`fn a_validate_gate_names_the_route_its_reject_prints_and_runs`), `crates/devflow-cli/src/commands.rs` (`fn gate_approve_prints_the_route_it_will_run`, `fn gate_reject_prints_the_route_it_will_run`), `crates/devflow-cli/src/pipeline_outcomes.rs` (`fn every_gate_names_only_the_routes_its_answers_run`, `fn way_on_sentence`).
- Commits `214754c`, `5340813`, `2bbb44e`, `a945465`, `517c792`, `0ed1a25`, `bb8a0df` and `feef0da` are ancestors of HEAD, and `git rev-list --count 9835cc9..HEAD` is 8.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
