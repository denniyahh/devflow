---
phase: 50-addressable-monitor-liveness
plan: 05
subsystem: supervisor
tags: [rust, unix-socket, liveness, probe, cargo-mutants, macos-compile-gate]

requires:
  - phase: 50-01
    provides: "tools/semver-gate.sh (run here as an extra check: --through baseline matches)"
  - phase: 50-02
    provides: "hermetic test conventions (short /tmp tempdirs, no env mutation)"
provides:
  - "devflow_core::supervisor: SocketPath (new/for_launch/as_path/sidecar_path/part_path), SocketPathError::TooLong, SUN_PATH_MAX{,_LINUX,_MACOS}, new_launch_id, boot_id, publish (+ crate publish_with seam), PublishError, Liveness {Gone, Stale, Alive(PingReply)}, ProbeError, probe"
  - "devflow_core::supervisor::protocol: MAX_LINE, IO_TIMEOUT, Request {Ping, Shutdown}, PingReply, ShutdownAccepted, ShutdownStopped; crate-internal Reply, read_line, read_request, write_reply, peer_is_same_user, ping, euid"
  - "registry::socket_dir_in (pub); registry::path_digest widened to pub(crate)"
  - "test_support::spawn_test_in_child (test-support feature)"
  - "DEV-SETUP-CHECKLIST.md §5 line for the aarch64-apple-darwin rustup target"
affects: [50-06, 50-07, 50-08, 50-11, 50-13, 50-23, 50-24]

actuals:
  tokens: 12984
  tasks: 3
  commits: 4
plan_head_before: c34d7b6379f304ae8f416eb14a2ac38b126bfaf4
plan_head_after: 3f8b0f35c7945d969dc2020eb8f2a6fea2af0436

tech-stack:
  added: []
  patterns:
    - "Socket publish: bind+listen <stem>.part, chmod 0600, hard_link to <stem>.sock, remove .part on every path; EEXIST is PublishError::AlreadyExists and the existing name is never touched"
    - "Probe order: lstat (NotFound = Gone) -> is_socket -> uid == euid -> connect (ECONNREFUSED = Stale, all else Connect error) -> one ping round trip (only a valid reply = Alive)"
    - "Server-only protocol items carry #[cfg_attr(not(test), expect(dead_code, reason = ...))] so the first non-test caller (50-07) must remove the expectation"

key-files:
  created:
    - crates/devflow-core/src/supervisor.rs
    - crates/devflow-core/src/supervisor/protocol.rs
  modified:
    - crates/devflow-core/src/registry.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/test_support.rs
    - .planning/user/DEV-SETUP-CHECKLIST.md

key-decisions:
  - "PingReply.owner and PingReply.launch are String, not new enums: the plan's artifact list names no owner/launch types, and 50-23 reads owner labels from runtime.rs OwnerPhase::as_str"
  - "PingReply.boot_id is Option<String>: boot_id() returns None where /proc is unreadable, and a reply must not invent one"
  - "\"ok\" is not a struct field: the server adds \"ok\":true/false (and the constant accepted/stopped markers) in Reply::to_json; the client requires \"ok\":true before deserializing PingReply"
  - "The V-05 mode-000 row is skipped when euid is 0 (root bypasses permission bits), following 999.113's ship.rs precedent; the row's readable-again Stale control still runs as root"
  - "V-06 is #[cfg(target_os = \"linux\")] (its 107/108 numbers are the Linux limit), with its path helper gated the same way so the macOS compile gate has no dead-code warning"

patterns-established:
  - "A child-process fixture that must die mid-listen: spawn_test_in_child + in_child_test guard + Child::kill (SIGKILL to the pid the test owns) + wait"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "probe answers exactly one of Gone, Stale, Alive (or a ProbeError) and reads no pid"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::probe_reports_alive_only_after_a_ping_reply (V-03)"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::probe_reports_stale_only_for_a_socket_whose_listener_is_gone (V-02)"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::probe_reports_gone_only_when_no_socket_file_exists (V-01)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Each fixture yields only its own state (six fixtures incl. a SIGKILLed holder child); non-sockets, unstat-able paths and an unreadable socket are probe errors, never Stale; mutation run on probe has zero survivors"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::each_fixture_produces_exactly_one_liveness_state (V-04)"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::a_non_socket_or_unreadable_path_is_a_probe_error_not_stale (V-05)"
        status: pass
      - kind: other
        ref: "Task 2 cargo-mutants gate (verbatim, plus TMPDIR on disk): mutants_exit=3 caught=6 missed=0 timeouts=2, gate rc 0"
        status: pass
    human_judgment: false
  - id: D3
    description: "sun_path bound by name (107 publishes with an equal-length .part, 108 is TooLong{108,107}); atomic publish never exposes or replaces a socket; sock/ 0700 and socket 0600"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::socket_path_of_107_bytes_binds_and_108_is_refused_by_name (V-06)"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::publish_never_exposes_an_unlistening_socket_or_replaces_a_live_one (V-11)"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::the_socket_dir_is_private_and_the_socket_is_owner_only"
        status: pass
    human_judgment: false
  - id: D4
    description: "MAC-1: devflow-core and devflow compile for aarch64-apple-darwin (--all-targets); peer_is_same_user has Linux SO_PEERCRED, macOS getpeereid and reject-elsewhere bodies"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "cargo check --target aarch64-apple-darwin -p devflow-core --all-targets && ... -p devflow --all-targets -> rc 0, no warnings (after Task 1 and at HEAD)"
        status: pass
    human_judgment: true
    rationale: "The gate compiles only. Nothing has bound, probed or checked a peer uid on macOS (no macOS host, backlog 999.160), so the macOS getpeereid body's runtime behaviour is unverified"

duration: 21min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 05: Socket Address, Atomic Publish and Liveness Probe Summary

**A `devflow_core::supervisor` module that builds a per-launch socket path under `cache_dir/sock` with the `sun_path` limit enforced by name, publishes a listener by bind-to-`.part` then hard link, and probes a recorded path into exactly Gone, Stale or Alive (or a named probe error) without reading any pid.**

## Performance

- **Duration:** 21 min
- **Started:** 2026-10-17T16:27:51Z
- **Completed:** 2026-10-17T16:49:20Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- `probe(path)`: `lstat` NotFound is Gone. A non-socket (symlink, file, directory) is `NotASocket` and another uid's socket is `ForeignOwner`, both decided before any connect. A refused connect is Stale. Every other connect error is `Connect`, including a socket removed between stat and connect, so an unlink race is reported rather than read as Gone. Only a valid ping reply is Alive. A listener that accepts and never replies is `NoReply` after the 2 s read timeout.
- `publish`: creates `sock/` with mode 0700, or tightens an existing looser one to 0700, and refuses a symlinked, non-directory or foreign-owned `sock/`. It binds and listens on `<stem>.part`, sets the socket to 0600, hard-links `<stem>.sock`, and always removes `.part`. A second publish onto a live name returns `AlreadyExists` and the first listener keeps answering.
- `SocketPath::new` checks both the final path and `part_path()` against `SUN_PATH_MAX`. `.part` is exactly as long as `.sock` (RV-01), so a 107-byte path both builds and publishes, and 108 bytes is `TooLong { len: 108, max: 107 }` before anything is bound.
- `protocol`: newline-JSON framing with a 4 KiB line cap (newline included) and 2 s timeouts. Also the `Request`, `PingReply` and shutdown reply wire shapes, pinned by `protocol::tests::every_reply_has_its_wire_shape`, and `peer_is_same_user` with three `cfg` bodies (MAC-1).

## Task Commits

1. **Task 1 (tracer): publish + probe Alive/Stale/Gone.** RED `c141930` test(50-05), GREEN `5b8a1b8` feat(50-05)
2. **Task 2: probe totality, fixture table, mutation run.** `e7f2060` test(50-05)
3. **Task 3: sun_path boundary and atomic publish.** `3f8b0f3` test(50-05)

**Tracer feedback gate:** the run was interactive (`auto_advance=false`, `_auto_chain_active=false`) with `human_verify_mode` at its default `end-of-phase`, and Task 1's `<verify>` is automated only, so row 3 applied. Both Task 1 gates were re-run at `5b8a1b8` with rc 0. A negative control was added for the macOS gate: an ungated `libc::SO_PEERCRED` makes `cargo check --target aarch64-apple-darwin -p devflow-core --all-targets` exit 101 with `error[E0425]: cannot find value SO_PEERCRED in crate libc`, while the Linux `cargo check` still exits 0. The tree was restored with `git checkout -- <file>`. Expansion followed.

## TDD Gate Compliance

| Task | RED | RED evidence | GREEN |
|------|-----|--------------|-------|
| 1 | `c141930` test(50-05). The three V-01..V-03 tests were run together under a scratch nextest JUnit profile: exit 100, 3 run, 0 passed, 3 failed | `gsd-tools check tdd-red-evidence` on the unedited JUnit report: `RED_EVIDENCE_OK` / `target_test_failed` / format `junit`, once per target test | `5b8a1b8` feat(50-05): each test passes as its own named run |
| 2 | none (see below) | substitute: hand mutations plus cargo-mutants | `e7f2060` |
| 3 | none (see below) | substitute: hand mutations | `3f8b0f3` |

**Task 1 semantic assessment.** All three targets ran and failed at a probe assertion, with `expected Alive with launch id …, got Err(NoReply("probe is not implemented yet"))`, because `probe` was a stub. The fixtures (publish, ping server thread) worked; there was no compile, load or fixture fault. In V-01 and V-02 the failing assertion is the in-test Alive control that precedes the target assertion, which is the first point where the stub diverges. In V-03 it is the target assertion itself.

**Tasks 2 and 3 have no RED commit, by the plan's structure.** Task 1's action implements all of `probe`, `publish` and `SocketPath` (the "total probe", the hard-link publish and the length check), so Tasks 2 and 3 add tests over code that already exists. Their tests passed on first run, which tdd.md's fail-fast rule 1 flags. I investigated: the behaviour was present from Task 1, not a wrong test. As the discrimination substitute, every check below failed with the named test exiting 100, and the source was then restored byte-identical (`cmp`):

- `probe` replaced with a constant `Ok(Gone)`, `Ok(Stale)`, `Err(NoReply)` or `Ok(Alive(..))`: V-04 fails for all four. Three of them fail during holder setup ("the holder child never answered a ping", after its 30 s wait); `Alive` fails at the table row "fixture `no file` must produce only Gone".
- `part_path` with a leading dot (the superseded RESEARCH spelling): V-06 fails ("the temporary name must be exactly as long as the final name").
- `SocketPath::new` checking the final path only: V-06 fails. The limit off by one: V-06 fails.
- `hard_link` replaced by `rename`: V-11 fails ("a second publish onto a live name must fail, got Ok(UnixListener …)").
- Socket chmod 0755 instead of 0600; no re-tightening of `sock/`; the symlink check removed: the private-dir test fails.

## Mutation Run (Task 2 gate)

- **First run (Task 2 tests as first written): gate rc 1.** 1 missed, 5 caught, 2 timeouts, 1 unviable. The survivor replaced the match guard `error.kind() == io::ErrorKind::NotFound` with `true` in `probe`, which turns every `lstat` error into Gone. No test produced a stat error other than NotFound.
- **Fix: strengthened a test, not `probe`.** V-05 gained a path below a regular file (`lstat` fails ENOTDIR, which works as root too), and it must be `ProbeError::Stat`.
- **Second run (the exact Task 2 commit content): gate rc 0.** `mutants_exit=3 caught=6 missed=0 timeouts=2`, plus 1 unviable.
  - Timed out (counted as caught by the gate): `supervisor.rs:305:8: delete ! in probe` and `supervisor.rs:308:19: replace != with == in probe` (the uid check). Both make the holder fixture's own socket unprobeable as Alive, so V-04's 30 s wait for the holder exceeds the 20 s auto timeout.
  - Unviable: `replace probe -> Result<Liveness, ProbeError> with Ok(Default::default())`, because `Liveness` has no `Default`. So cargo-mutants never builds an "always one state" mutant. The hand replacements listed above are the evidence for the plan's truth that such a mutant is caught.
- The gate ran verbatim with two additions that do not touch its verdict logic: `TMPDIR` pointed at a disk directory (cargo-mutants copies and builds the workspace there, and `/tmp` on this host is RAM-backed), and one `cp` of `mutants.out` to scratch for this report.

## Verification (commands run, actual exit codes)

| Gate | Result |
|------|--------|
| Task 1 named tests (V-03, V-02, V-01, `registry::tests::path_digest_is_stable_and_distinguishes_different_paths`) | rc 0; each `1 test run: 1 passed` |
| Task 1 macOS compile gate | rc 0 (after Task 1 and at HEAD); no warnings |
| Task 2 named tests (V-05, V-04) | rc 0; each `1 test run: 1 passed` |
| Task 2 cargo-mutants gate | rc 0 (second run; first run rc 1, see above) |
| Task 3 named tests (V-06, V-11, private dir) | rc 0; each `1 test run: 1 passed` |
| Task 3 lib suite + no `fs::rename(` + fmt + clippy workspace | rc 0; `893 tests run: 893 passed (1 leaky)`. Rename-scan control: the same pipeline with a planted `std::fs::rename(a, b)` line matches |
| Task 3 macOS compile gate | rc 0. The first run warned that `sock_path_of_len` is never used on macOS, so the helper was gated `cfg(target_os = "linux")` like V-06; re-run: rc 0, no warnings |
| Plan `<verification>` at HEAD `3f8b0f3` | `cargo nextest run -p devflow-core --lib --no-tests=fail` rc 0 (893 passed); `cargo fmt --all --check` rc 0; `cargo clippy --workspace --all-targets -- -D warnings` rc 0; macOS gate rc 0 |
| Extra: `semver-gate.sh check --through baseline` | rc 0, `SEMVER verdict=match`: no new semver break, the changes are additive only |
| Extra: CLI callers of the refactored `run_test_in_child` (`staleness`, `preflight`, `pipeline_*` modules) | rc 0, `263 tests run: 263 passed` |
| Acceptance: no pid read in `supervisor.rs` (comment-stripped `rg "agent_running\|process_start_time\|kill\("`) | No match at the Task 1 commit (control: a planted `agent::agent_running(pid)` matches). After Task 2, the only match is the V-04 fixture's `child.kill()`, the test-owned SIGKILL the plan requires. It is not on the probe path |

The `1 leaky` is `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper`. It predates this plan and is not touched by it.

**What these checks do not establish:**
- The macOS gate compiles only. `getpeereid`, binding, the 103-byte limit and the probe are not run on macOS.
- `ForeignOwner` and `peer_is_same_user`'s reject branch have no positive test, because that needs a second uid. The uid-check mutant was caught (via a timeout), so the equality is pinned, but a foreign-owned socket has never been probed.
- On a root run (the container pre-push gate), V-05's mode-000 row is skipped.

## Files Created/Modified

- `crates/devflow-core/src/supervisor.rs`: the socket path, publish and probe, plus the 8 supervisor tests
- `crates/devflow-core/src/supervisor/protocol.rs`: wire types, framing, peer uid check, client ping, 2 tests
- `crates/devflow-core/src/registry.rs`: `socket_dir_in`, `path_digest` made `pub(crate)`, module doc now says "the launch path"
- `crates/devflow-core/src/lib.rs`: `pub mod supervisor;` after `state`
- `crates/devflow-core/src/test_support.rs`: `spawn_test_in_child`; both child helpers now share one private `child_test_command`
- `.planning/user/DEV-SETUP-CHECKLIST.md`: §5 `aarch64-apple-darwin` line (in the Task 1 GREEN commit)

## Decisions Made

See `key-decisions` in the frontmatter. All are within the plan's remit (wire-type field shapes, test gating); none changes a planned contract.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug in test strength] Surviving mutant on the lstat NotFound guard**
- **Found during:** Task 2 (first mutation run)
- **Issue:** no fixture produced a non-NotFound stat error, so a `probe` that maps every stat error to Gone survived
- **Fix:** V-05 gains a path below a regular file and must be `ProbeError::Stat`
- **Verification:** second mutation run, 0 missed
- **Committed in:** `e7f2060`

**2. [Rule 1 - Bug in test strength] The symlink-check removal was invisible to the private-dir test**
- **Found during:** Task 3 hand mutations
- **Issue:** with the symlink check deleted, the following `!is_dir()` check (on lstat metadata) still refuses a symlinked `sock/` as `UnsafeDir`, so the test asserting only `UnsafeDir(_)` passed. Behaviour stayed safe; only the reason changed.
- **Fix:** the test requires the `UnsafeDir` message to contain "is a symlink"
- **Committed in:** `3f8b0f3`

**3. [Rule 3 - Blocking] dead_code under `-D warnings` for server-only protocol items**
- **Issue:** `read_request`, `write_reply`, `peer_is_same_user` and `Reply` have no non-test caller until 50-07, so the plan's clippy gate would fail
- **Fix:** `#[cfg_attr(not(test), expect(dead_code, reason = …))]` on each. `expect` (not `allow`) turns into an unfulfilled-expectation warning once 50-07 calls them, so it cannot linger.
- **Committed in:** `c141930`

**4. [Scope note] Additions beyond the named tests**
- `protocol::tests::every_reply_has_its_wire_shape` (pins the `<context>` JSON shapes and constructs every `Reply` variant, which also clears the test-build dead-code warning) and `protocol::tests::a_line_longer_than_max_line_is_refused` (the 4 KiB cap, with an at-the-cap control)
- `run_test_in_child`'s command construction moved into a private `child_test_command` shared with `spawn_test_in_child`, instead of duplicating it. Its callers' suites pass (893 core and 263 CLI).

**5. [Orchestrator-directed] Commit guard**
- The executor's per-agent-branch allow-list (`agent-*`) was not applied, because this is a sequential executor on `feature/phase-50` per the dispatch. The orchestrator's `pin-guard.sh` passed before the first edit and before every commit.

---

**Total deviations:** 3 auto-fixed (2 test-strength, 1 blocking lint), plus 2 scope/process notes
**Impact on plan:** none on contracts or the public API beyond the plan's artifact list

## Issues Encountered

- **Leftover `/tmp/dfs*` fixture directories.** Four directories (12:39 and 12:41) came from the two mutation runs: cargo-mutants kills a timed-out test process before `TempDir` can drop. A normal passing run leaves none (checked by listing before and after a full supervisor run). All four were removed, and no holder child process was left running.
- **`DEVFLOW_TEST_HOLDER_SOCKET`** is read through a `const`, as `CHILD_TEST_ENV` is, so `doc_check`'s literal-read scan does not list it. The `doc_check` tests pass (6/6). It is a test-only variable and is not documented in the user docs.

## Known Stubs

None. `probe`'s RED stub was replaced in `5b8a1b8`.

## User Setup Required

None. The `aarch64-apple-darwin` target is already installed on this host, and the checklist line documents it for other setups.

## Next Phase Readiness

- 50-06 can add `sweep_in`, `remove_launch_files`, `request_shutdown` and the sidecar on top of `SocketPath`, `protocol::read_line` (it takes any `BufRead`, so a client can keep one reader across the accepted and stopped lines) and `ShutdownAccepted`/`ShutdownStopped`.
- 50-07 must remove the `expect(dead_code)` attributes when it first calls `read_request`, `write_reply` and `peer_is_same_user`. The compiler will flag them if it does not.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*
