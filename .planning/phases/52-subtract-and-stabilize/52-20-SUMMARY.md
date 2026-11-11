---
phase: 52-subtract-and-stabilize
plan: 20
subsystem: gsd-integration
tags: [rust, devflow-core, gsd, subprocess, fail-closed, tdd]

requires:
  - phase: 52-18
    provides: removed-surface source scan (no token of this plan collides with it)
provides:
  - "pub mod gsd_reader: GsdStatus, StatusReport, ReaderFailure (+ label()), GSD_READER_TIMEOUT"
  - "locate_launcher, phase_dir, verification_status, frontmatter_field, runtime_version"
  - "git::hermetic_command_at (pub(crate)): the &Path-taking sibling of hermetic_command"
affects: [52-22, 52-27, 52-29]

actuals:
  tokens: 11015
  tasks: 3
  commits: 6
plan_head_before: aebe6c04ff06bee06c791b969fbbe47017ec4553
plan_head_after: e1ae20d7b723dd589951f2d0b409cffd745174b2

tech-stack:
  added: []
  patterns:
    - "Deadline-bounded one-shot subprocess: spawn in its own process group, drain stdout/stderr on threads, poll try_wait, SIGKILL the group and reap on expiry"
    - "Classify subprocess JSON by indexing; an absent or ill-typed field is a named failure, never a default"

key-files:
  created:
    - crates/devflow-core/src/gsd_reader.rs
  modified:
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/lib.rs

key-decisions:
  - "The &Path sibling of hermetic_command is git::hermetic_command_at, pub(crate); hermetic_command now delegates to it (Command::new on a bare name still searches PATH, so git commands are unchanged)"
  - "The reader sets HOME on the child to the home the launcher was found under, so GSD and the launcher agree on one home"
  - "phase_dir returns Result<PathBuf, ReaderFailure>; two {padded}- directories are AmbiguousPhaseDir rather than the first read_dir entry"
  - "ReaderFailure is #[non_exhaustive] so a new failure class is not a major release of the published crate; GsdStatus stays exhaustive so callers must route every status"
  - "GSD_READER_TIMEOUT is 60 s, a const; the test deadline goes through the private verification_status_within"

patterns-established:
  - "ReaderFailure::label() is the only text an event may carry; Display holds GSD's message and paths for the gate context and stderr"

requirements-completed: [GATE-02, GATE-03]

coverage:
  - id: D1
    description: "verification_status runs gsd_run by argv in the evidence root, with GIT_* removed, and reads passed; phase_dir finds the {padded}- directory"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::a_passed_report_reads_as_passed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::the_reader_command_removes_repo_local_git_vars"
        status: pass
    human_judgment: false
  - id: D2
    description: "The four other statuses (stale with moved files, gaps_found, missing, human_needed) map to GsdStatus variants"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::a_stale_report_reads_as_stale_with_its_moved_files"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::a_gaps_found_report_reads_as_gaps_found"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::a_missing_report_reads_as_missing"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::a_human_needed_report_reads_as_human_needed"
        status: pass
    human_judgment: false
  - id: D3
    description: "Every failure class is a distinct ReaderFailure: no launcher, spawn error, timeout (child killed and reaped), non-zero exit, non-JSON, unknown status, phase_dir_not_found and unparseable at exit 0, indeterminate passed, no status, no route, ambiguous phase dir"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests (14 failure tests, each run by name)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Event-safe labels: non-empty, distinct, no path for every variant"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::reader_failure_labels_carry_no_path"
        status: pass
    human_judgment: false
  - id: D5
    description: "frontmatter_field and runtime_version fail closed on an exit-0 error key, a non-zero exit and an absent field or version"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::frontmatter_field_reads_the_requested_field"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::a_missing_frontmatter_field_is_a_reader_failure"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::a_failing_frontmatter_read_is_a_reader_failure"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gsd_reader.rs#gsd_reader::tests::runtime_version_reads_the_gsd_version"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 20: GSD Verification Reader Summary

**`devflow_core::gsd_reader` runs GSD's own `gsd_run` readers (verification status, frontmatter get, runtime-identity). Each call runs by argv in the evidence root, with GIT_* removed, under a 60 s kill-and-reap deadline. Every answer comes back as one of five statuses or one of twelve named failures. Exit 0 never counts as success on its own.**

## Performance

- **Duration:** about 20 min
- **Started:** 2026-11-11T05:06:53Z
- **Completed:** 2026-11-11T05:27Z
- **Tasks:** 3 (all TDD: RED commit, then GREEN commit)
- **Files modified:** 3 (1 created)

## Accomplishments

- `verification_status(home, evidence_root, phase_dir)` returns a `StatusReport { status, route, moved_files }` or a `ReaderFailure`. Routing reads `status`/`route` and never `next_command`.
- `phase_dir(evidence_root, phase)` returns the one `{padded}-*` directory, so GSD can still answer `missing`. It refuses when two directories match.
- The classifier checks `staleCheckIndeterminate` first. It then matches `status` against the closed set and requires `route` as a string. `moved_files` must be a list of strings when GSD includes it; GSD leaves it out when nothing moved.
- `frontmatter_field` and `runtime_version` use the same runner. An `error` key is a failure even at exit 0.
- `ReaderFailure::label()` gives one static, path-free name per variant. GSD's message and any paths stay in the variant's fields and its `Display` text.

## Task Commits

1. **Task 1 (tracer) RED:** `2edd0a4` test(52-20): a passed GSD verification read through a stub launcher
2. **Task 1 GREEN:** `cbec233` feat(52-20): a fail-closed GSD verification reader over gsd_run
3. **Task 2 RED:** `57a7099` test(52-20): every GSD status and reader failure class, one test each
4. **Task 2 GREEN:** `57d1306` feat(52-20): classify every GSD status and fail closed on the rest
5. **Task 3 RED:** `3fce22e` test(52-20): the frontmatter and version readers must fail closed
6. **Task 3 GREEN:** `e1ae20d` feat(52-20): frontmatter and version readers on the same bounded runner

No REFACTOR commits were made. The plan's context suggested the subject `feat(core): ...`. The commits use the `52-20` scope instead, because the TDD gate validator greps `feat(52-20)`.

## Files Created/Modified

- `crates/devflow-core/src/gsd_reader.rs`: the reader module and its 26 stub-launcher tests.
- `crates/devflow-core/src/git.rs`: `hermetic_command_at(&Path, &Path)`, which `hermetic_command` now delegates to.
- `crates/devflow-core/src/lib.rs`: `pub mod gsd_reader;`.

## Decisions Made

- **Path sibling:** `git::hermetic_command_at`, `pub(crate)`. A path is never refused for being non-UTF-8.
- **HOME on the child:** the reader sets `HOME` to the home the launcher was found under. Production passes the real home, so nothing changes there. Tests get a hermetic home without needing a test-only hook. PATH is inherited because the launcher needs `node`. Tests never reach the real gsd-tools, since the launcher is always the planted stub, run by absolute path.
- **Timeout:** `GSD_READER_TIMEOUT = 60 s`. The child runs in its own process group. On expiry the group gets SIGKILL and the child is reaped. A pipe that is still open after the child exits also counts as a timeout, not as an empty answer.
- **`#[non_exhaustive]` on `ReaderFailure`:** an additive choice for the published crate. `GsdStatus` stays exhaustive.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch nextest config (`--config-file <scratch>/nextest.toml --profile red`). Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|------|---------|---------------------|
| `a_passed_report_reads_as_passed` | `RED_EVIDENCE_OK` (exit 100) | The first planned assertion failed: the skeleton `phase_dir` returned `PhaseDirNotFound` for the existing `07-demo` directory. |
| `the_reader_command_removes_repo_local_git_vars` | `RED_EVIDENCE_OK` (exit 100) | The skeleton command did not clear `GIT_ALTERNATE_OBJECT_DIRECTORIES`. |
| stale, gaps_found, missing, human_needed (4 tests) | `RED_EVIDENCE_OK` (exit 100) | `expect` panicked. Task 1's classifier knew only `passed`, so each returned `UnknownStatus`. |
| `a_nonzero_exit_is_a_reader_failure`, `a_nonzero_exit_wins_over_a_passed_json` | `RED_EVIDENCE_OK` | Task 1 ignored the exit status. The first test got `NotJson` and the second got `Ok(Passed)`. |
| `phase_dir_not_found_with_exit_0…`, `an_unparseable_status…`, `an_indeterminate_passed…`, `a_json_object_without_status…`, `a_passed_report_without_a_route…` | `RED_EVIDENCE_OK` | Each read as the wrong variant or as `Ok`. The indeterminate case and the no-route case were both fail-open in Task 1. |
| `two_matching_phase_dirs_are_a_reader_failure` | `RED_EVIDENCE_OK` | Task 1's `phase_dir` returned the first match. |
| `reader_failure_labels_carry_no_path` (Task 2) | `RED_EVIDENCE_OK` | The placeholder `label()` returned `""`. |
| `no_launcher…`, `a_spawn_error…`, `a_hung_reader_times_out`, `non_json_stdout…`, `an_unknown_status…` | `INVALID_RED` / `unexpected_green` | These were already true after Task 1 GREEN, because Task 1's action requires the launcher search, the bounded runner and the JSON parse. Each one got a mutant control instead (below). |
| All 6 Task 3 tests | `RED_EVIDENCE_OK` | The placeholder readers returned `Spawn("not implemented")`. |
| `reader_failure_labels_carry_no_path` (Task 3) | `INVALID_RED` / `unexpected_green` | The `GsdError` label arm had to land in the RED commit for the exhaustive match to compile. The test now guards the new variant. |

**Mutant controls for the five already-green tests.** Each mutation was applied to the GREEN source on its own, the named test was run, and the file was restored (byte-compared). All five tests failed with exit 100:
- `locate_launcher` accepts any path: `no_launcher_is_a_reader_failure` failed.
- A spawn error mapped to `NotJson`: `a_spawn_error_is_a_reader_failure` failed.
- `end()` no longer reaps the child: `a_hung_reader_times_out` failed on the "left running" assertion. The test checks reaping, not only the returned variant.
- `NotJson` mapped to `Spawn`: `non_json_stdout_is_a_reader_failure` failed.
- An unknown status read as `Missing`: `an_unknown_status_is_a_reader_failure` failed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Two matching phase directories are a failure**
- **Found during:** Task 2
- **Issue:** `read_dir` order is unspecified. Taking the first `07-*` directory, as `phase_verification_path` does, could read the wrong phase's report.
- **Fix:** added `ReaderFailure::AmbiguousPhaseDir` and the test `two_matching_phase_dirs_are_a_reader_failure`.
- **Committed in:** `57a7099` (RED), `57d1306` (GREEN)

**2. [Rule 2 - Missing critical] An absent `route`, `version` or frontmatter field is a failure**
- **Found during:** Tasks 2 and 3
- **Issue:** the plan names these, but its test list did not pin them. Task 1's minimal classifier read an absent `route` as `""`, which is fail-open.
- **Fix:** `ReaderFailure::MissingField`, plus the tests `a_passed_report_without_a_route_is_a_reader_failure`, `a_frontmatter_result_without_the_field_is_a_reader_failure` and `a_runtime_identity_without_a_version_is_a_reader_failure`. Also added `a_nonzero_exit_wins_over_a_passed_json`, which pins that the exit status is checked before stdout.
- **Committed in:** `57a7099`/`57d1306`, `3fce22e`/`e1ae20d`

**3. [Semver] `ReaderFailure` is `#[non_exhaustive]`**
- **Found during:** Task 3, after `cargo semver-checks`
- **Fix:** marked the enum `#[non_exhaustive]` so that adding a failure class later is not a major release. **Committed in:** `e1ae20d`

**Total deviations:** 3 (2 missing-critical, 1 semver hardening). **Impact:** each one is on the fail-closed path the plan exists for. Nothing outside `gsd_reader.rs` and the one `git.rs` sibling changed.

## Fixture provenance (what the tests do not establish)

- The `passed`, `stale`, `gaps_found`, `missing`, `phase_dir_not_found`, bogus-stderr, `frontmatter get` and `runtime-identity` shapes come from RESEARCH's gsd-core 1.20.1 probe table, recorded once (n=1 per case). Where the table gives only `status`/`route`, the other keys (`next_action`, `next_command`) follow `routeResult` and `VERIFICATION_ROUTES` in `~/.claude/gsd-core/bin/lib/verification.cjs`.
- `human_needed` and `unparseable` were never probed. Their fixtures are built from the enum and the route table, not from recorded output.
- No test runs the real gsd-core. If a later gsd-core changes a shape, the reader returns a failure (a gate), not a pass, but nothing here detects that the shape changed. Re-probe when `verification.cjs` changes.
- The hung-reader test covers a launcher that `exec`s. A launcher that forks a grandchild and exits relies on the group kill and on the bounded drain. Neither of those paths has a test of its own.

## Issues Encountered

- `cargo semver-checks check-release -p devflow-core` exits 100 with 11 major findings. None of them names `gsd_reader` or `git::`. The output does name items by path (59 `devflow_core::` paths), so it would have shown one of this plan's items if it were flagged. The findings are this phase's earlier cuts plus 52-19's `GateError::ApprovedAndAborted`. Pitfall 9 says the version bump to 4.0.0 belongs to the plan that runs the check as a gate. This plan does not bump the version.

## Verification

- All 21 plan `<automated>` gates were re-run by name (`cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=gsd_reader::tests::…)'`): 21 passed. Each run was also checked for `1 passed`. Control: a wrong test name exits 4.
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0.
- `cargo test -p devflow-core --lib`: 1047 passed, 0 failed, 2 ignored (includes `doc_check`).
- `cargo nextest run -p devflow --bin devflow`: 603 passed, a regression check for the `hermetic_command` delegation.
- `cargo build --workspace --tests` passed before every commit.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-21. The callers that remain (52-22's verdict, 52-27's gaps Plan post-check, 52-29's finish check) can call `gsd_reader::{phase_dir, verification_status, frontmatter_field, runtime_version}`. They should record `label()` in events and keep the `Display` text out of them.

## Self-Check: PASSED

- `crates/devflow-core/src/gsd_reader.rs`: FOUND
- Commits `2edd0a4`, `cbec233`, `57a7099`, `57d1306`, `3fce22e`, `e1ae20d`: all ancestors of HEAD (see below)

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
