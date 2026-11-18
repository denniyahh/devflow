---
phase: 52
phase_name: "Subtract and Stabilize"
project: "DevFlow"
generated: "2026-11-18"
counts:
  decisions: 10
  lessons: 8
  patterns: 8
  surprises: 6
missing_artifacts: []
---

# Phase 52 Learnings: Subtract and Stabilize

## Decisions

### Phase 52 is not dogfooded
Phase 52 ran with GSD commands in an agent session, not with `devflow start`, as a recorded exception to the dogfooding rule. The 3.0.0 dry-run control still applied.

**Rationale:** D-18 in 52-CONTEXT.md; the phase removes the stages a dogfood run would use.
**Source:** 52-RUN-NOTES.md

---

### No 4.0.0 release at close-out
Phase 52 merges to `develop` as `4.0.0-alpha.1`: no tag, no crates.io publish, nothing to `main`. 4.0.0 is cut at the v4.0 milestone close.

**Rationale:** Operator, 2026-11-11: DevFlow is not yet fully usable; the alpha version keeps a `develop` build from reporting the published 3.0.0.
**Source:** 52-RUN-NOTES.md

---

### Commit subjects use the GSD plan scope, not the plan's suggested scope
Commits use `feat(52-NN)!:` / `test(52-NN):` with a `BREAKING CHANGE:` footer instead of the plans' `feat(cli)!:` / `feat(core)!:`.

**Rationale:** GSD's TDD gate matches `^[a-z]+\(52-NN\)!?:`; `cut-release.sh` reads the `!` in subjects for the major bump (it cannot see footers, backlog 999.220).
**Source:** 52-02-SUMMARY.md, 52-05-SUMMARY.md, 52-09..52-17-SUMMARY.md

---

### Release preflight is a selftested Rust tool, not shell
The release checks moved to `tools/release-preflight`, each of six checks with a named passing and failing fixture, called from `scripts/cut-release.sh`.

**Rationale:** D-02 and the operator's avoid-shell-check-gates rule; reading pins with the `toml` crate avoids depending on `version.rs`, which 52-13 deleted.
**Source:** 52-06-SUMMARY.md

---

### A run ends as finished after Validate, and finishing touches no git
`finish_run` records the finish with state kept and branch kept, makes no git call, and does not read `stop_until`.

**Rationale:** Ship and the after-ship hooks are gone; the policy sign-off "finishing never merges, pushes, tags, deletes or resets" passed on the evidence.
**Source:** 52-08-SUMMARY.md, 52-UAT.md

---

### A permitted finish wins over a late stop
`finish_run` no longer stops at safe point 6; the supervisor answers the pending stop `already_finished`. This reverses 52-08's choice to honour the stop over the finish.

**Rationale:** Operator decision 2026-11-11.
**Source:** 52-29-SUMMARY.md, 52-08-SUMMARY.md

---

### Validate passes only when GSD's reader reports passed
The verdict comes only from `gsd_reader::verification_status`; DevFlow never parses VERIFICATION.md. An agent pass over any status but `passed` is a counted Validate failure, so the caps bound an agent that always says pass.

**Rationale:** Nothing may say passed unless GSD did; a reader failure is never a pass.
**Source:** 52-22-SUMMARY.md, 52-UAT.md

---

### One finish check, one routing table, one answer dispatch
After a Validate every route comes from `next_step`'s table (except a `failure_cap` gate that opens ahead of it); answers go through `screen_answer` and one `dispatch_answer`.

**Rationale:** The D-45/D-46 reset after five failed review rounds: stop DevFlow re-judging GSD and collapse the parallel routers.
**Source:** 52-29-SUMMARY.md

---

### Gate abort is an explicit answer kind
`gate_respond` takes a `GateAnswerKind` (Approve, Reject, Abort); approve+abort is refused in the shared writer (`GateError::ApprovedAndAborted`), and a note no longer means abort.

**Rationale:** The CLI cannot express a contradictory answer, and no writer can skip the refusal.
**Source:** 52-19-SUMMARY.md

---

### Rate limit without a fallback waits at a gate with one automatic relaunch
The cron-instructions file is removed; a rate limit opens a `rate_limit` gate carrying `not_before`, and a timer in the gate wait relaunches at most once per gate and once per run.

**Rationale:** D-27/D-38; the timer answers only rate-limit gates (policy sign-off passed; no real quota denial has exercised it, backlog 999.212).
**Source:** 52-09-SUMMARY.md, 52-10-SUMMARY.md, 52-31-SUMMARY.md, 52-32-SUMMARY.md, 52-UAT.md

---

## Lessons

### A witness that both shapes pass discriminates nothing
The capture-shape witness's original assertions were all satisfied by a stream-shape run; only asserting the `stage_launched` launch label is `capture` made the negative control fail.

**Context:** Found by running the negative control before trusting the test.
**Source:** 52-04-SUMMARY.md

---

### A removal test can pass for the wrong reason
`start --phase 1 --worktree x --dry-run` was already refused for its missing `--mode`, so a bare "unexpected argument" check was weak; `REMOVED_FLAG_INVOCATIONS` pairs each argv with the token clap must reject.

**Context:** Removed-flag tests in phase7_cli.
**Source:** 52-03-SUMMARY.md

---

### doc_check forces doc edits into the same commit as the source removal
Removing `devflow ship`, the hooks section and `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` failed `doc_check` until the doc mentions were removed in the same task, not the later task the plans ordered.

**Context:** 52-11 and 52-12 each moved doc work from Task 2 into Task 1.
**Source:** 52-11-SUMMARY.md, 52-12-SUMMARY.md

---

### Plan file lists miss callers; the build finds them
Several plans touched files outside `files_modified` (preflight.rs, gates.rs, mode.rs, e2e suites, workflow.rs) because deletions broke builds or clippy `-D warnings` (never-constructed variants, never-read fields).

**Context:** Deletion plans 52-03, 52-07, 52-10, 52-15, 52-16, 52-17.
**Source:** 52-07-SUMMARY.md, 52-10-SUMMARY.md, 52-16-SUMMARY.md, 52-17-SUMMARY.md

---

### Unbounded test runs hang on 7-day gate timeouts
A plain `cargo nextest run` hung on tests waiting at unanswered gates with the 7-day default timeout; one run left processes the agent was not permitted to kill. Later runs used a nextest slow-timeout and short gate timeouts.

**Context:** 52-16 and 52-22 test runs.
**Source:** 52-16-SUMMARY.md, 52-22-SUMMARY.md

---

### Mutation testing found 22 unpinned edges in planned tests
`cargo mutants` left 22 survivors against the planned reset-time tests (field ranges, fractions, leap rules); added tests brought them to 2.

**Context:** Global Rust harness rule applied after Task 2.
**Source:** 52-01-SUMMARY.md

---

### Exit codes through a pipe and ungated commits lie
A verify run piped through `tail` reported `tail`'s exit code; one commit was made in a chain where clippy's failure did not stop it.

**Context:** Re-run unpiped; the clippy fix landed in a follow-up commit.
**Source:** 52-02-SUMMARY.md, 52-29-SUMMARY.md

---

### Low host memory can kill the full check before it finishes
The verifier's `scripts/check.sh all` was killed by the low-memory reaper; the orchestrator re-ran each step in the foreground with `RUST_TEST_THREADS=4`, `CARGO_BUILD_JOBS=4` and got exit 0.

**Context:** SC2 was uncertain only for this reason.
**Source:** 52-VERIFICATION.md, 52-UAT.md

---

## Patterns

### Grow the removed-surface lists as the RED step of each cut
Each removed verb is appended to `REMOVED_SUBCOMMANDS` and each removed flag to `REMOVED_FLAG_INVOCATIONS` in phase7_cli.rs, and the help snapshot is regenerated.

**When to use:** Any plan that removes a CLI verb or flag.
**Source:** 52-02-SUMMARY.md, 52-03-SUMMARY.md, 52-07-SUMMARY.md

---

### Delete the subject's test, re-point the stand-in's test
A test whose subject was the removed feature is deleted; a test that used the feature only as a stand-in (a field, a stage, a reader) is re-pointed at another field, at Validate/Code, or at the underlying record.

**When to use:** Removing a feature, field or stage that many tests touch.
**Source:** 52-10-SUMMARY.md, 52-13-SUMMARY.md, 52-14-SUMMARY.md, 52-15-SUMMARY.md

---

### Removed-surface tokens are split with concat! in doc_check.rs
The removed-surface scan holds its tokens and planted lines as `concat!` splits and @-templates so the file never contains a whole token; controls found beyond the list get the same split.

**When to use:** A source scan that must not match itself.
**Source:** 52-18-SUMMARY.md

---

### A writer seam before the RED test
When output is a bare `println!`, add a behaviour-preserving `_to(&mut dyn Write, …)` refactor commit first, so the RED test compiles and fails on behaviour, not on a missing seam.

**When to use:** Testing operator-facing output under the TDD gate.
**Source:** 52-21-SUMMARY.md, 52-23-SUMMARY.md

---

### A deleted pub State field gets a load-and-save test
One test loads a JSON state carrying the removed key and asserts it is gone after a save, so 3.x state files still load.

**When to use:** Removing a persisted field with no migration.
**Source:** 52-14-SUMMARY.md, 52-24-SUMMARY.md

---

### Every gate context: way-on sentence, then cause
Every gate context is `[never-silent]?` + way-on sentence + cause; nothing after the sentence names an answer. Gates are recognised by `gate_kind`, never by context text; the file stores the reason whole and every rendering but `gate show` cuts to 450 characters.

**When to use:** Opening any gate.
**Source:** 52-30-SUMMARY.md, 52-31-SUMMARY.md, 52-33-SUMMARY.md

---

### The gap-closure loop inside a run
Validate `gaps_found` → Plan `--gaps` → post-check → Code `--gaps-only` → Validate; the marker is set in the Validate→Plan transition commit and cleared only by a passed read, the finish, an abort or a non-gaps fix route.

**When to use:** Driving a `gaps_found` verification to `passed` without leaving the run.
**Source:** 52-27-SUMMARY.md, 52-28-SUMMARY.md

---

### Capture-shape launches in tests run codex
To get the capture launch shape in a test, use `AgentKind::Codex` with a fake from `common::capture_agent_bin*` and commit a phase CONTEXT.md, since `start` refuses codex without one.

**When to use:** Tests of the capture shape after the legacy Claude launch path was removed.
**Source:** 52-04-SUMMARY.md, 52-05-SUMMARY.md

---

## Surprises

### Plan premises were wrong about what existed
Plans asked to remove a `version` import in commands.rs, OPERATIONS.md preflight rows, README evidence mentions and README/configuration yes-ship mentions that did not exist; the 3.0.0 control command also needed `--mode`.

**Impact:** No change needed for those items; recorded as plan-premise deviations. The control was run twice and both outputs recorded.
**Source:** 52-02-SUMMARY.md, 52-13-SUMMARY.md, 52-14-SUMMARY.md

---

### 52-27's counted gate did not count
The `no_gap_closure_plan` gate's doc said it counts as a failure, but the code opened it without the streak update.

**Impact:** 52-28 added the `record_validate_failure` call; auto mode now moves both counters on a gaps Plan pass with no new gap plan.
**Source:** 52-28-SUMMARY.md

---

### At equal deadlines the gate timed out over the timer's own answer
The rate-limit timer wrote its answer and the wait then saw its lifetime due at the same second, returning TimedOut.

**Impact:** The wait now reads the answer before re-checking the lifetime; found by the U8 e2e.
**Source:** 52-32-SUMMARY.md

---

### The display bound had to grow from 300 to 450
Putting the way-on sentence first left no room for the idle-timeout cause inside 300 characters; plans written before that (52-30) named 300.

**Impact:** `GATE_CONTEXT_DISPLAY_CHARS` is 450 and the notify cap shares it; 52-30's plan verify line names a test that no longer exists (exits 4 if run literally).
**Source:** 52-33-SUMMARY.md, 52-30-SUMMARY.md

---

### A guard restore would have revived a leaked true
The plan's "restore the bytes it read" would have put back a leaked `true` after an ineligible launch's `engage(false)`.

**Impact:** Bytes whose flag reads `true` are never kept.
**Source:** 52-29-SUMMARY.md

---

### A RED test run executed a real machine-wide reap
The first Task 2 RED run in 52-03 ran a real `--reap-strays` pass on the host against a binary that still had the flag.

**Impact:** The sweep invocation in tests now carries `--dry-run`; a guard test checks no suite runs a real reap.
**Source:** 52-03-SUMMARY.md, 52-18-SUMMARY.md
