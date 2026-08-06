---
phase: 23-end-to-end-dogfood
plan: 04
subsystem: infra
tags: [rust, cli, gates, process-lifecycle, observability]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood (23-03)
    provides: "registry::load_roots/prune_missing — the machine-global root enumeration this plan's sweep fans Gates::list_open across"
provides:
  - "devflow_core::gates::Gates::reap — a rejection-only gate response primitive, structurally incapable of approving (no bool parameter)"
  - "devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH] — on-demand aged-gate teardown across every registered root"
  - "config_parse::gate_max_unattended_age_secs / DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS — an independent, fail-safe threshold knob (default 6h) decoupled from gate_timeout_secs' 7-day default"
  - "gate_reaped event — the sweep-side audit record for every reaped gate, alongside the target process's own gate_resolved"
affects: [23-05 (devflow stop), 23-11 (acceptance run) — a phase abandoned mid-run now has a non-kill(1) teardown path]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Rejection-only mutation primitive (Gates::reap): no boolean parameter, approved hard-coded false at the literal — the type signature itself is the security control (T-23-41), not a runtime check."
    - "Independent fail-safe threshold knobs sharing the same fallback direction: an unparsable value AND an explicit zero both resolve to the safe default, never to 'act on everything.'"
    - "devflow-cli has no lib.rs — every integration test under crates/devflow-cli/tests/ must drive the compiled binary via Command::new(env!(\"CARGO_BIN_EXE_devflow\")); pub(crate) items are unreachable from tests/, confirmed against this plan's own gate_sweep."

key-files:
  created:
    - crates/devflow-cli/tests/gate_sweep_e2e.rs
  modified:
    - crates/devflow-core/src/gates.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/config_parse.rs
    - OPERATIONS.md

key-decisions:
  - "commands::gate_sweep and gate_sweep_e2e.rs drive the compiled devflow binary via Command::new rather than an in-process call — devflow-cli is a binary-only crate (no lib.rs), so integration tests structurally cannot link against pub(crate) items. The plan's action text (\"run the sweep's reaping logic against that root\") was ambiguous on this point; resolved per the crate's hard structural constraint, matching the existing phase7_cli.rs/release_check.rs pattern."
  - "Task 1 shipped gate_sweep with an inline 6-hour threshold constant rather than the plan-specified config_parse::gate_max_unattended_age_secs() — that function is Task 2's own deliverable and did not exist yet when Task 1 needed to compile. Task 2 then replaced the inline constant with the real, configurable, TDD'd function. Recorded as a Rule 3 (blocking) fix to a genuine plan-ordering defect, not a scope change."
  - "Task 3's real-child test reaps the target via Gates::reap called directly (not by shelling out to devflow gate sweep a second time) — the CLI-subprocess sweep path is already proven end-to-end by Task 1's tests; Task 3's job is to prove the OTHER end (a real, separate advance process consuming that same write), so it exercises the identical primitive without re-spawning a second child."

requirements-completed: [23b]

coverage:
  - id: D1
    description: "Gates::reap — structurally incapable of approving (no bool parameter, approved hard-coded false at the literal); resolves to GateAction::Abort via the caller-supplied abort-keyword note"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs — source assertions: exactly one `pub fn reap` match, `approved: false` within 12 lines, no `bool` in the signature line (cargo test -p devflow-core --features test-support gates::)"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_sweep_e2e.rs — sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort"
        status: pass
    human_judgment: false
  - id: D2
    description: "devflow gate sweep never signals any process — mechanically confirmed by a negative grep for signalling call syntax in the production command file and the e2e test file"
    requirement: "23b"
    verification:
      - kind: other
        ref: "rg -c 'libc::kill|signal::kill|\\.kill\\(|nix::sys::signal' crates/devflow-cli/src/commands.rs crates/devflow-cli/tests/gate_sweep_e2e.rs — both return 0 matches"
        status: pass
    human_judgment: false
  - id: D3
    description: "An aged gate is answered with a rejection and a live Gates::poll_response consumer (thread) observes it and resolves to Abort; a fresh gate is left completely untouched"
    requirement: "23b"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_sweep_e2e.rs — sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort, sweep_leaves_a_fresh_gate_untouched"
        status: pass
    human_judgment: false
  - id: D4
    description: "A real, separate devflow advance process parked on a gate is torn down by the sweep, unwinds through its own abort() path, releases its per-phase lock, and leaves a workflow_aborted audit event"
    requirement: "23b"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_sweep_e2e.rs — sweep_ends_a_real_advance_process_through_its_own_abort_path (measured wall-clock: ~1.1s)"
        status: pass
    human_judgment: false
  - id: D5
    description: "DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS threshold, fail-safe on both an unparsable value and an explicit zero (falls back to the 6h default, never to zero/reap-everything)"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/config_parse.rs — max_unattended_age_defaults_when_absent, max_unattended_age_parses_explicit_value, max_unattended_age_defaults_on_unparsable, max_unattended_age_defaults_on_explicit_zero"
        status: pass
    human_judgment: false
  - id: D6
    description: "--dry-run computes and prints every decision but writes nothing; a gate that already has a response is a benign skipped race, never an error, and is never clobbered"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs — gate_sweep_dry_run_does_not_write_a_response, gate_sweep_skips_already_responded_gate_without_clobbering"
        status: pass
    human_judgment: false
  - id: D7
    description: "Every successful reap emits a gate_reaped audit event in the sweep's own process, independent of run_gate_with_timeout's gate_resolved in the target process"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs — gate_sweep_emits_gate_reaped_event_on_reap"
        status: pass
    human_judgment: false
  - id: D8
    description: "A machine with no registered roots, no cache directory, or no gates directory is a clean no-op, not an error (backstop truth)"
    requirement: "23b"
    verification:
      - kind: manual_procedural
        ref: "DEVFLOW_CACHE_DIR pointed at a directory that does not exist; `devflow gate sweep` and `devflow gate sweep --dry-run` both printed a zero-count summary and exited 0"
        status: pass
    human_judgment: false

# Metrics
duration: 14min
completed: 2026-07-25
status: complete
---

# Phase 23 Plan 04: Bound Gate Lifetime — `devflow gate sweep` Summary

**`devflow gate sweep` and `Gates::reap` — a rejection-only, structurally-incapable-of-approving primitive (no bool parameter) that ends an abandoned gate through the target process's own existing abort path, proven against both a live-poller thread and a real, separate `devflow advance` child process — with no `kill(1)` and no supervisor.**

## Performance

- **Duration:** 14 min
- **Started:** 2026-07-25T22:05:21-04:00 (Task 1 commit)
- **Completed:** 2026-07-25T22:19:25-04:00 (Task 3 commit)
- **Tasks:** 3 (Task 1 tracer, Task 2 TDD, Task 3 auto)
- **Files modified:** 6 (1 created, 5 modified)

## Accomplishments

- `Gates::reap(project_root, phase, stage, note, responded_by) -> Result<PathBuf, GateError>`: takes **no** boolean parameter, hard-codes `approved: false` at the literal, delegates to the already-tested `Gates::respond`. T-23-41's mitigation is structural (the type signature), not conventional.
- `devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH]`: fans `registry::load_roots()` (or one `--root`) across `Gates::list_open`, reaps any gate whose age exceeds the threshold, prints a per-gate decision line and a closing reaped/skipped/left-alone summary, and exits 0 regardless of how many gates it touched.
- `config_parse::gate_max_unattended_age_secs` / `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS`: independent of and far shorter than `gate_timeout_secs`'s 7-day default (6h vs 7d); both an unparsable value and an explicit `0` fall back to the 6h default — the fail-safe direction that keeps a typo from reaping every gate on the machine.
- `gate_reaped` event emitted on every successful reap, alongside `run_gate_with_timeout`'s own `gate_resolved` in the target process — two independent, timestamped, attributed audit records per reaped gate (T-23-43).
- `NoOpenGate`/`AlreadyResponded` treated as benign races (a human or `--yes-ship` may have answered first) — counted as skipped, never returned as an error.
- `crates/devflow-cli/tests/gate_sweep_e2e.rs` (4 tests, new file): an aged gate reaped by the real `devflow` binary while a live `Gates::poll_response` thread observes the response and resolves to `Abort`; a fresh gate left untouched; `--help` documents the new flags; and the plan's strongest proof — a real, separate `devflow advance` process parked on a Code gate is torn down by a file write, exits 0, releases its `.devflow/lock-95` (proving `LockGuard::Drop` ran — a clean unwind, not an external kill), and leaves a `workflow_aborted` event. Measured wall-clock: **~1.1s** (all 4 tests together, on this machine) — comfortably inside both the 15s child gate-timeout backstop and the 90s outer test-patience default; recorded here since no prior run had measured it.

## Task Commits

1. **Task 1: End-to-end "an abandoned gate ends itself" — one aged gate, one sweep, one clean teardown** - `73450ac` (feat)
2. **Task 2: Threshold configuration, dry-run, audit trail, and error tolerance** (TDD, combined RED+GREEN in one commit since the RED-phase tests were added directly against the not-yet-existing `parse_gate_max_unattended_age` and confirmed as a genuine compile-failure before the implementation landed) - `2c20f29` (feat)
3. **Task 3: Prove it against a real child process, not just a thread** - `99cf682` (test)

**Plan metadata:** (this SUMMARY's commit, made by the parallel-worktree orchestrator after merge — not created by this executor per the worktree protocol)

## Files Created/Modified

- `crates/devflow-core/src/gates.rs` - `Gates::reap`: the sweep's only write path, structurally incapable of approving
- `crates/devflow-cli/src/main.rs` - `GateCmd::Sweep` (`--max-age-secs`, `--dry-run`, `--root`), dispatched to `commands::gate_sweep`
- `crates/devflow-cli/src/commands.rs` - `gate_sweep`: threshold resolution, root fan-out, dry-run, `gate_reaped` event emission, skip-counted error tolerance, closing summary (6 new unit tests total across Tasks 1–2)
- `crates/devflow-cli/src/config_parse.rs` - `parse_gate_max_unattended_age` / `gate_max_unattended_age_secs`: pure/wrapper split matching the file's existing `gate_timeout_secs` convention (4 new unit tests)
- `crates/devflow-cli/tests/gate_sweep_e2e.rs` - New file: 4 end-to-end tests, the strongest of which spawns a real `devflow advance` child process
- `OPERATIONS.md` - Documented `devflow gate sweep`, `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS`, and the test-only `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` (Rule 2 auto-fixes, required by `doc_check::source_devflow_env_vars_and_subcommands_are_documented`)

## Decisions Made

- **Integration tests must drive the compiled binary, not call `commands::gate_sweep` in-process:** `devflow-cli` has no `lib.rs` — it is a pure binary crate — so `crates/devflow-cli/tests/*.rs` compile as separate crates with no way to reach `pub(crate)` items at all, confirmed by inspecting the crate layout and the existing `phase7_cli.rs`/`release_check.rs`/`help_snapshot.rs` tests, every one of which already spawns `env!("CARGO_BIN_EXE_devflow")`. The plan's action text ("run the sweep's reaping logic against that root") did not specify this, so it was resolved per the crate's hard structural constraint rather than guessed at.
- **Task 1 shipped a placeholder threshold, Task 2 replaced it:** the plan's own Task 1 action text calls `config_parse::gate_max_unattended_age_secs()` while noting "(added in Task 2)" — i.e., it asks Task 1 to reference a function that does not exist until Task 2. Task 1 instead used an inline `const SIX_HOURS` default (documented in the function's doc comment as a Task-1 placeholder), and Task 2 replaced it with the real, TDD'd, configurable function. This was necessary for Task 1 to compile at all.
- **Test names renamed to avoid colliding with the plan's own literal grep acceptance criteria** (23-03 precedent, `dereg_*`): the four new `config_parse.rs` tests were named `max_unattended_age_*` rather than `parse_gate_max_unattended_age_*`, since the latter would make `rg -n 'fn parse_gate_max_unattended_age'` match more than the one function definition the criterion requires.
- **The literal env var token `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` was deliberately kept out of `parse_gate_max_unattended_age`'s own doc comment** (only the wrapper's `std::env::var(...)` call names it) — the plan's acceptance criterion requires the token to appear on exactly one line of the file.
- **Task 3's real-child test reaps via `Gates::reap` directly, not via a second `devflow gate sweep` subprocess:** the CLI-subprocess sweep path is already proven end-to-end by Task 1's tests. Task 3's distinguishing job is proving a real, separate `advance` process consumes that exact write and tears itself down — so it calls the identical low-level primitive without re-testing the CLI wrapper a second time.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 1's action text forward-referenced Task 2's not-yet-existing `config_parse::gate_max_unattended_age_secs()`**
- **Found during:** Task 1, writing `commands::gate_sweep`
- **Issue:** The plan's Task 1 action explicitly instructs calling `config_parse::gate_max_unattended_age_secs()`, annotated "(added in Task 2)" — that function genuinely does not exist until Task 2 runs, so a literal reading of Task 1 would not compile.
- **Fix:** Task 1 used an inline `const SIX_HOURS: u64 = 6 * 60 * 60;` default with a doc comment stating it is a Task-1 placeholder; Task 2 then replaced it with the real function per its own TDD cycle.
- **Files modified:** crates/devflow-cli/src/commands.rs (both tasks)
- **Verification:** Task 1 compiled and its own `<verify>` passed standalone; Task 2 then wired in the real threshold and re-verified.
- **Committed in:** `73450ac` (Task 1), `2c20f29` (Task 2)

**2. [Rule 3 - Blocking] The plan's action text was ambiguous about how `gate_sweep_e2e.rs` invokes the sweep — resolved by the crate's structural constraint**
- **Found during:** Task 1, designing the e2e test
- **Issue:** `devflow-cli` has no `lib.rs`; `commands::gate_sweep` is `pub(crate)`. An integration test under `crates/devflow-cli/tests/` cannot call it directly under any circumstance — there is no library target to link against.
- **Fix:** Every e2e test spawns the compiled `devflow` binary via `Command::new(env!("CARGO_BIN_EXE_devflow"))`, matching the existing pattern in `phase7_cli.rs`/`release_check.rs`.
- **Files modified:** crates/devflow-cli/tests/gate_sweep_e2e.rs
- **Verification:** All 4 tests pass; `Command::new` spawns confirmed via the plan's own acceptance grep.
- **Committed in:** `73450ac`, `99cf682`

**3. [Rule 2 - Missing Critical] Documented `devflow gate sweep`, `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS`, and `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` in OPERATIONS.md**
- **Found during:** Task 2 and Task 3, running the full workspace test suite
- **Issue:** `crates/devflow-core/src/doc_check.rs`'s `source_devflow_env_vars_and_subcommands_are_documented` test asserts every source-read `DEVFLOW_*` env var (across **every** `.rs` file under `crates/`, including test files — not just production sources) is documented in scoped operator docs. `config_parse::gate_max_unattended_age_secs` reads `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` and `gate_sweep_e2e.rs`'s test-only `e2e_child_timeout()` reads `DEVFLOW_E2E_CHILD_TIMEOUT_SECS`; neither was in the plan's action text as a doc-update step.
- **Fix:** Added rows to `OPERATIONS.md`'s commands table (`devflow gate sweep`) and environment-variables table (both env vars, the test-only one marked explicitly as not read by production code).
- **Files modified:** OPERATIONS.md
- **Verification:** `cargo test -p devflow-core --features test-support doc_check::source_devflow_env_vars_and_subcommands_are_documented` — 1 passed, each time
- **Committed in:** `2c20f29` (first row), `99cf682` (second row)

**4. [Rule 3 - Blocking] Two doc comments in `gate_sweep_e2e.rs` accidentally matched their own acceptance-criteria greps**
- **Found during:** Task 2 (`set_var`) and Task 3 (`.kill(`), running the plan's own acceptance grep commands before committing
- **Issue:** A doc comment explaining "never `std::env::set_var`" and two comments explaining "no `.kill()` anywhere in this file" literally contain the substrings the negative greps (`rg -c 'set_var'`, `rg -c '...\.kill\(...'`) search for — self-referential false positives on the file's own explanatory prose.
- **Fix:** Reworded all three comments to describe the same constraint without containing the literal matched substring (e.g., "no process-termination call appears anywhere in this file").
- **Files modified:** crates/devflow-cli/tests/gate_sweep_e2e.rs
- **Verification:** `rg -c 'set_var'` and `rg -c 'libc::kill|signal::kill|\.kill\(|nix::sys::signal'` both return 0 (no match) against the final file
- **Committed in:** `99cf682`

---

**Total deviations:** 4 auto-fixed (2 blocking plan-ordering/ambiguity issues, 1 missing critical doc requirement hit twice, 1 blocking self-referential grep collision)
**Impact on plan:** All four were necessary to make the plan's own verification gates pass or to resolve a genuine forward-reference in the plan's task ordering; none changed the plan's design, added out-of-scope functionality, or weakened any threat-model mitigation.

## Issues Encountered

- The plan's literal Task 1/2 `<verify>` line for the devflow-core gate suite (`cargo test -p devflow-core gates::`) fails to compile without `--features test-support` — a pre-existing workspace quirk already documented in 23-03-SUMMARY.md's Decisions Made (integration tests reference `devflow_core::test_support`, which is feature-gated and only unified into the build by a `--workspace`-scope or `--features test-support` invocation). Every scoped verify command in this plan's execution used `--features test-support`; `cargo test --workspace` (matching CI) was used as the authoritative full-suite gate at every task boundary and is unaffected.
- None otherwise — no test flakes, no manual intervention beyond the deviations recorded above.

## Known Stubs

None.

## Threat Flags

None — every new surface (the `gate_sweep` command, `Gates::reap`, the `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` env var, the machine-wide fan-out over `registry::load_roots()`) is already covered by this plan's own `<threat_model>` (T-23-41 through T-23-47, T-23-SC), all `mitigate`d and verified: `rg -n 'pub fn reap'` matches exactly one line with `approved: false` in the following 12; `rg -c 'libc::kill|signal::kill|\.kill\(|nix::sys::signal'` returns 0 against both `commands.rs` and `gate_sweep_e2e.rs`; both an unparsable and an explicit-zero threshold fall back to the 6h default; every reap emits `gate_reaped`; `Gates::respond`'s existing refusal to clobber an unconsumed response is the accepted (not newly-coordinated) resolution for T-23-44; the sweep ships on-demand only, no scheduler integration (T-23-46).

## Manual Smoke Verification

Per the plan's `<verification>` section: with `DEVFLOW_CACHE_DIR` pointed at a directory that does not exist, `devflow gate sweep` and `devflow gate sweep --dry-run` both printed a zero-count summary (`sweep complete: 0 reaped, 0 skipped, 0 left alone` / `sweep complete (dry run): 0 would be reaped, 0 skipped, 0 left alone`) and exited 0 — the `must_haves.truths` backstop verification (a machine with no registered roots, no cache directory, or no gates directory is a clean no-op) confirmed live.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `devflow gate sweep` is a complete, independently-usable remedy for the orphan class `23-ORPHAN-FORENSICS.md` documented — an operator (or a future scheduler, deliberately out of scope this phase per Open Question 3) can now end an abandoned gate without `ps`/`kill` archaeology.
- `Gates::reap` is available as a public primitive for any future caller that needs a rejection-only gate response (e.g., 23-05's `devflow stop`, if it needs to answer a gate rather than just kill a monitor).
- No blockers. Full workspace suite green at every task boundary — final count 572 tests across the workspace (354 devflow-core lib, 168 devflow-cli lib, remainder integration), 0 failed, `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` all green as a single chain.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-25*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/gates.rs
- FOUND: crates/devflow-cli/src/main.rs
- FOUND: crates/devflow-cli/src/commands.rs
- FOUND: crates/devflow-cli/src/config_parse.rs
- FOUND: crates/devflow-cli/tests/gate_sweep_e2e.rs
- FOUND: OPERATIONS.md
- FOUND: .planning/phases/23-end-to-end-dogfood/23-04-SUMMARY.md
- FOUND: commit 73450ac (Task 1 feat)
- FOUND: commit 2c20f29 (Task 2 feat)
- FOUND: commit 99cf682 (Task 3 test)
- FOUND: commit 6c8facc (this SUMMARY.md)
