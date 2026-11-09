---
phase: 51-rate-limit-agent-failover
plan: 10
subsystem: pipeline
tags: [rust, supervisor, liveness, failover, test-seam, evidence]

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-06: FailoverRelaunchGuard and the routing of a failed failover relaunch to handle_infra_outcome (R2-D1); 51-01: the 50-13 handoff harness with a second agent stub; 51-09: the resume test conventions"
provides:
  - "Test-only seam DEVFLOW_TEST_WITHHOLD_PUBLISH_PROBE=<agent>:<stage> in monitor.rs (test-support gated), with its compiled-out pin and an exact publish-wait count pin"
  - "wait_for_publish takes a probe_withheld flag (private fn; release builds pass false)"
  - "Three D-12 experiment tests: the failover publish wait, its ordinary-transition control, and the cron arm's recorded-socket resume read"
  - "Test-only checker supv01_evidence_violations(text, complete, root), reused by 51-13, and its two tests"
  - "51-SUPV01-EVIDENCE.md: the trace (seven required reads plus three gap rows) and the first experiments"
affects: [51-11, 51-13]

actuals:
  tokens: 18367
  tasks: 3
  commits: 5
plan_head_before: bb1db996df234a5ffba3a2b2b710c42f73715a22
plan_head_after: 2e2723dc109b7fff0218a26cf7f8352dc37f7dcc

tech-stack:
  added: []
  patterns:
    - "A test seam keyed by the launch being spawned (agent and stage), so one launch in a chain can be withheld while the others run"
    - "An evidence file whose rows are checked by a named Rust test that resolves every cited file, line and test against the source tree"

key-files:
  created:
    - .planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md
  modified:
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - doc-check-allowlist.toml

key-decisions:
  - "The failover publish-wait test pins the end a test build reaches: the successor's own promotion expiry (PROMOTION_WAIT 5 s under cfg(test)) precedes the launcher's 10 s PUBLISH_WAIT, so the NotReady text is 'exited by itself before publishing'; the release path ('did not publish within', end_unpublished_child) is not reached by any test here"
  - "The trace adds three rows the planning-time list did not have (live_holder, remove_abandoned_waiters, sweep_entry), each recorded as a gap with its reason, so 51-13 has no new experiment to run for them"
  - "The ordinary path's missing cause record is filed as an extension of 51-06's backlog finding 2 (same mechanism), not as a second finding; 51-06's SUMMARY is not edited"

patterns-established:
  - "Run a seam-driven failure test with a positive control taken while the target is still live, then assert its absence afterwards"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "With the fallback's publish probe withheld, the failover hop ends at agent_handoff_relaunch_failed and the Code gate, with no cron arm, no Codex socket and no live member left in the fallback agent's group (positive control taken first)"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::failover_with_the_publish_probe_withheld_ends_at_a_gate"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage"
        status: pass
    human_judgment: false
  - id: D2
    description: "The seam is compiled out of release builds and non-test monitor.rs holds exactly one publish wait"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::the_publish_probe_seam_is_compiled_out_of_non_test_builds"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::non_test_code_waits_for_publish_exactly_once"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
    human_judgment: false
  - id: D3
    description: "An ordinary Plan to Code transition under the same intervention publishes no Code successor (observable outcome only)"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_ordinary_transition_with_the_publish_probe_withheld_fails_the_same_way"
        status: pass
    human_judgment: false
  - id: D4
    description: "A cron-arm resume and an ordinary resume refuse alike when the recorded supervisor's probe errors; a GONE recorded socket resumes"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_recorded_probe_cannot_answer"
        status: pass
    human_judgment: false
  - id: D5
    description: "The evidence file's trace rows are checked row by row, rejecting a headings-only stub and a fabricated well-formed text; the real file passes in trace mode"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_supv01_evidence_check_rejects_a_stub"
        status: pass
      - kind: unit
        ref: "cargo nextest run -p devflow --bin devflow --run-ignored only --no-tests=fail -E 'test(=pipeline_launch::tests::the_supv01_evidence_trace_rows_are_complete)'"
        status: pass
    human_judgment: false
  - id: D6
    description: "The trace is exhaustive: every liveness read reachable from either arm is listed. The checker cannot prove that a cited line holds the read or that the walk missed nothing"
    requirement: "SUPV-02"
    verification: []
    human_judgment: true
    rationale: "The exhaustiveness of a call-graph walk is a reading of the code, not something a test asserts; the verifier should re-walk or spot-check the 'How the walk was done' paragraph"

duration: 26min
completed: 2026-11-07
status: complete
---

# Phase 51 Plan 10: SUPV-01 Trace and Publish-Wait Experiments Summary

**Failover depends on SUPV-01's publish wait exactly as an ordinary Code launch does. With the probe answer withheld for one Code launch, neither the failover relaunch nor an ordinary Plan → Code transition publishes a successor, and no agent group survives. Failover's failure is visible (an event, then the never-silent Code gate). The ordinary transition's is not: it leaves no record of why. The cron arm's later `resume` reads the recorded socket exactly as every `resume` does and fails closed. The evidence file's trace rows are now checked row by row by a Rust test.**

## Needs attention

- **A test build never reaches the launcher's own publish-wait expiry.** Under `cfg(test)`, `PROMOTION_WAIT` is 5 s, shorter than the 10 s `PUBLISH_WAIT`. So the withheld successor ends itself (`orphaned_pending`, and it ends its own agent group), and the launcher sees it "exited by itself before publishing". In a release build the order is reversed (15 s against 10 s): the launcher's `end_unpublished_child` ends the successor, and `wait_for_publish` then ends the agent group its sidecar records. The plan expected the release ordering. Both orderings end at `NotReady`, a failed relaunch and the Code gate. But R2-13's "nothing orphaned" is shown here only for the successor's own group end. Release-path coverage comes from `monitor.rs`'s existing `wait_for_publish` unit tests, not from these e2e tests. The test names the text it pins in `PUBLISH_WAIT_ENDED_IN_TEST_BUILDS`, with this reason.
- **The walk found three reads the plan did not list:** `live_holder`, `remove_abandoned_waiters` and `sweep_entry`. All three are recorded in the trace as gaps, each with its reason, so 51-13 has no new experiment to run. `resume` also reaches read (4) through `reserve_launch`, and reads (1) and (2) through its own launch.

## Performance

- **Duration:** 26 min
- **Started:** 2026-11-07T23:07:00Z
- **Completed:** 2026-11-07T23:33:23Z
- **Tasks:** 3
- **Files modified:** 4 (1 created)

## Accomplishments
- **The seam.**
  - `DEVFLOW_TEST_WITHHOLD_PUBLISH_PROBE=<agent>:<stage>` is read by `publish_probe_withheld(state)`, which sits under `#[cfg(any(test, feature = "test-support"))]`. So is its one call in `spawn_monitor`. A release build passes `false`.
  - `wait_for_publish` gains a `probe_withheld` parameter that skips the probe, so the real expiry path runs. It is a private fn, so `cargo semver-checks` reports no semver update.
  - The variable is allowlisted in `doc-check-allowlist.toml`. Negative control: with the entry removed, `doc_check` fails naming the variable.
- **The pins.**
  - `the_publish_probe_seam_is_compiled_out_of_non_test_builds` is a source scan. It checks that the read is literal and appears only in the seam's body, that the seam is defined once directly under the gate, and that every call sits under the gate. It plants seven cases, among them an ungated decoy call and an ungated decoy read.
  - `non_test_code_waits_for_publish_exactly_once` asserts the count is exactly 2. Its planted control has a second call, counts 3 and is reported.
- **The experiments.** These are the three tests listed in the coverage block, plus two runs with the intervention removed (below).
- **`supv01_evidence_violations` and the trace.** The checker and `51-SUPV01-EVIDENCE.md`'s trace table: seven required rows, plus three gap rows the walk found.

## Task Commits

1. **Task 1 RED:** `f91bffa` test(51-10): add failing tests for the withheld failover publish wait
2. **Task 1 GREEN (tracer):** `0b5e55c` feat(51-10): add a test-only seam that withholds one publish probe
3. **Task 2:** `32d1da1` test(51-10): control the publish wait and the cron arm's resume read. These are tests only; they measure existing behaviour.
4. **Task 3 RED:** `bfd8fde` test(51-10): add failing tests for the SUPV-01 evidence row checker
5. **Task 3 GREEN:** `2e2723d` feat(51-10): check the SUPV-01 evidence trace row by row. Test-module code and the `.planning` evidence file only.

## Files Created/Modified
- `crates/devflow-core/src/monitor.rs`: the seam, the `probe_withheld` parameter (the 4 test call sites pass `false`), and two pin tests
- `crates/devflow-cli/src/pipeline_launch.rs`:
  - `run_handoff_child_with_stubs_and_env`, an additive extension; existing callers are unchanged
  - the group-recording stub and the R2-13 helpers
  - three experiment tests
  - the evidence checker and its two tests
- `doc-check-allowlist.toml`: the `DEVFLOW_TEST_WITHHOLD_PUBLISH_PROBE` exception
- `.planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md`: the trace, the experiments and the never-silent findings. Its locations were read at `bfd8fde`; the GREEN commit changes no non-test line in the cited files.

## Experiment results

**R2-13 positive control (failover test).** The recorded pid was 1873785 and the pgid was 1873785: pid = pgid, so the fallback agent leads its own group, which is R-3's shape. Live members when it recorded itself: `[1873785, 1873788]` (the stub and its `sleep`). After the gate, the group had no live member within the 10 s poll. In the ordinary test, pid = pgid as well (1886176), with live members `[1886176]`.

**Intervention removed, ordinary transition.** I ran a temporary test, deleted afterwards: the same Plan → Code rig with the seam unset.
- Code published. `stage_launched` appeared with `stage: code`, `agent: claude` and launch `18e5e6a671bfc992`, and the state recorded that supervisor.
- After `go2`, Code completed: `workflow_finished` with reason `stopped_at`, `stopped: true`, `stop_reason: "stopped after code completed (--until code)"`.
- The outcome flips against the withheld run: there, no Code successor was published and the run settled at Code with no record. So the attribution comes from the intervention.

**Intervention removed, cron-arm read.** Baseline (a), inside the committed test: a paused run whose recorded socket is GONE resumes and records its own supervisor. A temporary run did the same for a `--until`-stopped run with a GONE socket, and it resumed too.

**Mutation, cron-arm read.** I changed `socket_live_socket`'s `Err(error) => Some(live(..))` arm to `None`. The test then failed with `phase 5197: resume over an unprobeable record launched`, and the classifier returned `RED_EVIDENCE_OK`. The file was restored and confirmed with `cmp`.

**Evidence checker mutants.** Each of the three rules below was mutated, run and restored (`cmp`):
- searching the whole file instead of its non-test part: case (k) failed
- dropping the past-end-of-file line check: case (h) failed
- dropping the after-`mod tests {` rule for tests: case (i) failed

**Real-file negative control.** I planted `commands.rs:99999` and `pipeline_launch::tests::no_such_test` in a copy of the evidence file. The real-file test reported both, and the original file was restored with `cmp`.

## TDD Gate Compliance

The RED → GREEN pairs are `f91bffa` → `0b5e55c` and `bfd8fde` → `2e2723d`. Task 2 has no GREEN commit because it measures existing behaviour. No REFACTOR commit was needed.

RED evidence came from nextest JUnit reports, written by a scratch `red` profile and passed through `gsd-tools check tdd-red-evidence`. Every record returned `RED_EVIDENCE_OK`. The records are in `target/red-evidence/51-10-*.json` (gitignored).

| Test | Failed on | Semantic assessment |
|------|-----------|---------------------|
| `the_publish_probe_seam_is_compiled_out_of_non_test_builds` | the real monitor.rs: `no literal std::env::var_os read of DEVFLOW_TEST_WITHHOLD_PUBLISH_PROBE`, `fn publish_probe_withheld( is not defined exactly once` | Intended: the seam did not exist. The planted cases passed first |
| `non_test_code_waits_for_publish_exactly_once` | did not fail: the baseline is exactly 2 | As the plan expects. RED evidence is the planted control (definition plus two calls), which counts 3 and is reported |
| `failover_with_the_publish_probe_withheld_ends_at_a_gate` | `no Code gate fired` after 45 s. The log ends at `stage_launched` for `codex` | Intended: nothing withheld the probe, so the relaunch published. The positive control (stub recorded, group live) had already passed |
| `the_supv01_evidence_check_rejects_a_stub` | `(a) wait_for_publish is reported as missing its row: []` | Intended: the stub checker returned nothing |
| `the_supv01_evidence_trace_rows_are_complete` | `read …/51-SUPV01-EVIDENCE.md: No such file or directory` | Intended: the plan names the missing file as the state before Task 3 |

## Verification

| Check | Command | Result |
|-------|---------|--------|
| Task 1 gate (also re-run as the tracer gate before Task 2) | the plan's 6 chained `cargo nextest run … --no-tests=fail -E 'test(=…)'` plus `cargo semver-checks check-release -p devflow-core` | exit 0; semver: "no semver update required" |
| Task 2 gate | the plan's 2 chained nextest commands | exit 0 |
| Task 3 gate | the plan's 3 chained nextest commands, the real-file one with `--run-ignored only` | exit 0 |
| Plan-level | `cargo test -p devflow-core --lib` | 1139 passed, 0 failed, 2 ignored |
| Plan-level | `cargo test -p devflow --bin devflow` | 676 passed, 0 failed, 3 ignored (the third is the new real-file test) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings`, plus `cargo clippy -p devflow-core --lib` and `-p devflow --bin devflow` (non-test builds, where the seam is compiled out) | exit 0 each |
| Scope | `git diff --name-only bb1db99..HEAD` | the 4 files above only; `Cargo.lock` unchanged (T-51-SC) |

**What these checks do not establish.**
- **The release-build ordering** of the publish wait against the promotion wait is not exercised end to end (see Needs attention).
- **The ordinary transition's assertions** cover only what the run leaves on disk and in the process table. They do not pin the missing cause record, so a later fix will not break them.
- **The cron-arm test** withholds only read (5). Reads (6) and (7) are 51-13's.
- **The real-file evidence check** is `#[ignore]`d, because `develop` and `main` carry no `.planning/`. It runs in this plan's gate and in 51-13's, not in `scripts/check.sh all`. It proves that cited files exist and are long enough, that one cited file names the row's read in its non-test code, and that cited tests are declared. It does not prove that a cited line holds the read, or that a cited test exercises it. I re-read every cited line by hand at `bfd8fde`.
- **Run counts.** Each e2e test passed on every run it got:
  - failover: 3, counting the gate run, the tracer re-run and the Task 3 gate
  - ordinary transition: 2
  - cron resume: 2
  - all three again in the full suite

  That is a weak bound and does not size flakiness. Both e2e tests take about 5 to 6 s, which is set by the 5 s test promotion wait.

## Decisions Made
- **Pinning the measured end.** The failover test asserts the `NotReady` text a test build produces (`exited by itself before publishing`), not "did not publish". It explains why in a named constant. If the constants change, the test fails and the comment says why, rather than passing on an alternation of both texts.
- **The R2-13 numbers leave the child through a non-`DEVFLOW_` report variable (`P51_GROUP_REPORT`).** libtest discards a passing child's output, and the plan requires the pgid in this SUMMARY. The parent prints the report under `--no-capture` and fails if the child wrote none.
- **Gap rows.** `live_holder` (decides only the pid a refusal names), `remove_abandoned_waiters` (gate-waiter registrations, never a supervisor) and `sweep_entry` (reached only without a loadable state, which a cron-paused run has) are gaps, not experiments. Withholding any of them cannot change whether either arm launches or refuses.
- **The cron-arm fixture** records launch id `18e5e63a18d5674f`, in the real launch-id format, so the GONE baseline's `held_without_socket` derives a liveness path and finds it missing. An invalid id would have skipped that read silently.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Test expectation] The withheld failover relaunch ends through the successor's promotion expiry, not `end_unpublished_child`**
- **Found during:** Task 1 GREEN.
- **Issue:** the behaviour block expected the reason to name "did not publish". The run produced `supervisor N exited by itself before publishing (exit status: 0); 0 child process groups ended; no sidecar for launch …`. The cause is the test-build `PROMOTION_WAIT` (5 s), which is shorter than `PUBLISH_WAIT` (10 s).
- **Fix:** the assertion now requires `supervision shape not ready` together with `PUBLISH_WAIT_ENDED_IN_TEST_BUILDS`, with the reason documented. Everything else in the behaviour block is asserted as written: the event order, the gate, no cron arm, no socket, the state, and R2-13.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Commit:** `0b5e55c`

**2. [Rule 3 - Blocking] A parent-readable report for the R2-13 pgid**
- **Found during:** Task 1.
- **Issue:** the plan wants the recorded pgid in the SUMMARY, but the child's output is discarded on a pass.
- **Fix:** `P51_GROUP_REPORT`, a path in a parent-owned tempdir, plus `run_handoff_child_with_stubs_and_env`.
- **Commit:** `f91bffa`

---

**Total deviations:** 2 auto-fixed (1 test expectation, 1 blocking). **Impact:** none on scope. The first narrows what R2-13 shows end to end, as stated under Needs attention.

## Backlog finding for the orchestrator

**This extends 51-06's backlog finding 2 (same mechanism); file one `999.x` entry for both.** The defect is that an in-supervisor launch failure outside failover leaves no record of its cause.
- **Reproduction:** `pipeline_launch::tests::an_ordinary_transition_with_the_publish_probe_withheld_fails_the_same_way`, an ordinary Plan → Code transition whose Code launch does not publish. 51-06 found the same defect by reading the `Ambiguous` re-drive.
- **Mechanism:** the failed launch clears the record (WR-04). The advance error goes back through `run_monitor`'s `other =>` arm, and the supervisor's stderr is `/dev/null`.
- **Observable, confirmed in a temporary run:**
  - The run is left at stage Code with `supervisor: None`, not stopped, no gate and nothing scheduled.
  - None of `publish`, `NotReady`, `not ready` or `before publishing` appears in `.devflow/events.jsonl`, in `.devflow/phase-NN-monitor.log` (only the two legacy-launch notices are there) or in any other file under `.devflow/`.
  - The only durable trace is the successor's own `supervisor_superseded` event, with reason `orphaned_pending` and `agent_group: ended`. It says that a pending launch was never promoted, not why. In a release build the launcher's `end_unpublished_child` ends the successor at 10 s, before its 15 s promotion expiry, so that event may not appear at all. This is unverified, because no test runs the release constants.
- **Candidate fix:** as in 51-06. Route a non-`NotOwner` launch failure on any in-supervisor path to the infra gate (`gate_or_abort_infra`), with a record-keeping exemption like `FailoverRelaunchGuard`, so the gate write is not refused as `NotOwner`.

No agent group survived a withheld publish (R2-13), so there is no second finding.

## Issues Encountered
- My first cron-arm assertion looked for the word `liveness` in the refusal. The actual text says `probe failed (stat failed: … Not a directory)`, and the assertion was corrected before the commit.
- The prose line reference for `claim_phase_holder`'s `try_lock` was off by one (353). It was corrected to 352 before the commit.

## Known Stubs
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 51-13 can reuse `supv01_evidence_violations(text, true, root)` for complete mode, `run_handoff_child_with_stubs_and_env`, `group_recording_script`, `agent_group_was_live` and `agent_group_ended`.
- 51-13 also needs to replace the five `51-13` / `pending (51-13)` rows, (2), (3), (4), (6) and (7), and write `## Conclusion` with `Failover arm: ` and `Cron arm: ` lines.

## Self-Check: PASSED
- FOUND: `.planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md`, `crates/devflow-core/src/monitor.rs`, `crates/devflow-cli/src/pipeline_launch.rs`, `doc-check-allowlist.toml`
- FOUND (ancestors of HEAD): `f91bffa`, `0b5e55c`, `32d1da1`, `bfd8fde`, `2e2723d`
- Commits measured from the ledger: `git rev-list --count bb1db99..2e2723d` = 5
