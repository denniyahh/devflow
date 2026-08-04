---
phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
plan: 04
subsystem: infra
tags: [completion-oracle, exit-code, cli-flags, escape-hatch, provenance, clap, serde]

requires:
  - phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
    provides: "31-01's launch-selection seam (claude_stream_launch_enabled, STREAM_JSON_STAGES, MonitorLaunch, exec_command_single_document, monitor_log_path) and 31-02's IdleTimeout verdict, which the arbitration must leave alone"
provides:
  - "A stream-derived `Success` is arbitrated against the recorded exit code before it is returned"
  - "137 and 127 keep evaluate_layer2's ResourceKilled / AgentUnavailable mapping through the arbitration"
  - "`--legacy-claude-launch` on `start` and `resume`, plus `DEVFLOW_CLAUDE_LEGACY_LAUNCH`, off by default"
  - "`State::legacy_claude_launch`, persisted so a detached advance process honours it"
  - "A three-channel loud notice naming 999.64 as what the legacy path gives up"
  - "D-12's inverse assertion: a stream-json capture is not consumed by the single-document envelope path"
affects: [31-05 acceptance run]

actuals:
  tokens: 51500
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "Arbitration as a sibling of reconcile_layer0_verdict: take a returned result, reconcile it against one authoritative out-of-band signal, do not reorder the cascade"
    - "verdict: None on any downgraded result — the third site in this codebase to dodge classify_validate_outcome's Some(Verdict::Pass)-first match arm"
    - "Escape-hatch provenance on three channels (stdout, monitor log, event ledger) because an unattended run has nobody watching the first"
    - "OR-only opt-out combination: a resume never clears a persisted operator choice"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/parallel.rs
    - OPERATIONS.md

key-decisions:
  - "The env resolver `config::claude_legacy_launch()` takes no project_root — D-11 specifies one flag and one environment variable, and a standing devflow.toml default for an escape hatch is exactly the 'used routinely, erodes what it protects' shape D-11 warns against"
  - "`resolve_launch_shape` extracted from `launch_stage_inner` verbatim so the resolved (program, argv, monitor arm) is assertable without spawning a process; `stream_launch` is threaded in rather than recomputed so one predicate still governs shape, canary gate and notice"
  - "A local five-line `append_monitor_log` in pipeline_launch.rs rather than exporting devflow-core's private equivalent — one CLI caller does not justify widening the core crate's public surface"
  - "`devflow parallel` passes the opt-out as `false`: it is precisely the multi-run shape whose delegated work the legacy path orphans; an operator who wants it can still set the env var, which `start` reads per phase"
  - "MonitorLaunch gained no derives; tests use `matches!` instead, keeping devflow-core untouched by task 2 (review W3 recorded monitor.rs as unchanged by this plan's body)"

patterns-established:
  - "A `--exact` reading is only reported alongside a same-session negative control proving a non-matching name yields `0 passed` and still exits ok"

requirements-completed: [Constraint-9, D-11, D-12]

duration: 22min
completed: 2026-08-03
status: complete
---

# Phase 31 Plan 04: Exit-Code Arbitration and the D-11 Opt-Out Summary

**A Claude stage that claims success in its stream can no longer outrank a contradicting exit
code, and an operator can force the pre-31 launch path with one flag or one environment
variable — off by default, announced on three channels, and never selected automatically by a
parse failure.**

## Performance

- **Duration:** 22 min
- **Tasks:** 2 of 2
- **Files modified:** 8

## Test numbers actually observed

Reported as printed. `cargo test --workspace` was run to completion and its **own** exit code
captured, not a pipeline's.

| Command | Result |
|---|---|
| `cargo test --workspace` (final) | **exit 0**; 22 `test result:` lines, **0** of them with a non-zero failure count |
| `devflow-core` lib target | `537 passed; 0 failed; 0 ignored; 0 filtered out` (was 530 — +7) |
| `devflow` bin target | `262 passed; 0 failed; 0 ignored; 0 filtered out` (was 255 — +7) |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --check` | exit 0 |
| `scripts/check.sh all` | exit 0 (`==> check.sh: all OK`) |

### Task 1 — every named test, `-- --exact`

All against `cargo test -p devflow-core --lib agent_result::tests::<name> -- --exact`.

| Test | Result |
|---|---|
| `stream_success_cannot_stand_against_nonzero_exit_code` | `1 passed; 0 failed; 536 filtered out` |
| `stream_success_stands_when_the_exit_code_is_zero` | `1 passed; 0 failed; 536 filtered out` |
| `stream_success_stands_when_no_exit_file_exists` | `1 passed; 0 failed; 536 filtered out` |
| `rate_limited_verdict_is_not_arbitrated_by_exit_code` | `1 passed; 0 failed; 536 filtered out` |
| `idle_timeout_verdict_is_not_arbitrated_by_exit_code` | `1 passed; 0 failed; 536 filtered out` |
| `arbitration_preserves_layer2s_resource_and_unavailable_codes` | `1 passed; 0 failed; 536 filtered out` |
| `stream_json_capture_is_not_consumed_by_the_single_document_path` | `1 passed; 0 failed; 536 filtered out` |

### The two D-12 isolation tests, recorded verbatim as the plan required

| Test | Result |
|---|---|
| `single_doc_envelope_not_consumed_by_claude_stream_parser` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 536 filtered out; finished in 0.00s` |
| `claude_stream_wiring_leaves_single_document_capture_unchanged` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 536 filtered out; finished in 0.00s` |

### Task 2 — every named test, `-- --exact`

All against `cargo test -p devflow --bin devflow pipeline_launch::tests::<name> -- --exact`.
The package is `devflow` and the target is `--bin devflow`; `-p devflow --lib` was run once to
confirm the trap the plan repaired and **exits 101** (`no library targets found`).

| Test | Result |
|---|---|
| `legacy_launch_flag_forces_the_single_document_path` | `1 passed; 0 failed; 261 filtered out` |
| `legacy_launch_is_off_by_default` | `1 passed; 0 failed; 261 filtered out` |
| `legacy_launch_use_is_recorded_in_provenance` | `1 passed; 0 failed; 261 filtered out` |
| `legacy_launch_skips_the_delivery_canary` | `1 passed; 0 failed; 261 filtered out` |
| `parse_failure_does_not_trigger_a_fallback` | `1 passed; 0 failed; 261 filtered out` |
| `legacy_launch_env_var_is_parsed_as_a_bool` (W4) | `1 passed; 0 failed; 261 filtered out` |
| `resume_does_not_clear_a_persisted_legacy_launch` (W5) | `1 passed; 0 failed; 261 filtered out` |
| whole module: `pipeline_launch::` | `30 passed; 0 failed; 232 filtered out` |

### Negative controls for every `1 passed` above

Run in the same session, because `CLAUDE.md` records that a non-matching `--exact` name exits 0.

- `cargo test -p devflow-core --lib agent_result::tests::this_test_does_not_exist_negative_control -- --exact`
  → `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 537 filtered out`
- `cargo test -p devflow --bin devflow pipeline_launch::tests::this_does_not_exist_control -- --exact`
  → `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 262 filtered out`

Both report `ok` while running nothing. The readings above are therefore real, not vacuous.

### Mutation check — the arbitration's negative control has teeth

Replacing `if exit_code == 0 { return result; }` with `if false { ... }` (an arbitration that
fires on *every* exit file, including zero) reddened exactly two tests:
`stream_success_stands_when_the_exit_code_is_zero` and the pre-existing
`evaluate_agent_result_reads_files_end_to_end` — `147 passed; 2 failed`. The mutation was
reverted with `git checkout -- crates/devflow-core/src/agent_result.rs` and the suite returned to
`149 passed; 0 failed`; nothing of it is in any commit. This is the check that distinguishes "the
arbitration works" from "the arbitration downgrades everything", and it discriminates.

### Grep-based acceptance criteria

| Criterion | Before | After |
|---|---|---|
| `rg -c 'reconcile_stream_success_against_exit_code' crates/devflow-core/src/agent_result.rs` | 0 | **3** (required ≥2) |
| <!-- planner-discipline-allow: claude_stream_gate_shape --> `rg -c 'claude_stream_gate_shape' crates/ --glob '*.rs'` | `agent_result.rs:4` | `agent_result.rs:4` — **unchanged**, no new identifier uses the retired name |
| `rg -c 'legacy_claude_launch' crates/devflow-core/src/state.rs` | 0 | **2** (required ≥1) |
| `rg -c 'claude_legacy_launch_forced' crates/devflow-cli/src/pipeline_launch.rs` | 0 | **3** (required ≥1) |
| `cargo run -q -p devflow -- start --help \| rg -c 'legacy-claude-launch'` | 0 | **1** |
| `STREAM_JSON_STAGES` | `&[Stage::Code]` | **`&[Stage::Code]` — unchanged** (see Open follow-on below) |

## What these tests do NOT establish

This is the most important section in this summary, and it is longer than the pass counts on
purpose.

1. **Nothing in this phase has run the real `claude` binary against the arbitration — and that is
   the arbitration's load-bearing premise.** Every task-1 fixture is a synthetic capture paired
   with a hand-written exit file. The arbitration assumes that a writer which died between
   flushing turn N and turn N+1 also died non-zero, and that a *healthy* run exits zero after
   `CloseRule` releases stdin. The second half of that is **unmeasured**. If the real CLI exits
   non-zero when its stdin is closed after a completed turn, this arbitration downgrades **every
   successful Code stage to `Failed`**. 31-05's acceptance run is what will reveal it, and it is
   sequenced after this plan. This is the single largest risk introduced here.
2. **The tests prove the arbitration fires on a synthetic truncated capture paired with a
   non-zero exit file. They do not establish that a real boundary-truncated capture in production
   is always accompanied by a non-zero exit.** That premise comes from constraint 9's reasoning,
   not from measurement. If a truncated capture can ever be paired with exit 0, the residual
   survives untouched and this plan closed nothing.
3. **`legacy_launch_use_is_recorded_in_provenance` asserts two of the three channels, not three.**
   The monitor log and the event ledger are read back from disk; the `println!` to stdout is
   observed only incidentally in `--nocapture` output and is not asserted. A refactor that dropped
   the `println!` would not redden anything.
4. **`parse_failure_does_not_trigger_a_fallback` covers *parse-failure-driven* fallback only.** It
   proves that an unparseable capture leaves the launch shape unchanged and writes no
   `claude_legacy_launch_forced` event. It does **not** prove that nothing anywhere selects legacy
   automatically — see the known un-migrated route below, which it never touches.
5. **The task-2 tests exercise `resolve_launch_shape` and `claude_stream_launch_enabled`, not a
   real spawn.** `legacy_launch_flag_forces_the_single_document_path` asserts the returned argv is
   byte-identical to `exec_command_single_document`, which is a strong statement about the
   resolution and says nothing about what the monitor then does with it.
6. **`arbitration_preserves_layer2s_resource_and_unavailable_codes` pins a mapping whose 137 arm is
   currently unreachable on the path this phase built** — see below. It proves the arbitration
   agrees with `evaluate_layer2`; it does not prove either one ever sees a 137 from a
   `PipeOwning` launch.

## Known un-migrated route (recorded, deliberately not fixed)

**`relaunch_checkpoint_session` hardcodes `MonitorLaunch::Legacy`** (`pipeline_launch.rs`, in the
`spawn_agent_and_record` call at the end of that function) and is reached by *unconditional*
checkpoint auto-decide — the code itself records the policy as *"D-03: unconditional agent
auto-decide, no flag/config toggle"*. That route calls `spawn_agent_and_record` directly rather
than `launch_stage_inner`, so it bypasses **both** `canary_gate` and
`claude_stream_launch_enabled` entirely, and therefore also bypasses the new opt-out and the new
notice.

So the honest claim this plan makes is **no *parse-failure-driven* fallback**, not "nothing selects
legacy automatically" — the unqualified version was already false on `develop` before this plan
started. Not migrated here: it is out of scope, and changing checkpoint-resume's launch shape
without the acceptance run's evidence would be exactly the behaviour prediction constraint 1
forbids. The predicate's doc comment names it so the next reader does not have to rediscover it.

## The `ResourceKilled` arm is currently unreachable on the `PipeOwning` path

`run_pipe_owning_monitor` writes `status.code().unwrap_or(-1)`, so a SIGKILLed child records
**`-1`, not `137`**. The arbitration's 137 → `ResourceKilled` arm is therefore dead on the path
Phase 31 built; it remains reachable from the `Legacy` arm's `sh` monitor, whose `$?` does carry
`128 + signal`. Recorded rather than silently relabelling a real OOM as `Failed` — the mapping is
preserved because `outcome_policy::decide_action` routes 137/127 to `GateInfra` rather than
`GateReview`, and the same exit code must not reach two different operator gates depending on
whether a stale Layer 1 success happened to be present (review W1).

## Open follow-on for the operator

**`STREAM_JSON_STAGES` remains `&[Stage::Code]` at the end of this phase, and that is deliberate.**
D-09 sequences the rollout; D-10 makes Code first because that is where 999.64 was observed and it
is the only stage that actually backgrounds work. Widening the list is a later change gated on
real production captures from a passing acceptance run — not something to do on the strength of a
green unit suite. Flagging it explicitly so it is an open decision rather than an implicit one.

## Task Commits

1. **Task 1 RED** — `ddf450c` (test): the five `<behavior>` tests plus the exit-code fidelity
   test and D-12's inverse assertion. Two failed as intended; five passed by construction (see
   TDD Gate Compliance).
2. **Task 1 GREEN** — `1f3f0f9` (feat): `reconcile_stream_success_against_exit_code` and its wiring.
3. **Task 2 RED** — `d9fd43d` (test): seven tests; RED was a **compile failure, 28 errors**.
4. **Task 2 GREEN** — `74e3e5d` (feat): the opt-out end to end, plus `OPERATIONS.md`.

No REFACTOR commit was needed for either task.

## TDD Gate Compliance

Both tasks show a `test(...)` → `feat(...)` sequence in `git log`, so the gate order holds. Two
qualifications, stated rather than smoothed over:

**Task 1's RED was partial, by construction.** Of the seven tests in commit `ddf450c`, only
`stream_success_cannot_stand_against_nonzero_exit_code` and
`arbitration_preserves_layer2s_resource_and_unavailable_codes` failed. The other five —
`stream_success_stands_when_the_exit_code_is_zero`, `..._when_no_exit_file_exists`, the
`RateLimited` and `IdleTimeout` exclusions, and the D-12 inverse assertion — are *must-not-change*
tests that pass before and after by definition. This is not the failure mode the fail-fast rule
targets (a behaviour-adding test that passes because the feature already exists); it is a pin, and
should be read as one. The mutation check above is what gives the most important of them teeth.

**Task 2's RED commit `d9fd43d` does not compile.** Every symbol it drives was absent
(`State::legacy_claude_launch`, `config::claude_legacy_launch`, `resolve_launch_shape`,
`announce_forced_legacy_launch`, `forced_legacy_launch_notice`, `apply_legacy_launch_opt_out`, and
the third argument to `claude_stream_launch_enabled`) — 28 compile errors, recorded in the commit
message. A compile failure is a weaker RED than a behavioural one and it has a real cost: a
`git bisect` landing on `d9fd43d` sees a crate that does not build. I took it anyway to keep the
gate sequence real for both tasks rather than trading it for tidiness, following 31-01's precedent
of accepting a transient red and stating the cost. **The branch is green only as of `74e3e5d`.**

## Deviations from Plan

### Auto-fixed issues

**1. [Rule 3 - Blocking] `commands::start`'s new parameter broke `parallel.rs`**

- **Found during:** Task 2
- **Issue:** `parallel::parallel` calls `start` positionally; the added `legacy_claude_launch`
  parameter made it a 9-of-10 argument call.
- **Fix:** Passed `false`, with a comment giving the reason specific to this call site rather than
  a generic one: `devflow parallel` is exactly the multi-run shape whose delegated work the legacy
  path orphans. An operator who genuinely wants it can still set the environment variable, which
  `start` reads per phase.
- **Committed in:** `74e3e5d`

**2. [Rule 3 - Blocking] The new environment variable reddened the doc-parity gate**

- **Found during:** Task 2
- **Issue:** `doc_check::source_devflow_env_vars_and_subcommands_are_documented` failed with
  ``source-read environment variable `DEVFLOW_CLAUDE_LEGACY_LAUNCH` is missing from scoped
  operator docs``.
- **Fix:** Added a row to `OPERATIONS.md`'s environment table. **This is also the gate's own
  negative control:** the plan warned that a const-mediated read passes `doc_check` *by
  blindness*, and the variable is read through `env_value("DEVFLOW_CLAUDE_LEGACY_LAUNCH")` with the
  name as a literal — so the gate saw it, reddened, and was satisfied only by real documentation.
  Observed reddening first, then green (`6 passed; 0 failed; 531 filtered out`).
- **Committed in:** `74e3e5d`

**3. [Rule 1 - Test bug] Two assertions in the task-2 RED tests were wrong about their own
fixtures**

- `events[0]["stage"]` was asserted as `"Code"`; `Stage`'s `Display` is the lowercase wire form
  and every other event in the ledger records it that way. Corrected to `"code"`.
- The 999.64 notice assertion used `contains("orphan")` against a notice that says `ORPHAN` for
  emphasis. Made case-insensitive — the requirement is the word, not its casing.
- `parse_failure_does_not_trigger_a_fallback` asserted `evaluate_layer1(..).is_none()` for an
  unparseable capture. It is not `None`: constraint 9 item 1 makes a torn line fail **closed** with
  a `Failed` verdict rather than letting an earlier turn stand in. The assertion was corrected to
  the property the test actually cares about — never `Success` — rather than being weakened to
  match. This is a case where the fixture taught me something about landed behaviour.
- **Committed in:** `74e3e5d`

### Deviations of substance from the plan's literal wording

**A. The env resolver lives in `devflow-core/src/config.rs` and takes no `project_root`.** The plan
put `DEVFLOW_CLAUDE_LEGACY_LAUNCH` in the artifact table as "read in `commands.rs` /
`pipeline_launch.rs`" but also required reading it "through the config helper" so `doc_check` can
see the literal. `env_value` is private to `config.rs`, so the resolver had to live there. It takes
no `project_root` because D-11 specifies a flag and an environment variable and nothing else —
adding a `devflow.toml` key would create exactly the standing per-project default for an escape
hatch that D-11 warns about.

**B. `resolve_launch_shape` was extracted.** The plan did not name it. `launch_stage_inner` spawns a
detached monitor, so `legacy_launch_flag_forces_the_single_document_path` and
`legacy_launch_is_off_by_default` could not assert on a `MonitorLaunch` without it. The body is the
pre-extraction `if/else if/else` verbatim.

**C. `MonitorLaunch` gained no derives.** Tests use `matches!`. Review W3 recorded `monitor.rs` as
unchanged by this plan's body, and it stayed that way.

---

**Total deviations:** 3 auto-fixed (2× Rule 3 blocking, 1× Rule 1 test bug) + 3 recorded
implementation choices. No scope creep; all three auto-fixes were required by the plan's own gates.

## Issues Encountered

**One real test failure read as ten, via a poisoned mutex.** `ENV_MUTEX` is a `std::sync::Mutex`
and every test that touches process environment does `.lock().unwrap()`. The first genuinely
failing test panicked while holding it, poisoning it, and nine unrelated tests then failed on
`PoisonError` — including three `resume_*` tests and three `relaunch_checkpoint_session_*` tests
that had nothing to do with this change. The failure list is actively misleading in that state:
the root cause has to be found by re-running the first failure in isolation. Recorded because the
next person to see a ten-test redline in this module will otherwise start debugging the wrong six.

**Package-scoped clippy fails for an unrelated pre-existing reason.**
`cargo clippy -p devflow-core --all-targets -- -D warnings` errors on `pub mod test_support` being
feature-gated; `cargo clippy --workspace --all-targets -- -D warnings` (what the plan specifies, and
what `scripts/check.sh` runs) exits 0. Not caused by this plan; noted so the narrower command is
not mistaken for a regression.

## Note on `actuals.tokens`

Recorded as 51,500 = chars/4 over the realized diff (~206,000 chars across the four commits).
31-01 recorded both this reading and the whole-file reading because it could not determine which
scale `estimate.tokens` used, and that ambiguity is still unresolved. The whole-file reading over
the eight files touched here would be substantially larger. Against `estimate.tokens: 68000` the
diff-scale reading is a ~24% underrun. Stated with its scale named so a future calibration pass can
discard it if it picks the other convention.

## Self-Check: PASSED

- All eight claimed files exist on disk and are modified in `git log 119c8b1..HEAD`.
- All four claimed commits exist: `ddf450c`, `1f3f0f9`, `d9fd43d`, `74e3e5d`.
- Working tree clean before this summary was written; the mutation-check edit was reverted and
  verified absent (`git status --short` empty, suite back to `149 passed; 0 failed`).
- `STATE.md` and `ROADMAP.md` deliberately untouched — the orchestrator owns those writes after
  the wave merges.

---
*Phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl*
*Completed: 2026-08-03*
