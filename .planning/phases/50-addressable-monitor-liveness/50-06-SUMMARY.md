---
phase: 50-addressable-monitor-liveness
plan: 06
subsystem: supervisor
tags: [rust, unix-socket, sidecar, sweep, shutdown-client, state]

requires:
  - phase: 50-05
    provides: "SocketPath (+ for_launch, sidecar_path, part_path), publish, probe, Liveness, ProbeError, protocol::{read_line, IO_TIMEOUT, Request, ShutdownStopped}, registry::{socket_dir_in, path_digest}"
  - phase: 50-01
    provides: "semver-gate.sh (run here as an extra check: --through baseline matches)"
provides:
  - "devflow_core::supervisor: Sidecar, SidecarError, read_sidecar, write_sidecar (crate), SweepEntry (sidecar_error, launch_id_in_name), sweep_in, sweep_for_in (RV-05 name fallback), remove_launch_files, ShutdownOutcome {Stopped, HandedOff, Refused}, request_shutdown"
  - "crate-internal: supervisor::{rfc3339_from_unix, rfc3339_now, own_socket_at, socket_name_parts}; ship::civil_from_days widened to pub(crate)"
  - "devflow_core::state: SupervisorRecord (+ new), State::supervisor"
affects: [50-07, 50-09, 50-11, 50-13, 50-15, 50-16, 50-23]

actuals:
  tokens: 12310
  tasks: 3
  commits: 6
plan_head_before: 92d1b86eb199af97c9116525332c18b02210ca5e
plan_head_after: a0be8bad0f6ddd7268a1114dee07b850924988f7

tech-stack:
  added: []
  patterns:
    - "Sidecar write: create_new temp <stem>.<pid>-<n>.tmp with mode 0600 in a validated 0700 sock/, write + fsync, chmod 0600, rename onto <stem>.json; temp removed on any failure"
    - "Sweep attribution order: a parsed sidecar decides; else a scheme-fitting name (digest + padded phase); else, for a name outside the scheme, an Alive ping reply's phase (root unknowable, so it may belong)"
    - "Shutdown client: one BufReader across the accepted and final lines; only an ok+stopped final line is Stopped"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/supervisor.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/ship.rs

key-decisions:
  - "Sidecar.created_at is RFC 3339 UTC from a new rfc3339_from_unix built on ship::civil_from_days: the events module the plan named writes unix seconds and has no RFC 3339 helper"
  - "write_sidecar runs publish's ensure_socket_dir first (create 0700, refuse a symlinked, non-directory or foreign-owned sock/), so a sidecar written before publish still lands in a private directory (T-50-13)"
  - "request_shutdown on a path with no file is ProbeError::Stat(NotFound): its return type has no Gone, and a missing socket must never read as a stop"
  - "handed_off is recognised on either line (instead of accepted, or after it); a stopped line before accepted, or a second accepted line, is an error"
  - "probe's lstat/socket/uid checks moved into a private own_socket_at shared with request_shutdown; probe's behaviour is unchanged (all 50-05 probe tests re-run green)"

patterns-established:
  - "Read-only vs acting split in supervisor: sweep_in, sweep_for_in, probe, read_sidecar never remove or signal; remove_launch_files is the one acting function"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "A live supervisor is found by project root and phase with no state file: sidecar written 0600 beside the socket, sweep_for_in returns exactly that Alive entry and nothing for another phase or root"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::a_published_socket_is_found_by_root_and_phase_without_state"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::sidecar_round_trips_and_is_owner_only"
        status: pass
    human_judgment: false
  - id: D2
    description: "RV-05 / V-36: a socket whose sidecar is broken or missing is still found by its name's digest and phase (missing told apart from broken), a name outside the scheme by its ping phase, and a parsed sidecar wins over the name"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::a_socket_whose_sidecar_will_not_parse_is_still_found_by_its_name"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::socket_name_parts_accepts_only_the_launch_scheme"
        status: pass
    human_judgment: false
  - id: D3
    description: "V-22: sweep_in reports Alive, Stale and Gone and leaves every name, length and mtime under sock/ unchanged, sends only pings, and the live listener still answers; control: remove_launch_files changes exactly the stale .sock and .json"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::sweep_reports_without_removing_or_signalling"
        status: pass
    human_judgment: false
  - id: D4
    description: "request_shutdown reports Stopped only after the final stopped line; handed_off is HandedOff, other refusals Refused; close, timeout or malformed final line is an error"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::request_shutdown_reads_accepted_then_stopped"
        status: pass
    human_judgment: false
  - id: D5
    description: "State::supervisor: a saved SupervisorRecord reloads equal through save_state/load_state; a state file without the key loads with None"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::supervisor_record_round_trips_and_absent_reads_as_none"
        status: pass
    human_judgment: false

duration: 16min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 06: Sidecar, State-less Sweep, Shutdown Client and State Handle Summary

**Each socket now has a 0600 JSON sidecar, and a read-only `sweep_for_in(cache, root, phase)` finds a live supervisor with no state file. When the sidecar is broken or missing it falls back to the socket's name, or to the ping reply's phase. `remove_launch_files` is the one acting function. `request_shutdown` reports `Stopped` only after the final stopped line. `State` gains a persisted `supervisor: Option<SupervisorRecord>`.**

## Performance

- **Duration:** 16 min
- **Started:** 2026-10-17T16:54:34Z
- **Completed:** 2026-10-17T17:10:24Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- `write_sidecar` / `read_sidecar`: an atomic temp + rename write at mode 0600 into a validated 0700 `sock/`. A truncated file is `SidecarError::Json` and a missing one is `SidecarError::Io(NotFound)`.
- `sweep_in`: one entry per `.sock`/`.json` stem, sorted by socket path, with `.part`/`.tmp` names skipped. Each present socket is probed. A sidecar with no socket reads `Ok(Gone)` with `socket_present: false`. A missing or unlistable `sock/` returns an empty Vec.
- `sweep_for_in`: a parsed sidecar decides. Without one, the stem `<016x digest>-<padded phase>-<launch id>` is matched on digest and phase. A stem outside that scheme matches any root when the socket is Alive and its reply names the phase. `SweepEntry.sidecar_error` separates a broken sidecar from a missing one, and `launch_id_in_name` carries the name's launch id.
- `request_shutdown(path, final_timeout)`: the same pre-connect checks as `probe`, then `{"op":"shutdown"}`, then the accepted line within `IO_TIMEOUT` and the final line within `final_timeout`.
- `SupervisorRecord::new(socket_path, launch_id, agent_pgid, agent_start_time, boot_id)` and `#[serde(default)] State::supervisor`. `monitor_pid` stays until 50-16.

## Task Commits

1. **Task 1 (tracer): sidecar and sweep by root and phase.** RED `adc7724` test(50-06), GREEN `85257d1` feat(50-06)
2. **Task 2: read-only sweep (V-22), remove_launch_files, shutdown client.** RED `2cdabd9` test(50-06), GREEN `afb8c68` feat(50-06)
3. **Task 3: SupervisorRecord and State::supervisor.** RED `629cbed` test(50-06), GREEN `a0be8ba` feat(50-06)

No REFACTOR commits were needed.

**Tracer feedback gate:** the run was interactive (`auto_advance=false`, `_auto_chain_active=false`), `human_verify_mode` was at its default `end-of-phase`, and Task 1's `<verify>` is automated only, so row 3 applied. Task 1's gate was re-run verbatim at `85257d1`: rc 0, and each test reported `1 test run: 1 passed`. Expansion followed.

## TDD Gate Compliance

| Task | RED | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------------------------------------------------------------------|-------|
| 1 | `adc7724` | exit 100, 3 run / 3 failed; `RED_EVIDENCE_OK target_test_failed` for each of the three targets | `85257d1` |
| 2 | `2cdabd9` | exit 100, 2 run / 2 failed; `RED_EVIDENCE_OK target_test_failed` for both | `afb8c68` |
| 3 | `629cbed` | exit 100, 1 run / 1 failed; `RED_EVIDENCE_OK target_test_failed` | `a0be8ba` |

**Semantic assessment.** Every target compiled and ran, and each one failed on a planned assertion. None failed on a compile, load or fixture fault. The stubs returned empty values or no-ops rather than panicking.
- Task 1, tracer: `exactly one entry for (root, 7): []` (`sweep_for_in` stub returned `[]`).
- Task 1, round trip: `read_sidecar returns what was written, got Err(Io(RED stub))`.
- Task 1, V-36: `a broken sidecar is found by name`, left `[]`.
- Task 2, V-22: failed at its control, `the snapshot detects a removal`, because `remove_launch_files` was a no-op. Its read-only half already passed at RED, since `sweep_in` was built in Task 1. The test's RED therefore proves the control discriminates, not that the sweep was once mutating.
- Task 2, shutdown: `accepted then stopped is Stopped, got Err(NoReply("RED stub"))`.
- Task 3: `the supervisor record must survive a save and load`, left `None`. The field was `#[serde(skip)]` at RED.

## Discrimination (hand mutations; source restored byte-identical with `cmp` after each set)

Caught (test exit 100):
- No name fallback: caught by the V-36 other-root control.
- Foreign fallback ignoring the phase: caught by the phase-8 control.
- Digest check dropped: caught by the other-root control.
- A parsed sidecar not winning: caught by assertion (4).
- `.mode(0o644)` has no effect: the explicit chmod to 0600 still runs (see below).
- `request_shutdown` returning Stopped without reading the final line: the scripted server's write of the stopped line fails because the client has hung up.
- EOF read as stopped: caught by the "close after accepted" assertion.
- A final line accepted without `stopped:true`: caught by the malformed-line assertion.
- `handed_off` mapped to Refused: caught by the hand-off assertion.
- `remove_launch_files` removing only the `.sock`: caught.
- NotFound treated as an error: caught by "already gone succeeds".
- `sweep_in` deleting `.part` files: caught by the V-22 snapshot.

Survived (by design, not fixed):
- Reporting a sidecar that disappears between listing and read as broken. That branch only runs in a listing/read race, which no test produces.
- Changing the create-time `.mode(0o600)`. The explicit `set_permissions(0o600)` after the write makes it redundant. Both are kept so the temp is never group- or world-readable, even briefly.

## Verification (commands run, actual exit codes)

| Gate | Result |
|------|--------|
| Task 1 `<automated>` (3 named tests), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 2 `<automated>` (V-22, shutdown), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 3 `<automated>` named test, verbatim | rc 0, `1 test run: 1 passed` |
| Task 3 second gate, verbatim: core lib + `devflow` bin + fmt + clippy workspace | rc 0. Core `900 tests run: 900 passed (1 leaky)`. CLI `415 tests run: 415 passed (2 leaky)` |
| Acceptance: no `remove_file`/`remove_dir`/`kill`/shutdown/`rename`/`set_permissions`/`write` in the comment-stripped bodies of `sweep_in`, `sweep_entry`, `sweep_for_in`, `may_belong`, `socket_name_parts`, `read_sidecar`, `probe`, `own_socket_at` | None found (rg rc 1). Control: a planted `std::fs::remove_file(&dir)` in `sweep_in` is found |
| Extra: 50-05 regressions (`probe_*`, V-04, V-05) after the `own_socket_at` refactor | rc 0 each |
| Extra: `semver-gate.sh check --through baseline` | rc 0, `SEMVER verdict=match`. The additions are additive |
| Extra: `cargo check --target aarch64-apple-darwin` for `devflow-core` and `devflow`, `--all-targets` | rc 0, both `Finished`, no warnings |

The leaky tests are `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper`, `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling` and `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates`. They predate this plan and are in files it does not touch.

**What these checks do not establish:**
- Task 3's absent-key half passes whether or not `#[serde(default)]` is present, because serde already reads a missing `Option` as `None`. It pins the behaviour, but it is not sensitive to the attribute.
- No test runs the listing/read race, a foreign-uid socket in `sock/`, or an unreadable `sock/`.
- The macOS check only compiles the code. Nothing has run on macOS.

## Files Created/Modified

- `crates/devflow-core/src/supervisor.rs`: the sidecar, sweep, name parser, acting removal and shutdown client. Also the `own_socket_at` refactor and 6 new tests.
- `crates/devflow-core/src/state.rs`: `SupervisorRecord`, `State::supervisor`, and 1 test.
- `crates/devflow-core/src/ship.rs`: `civil_from_days` widened from private to `pub(crate)`.

## Decisions Made

See `key-decisions` in the frontmatter. Each decision fills a gap in the plan's text. None changes a planned signature.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The RFC 3339 helper the plan cited does not exist**
- **Found during:** Task 1
- **Issue:** `created_at` is specified as "RFC 3339, using the timestamp helper the events module already uses". `events::emit` writes `unix_now()` seconds, and the crate has no RFC 3339 formatter.
- **Fix:** added `rfc3339_from_unix` and `rfc3339_now` in `supervisor`, built on `ship::civil_from_days` (widened to `pub(crate)`). Pinned by three known values in the sidecar test: 1970-01-01, 2023-11-14 and the leap day 2000-02-29.
- **Committed in:** `adc7724` (test), `85257d1` (implementation)

**2. [Rule 2 - Missing critical] `write_sidecar` validates `sock/` before writing**
- **Found during:** Task 1
- **Issue:** the plan does not say whether `write_sidecar` creates the directory. A sidecar written before `publish` could otherwise land in a directory nobody checked, which breaks T-50-13's "0700 dir owned by the user".
- **Fix:** `write_sidecar` calls `ensure_socket_dir`. Its `UnsafeDir` error is mapped to `io::ErrorKind::PermissionDenied` so the planned `io::Result` signature stays.
- **Committed in:** `85257d1`

**3. [Rule 3 - Blocking] `dead_code` under `-D warnings`**
- **Issue:** `write_sidecar` and `rfc3339_now` have no non-test caller until 50-07.
- **Fix:** `#[cfg_attr(not(test), expect(dead_code, reason = …))]`, following 50-05. Because it is `expect`, it turns into an error once 50-07 calls them.
- **Committed in:** `adc7724`

**4. [Scope note] Additions beyond the named tests and items**
- `supervisor::tests::socket_name_parts_accepts_only_the_launch_scheme`: the pure name parser, covering a launch id with `-`, a `35.1` phase and nine malformed stems.
- `own_socket_at`: `probe`'s pre-connect checks factored out so `request_shutdown` applies the same checks rather than a copy.
- The shutdown test also covers a timeout, a malformed final line, a non-ok refusal and a regular-file path, beyond the three cases in `<behavior>`.

**5. [Orchestrator-directed] Tracking and commit guard**
- STATE.md, ROADMAP.md and the REQUIREMENTS.md checkbox were not touched, per the dispatch. `SUPV-01` is declared by later plans too, so marking it complete is left to the orchestrator.
- The executor's `agent-*` branch allow-list was not applied, because this is a sequential executor on `feature/phase-50`. `pin-guard.sh` passed before the first edit and before every commit.

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 missing critical), plus 2 scope/process notes
**Impact on plan:** none on the planned public API. The only widened item is crate-internal (`ship::civil_from_days`).

## Issues Encountered

- The first draft of the shutdown test could hang. A scripted server blocked on `accept` forever if the client never connected, as the RED stub does not. The silent-server case also waited a fixed 10 s. Both were fixed before RED was committed: the server polls `accept` for up to 5 s, and the silent server waits for the client to close.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: fail-open-enumeration | crates/devflow-core/src/supervisor.rs | `sweep_in` returns an empty Vec when `sock/` is missing, and also when it exists but cannot be listed (for example EACCES), following the plan's "enumeration degrades" shape. A D-09 guard (50-15) that reads an empty sweep as "nothing alive" would fail open on an unlistable `sock/`. 50-15 should check that `sock/` is listable before it trusts an empty result. |

## Known Stubs

None. Every `RED stub` body was replaced by its GREEN commit. A stub-pattern scan of the three files at HEAD finds nothing, while the same scan of `adc7724` finds 5.

## User Setup Required

None.

## Next Phase Readiness

- 50-07 (the supervisor runtime) can call `write_sidecar` and `rfc3339_now` before `publish`, and must then drop their `expect(dead_code)` attributes, as with 50-05's server-side protocol items.
- 50-09 (backstop) and 50-11/50-13/50-15 can read `SweepEntry` and `Sidecar` identity fields. `stop` can call `request_shutdown`, then `remove_launch_files`.
- 50-16 removes `State::monitor_pid`. `State::supervisor` is already in place.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- Files: supervisor.rs, state.rs and ship.rs are present.
- Commits adc7724, 85257d1, 2cdabd9, afb8c68, 629cbed and a0be8ba are ancestors of HEAD. Each RED `test(50-06)` commit precedes its GREEN `feat(50-06)` commit (log order 1 through 6).
