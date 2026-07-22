# Test Suite QA Review

**Review date:** 2026-07-21  
**Branch:** `develop`  
**Status:** Initial cleanup complete; structural follow-up recommended  
**Independent reviews:** Code review `APPROVE`; architecture review `WATCH`

## Purpose

This document records the first full QA review of the DevFlow test suite. The
review goals were to remove tests that did not prove useful behavior, repair
broken or environment-dependent tests, add obviously missing coverage, and
identify the highest-leverage code-quality improvements for a project developed
primarily through AI agents.

The test changes described here are currently uncommitted on `develop`. Other
pre-existing worktree changes were not part of this review.

## Executive Assessment

The Rust suite is broad and behaviorally stronger than its raw test count
suggests. It exercises real Git repositories, subprocesses, concurrent phase
flows, state transitions, logging, and CI contracts. The final suite has no
known failing tests or blocking review findings.

The primary residual risk is uneven confidence. Core Rust behavior is heavily
tested, while user-facing shell entry points have no direct behavioral tests.
Large inline test modules also couple many tests to private implementation
details and make AI-generated additions harder to review for duplication and
false confidence.

| Dimension | Assessment | Notes |
| --- | --- | --- |
| Correctness | Strong | Full workspace and static gates pass |
| Coverage | Strong | 92.81% line coverage |
| Isolation | Strong after fixes | Suite passes with a hostile global Git hook |
| Test signal | Good | Five low-value tests removed; focused gaps added |
| Maintainability | Watch | Very large inline test modules |
| Shell/operations coverage | High risk | Install, sync, and deploy scripts lack direct tests |
| AI-output robustness | Watch | Parsers are covered, but fuzz/property testing is absent |

## Validation Evidence

Final verification:

```text
GIT_CONFIG_GLOBAL=/tmp/devflow-test-global.gitconfig \
  cargo test --workspace --all-targets
426 passed; 0 failed

cargo clippy --workspace --all-targets -- -D warnings
passed

cargo fmt -- --check
passed

git diff --check
passed

cargo llvm-cov --workspace --all-targets --summary-only
92.81% line coverage; 92.93% region coverage; 93.19% function coverage
```

The hostile Git configuration installed a global `pre-commit` hook that always
exited with status 99. Passing under that configuration verifies that temporary
Git repositories do not inherit developer-specific hooks. Baseline line
coverage was 91.91%, so coverage improved despite reducing the test count from
427 to 426.

## Completed Work

### Removed low-value coverage

Five tests were deleted:

1. Two exact duplicate `agent_result` exit-code tests.
2. A determinism test that compared `decide_action(input)` with the same call.
3. A reviewer-set adapter test that exercised only test-defined behavior that
   did not exist in production.
4. A `.devflow.yaml` smoke test that ran `doctor`, even though `doctor` never
   reads `.devflow.yaml`.

Redundant and tautological assertions were also removed from outcome-policy and
monitor integration tests.

### Repaired test reliability

- Every temporary Git repository that creates commits or tags now configures
  `core.hooksPath=/dev/null`.
- Capture-output testing now writes into an injectable buffer. It asserts the
  bytes and offsets without leaking `hello world` into the test runner.
- The CLI unit suite also passed with 32 test threads, providing a basic stress
  check for parallel execution.

### Added missing coverage

- OpenCode command construction and prompt parity across Claude, Codex, and
  OpenCode.
- Empty, whitespace-only, malformed, and type-invalid external-verification
  approval values.
- Empty quoted external-verification commands in plan frontmatter.
- Orphaned archived captures, archived reviews, and recursively located live
  reviews in history rendering.
- Actual capture bytes, unchanged offsets, appended output, and missing-file
  behavior.

### Production defect found through testing

External verification previously accepted an empty JSON approval array and
empty quoted commands. An empty command could execute as `sh -c ""` and return
success, incorrectly acting as affirmative verification. Approval parsing and
frontmatter parsing now reject empty command sets and blank commands.

## Remaining Findings

### 1. Public shell entry points lack direct tests

The highest remaining risk is the absence of behavioral tests for:

- `scripts/install.sh`, which downloads tools, clones repositories, builds, and
  copies files.
- `scripts/sync-main-to-develop.sh`, which fetches and mutates Git history.
- `scripts/deploy.sh`, which builds and pushes deployment output.

These are user-facing, side-effecting paths. The Rust suite does not protect
their argument handling, fail-fast behavior, command ordering, or cleanup.

### 2. Test ownership is concentrated in monolithic modules

The CLI test module begins around `crates/devflow-cli/src/main.rs:4027` and
continues for thousands of lines. The `agent_result` tests have a similar shape
starting around `crates/devflow-core/src/agent_result.rs:1116`.

This organization encourages tests of private helpers, makes duplicate tests
harder to detect, and gives AI agents too much unrelated context when changing
one behavior. It also conflicts with the existing Phase 999.8 goal to split
`main.rs`.

### 3. Some infrastructure tests inspect source text instead of behavior

`devcontainer_ci_failfast.rs` contains useful CI contract coverage, but part of
it verifies literal command text in `main.rs`. A source-grep assertion can pass
while runtime argument construction or command ordering is wrong. A fake
`cargo` executable on `PATH` should capture actual argv and invocation order.

### 4. One integration test is disproportionately expensive

`build_provenance.rs` performs nested Cargo builds and dominates suite runtime.
The behavior is important and should not be deleted, but it should use the
smallest possible synthetic fixture or run in a clearly identified slow CI
lane.

### 5. AI-facing protocol boundaries lack generative testing

DevFlow parses agent markers, JSON event streams, rate-limit responses, YAML
frontmatter, shell commands, and Git output. Example-based tests are extensive,
but there is no fuzzing or property testing for malformed, truncated, nested,
or adversarial agent output.

## Prioritized Recommendations

### P0: Test the shell entry points hermetically

Add `shellcheck` to CI and behavioral tests that run the scripts with a fake
`PATH`. Record argv and simulate success/failure for `curl`, `git`, `cargo`,
`cp`, and deployment commands. Tests must use temporary repositories and must
never access the network or a real remote.

Acceptance targets:

- Every external command and destructive step has a tested failure path.
- Fail-fast ordering is observable through captured invocations.
- Re-running installation is tested for idempotency.
- Sync and deploy tests prove the intended ref/remote rather than merely
  matching script source text.

### P0: Establish an AI change acceptance contract

Require every AI-generated behavioral change to include:

1. A regression test that fails before the implementation change.
2. At least one assertion at a public or stable domain boundary.
3. Evidence that the test fails for the intended reason.
4. Full affected-package tests, Clippy with warnings denied, and formatting.
5. Independent review of both implementation and test signal.

Reject tests that only assert constants, reproduce the production algorithm,
compare a function call with itself, or grep implementation text without a
runtime contract.

### P1: Split tests by domain and boundary

Move CLI orchestration into testable library modules as part of Phase 999.8,
then organize integration tests by command or domain: `doctor`, `advance`,
`logs`, `ship`, `staleness`, and parallel workflows. Keep small pure-function
tests inline; move subprocess and filesystem behavior to integration suites with
shared fixture helpers.

### P1: Add mutation testing

Introduce `cargo-mutants` initially as a scheduled or manual gate. Prioritize
outcome policy, external verification, agent-result parsing, stage transitions,
and Git safety logic. Track surviving mutants rather than treating line
coverage as the primary quality score.

### P1: Add property and fuzz tests for protocol parsers

Use `proptest` for invariants and `cargo-fuzz` for byte-oriented parsers.
Priority targets are agent result markers, JSON envelopes, rate-limit detection,
frontmatter extraction, event logs, and shell quoting. Parsers must never panic,
must fail closed on ambiguous approval data, and must preserve documented
precedence rules.

### P1: Separate fast and slow validation lanes

Keep deterministic unit and ordinary integration tests in the fast pull-request
lane. Put nested-build provenance tests, mutation testing, repeated concurrency
stress, and fuzz smoke runs in explicit slow or scheduled lanes. Both lanes
should remain visible and required at an appropriate release boundary.

### P2: Add differential coverage enforcement

Do not optimize for a global percentage alone. Enforce high coverage on changed
lines and require a written justification when new branches are intentionally
uncovered. Coverage should support review, not replace behavioral inspection or
mutation results.

### P2: Refresh the testing map

`.planning/codebase/TESTING.md` is partly stale. For example, it says
`main.rs` has no inline tests and still uses the deleted
`devflow_ignores_stray_devflow_yaml` test as its primary example. Regenerate or
update that document after the test/module restructuring is complete.

## Suggested Delivery Order

1. Shellcheck and hermetic shell-script tests.
2. Phase 999.8 test/module extraction.
3. Replace source-grep infrastructure assertions with runtime command capture.
4. Add `cargo-mutants` for the highest-risk core modules.
5. Add parser property tests, then a small continuous fuzz corpus.
6. Introduce fast/slow CI lanes and changed-line coverage reporting.

## Requested Claude Review

Please review this document against the current codebase and answer:

1. Are any completed test deletions removing a unique production contract?
2. Are the three shell scripts the correct highest-priority uncovered surface?
3. Should test extraction be part of Phase 999.8 or a separate phase?
4. Which modules should be the initial `cargo-mutants` scope, and what surviving
   mutants would be acceptable?
5. Which parser invariants deserve `proptest` versus `cargo-fuzz`?
6. Are any recommendations likely to add more CI cost than defect-detection
   value?

Claude should return concrete disagreements, missed risks, and a revised
priority order rather than only confirming the recommendations.
