---
phase: 50-addressable-monitor-liveness
plan: 12
subsystem: cli
tags: [rust, supervisor, unix-socket, liveness, doctor, status, cleanup, read-only]

requires:
  - phase: 50-11
    provides: state.supervisor recorded by every launch; the /proc stray census now blind to 3.x
  - phase: 50-06
    provides: sweep_in, sweep_for_in (file-name fallback), SweepEntry, Sidecar, remove_launch_files
  - phase: 50-05
    provides: probe, Liveness, ProbeError, SocketPath::for_launch, publish
provides:
  - "PhaseLiveness (Alive/Stale/Gone/GoneWhileActive/NotLaunched/ProbeFailed), describe, label, the pure phase_liveness and the one-probe probe_phase"
  - "status prints 'supervisor: <socket> — <state>' with stop-then-resume for STALE and resume for GONE while active; no pid on the path"
  - "cleanup refuses ALIVE, keeps STALE (and a failed probe) unless --force"
  - "check_supervisor (replaces check_dead_monitor); PhaseFacts.supervisor and supervisor_launch_id; doctor --json phase supervisor {liveness, owner, launch, launch_id}"
  - "collect_orphan_supervisors: read-only socket sweep for no-state, superseded, deleted-root and sidecar-less sockets, each repaired by devflow stop --phase N --launch <id>"
  - "doctor_in(project_root, json, cache_dir); doctor --json orphan_supervisors and orphan_supervisor_sweep_error"
  - "/proc stray census documented as 2.x-only; pinned by a unit test and by a live 3.x supervisor in the status e2e"
affects: [50-13, 50-14, 50-15, 50-16, 50-17, 50-22]

actuals:
  tokens: 26400   # chars/4 over `git diff 7cf2d5f..99b056e` (105735 chars, context lines included)
  tasks: 3
  commits: 6
plan_head_before: 7cf2d5f49f11bb08c36969e47bc4cad7a5cd84dd
plan_head_after: 99b056e0f235bb8b0ddfcceb3a2263e9183436df

tech-stack:
  added: []
  patterns:
    - "Liveness presentation = enum + describe + one pure mapping; status, cleanup and doctor all read the same mapping"
    - "Read-only source-scan tests (orphan sweep body, census docs) run their planted controls through the same extractor as the real scan"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-core/src/agent.rs
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "A failed probe (NoReply included) is ProbeFailed: never ALIVE or GONE. status points at the socket path, doctor reports a Problem whose repair is `devflow status`, cleanup keeps the worktree unless --force"
  - "doctor's STALE repair is `devflow stop --phase N && devflow resume --phase N`, so the one repair string names both verbs the must-have truth requires"
  - "doctor sweeps a cache dir passed in (doctor_in); unit tests pass a tempdir and never touch the operator's real cache"
  - "An unlistable socket directory is reported (text warn, JSON orphan_supervisor_sweep_error) instead of reading as 'no orphans'"

patterns-established:
  - "Orphan repairs always carry --launch <id>; the bare `devflow stop --phase N` is never printed for a socket the state does not claim"

requirements-completed: []  # SUPV-01 is declared by later 50-xx plans; requirements.ready-ids returned 0/1 ready

coverage:
  - id: D1
    description: "status answers ALIVE / STALE / GONE from the recorded socket and names the repair verb for STALE and GONE"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#status_reports_each_liveness_state_with_its_repair_verb"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::liveness_matrix_covers_every_probe_outcome"
        status: pass
    human_judgment: false
  - id: D2
    description: "doctor reports STALE, GONE-while-active and failed probes as Problems with repairs; silent for ALIVE, stopped and unlaunched; --json supervisor facts path-free"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "commands::tests::doctor_reconciliation::check_supervisor_reports_stale_and_gone_while_active"
        status: pass
      - kind: unit
        ref: "commands::tests::doctor_reconciliation::check_supervisor_is_silent_when_alive_or_stopped"
        status: pass
      - kind: unit
        ref: "commands::tests::doctor_reconciliation::doctor_json_reports_supervisor_liveness_without_paths"
        status: pass
    human_judgment: false
  - id: D3
    description: "doctor's read-only orphan sweep lists no-state, superseded, deleted-root and sidecar-less sockets with the --launch repair and changes nothing"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "commands::tests::doctor_reconciliation::doctor_reports_an_orphan_socket_without_removing_it"
        status: pass
      - kind: unit
        ref: "commands::tests::doctor_reconciliation::the_orphan_sweep_only_reads_and_attributes_through_sweep_for_in"
        status: pass
      - kind: unit
        ref: "commands::tests::doctor_reconciliation::doctor_is_read_only_on_a_mismatched_project"
        status: pass
    human_judgment: false
  - id: D4
    description: "the /proc stray census is documented as 2.x-only and never matches a 3.x supervisor"
    verification:
      - kind: unit
        ref: "agent::tests::the_stray_census_matches_only_the_2x_process_shapes"
        status: pass
      - kind: unit
        ref: "agent::tests::the_stray_census_docs_say_it_is_2x_only"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#status_reports_each_liveness_state_with_its_repair_verb (census_never_sees_a_live_supervisor)"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 12: Probe-Answered Liveness for status, cleanup and doctor Summary

**`status`, `cleanup` and `doctor` now answer ALIVE / STALE / GONE from the socket in `state.supervisor`, with no pid read, and each non-ALIVE answer names its repair. `doctor` also lists supervisor sockets that no state claims (no state, superseded, deleted root, missing or broken sidecar). Each of those names `devflow stop --phase N --launch <id>`, and the sweep changes nothing on disk.**

## Needs awareness

- **50-14 must resolve a deleted-root launch.** A deleted-root finding repairs with `devflow stop --phase N --launch <id>`, but that root no longer exists. 50-14 has to find the socket by launch id alone, wherever the operator runs `stop`. Until then, that repair text names a verb form that does not exist yet.
- **A launch can show as an orphan for a moment.** A supervisor publishes its socket before the launcher saves `state.supervisor`. A `doctor` run in that gap lists the new socket as ALIVE with "records no supervisor" or "superseded". This is read-only and self-correcting, but the operator may see it.
- **`cleanup`'s refusal text for a live run still says ``run `devflow resume` ``.** For an ALIVE run the right verb is `stop`, or waiting. The wording is older than this plan and outside its truths, so I left it unchanged.
- **Commit `fa3e462` fails `clippy --all-targets -D warnings`.** The cause is a `useless_vec` in a test, fixed in `99b056e`. A bisect that lands on `fa3e462`, `df267bf` or `879bfbc` will see that lint. Tests pass at every commit.
- **One flaky core test.** `agents::opencode::tests::probe_rejects_a_setpgid_pipe_holder_before_parent_exit` failed once in a full `devflow-core --lib` run. It passed 3 of 3 times alone, and the next full run was 926/926. This plan does not touch that module.

## Performance

- **Duration:** about 25 min
- **Started:** 2026-10-17T20:37:12Z
- **Completed:** 2026-10-17T21:02Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- `PhaseLiveness` and the pure `phase_liveness` replace the pid-based `Liveness`/`liveness`. `probe_phase` probes once. `status` prints `supervisor: <socket path> — <state>`. Its hints are `devflow stop --phase N` then `devflow resume --phase N` for STALE, `devflow resume --phase N` (plus `advance` when gate-pending) for GONE while active, and the socket to inspect for a failed probe.
- `cleanup` refuses an ALIVE run even with `--force`. It keeps a STALE run's worktree, or one whose probe failed, unless `--force`. GONE and NotLaunched follow the existing rules. `status` and `cleanup` no longer read `monitor_pid`.
- `check_supervisor` replaces `check_dead_monitor` in `reconcile_phase`'s fixed order and reuses the facts' `PhaseLiveness`, so there is no second mapping. Findings name the phase and launch id, never a path. `doctor --json` phase entries carry `supervisor {liveness, owner, launch, launch_id}`.
- `collect_orphan_supervisors` sweeps `sock/` read-only and covers three classes:
  1. This root's sockets whose state is missing, does not load, records no supervisor, or records another launch.
  2. Machine-wide, sockets whose sidecar root no longer exists.
  3. (R5-I3) This root's sockets with a missing or unparsable sidecar, attributed through `sweep_for_in`.
- The `/proc` stray census is documented as 2.x-only. This covers `StrayLayer`, `discover_stray_devflow_processes`, `MONITOR_WRAPPER_MARKER`, `collect_stray_process_findings` and the `gate sweep --reap-strays` help. Two tests pin it: a unit test over real 3.x argv shapes, and a check in the status e2e against a **live** 3.x supervisor, with a planted 2.x wrapper the same census run must list.

## Task Commits

1. **Task 1 (tracer, TDD):** RED `3a42839` `test(50-12): add failing tests for probe-answered status liveness`; GREEN `fa3e462` `feat(50-12): answer status and cleanup liveness from the supervisor socket`
2. **Task 2 (TDD):** RED `df267bf` `test(50-12): add failing tests for doctor's supervisor check`; GREEN `b382044` `feat(50-12): report supervisor liveness in doctor and doctor --json`
3. **Task 3 (TDD):** RED `879bfbc` `test(50-12): add failing tests for the orphan sweep and 2.x census`; GREEN `99b056e` `feat(50-12): add doctor's read-only orphan supervisor sweep`

Tracer gate: auto mode off (`auto_advance`/`_auto_chain_active` false), `human_verify_mode` at its default `end-of-phase`, and the tracer's verify is automated only. I re-ran the tracer verify chain after `fa3e462` (exit 0) before expanding.

## TDD Gate Compliance

RED evidence came from a nextest JUnit report classified by `gsd-tools check tdd-red-evidence`. Records are under `target/red-evidence/`.

| Test | RED | Evidence | GREEN |
|------|-----|----------|-------|
| `commands::tests::liveness_matrix_covers_every_probe_outcome` | `3a42839` | exit 100, 1/1 failed, `RED_EVIDENCE_OK` | `fa3e462` |
| `status_reports_each_liveness_state_with_its_repair_verb` (e2e) | `3a42839` | exit 100, 1/1 failed, `RED_EVIDENCE_OK` | `fa3e462` |
| `…doctor_reconciliation::check_supervisor_reports_stale_and_gone_while_active` | `df267bf` | exit 100, `RED_EVIDENCE_OK` | `b382044` |
| `…doctor_reconciliation::doctor_json_reports_supervisor_liveness_without_paths` | `df267bf` | exit 100, `RED_EVIDENCE_OK` | `b382044` |
| `…doctor_reconciliation::check_supervisor_is_silent_when_alive_or_stopped` | `df267bf` | **INVALID_RED unexpected_green, by design**: it is the control | `b382044` |
| `…doctor_reconciliation::doctor_reports_an_orphan_socket_without_removing_it` | `879bfbc` | exit 100, `RED_EVIDENCE_OK` | `99b056e` |
| `…doctor_reconciliation::the_orphan_sweep_only_reads_and_attributes_through_sweep_for_in` | `879bfbc` | exit 100, `RED_EVIDENCE_OK` | `99b056e` |
| `agent::tests::the_stray_census_docs_say_it_is_2x_only` | `879bfbc` | exit 100, `RED_EVIDENCE_OK` | `99b056e` |
| `agent::tests::the_stray_census_matches_only_the_2x_process_shapes` | `879bfbc` | **INVALID_RED unexpected_green, expected**: the census is doc-only and its behaviour already holds | n/a |

**What each RED failure showed:**
- **Matrix:** failed on `rows[1]` (`NotLaunched` from the stub, `GoneWhileActive` expected).
- **e2e:** failed on `a live run reads ALIVE with its owner`. The old output showed `monitor_pid:` and `liveness: healthy`.
- **check_supervisor:** failed on `a STALE supervisor is a finding`.
- **JSON:** failed with `supervisor.liveness` Null.
- **Orphan test:** failed with `[]` against the five expected findings. The snapshot half passed, because the stub reads nothing.
- **Orphan scan:** failed with `no sweep_for_in( call`.
- **Census-docs scan:** failed with `the doc above pub enum StrayLayer does not name 2.x`.

None of these were compile, fixture or load faults. The RED commits for Tasks 1–3 hold compiling stubs (type plus a body that answers "nothing"), so the failures are assertion failures. Each GREEN commit removed its stubs.

`doctor_reports_an_unlistable_socket_dir_instead_of_nothing` is a Rule 2 addition. It was written with GREEN and has no RED. The mutation below is its discrimination evidence.

## Mutation and Discrimination Evidence

For each mutant: apply it, run the named test, restore the file from a saved copy, and confirm the restore with `cmp`. All restores matched.

| Mutant | Test | Result |
|---|---|---|
| STALE hints drop `stop` | status e2e | caught: `STALE names devflow stop --phase 68` |
| GONE always maps to stopped `Gone` | status e2e | caught: `GONE while active names devflow resume --phase 68` |
| `check_supervisor` fires on ALIVE | silent control | caught |
| `check_supervisor` fires on stopped GONE | silent control | caught |
| `ProbeFailed` carries `ProbeError::to_string()` | JSON path test | caught: the detail held `/home/someone/.cache/…/x.sock` |
| no sidecar-less branch | orphan test | caught: (e) and (f) missing |
| sidecar-less socket attributed without `sweep_for_in` | orphan test | caught: (g), another root's, listed |
| no deleted-root rule | orphan test | caught: (d) missing |
| superseded launch not reported | orphan test | caught: (b) missing |
| census matches `__monitor` as an advance child | census unit test | caught |
| same census mutant | status e2e (live 3.x supervisor) | caught: the census listed the supervisor pid as `AdvanceChild` |
| `socket_dir_unlistable` ignores PermissionDenied | unlistable test | caught |

These checks have their own controls:
- The orphan test's snapshot comparison has a removal control that must change the snapshot.
- The e2e census check has a planted 2.x wrapper that must be listed.
- Both scan tests run planted pass and fail controls through the same extractor.
- The `main.rs` presence check exits 1 against `HEAD` before the change and 0 after.

## Verification (commands run, actual exit codes)

| Command | Result |
|---|---|
| Task 1 gate (matrix test, then the status e2e) | exit 0, 1 passed each (re-run at `99b056e`: exit 0) |
| Task 2 gate (3 named doctor tests) | exit 0 (re-run at `99b056e`: exit 0) |
| Task 3 gate A (orphan test, `doctor_is_read_only_on_a_mismatched_project`, census test) | exit 0 |
| Task 3 gate B (orphan scan, census-docs scan, `main.rs` presence, `cargo nextest run -p devflow --bin devflow --no-tests=fail`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`) | exit 0. Bin: 425 passed (2 leaky), 2 skipped. The first attempt exited 101 on clippy `useless_vec` in the Task 1 test, which was fixed before the re-run. |
| `cargo nextest run -p devflow --test supervisor_e2e --no-tests=fail` | 4 passed |
| `cargo nextest run -p devflow --test doctor_antigravity --test phase7_cli --test start_lock_e2e --test stop_e2e` | 64 passed |
| `cargo nextest run -p devflow-core --lib --no-fail-fast` | 926 passed (1 leaky). An earlier run hit the flaky opencode test (see Needs awareness). |

**What these checks do not establish:**
- The orphan sweep's ALIVE case uses a test thread that speaks the ping protocol, not a real supervisor.
- I did not run `scripts/check.sh all` or the container parity check.
- `devflow-core` gained no `pub` change. Its `agent.rs` diff outside tests is `///` lines only (checked with `git diff -U0`). So I did not run the 50-01 semver gate, and `tools/semver-gate.sh` is untouched.

## Files Created/Modified

- `crates/devflow-cli/src/commands.rs`: `PhaseLiveness`, `phase_liveness`, `probe_error_summary`, `probe_phase`, `recovery_hints`; `status`, `cleanup`; `PhaseFacts`, `check_supervisor`, `supervisor_json`; `OrphanSupervisorFinding`, `collect_orphan_supervisors`, `OrphanReason`, `unclaimed_by_state`, `phase_in_socket_name`, `socket_dir_unlistable`, orphan renderers; `doctor`/`doctor_in`, `doctor_json_body`; the tests.
- `crates/devflow-cli/tests/supervisor_e2e.rs`: the status e2e, `PlantedWrapper`, `census_never_sees_a_live_supervisor`.
- `crates/devflow-core/src/agent.rs`: 2.x-only docs for the census; the census unit test and the docs scan.
- `crates/devflow-cli/src/main.rs`: the `gate sweep --reap-strays` help names 2.x and `devflow stop --phase N --launch <id>`.

## Decisions Made

See `key-decisions`. One more: `BetweenStages` folds into ALIVE, as the plan recorded. An ALIVE supervisor with a dead agent is not a finding. That case is pinned by `reconcile_is_silent_when_supervisor_alive_and_agent_dead`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] `collect_orphan_supervisors` returns `Vec<OrphanSupervisorFinding>`, not `Vec<PhaseFinding>`**
- **Found during:** Task 3
- **Issue:** `PhaseFinding` cannot carry the `launch_id`, `liveness` and `root_exists` that the plan's `orphan_supervisors` JSON requires.
- **Fix:** `OrphanSupervisorFinding { finding: PhaseFinding, launch_id, liveness, root_exists }`. The finding inside keeps the plan's path-free detail and repair.
- **Commit:** `879bfbc` (type) / `99b056e`

**2. [Rule 2 - Missing critical] An unlistable socket directory is reported, not shown as nothing**
- **Found during:** Task 3 (carried-forward item from 50-06)
- **Issue:** `sweep_in` returns `[]` for a `sock/` directory it cannot list, so doctor would report "no orphans" when it never looked.
- **Fix:** `socket_dir_unlistable` adds a text warn line and a new top-level JSON key, `orphan_supervisor_sweep_error`. `doctor --json` now has six top-level keys. Three existing key-count tests were updated from 4 to 6. The launch-guard side stays with 50-15.
- **Verification:** `doctor_reports_an_unlistable_socket_dir_instead_of_nothing` plus a mutant.
- **Commit:** `99b056e`

**3. [Rule 3 - Hermeticity] `doctor` split into `doctor`/`doctor_in(cache_dir)`**
- **Issue:** The in-process unit tests that call `doctor()` would have swept, and pinged, the operator's real `~/.cache/devflow/sock`.
- **Fix:** `doctor_is_read_only_on_a_mismatched_project` and `doctor_finds_a_real_stray_and_never_signals_it_across_two_runs` now call `doctor_in` with a tempdir. The read-only test also plants a STALE orphan socket and sidecar, and asserts both survive two doctor runs.
- **Commit:** `99b056e`

**4. [Plan text conflict] doctor's STALE repair**
- **Issue:** The `<behavior>` says the STALE repair is `devflow stop --phase N`. The must-have truth says STALE names stop **then** resume.
- **Fix:** I followed the truth. The repair is `devflow stop --phase N && devflow resume --phase N`. The `&&` also means resume does not run if stop refuses.
- **Commit:** `b382044`

**5. [Unspecified by plan] What a failed probe does**
- doctor's repair for ProbeFailed is `devflow status`, which prints the operator's own socket path, since doctor must not.
- `cleanup` keeps a ProbeFailed run's worktree unless `--force`, the same as STALE (fail-closed with an override).
- **Commit:** `fa3e462` / `b382044`

**6. [Plan context not applied] `supervisor::backstop::assess` is not used for the STALE display**
- **Reason:** The plan's `PhaseLiveness::Stale` carries no payload. `BackstopDecision::describe`'s texts are written for `stop`: they say files "are removed" and the group "is ended", which would misstate what happened in a read-only command. The decision stays with 50-14 (stop) and 50-15 (guard).

**7. [Sequencing] `check_dead_monitor` was inlined between Tasks 1 and 2**
- **Issue:** Task 1's acceptance removes `fn liveness`, which `check_dead_monitor` still called.
- **Fix:** Its old Stuck condition was inlined (monitor recorded and not alive) until Task 2 replaced the function.
- **Commit:** `fa3e462`

**8. [Rule 1 - Lint] `useless_vec` in the Task 1 matrix test**
- **Issue:** Found by Task 3's clippy gate.
- **Fix:** Fixed in `99b056e`. See Needs awareness for what this means for a bisect.

**9. [Test additions / rewrites]**
- **Added:**
  - `recovery_hints_names_stop_then_resume_for_stale`
  - `recovery_hints_empty_for_alive_stopped_and_unlaunched`
  - fixture (i), a sidecar-ful socket that its state claims, as a control
  - the e2e census check with its planted 2.x control
  - the unlistable-dir test
- **Removed:** `liveness_unknown_when_no_monitor_recorded` and `liveness_matrix_covers_all_four_rows`, since the functions they tested were deleted. The new matrix test supersedes them.
- **Rewritten:** six `doctor_reconciliation` tests and the `recovery_hints_*` tests, moved from `monitor_pid`/`Liveness::Stuck` to `PhaseLiveness`.

**10. [Hygiene] Commit `fa3e462`'s subject is 74 characters**
- The `commit-msg` hook only warned; the limit is 72. I did not amend it, as instructed.

**Total deviations:** 10 (2 Rule 2, 1 Rule 3, 1 Rule 1, 6 plan-shape or sequencing). **Impact:** no scope creep beyond the read-only reporting the carried-forward items asked for.

## Issues Encountered

- `doctor_is_read_only_on_a_mismatched_project` takes about 9 s. Most of that is `doctor_checks()` probing tools, not the sweep.

## Known Stubs

None. Every RED stub was removed in its GREEN commit; `rg 'RED stub' crates/` finds nothing.

## Threat Flags

None new. Both new outputs are read-only and path-free:
- the doctor sweep finding (T-50-31, T-50-32, T-50-51, T-50-84);
- `orphan_supervisor_sweep_error`, which holds an `io::ErrorKind` string, never a path.

## Cleanup

- **Processes:** every e2e and mutant run was followed by a `ps` check for `__monitor`, `sleep 120` and `sleep 30`. None survived. A known leaky core test left one `sleep 30`. I confirmed its `CARGO_MANIFEST_DIR` was this worktree and killed it by its recorded PID (3120472), then checked it was gone.
- **Files:** no `/tmp/dfs*`, `/tmp/dfh*` or `/tmp/dfu*` directories remain.
- **Kept on purpose:**
  - the scratch directory `exec-50-12/`, which holds the RED script, logs and mutant backups, removed at hand-off;
  - the RED evidence records under `target/red-evidence/`, inside the gitignored build directory.

## User Setup Required

None.

## Next Phase Readiness

- 50-14: `stop --phase N --launch <id>` is now the repair doctor prints for every orphan. That includes sidecar-less sockets, resolved by file name, and deleted-root sockets.
- 50-15: the launch guard owns the unlistable-`sock/` case on its side.
- 50-17 (OPERATIONS): document the new `status` line (`supervisor: <socket> — ALIVE|STALE|GONE`), doctor's "supervisor sockets no state claims" section, and the two new `doctor --json` keys. `OPERATIONS.md` still describes `liveness`/`monitor_pid` wording.
- 50-22: `registry_reachable_pids` is unchanged here.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- Commits `3a42839`, `fa3e462`, `df267bf`, `b382044`, `879bfbc` and `99b056e` are all ancestors of HEAD (`git merge-base --is-ancestor`).
- The four modified files exist.
- `commands.rs` has `fn phase_liveness`, `fn probe_phase`, `fn check_supervisor`, `fn collect_orphan_supervisors`, `supervisor::probe`, `sweep_in` and `--launch`.
- `commands.rs` no longer has `fn liveness(`, `enum Liveness` or `fn check_dead_monitor`.
- `PhaseFacts` has no `monitor_pid`/`monitor_alive`.
- The `status` and `cleanup` bodies read no `monitor_pid` once comments are stripped.
