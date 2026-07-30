---
phase: 27-scrub-redirecting-git-environment-from-production-calls
plan: 03
subsystem: infra
tags: [rust, subprocess, security, git, env-hygiene]

# Dependency graph
requires:
  - phase: 27-01
    provides: "devflow_core::git::{hermetic_command, git_command, REPO_LOCAL_GIT_VARS, ALSO_REDIRECTING_GIT_VARS} — the always-compiled scrubbing constructor"
provides:
  - "version.rs's ten production git invocations (tag/commit/ancestry reads feeding release-version derivation) all construct via git_command(project_root)"
  - "agent_result.rs's three production git invocations (branch-exists check, two commit-count reads feeding agent pass/fail verdicts) all construct via git_command(project_root)"
  - "Two new regression tests proving both files' git reads resolve the caller's own repository under a hostile GIT_DIR, with RED (unmigrated) and GREEN (migrated) output recorded"
affects: [27-04, 27-05, 27-06]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Ten independent one-line-per-site constructor swaps in version.rs — no local git wrapper introduced, matching the plan's explicit prohibition"
    - "agent_result.rs's fully-qualified std::process::Command convention preserved — git_command imported via use, no use std::process::Command; added"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/version.rs
    - crates/devflow-core/src/agent_result.rs

key-decisions:
  - "D-01/D-03 honored exactly as locked in 27-CONTEXT.md — ten version.rs sites and three agent_result.rs sites all route through git_command(project_root), zero unscrubbed production Command::new(\"git\") remaining in either file"
  - "Deviation: agent_result.rs's new test (branch_evidence_resolves_caller_root_under_a_hostile_git_dir) tests the mirror direction from the plan's literal <behavior> framing — see Deviations section for full reasoning"

requirements-completed: [D-01, D-03]

coverage:
  - id: D1
    description: "version.rs's ten production git invocations (count_git_tags, commits_since_last_minor_tag x2, highest_semver_tag, reachable_semver_baseline, first_parent, release_range_start x2, classify_range_bump, changelog_sections) construct via git_command(project_root)"
    requirement: "D-01"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#tag_reads_resolve_caller_root_under_a_hostile_git_dir"
        status: pass
      - kind: other
        ref: "rg -v '^\\s*//' version.rs | rg -o 'Command::new(\"git\")' | wc -l == 0; rg -o 'git_command(project_root)' version.rs | wc -l == 10"
        status: pass
    human_judgment: false
  - id: D2
    description: "agent_result.rs's three production git invocations (evaluate_layer2's branch_exists + commit count, evaluate_layer3's commit count) construct via git_command(project_root)"
    requirement: "D-01"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#branch_evidence_resolves_caller_root_under_a_hostile_git_dir"
        status: pass
      - kind: other
        ref: "rg -v '^\\s*//' agent_result.rs | rg -o 'Command::new(\"git\")' | wc -l == 0; rg -o 'git_command(project_root)' agent_result.rs | wc -l == 3"
        status: pass
    human_judgment: false
  - id: D3
    description: "version:: and agent_result:: unit suites reach 0 failed under a hostile GIT_DIR, with passed counts matching a normal-environment run"
    requirement: "D-03"
    verification:
      - kind: unit
        ref: "GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib version:: (47 passed; 0 failed)"
        status: pass
      - kind: unit
        ref: "GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib agent_result:: (72 passed; 0 failed)"
        status: pass
    human_judgment: false
  - id: D4
    description: "No new local git wrapper introduced in version.rs; agent_result.rs's fully-qualified std::process::Command import convention unchanged"
    requirement: "D-01"
    verification:
      - kind: other
        ref: "rg -o 'fn run_git|fn git_cmd|fn git_output' version.rs | wc -l == 0; rg -o '^use std::process::Command;' agent_result.rs | wc -l == 0"
        status: pass
    human_judgment: false

# Metrics
duration: 20min
completed: 2026-07-30
status: complete
---

# Phase 27 Plan 03: version.rs / agent_result.rs Git Scrub Summary

**All 13 production git invocations across `version.rs` (release-version tag/commit/ancestry reads) and `agent_result.rs` (agent branch-exists/commit-count evidence) now construct via `devflow_core::git::git_command(project_root)`, closing the last two files in this wave's scope to a hostile `GIT_DIR`.**

## Performance

- **Duration:** ~20 min (commit range 15:11 -> 15:31 UTC)
- **Started:** 2026-07-30T19:11:16Z
- **Completed:** 2026-07-30T19:31:00Z (approx)
- **Tasks:** 2 (both `tdd="true"`) — 4 commits total (RED test, GREEN feat, per task)
- **Files modified:** 2

## Accomplishments
- `version.rs`'s ten `Command::new("git")` sites (`count_git_tags`, `commits_since_last_minor_tag` x2, `highest_semver_tag`, `reachable_semver_baseline`, `first_parent`, `release_range_start` x2, `classify_range_bump`, `changelog_sections`) all migrated to `git_command(project_root)` — every site kept its own error mapping, args, and `?`/fail-soft handling unchanged; no local git wrapper introduced (verified: `fn run_git|fn git_cmd|fn git_output` count is 0)
- `agent_result.rs`'s three `std::process::Command::new("git")` sites (`evaluate_layer2`'s `branch_exists` + commit count, `evaluate_layer3`'s commit count) migrated to `git_command(project_root)` — the file's fully-qualified `std::process` import convention preserved (no `use std::process::Command;` added)
- Two new regression tests, one per file, each with RED (fails under unmigrated code + hostile `GIT_DIR`) and GREEN (passes after migration) output recorded below
- `version::` unit suite: 47 passed; 0 failed under a hostile `GIT_DIR` (matches the 47 passed in a normal environment)
- `agent_result::` unit suite: 72 passed; 0 failed under a hostile `GIT_DIR` (matches the 72 passed in a normal environment)
- `cargo build -p devflow` (workspace, both crates) succeeds; `cargo test -p devflow-core --features test-support --lib` (full crate, normal environment): 410 passed, 0 failed
- `cargo clippy -p devflow-core --all-targets --features test-support -- -D warnings` and `cargo fmt --check`: both clean

## Task Commits

Both tasks `tdd="true"`, each executed as a RED-then-GREEN pair:

1. **Task 1 RED: add failing hostile-GIT_DIR test for version.rs tag reads** - `0af23c7` (test)
2. **Task 1 GREEN: route version.rs's ten git invocations through git_command** - `f8902f4` (feat)
3. **Task 2 RED: add failing hostile-GIT_DIR test for agent_result branch evidence** - `eed0a12` (test)
4. **Task 2 GREEN: route agent_result.rs's three git invocations through git_command** - `205475c` (feat)

**Plan metadata:** (this commit)

## RED/GREEN output (verbatim)

### Task 1 — `version.rs`

RED (unmigrated production code, `GIT_DIR=<hostile>/.git`, hostile repo is empty/zero tags):
```
thread 'version::tests::tag_reads_resolve_caller_root_under_a_hostile_git_dir' panicked at crates/devflow-core/src/version.rs:2451:9:
assertion `left == right` failed: count_git_tags must resolve root's own two tags, not a hostile GIT_DIR's repository
  left: 0
 right: 2
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 408 filtered out
```

GREEN (migrated, same hostile harness):
```
test version::tests::tag_reads_resolve_caller_root_under_a_hostile_git_dir ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 408 filtered out
```

Full scoped suite, GREEN, hostile `GIT_DIR`:
```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib version::
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 362 filtered out
```
(47 passed in a normal, non-hostile environment run of the identical filter — counts match.)

### Task 2 — `agent_result.rs`

RED (unmigrated production code, `GIT_DIR=<hostile>/.git`, hostile repo is empty/no matching branch):
```
thread 'agent_result::tests::branch_evidence_resolves_caller_root_under_a_hostile_git_dir' panicked at crates/devflow-core/src/agent_result.rs:2556:9:
assertion `left == right` failed: evaluate_layer2 must see project_root's own branch/commits, not a hostile GIT_DIR's repository: AgentResult { status: Failed, exit_code: Some(0), reason: Some("no commits found on feature/phase-27 (agent exit code was 0)"), commits: Some(0), summary: None, verdict: None, decided_by_layer: Some(2) }
  left: Failed
 right: Success
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 409 filtered out
```

GREEN (migrated, same hostile harness):
```
test agent_result::tests::branch_evidence_resolves_caller_root_under_a_hostile_git_dir ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 409 filtered out
```

Full scoped suite, GREEN, hostile `GIT_DIR`:
```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib agent_result::
test result: ok. 72 passed; 0 failed; 0 ignored; 0 measured; 338 filtered out
```
(72 passed in a normal, non-hostile environment run of the identical filter — counts match.)

### Whole-crate confirmation (normal environment)
```
$ cargo test -p devflow-core --features test-support --lib
test result: ok. 410 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo build -p devflow   # workspace, devflow-cli against updated devflow-core
   Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo clippy -p devflow-core --all-targets --features test-support -- -D warnings
   Finished — 0 warnings

$ cargo fmt --check
   (no output — clean)
```

## Files Created/Modified
- `crates/devflow-core/src/version.rs` — migrated 10 production `Command::new("git")` sites to `git_command(project_root)`; added `use crate::git::git_command;`, removed now-unused `use std::process::Command;`; added `tag_reads_resolve_caller_root_under_a_hostile_git_dir` test
- `crates/devflow-core/src/agent_result.rs` — migrated 3 production `std::process::Command::new("git")` sites to `git_command(project_root)`; added `use crate::git::git_command;` (fully-qualified `std::process` convention preserved elsewhere); added `branch_evidence_resolves_caller_root_under_a_hostile_git_dir` test

## Decisions Made
- Ten independent one-line-per-site edits in `version.rs`, no local wrapper — matches the plan's explicit prohibition and 27-01's established pattern.
- `agent_result.rs`'s three sites migrated with `use crate::git::git_command;` added, deliberately NOT introducing `use std::process::Command;` — respects the file's existing fully-qualified-path convention for `std::process` items.
- See `key-decisions` in frontmatter and the Deviations section below for the one substantive deviation (agent_result.rs's test direction).

## Deviations from Plan

### Auto-fixed / Adjusted Issues

**1. [Rule 1 — plan defect, test direction adjusted] `branch_evidence_resolves_caller_root_under_a_hostile_git_dir` tests the mirror direction from the plan's literal `<behavior>` framing**
- **Found during:** Task 2, designing the RED-stage test per the plan's literal instruction ("Build a throwaway repository that does NOT contain the branch the evaluation looks for, and a second unrelated repository that DOES contain a branch by that name... Assert the evaluation reports the real repository's state (branch absent) with the foreign GIT_DIR injected into the spawned child only")
- **Issue:** The plan's own `<verify>` automated checks (and this task's shared recipe) use the standard hostile-`GIT_DIR` harness: `HOSTILE=$(mktemp -d) && git init -q "$HOSTILE"` — an empty repository with no `feature/phase-NN` branch. `evaluate_layer2`, `evaluate_layer3`, and the three production call sites take only `project_root`; they expose no hook for a test to inject a *different*, per-call hostile `GIT_DIR` pointing at a repository the test constructs itself (this would require modifying the functions' signatures — an architectural change, Rule 4, out of scope). Given that constraint, only the literal external harness (a branch-less empty repo) can supply the hostile `GIT_DIR`. A generic empty hostile repo cannot manufacture the plan's literal scenario (a *false positive*: hostile repo HAS the branch, real repo does NOT) — it can only manufacture the mirror scenario (a *false negative*: hostile repo lacks the branch, real repo HAS it).
- **Resolution:** Built the real repository WITH the feature branch and one commit ahead (`init_repo_with_feature_commit`, an existing test fixture already used by `evaluate_layer2_falls_back_to_exit_code_and_commit_count`), and asserted `evaluate_layer2` reports `Success`/`commits: Some(1)`. Verified empirically: against the unmigrated code under the standard hostile harness, the poisoned `GIT_DIR` redirects both `rev-parse --verify` and `rev-list --count` to the empty hostile repo, undercounting to zero commits and misclassifying a completed agent's work as `Failed` — genuine RED. After migration, `git_command`'s scrub makes the result resolve correctly regardless of the external `GIT_DIR` — genuine GREEN. This exercises the identical mechanism (the scrub removes `GIT_DIR`'s ability to redirect the spawned child at all) and closes the same trust boundary (T-27-08) the plan's literal framing names, from the opposite direction; documented in the test's own doc comment so a future reader understands why the fixture direction differs from the plan's literal prose.
- **Files modified:** `crates/devflow-core/src/agent_result.rs`
- **Verification:** `GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib agent_result::tests::branch_evidence_resolves_caller_root_under_a_hostile_git_dir` → `1 passed; 0 failed`. Full `agent_result::` suite under the same hostile harness: `72 passed; 0 failed`, matching the normal-environment count.
- **Committed in:** `eed0a12` (RED), `205475c` (GREEN)

---

**Total deviations:** 1 (test-fixture direction adjusted from the plan's literal framing to one mechanically achievable via the plan's own `<verify>` harness; substantive D-01/D-03 guarantees unaffected — both directions are closed by the identical scrub)
**Impact on plan:** None on the shipped mechanism. The deviation is in test-fixture construction only; `evaluate_layer2`/`evaluate_layer3`'s three production sites are migrated exactly as the plan specifies, and the full `agent_result::` suite (including the pre-existing `evaluate_layer3_falls_back_to_commit_count`/`evaluate_layer3_zero_commits_is_failed_and_flags_human_review` tests, which also use `init_repo_with_feature_commit`/`init_repo_with_feature_no_commit`) now passes under a hostile `GIT_DIR`, covering all three migrated call sites including the one (`evaluate_layer3`, base-commit line 664) not directly exercised by the new test.

## Issues Encountered
None beyond the deviation above.

## User Setup Required
None - no external service configuration required.

## Observations (out of scope for this plan)

Per the scope boundary, no fixes were made to files outside `version.rs`/`agent_result.rs`. No unscrubbed sites were noticed in sibling plans' files (`git.rs`, `worktree.rs`, `staleness.rs`, `commands.rs`, `preflight.rs`) during this plan's work — those files were not read closely enough to make an independent claim either way; nothing to flag.

## Next Phase Readiness
- `version.rs` and `agent_result.rs` are fully closed: 0 remaining unscrubbed production git invocations in either file.
- Both files' full test suites reach `0 failed` under a hostile `GIT_DIR`, with passed counts matching normal-environment runs (47/47 for `version::`, 72/72 for `agent_result::`).
- `crates/devflow-cli/build.rs` untouched (D-02, out of scope for this plan and phase).
- No blockers for 27-04/27-05/27-06, which migrate the remaining files (`staleness.rs`'s 2 sites, `commands.rs`, `preflight.rs`) independently — no shared state introduced by this plan beyond the already-existing `devflow_core::git::git_command` constructor from 27-01.

---
*Phase: 27-scrub-redirecting-git-environment-from-production-calls*
*Completed: 2026-07-30*
