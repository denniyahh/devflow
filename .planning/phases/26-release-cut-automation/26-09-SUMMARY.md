---
phase: 26-release-cut-automation
plan: 09
subsystem: cli
tags: [cli, rust, security, release-automation, tdd, tracer, gap-closure, c-06]

# Dependency graph
requires:
  - phase: 26-release-cut-automation (26-07)
    provides: "the `release --execute --yes-release` dispatch arm and the `Command::Sync` arm this plan re-points at a different resolver"
  - phase: 26-release-cut-automation (26-06)
    provides: "devflow_core::release::run_release's four entry guards, which only mean what they say once they run against the invoking repository"
provides:
  - "crates/devflow-cli/src/main.rs::mutating_project_root — git-toplevel root resolution for mutating commands, refusing on any redirect"
  - "crates/devflow-cli/tests/project_root_guard.rs — the worktree-beside-parent regression suite that distinguishes the fix from the defect"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Two resolvers, split by mutability and documented as a decision rather than an accident: read-only commands keep `project_root`'s upward `.devflow` walk (they legitimately need to find their owning project from a subdirectory), while `release --execute` and `sync` resolve via `git rev-parse --show-toplevel` from the supplied path and refuse on any mismatch. The `Release` dispatch arm carries an inline comment saying the asymmetry is deliberate, because a future reader would otherwise 'simplify' the two back together."
    - "Structural over detected: resolving to the repository the operator is standing in makes a silent redirect impossible rather than merely caught, which is the latitude D-13 explicitly grants."
    - "Canonicalize BOTH sides before comparing paths — a raw-vs-resolved comparison produces a spurious refusal on symlinked checkouts, and a spurious refusal in a release tool teaches operators to route around the guard (T-26-52)."
    - "No bypass channel: no `--force`-shaped flag, no environment variable. An escape hatch would recreate C-06 for exactly the operator most likely to be in a hurry."
    - "A regression test for a defect that ALREADY exits non-zero must assert which repository the refusal concerns, not the exit status. Recorded as a comment in the test file so the weaker assertion cannot creep back in."

key-files:
  created:
    - crates/devflow-cli/tests/project_root_guard.rs
  modified:
    - crates/devflow-cli/src/main.rs
    - CONTRIBUTING.md

key-decisions:
  - "CONTRADICTION WITH THE PLAN AND WITH D-13, resolved by telling the truth: both say the second remedy is `--project`. No such flag exists. `project` is a POSITIONAL argument on `Command::Release` and `Command::Sync` (`#[arg(default_value = \".\")]`, no `long`), verified against the built binary — `devflow release --check --project /tmp` answers `error: unexpected argument '--project' found`. The refusal message and the CONTRIBUTING.md bullet therefore name the command's `[PROJECT]` argument instead. Adding a real `--project` flag was rejected: it would change `devflow release --help`, break `release_check.rs`/`release_execute.rs`'s positional invocations, and exceed a plan whose stated scope is 'root resolution and nothing else'. Worth noting the doc_check invariant would NOT have caught the lie — its flag rule falls back to matching the `project:` field name in source, so `--project` in a scoped doc passes while being unusable."
  - "`git rev-parse --show-toplevel` is invoked with a plain `std::process::Command`, not a scrubbed one. That matches `release.rs`'s and `sync.rs`'s own git invocations; the hermeticity discipline (999.37) is a TEST-fixture concern, and `git_env_hermeticity.rs` already fails the suite fast if the redirecting variables are present at launch. Every git call inside the new test file goes through `devflow_core::test_support::git_command`."
  - "The worktree fixture's parent checkout deliberately has NO remote, so even a run that redirected all the way through could not push anything anywhere — the test cannot mutate a real remote even if the resolver regressed."

# HONEST: this gap-closure plan closed review finding C-06 only. The plan's
# frontmatter lists requirements ["999.25", "999.52"]; both were already claimed
# complete by 26-07-SUMMARY.md and 26-04-SUMMARY.md respectively, and re-listing
# them here would restate someone else's delivery as this plan's.
requirements-completed: []

coverage:
  - id: D1
    description: "A mutating command never silently retargets: `release --execute --yes-release` from a linked phase worktree refuses on the WORKTREE's own state, and the parent checkout's HEAD, porcelain status, and tag list are byte-identical across the call (closes 26-REVIEW.md C-06)"
    requirement: "26-REVIEW.md C-06"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/project_root_guard.rs#release_execute_from_a_worktree_refuses_on_the_worktree_not_the_parent"
        status: pass
    human_judgment: false
  - id: D2
    description: "`devflow sync` from the same worktree likewise refuses without mutating the parent (D-13 names both commands)"
    requirement: "26-CONTEXT.md D-13"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/project_root_guard.rs#sync_from_a_worktree_does_not_mutate_the_parent"
        status: pass
    human_judgment: false
  - id: D3
    description: "The refusal is loud and actionable: it names the invoking path, the resolved repository root, and both remedies"
    requirement: "26-CONTEXT.md D-13"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::mutating_project_root_refuses_a_subdirectory_and_names_both_paths"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/project_root_guard.rs#release_execute_from_a_subdirectory_names_both_paths"
        status: pass
    human_judgment: false
  - id: D4
    description: "`.devflow` no longer participates on the mutating path — a nested git repository inside a `.devflow`-carrying ancestor resolves to the nested repository"
    requirement: "26-REVIEW.md C-06"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::mutating_project_root_does_not_consult_devflow"
        status: pass
    human_judgment: false
  - id: D5
    description: "Read-only commands are unchanged: `release --check` still walks up to the owning `.devflow` from a subdirectory and runs its preflight, and `project_root`'s own unit test passes with its source unmodified (D-13's carve-out)"
    requirement: "26-CONTEXT.md D-13"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/project_root_guard.rs#release_check_from_a_subdirectory_still_walks_up"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::project_root_walks_up_to_nearest_devflow_ancestor (1 passed; git diff shows no deletion inside it)"
        status: pass
    human_judgment: false
  - id: D6
    description: "No bypass channel was added and no read-only command was rerouted: `mutating_project_root` has exactly two non-test call sites, `release --check` still uses `project_root(project)`, and no new `--force`/env-var gate on root resolution exists"
    requirement: "26-CONTEXT.md D-13"
    verification:
      - kind: other
        ref: "rg -n 'mutating_project_root\\(' main.rs -> definition + 2 call sites (+4 unit-test uses); rg -n 'force|DEVFLOW_.*ROOT|allow_redirect' main.rs -> only the pre-existing Start/Parallel/Cleanup/Ship `force` fields"
        status: pass
    human_judgment: false
  - id: D7
    description: "The CLI surface is unchanged: `crates/devflow-cli/tests/snapshots/devflow-help.txt` is byte-identical and `help_snapshot` passes"
    requirement: "26-09-PLAN.md prohibition"
    verification:
      - kind: other
        ref: "git diff -- crates/devflow-cli/tests/snapshots/devflow-help.txt -> empty; cargo test -p devflow --test help_snapshot -> 1 passed; 0 failed"
        status: pass
    human_judgment: false
  - id: D8
    description: "CONTRIBUTING.md's release preconditions list states that mutating release commands must be run from the repository being released, and every documentation invariant stays green"
    requirement: "26-CONTEXT.md D-13"
    verification:
      - kind: other
        ref: "CONTRIBUTING.md § Cutting a Release, one bullet added to the existing preconditions list (git diff --stat: 1 file, 8 insertions, no new heading); cargo test --workspace --features devflow-core/test-support --lib doc_check:: -> 6 passed; 0 failed"
        status: pass
    human_judgment: false
  - id: D9
    description: "backstop truth: an operator who runs `devflow release --execute --yes-release` from a phase worktree is told which repository would have been acted on, rather than discovering it from the resulting commit"
    verification: []
    human_judgment: true
    rationale: "Marked `verification: backstop` by the plan itself. The mechanism is fully covered by D1/D3 against a real linked worktree and the real binary; what remains human is an operator reading the message on their own machine. Not a hermetic assertion."

# Metrics
duration: 30min
completed: 2026-07-30
status: complete
---

# Phase 26 Plan 09: A mutating command acts on the repository you named (C-06) Summary

**`devflow release --execute --yes-release` and `devflow sync` no longer silently retarget to an ancestor checkout: they resolve the repository the operator is standing in via `git rev-parse --show-toplevel`, or refuse and name both candidate paths — so the executor's four entry guards finally test the repository the mutation would land on.**

## Performance

- **Duration:** ~30 min
- **Tasks:** 3/3
- **Files modified:** 3 (1 new, 2 modified)

## Accomplishments

- `mutating_project_root` added directly beneath `project_root` in `main.rs`, with a doc comment carrying the whole rationale — C-06's failure mode, D-13's refusal requirement, why `.devflow` is deliberately not consulted (neither `release.rs` nor `sync.rs` reads it; they need a git repository root), and that there is intentionally no bypass flag. It canonicalizes the supplied path, runs `git rev-parse --show-toplevel` with `current_dir` set to it, canonicalizes the reported toplevel, and compares canonical-to-canonical. A non-zero exit or a spawn failure refuses saying the path is not inside a git repository — never a fallback to the upward walk, which is the silent redirect being removed.
- Wired into exactly two call sites: the `execute && yes_release` branch of `Command::Release` and the `Command::Sync` arm. The `check` branch keeps `project_root`, and the `Release` arm carries an inline comment recording that the two branches resolve their root by different rules on purpose.
- `project_root` and `project_root_walks_up_to_nearest_devflow_ancestor` are untouched — `git diff` over that test shows no deleted line, and it passes under `-- --exact`. That is D-13's read-only carve-out and its evidence, both intact.
- Four unit tests (`mutating_project_root_accepts_the_repository_root`, `..._refuses_a_subdirectory_and_names_both_paths`, `..._refuses_outside_a_repository`, `..._does_not_consult_devflow`), all using hermetic `git_command`.
- `crates/devflow-cli/tests/project_root_guard.rs` (new, 4 tests) drives the real binary against the exact topology the review verified on disk: a clean parent on `develop` owning the only `.devflow`, and a **linked** worktree created with `git worktree add` on `feature/phase-99`, dirty, with no `.devflow`. The parent has no remote at all, so no test in this file can reach a real push.
- One bullet added to CONTRIBUTING.md's existing "Environment preconditions" list — no new section, no change to the seven per-step annotations.

## Task Commits

Each task was committed atomically. The regression suite was written and run **before** the resolver fix, producing a genuine, defect-shaped RED.

1. **Task 1: A mutating command resolves the repository it was invoked in, or refuses** — `4b236f4`
   - RED (captured from Task 2's suite, run first against the unfixed binary): 3 of 4 failed, and with the defect's own signature — `expected the refusal to be about the WORKTREE's own dirty tree, got: error: no git remote configured`. The binary had walked up to the parent, found it clean, on `develop`, and remote-less, and refused on *its* state. `release_execute_from_a_subdirectory_names_both_paths` failed too; `release_check_from_a_subdirectory_still_walks_up` passed, which is correct — it pins pre-existing carve-out behavior.
   - GREEN: `cargo test -p devflow --bin devflow mutating_project_root` → **4 passed; 0 failed** (names confirmed with `-- --list` first). `tests::project_root_walks_up_to_nearest_devflow_ancestor -- --exact` → **1 passed; 0 failed**. `release_check` **11 passed**, `help_snapshot` **1 passed** with the snapshot file byte-identical. Full bin target **234 passed; 0 failed** (was 230).
2. **Task 2: The worktree regression — the guards now test the repository they are guarding** — `bb93c57`
   - `cargo test -p devflow --test project_root_guard` → **4 passed; 0 failed**. One `cargo clippy` fixup (`needless_borrows_for_generic_args` on a `&parent.join(...)`) applied before the commit.
3. **Task 3: Say it in the preconditions list** — `5cd9580`
   - `cargo test --workspace --features devflow-core/test-support --lib doc_check::` → **6 passed; 0 failed**. `git diff --stat CONTRIBUTING.md` → 1 file, 8 insertions, single hunk, no new heading.

_No plan-metadata commit in this worktree — STATE.md/ROADMAP.md are updated centrally after the wave merges._

## Files Created/Modified

- `crates/devflow-cli/tests/project_root_guard.rs` (new) — 4 integration tests plus the fixture that reproduces C-06's topology
- `crates/devflow-cli/src/main.rs` — `mutating_project_root` + 4 unit tests; two dispatch call sites re-pointed; one explanatory comment on the `Release` arm
- `CONTRIBUTING.md` — one bullet in the existing release preconditions list

## Verification Results

Real counts, from the commands themselves.

| Command | Result |
|---|---|
| `cargo test -p devflow --bin devflow mutating_project_root` | **4 passed; 0 failed** |
| `cargo test -p devflow --bin devflow tests::project_root_walks_up_to_nearest_devflow_ancestor -- --exact` | **1 passed; 0 failed** |
| `cargo test -p devflow --test project_root_guard` | **4 passed; 0 failed** |
| `cargo test -p devflow --test release_check` | **11 passed; 0 failed** (unchanged) |
| `cargo test -p devflow --test release_execute` | **8 passed; 0 failed** (unchanged) |
| `cargo test -p devflow --test help_snapshot` | **1 passed; 0 failed**, snapshot byte-identical |
| `cargo test --workspace --features devflow-core/test-support --lib doc_check::` | **6 passed; 0 failed** |
| `cargo test --workspace` (devflow bin) | **234 passed; 0 failed** (pre-plan: 230) |
| `cargo test --workspace` (devflow-core lib) | **445 passed; 0 failed** |
| `cargo test --workspace` (every other target) | **0 failed** |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |

## Decisions Made

See `key-decisions` in the frontmatter — chiefly the `--project` contradiction, resolved by naming the real CLI surface.

## Deviations from Plan

1. **[Rule 3 — plan contradicts the code] The `--project` remedy does not exist.** The plan (Task 1, Task 2, Task 3, and its acceptance criterion "that flag really exists on `Command::Release` and `Command::Sync`") and D-13 itself both say the operator's second remedy is `--project`. It is not a flag: `project` is a **positional** argument on both commands. Verified against the built binary — `devflow release --check --project /tmp` → `error: unexpected argument '--project' found`, `Usage: devflow release --check [PROJECT]`. The refusal message and the CONTRIBUTING.md bullet name the command's `[PROJECT]` argument instead, and the tests assert `[PROJECT]` rather than `--project`. **Adding a real `--project` flag was considered and rejected**: it would change `devflow release --help`, break the positional invocations in `release_check.rs` and `release_execute.rs`, and exceed a plan whose stated scope is "root resolution and nothing else". D-13's substance — name both paths, offer `cd` or an explicitly named target — is fully satisfied. **This should be corrected in D-13's recorded text**, or a `--project` alias filed as its own item, rather than left as a decision the code silently diverges from.
2. **[Rule 1 — criterion arithmetic] `rg -n 'mutating_project_root\(' main.rs` returns 7 matches, not the criterion's 3.** The definition plus two call sites is exactly right (no read-only command was rerouted, which is what the criterion exists to check); the other four are this plan's own unit tests calling the function under test, which the criterion did not account for.
3. **[Rule 1 — criterion artifact] `rg -n 'CARGO_REGISTRY_TOKEN|cargo publish|push ' project_root_guard.rs` returns one match**, on the doc-comment phrase "could not push anything anywhere" explaining that the fixture has no remote. No test in the file performs a publish or a push; the criterion's intent holds.

## Issues Encountered

- One clippy finding (`needless_borrows_for_generic_args`) in the new test file, fixed before the Task 2 commit.

## User Setup Required

None.

## Next Phase Readiness

- 26-REVIEW.md **C-06** is closed: the mutating commands resolve the repository the operator is standing in, the executor's four entry guards therefore test the repository the mutation would land on, read-only commands are provably unchanged, and the worktree-beside-parent topology is now a regression test that fails against the redirecting binary.
- This plan is CLI-only by construction and shares no file with 26-08 (core-only). `crates/devflow-core/` and `crates/devflow-cli/src/commands.rs` were not modified.
- **One item for the operator:** D-13's recorded text names a `--project` flag that does not exist (Deviation 1). Either amend D-13's wording to `[PROJECT]`, or file adding a `--project` alias as its own change.

---
*Phase: 26-release-cut-automation*
*Completed: 2026-07-30*

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/tests/project_root_guard.rs`
- FOUND: `crates/devflow-cli/src/main.rs`
- FOUND: `CONTRIBUTING.md`
- FOUND: `.planning/phases/26-release-cut-automation/26-09-SUMMARY.md`
- FOUND: commit `4b236f4` (Task 1)
- FOUND: commit `bb93c57` (Task 2)
- FOUND: commit `5cd9580` (Task 3)
