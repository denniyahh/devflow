---
phase: 52-subtract-and-stabilize
plan: 02
subsystem: cli
tags: [rust, clap, devflow-cli, devflow-core, cut, doc_check, help-snapshot, tdd]

requires: []
provides:
  - "`devflow list`, `devflow reference` and `devflow test` removed: clap rejects them as unrecognized subcommands"
  - "phase7_cli::removed_subcommands_are_unknown with a REMOVED_SUBCOMMANDS list for later cut plans to append to"
  - "phase7_cli::cleanup_worktree_cli_flow (renamed from reference_and_cleanup_worktree_cli_flow)"
  - "devflow-core no longer exports worktree::reference_path or worktree::add_detached (breaking, for 4.0.0)"
  - "52-RUN-NOTES.md: the installed 3.0.0 binary's dry-run stage list (criterion 1 control) and the D-18 exception"
affects: [52-03, 52-04, 52-05, 52-06, 52-07, 52-08, 52-09, 52-10, 52-11, 52-12, 52-13, 52-31, 52-32, 52-33]

actuals:
  tokens: 10363
  tasks: 3
  commits: 5
plan_head_before: 951f542a29c0ccf20d60034015a7b865d5edd0f9
plan_head_after: 73adca8501cd5d5f7c1627837fdd4d60af9e42fb

tech-stack:
  added: []
  patterns:
    - "A removed CLI verb gets an entry in REMOVED_SUBCOMMANDS: clap must reject it and --help must not list it, with status as the control"
    - "Breaking removals use the `feat(52-NN)!:` subject: GSD's TDD gate accepts `!` and scripts/cut-release.sh reads it as a major bump"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-RUN-NOTES.md
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-core/src/worktree.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/ci_parity_guards.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - README.md
    - OPERATIONS.md
    - ARCHITECTURE.md
    - CONTRIBUTING.md

key-decisions:
  - "worktree::add_detached was deleted along with reference_path: commands::reference was its only caller (rg found no other). This is a second pub removal in devflow-core, covered by the 4.0.0 major bump"
  - "Removal commits use `feat(52-02)!:`. GSD's plan-scope pattern is `^[a-z]+\\(52-02\\)!?:`, so the GREEN gate still counts them, and cut-release.sh reads the `!` before the colon as a major bump"
  - "CONTRIBUTING now names `scripts/check.sh all` as the local run of the CI checks (it runs fmt, clippy and the workspace tests), in place of `devflow test`"
  - "The 3.0.0 control ran with `--mode supervise` added. The plan's command without it was refused by clap (`--mode` is required). Both runs are recorded verbatim"

patterns-established:
  - "Pattern: grow REMOVED_SUBCOMMANDS in phase7_cli.rs for each later verb removal, and regenerate the help snapshot with `cargo run -q -p devflow -- --help`"

requirements-completed: [CUT-04, CUT-01]

coverage:
  - id: D1
    description: "`devflow list`, `devflow reference` and `devflow test` exit non-zero with clap's `unrecognized subcommand` error and are not in `devflow --help`. `devflow status --help` still succeeds"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_subcommands_are_unknown"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D2
    description: "`devflow cleanup --force` still removes a phase worktree after its run ends, with the reference-worktree branch gone"
    requirement: CUT-04
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_worktree_cli_flow"
        status: pass
    human_judgment: false
  - id: D3
    description: "No scoped doc names a removed verb, and every remaining Command variant is still documented"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
    human_judgment: false
  - id: D4
    description: "The installed 3.0.0 binary's `start --dry-run` stage list (Define first, Ship last) and the D-18 exception are recorded in 52-RUN-NOTES.md before any reinstall"
    requirement: CUT-01
    verification:
      - kind: other
        ref: "test -s 52-RUN-NOTES.md && grep -q 'devflow 3.0.0' && grep -q 'D-18'"
        status: pass
    human_judgment: false
  - id: D5
    description: "Full workspace check suite green after the removals"
    verification:
      - kind: other
        ref: "scripts/check.sh all (exit 0; 45 test binaries, 2136 passed, 0 failed, 6 ignored)"
        status: pass
    human_judgment: false

duration: 13min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 02: Remove the list, reference and test commands Summary

**`devflow list`, `devflow reference` and `devflow test` are gone: clap rejects them. Their docs, tests, help lines and the core's `reference_path`/`add_detached` are removed. An e2e test holds the list of removed verbs, and the installed 3.0.0 binary's Define-to-Ship dry run is on record.**

## Performance

- **Duration:** 13 min
- **Started:** 2026-11-10T18:30:25Z
- **Completed:** 2026-11-10T18:43:26Z
- **Tasks:** 3
- **Files modified:** 12 (11 modified, 1 created)

## Accomplishments
- The three leaf verbs are removed from `enum Command`, from dispatch, and from their handlers.
- `status` keeps its open-branch list: `print_open_branches` and `git::list_feature_branches` stay.
- `removed_subcommands_are_unknown` checks each verb in `REMOVED_SUBCOMMANDS` two ways: clap must reject it, and `--help` must not list it. `status` is the control for both checks. Later cut plans add to the list.
- The reference worktree is gone from the code:
  - `worktree::reference_path` and `worktree::add_detached` are deleted from devflow-core (`rg` found no caller left).
  - cleanup's keep-the-reference branch is deleted.
  - The `cleanup --force` help and the `commands::cleanup` doc no longer mention it (review concern C18).
- The verbs are removed from README, OPERATIONS, ARCHITECTURE and CONTRIBUTING. README's empty "Quality" section went with them.
- The help snapshot is regenerated. Ignoring whitespace, the diff is exactly the three removed lines. Removing `reference`, the longest name, narrowed the column by one, so every line re-aligned.
- `52-RUN-NOTES.md` holds the 3.0.0 control and the D-18 exception.

## Task Commits

1. **Task 1 (tracer): `devflow list` removed end to end**
   - RED `4c81863` test(52-02): add failing test for removed subcommands
   - GREEN `4ca4aea` feat(52-02)!: remove the list command
2. **Task 2: `reference` and `test` removed**
   - RED `3446749` test(52-02): expect reference and test to be unknown subcommands
   - GREEN `8ec62fd` feat(52-02)!: remove the reference and test commands
3. **Task 3: 3.0.0 control and D-18 exception**
   - `73adca8` docs(52-02): record the 3.0.0 dry-run control and the D-18 exception

No REFACTOR commits: nothing needed cleaning up after GREEN.

## TDD Gate Compliance

| Task | RED evidence | Verdict | Semantic assessment |
|------|--------------|---------|---------------------|
| 1 | nextest JUnit (`--profile red`, scratch config), exit 100 | RED_EVIDENCE_OK | Failed at `phase7_cli.rs:758`, the planned assertion. `devflow list` ran the list command, which failed with a git error and printed no `unrecognized subcommand`. |
| 2 | same setup, exit 100 | RED_EVIDENCE_OK | Failed at the same assertion for `reference` (a git worktree error, not a clap rejection). The loop stops at the first failure, so `test` was probed separately against the pre-removal binary: it ran the cargo quality checks (exit 1, no `Cargo.toml` in the scratch dir). It was not rejected by clap. |

Each GREEN `feat(52-02)!:` commit descends from its RED `test(52-02):` commit.

## Verification

| Command | Exit |
|---------|------|
| Task 1 `<automated>` chain (4 nextest runs, `--no-tests=fail`), run unpiped | 0 |
| Task 2 targeted runs: `removed_subcommands_are_unknown`, `cleanup_worktree_cli_flow`, `help_output_matches_committed_snapshot`, `doc_referenced_identifiers_exist_in_source` | 0 |
| `scripts/check.sh all` (fmt, clippy `--workspace --all-targets -D warnings`, workspace tests) | 0 (45 binaries, 2136 passed, 0 failed, 6 ignored) |
| `cargo semver-checks check-release -p devflow-core` | 100, as expected: `function_missing` for exactly `worktree::add_detached` and `worktree::reference_path`. A major version is required, which the planned 4.0.0 bump covers |
| Task 3 `<automated>` (`test -s` + two greps) | 0. Negative control `grep -q 'devflow 2.9.9'` gave 1 |

Negative controls for the acceptance checks, run against the pre-change HEAD:
- The same `awk` scan of `enum Command` found `Reference` and `Test` there.
- `rg` found 2 `reference worktree` mentions in the old `main.rs`.
- `rg` found the old `devflow_test_clippy_matches_ci_scope`.

**What this does not establish:**
- The e2e test proves clap rejects the verbs. It does not prove that no code path reaches the deleted functions by another name. Clippy's dead-code lint covers only private leftovers. A `rg` for `reference_path|add_detached|test_cmd` under `crates/` found no code reference.
- Task 2's RED proved only `reference` against the test assertion. `test` was established by a separate probe.

## Files Created/Modified
- `crates/devflow-cli/src/main.rs`: `List`, `Reference`, `Test` variants, their arms and imports removed. The `cleanup --force` doc now reads "Force-remove dirty worktrees."
- `crates/devflow-cli/src/commands.rs`: `list`, `reference` and `test_cmd` removed. The `hermetic_command` import is gone (only `test_cmd` used it here). The reference branch and the reference mentions in cleanup's docs and comments are removed.
- `crates/devflow-core/src/worktree.rs`: `reference_path`, `add_detached` and the `reference_path` test assertion removed.
- `crates/devflow-core/src/git.rs`: the doc comment on the feature-branch summary now names `status`'s open-branch list, not `devflow list`.
- `crates/devflow-cli/tests/phase7_cli.rs`: `REMOVED_SUBCOMMANDS`, `help_subcommand_names` and `removed_subcommands_are_unknown` added. `cleanup_worktree_cli_flow` renamed and its reference steps dropped.
- `crates/devflow-cli/tests/ci_parity_guards.rs`: `devflow_test_clippy_matches_ci_scope` and the WR-10 module-doc bullet removed.
- `crates/devflow-cli/tests/snapshots/devflow-help.txt`: regenerated.
- `README.md`, `OPERATIONS.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`: mentions of the removed verbs removed.
- `.planning/phases/52-subtract-and-stabilize/52-RUN-NOTES.md`: created.
- `docs/guides/quickstart.md`: listed in the plan's `files_modified`, but **not changed**. `rg -n 'devflow (list|reference|test)\b'` found no mention in `docs/`.

## Decisions Made
See `key-decisions` in the frontmatter. Each was inside the plan's remit:
- deleting a helper only the removed handlers used;
- the commit subject form;
- the CONTRIBUTING replacement;
- the `--mode` argument clap required.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The control's dry-run command needed `--mode`**
- **Found during:** Task 3
- **Issue:** The plan's command, `devflow start --dry-run --phase 52`, is refused by 3.0.0's clap: `--mode <MODE>` is required (exit 2). Recording only the refusal would leave the must-have truth unmet: the dry-run output listing Define and Ship.
- **Fix:** The refusal is recorded verbatim as the plan says. Then the same command was run with only `--mode supervise` added, and its output recorded verbatim. Both runs are labelled in the notes.
- **Files modified:** `52-RUN-NOTES.md`
- **Committed in:** `73adca8`

**2. [Rule 3 - Blocking] The scratch clone used `--local --shared`**
- **Found during:** Task 3
- **Issue:** `git clone --local` into the session scratchpad failed: the scratchpad is tmpfs and `--local` hardlinks across devices (`Invalid cross-device link`). A full copy would have put about 2.8 GB of objects into RAM-backed /tmp while the workspace tests were running.
- **Fix:** `git clone --local --shared`. It uses git alternates, so nothing is copied, and it is still a local clone outside the worktree. The clone was deleted afterwards.
- **Verification:** `git status --short --ignored` in the clone was empty after both runs.

**3. [Rule 1 - Bug] A stale doc comment in devflow-core**
- **Found during:** Task 1
- **Issue:** `git.rs` described the feature-branch summary as "for the `devflow list` command".
- **Fix:** It now names `status`'s open-branch list. The change is doc-only and not a semver change. The file was not in `files_modified`.
- **Committed in:** `4ca4aea`

**4. [Plan wording] Commit subjects**
- **Issue:** The plan suggests `feat(cli)!: remove the list, reference and test commands`.
- **Fix:** The commits use `feat(52-02)!:`, one per GREEN. This keeps the breaking marker that `cut-release.sh` reads, and keeps the plan scope that the GSD TDD gate needs.

---

**Total deviations:** 4 (2 blocking, 1 bug, 1 wording)
**Impact on plan:** None changes scope. The `add_detached` removal is the plan's own "any helper only they used" rule applied to core.

## Issues Encountered
- My first Task 1 verify run piped every nextest command through `tail`, so the chain's exit code was `tail`'s. It was re-run unpiped, and that run gave the recorded exit 0.
- The first clone attempt failed (deviation 2). The chain stopped at the failed `cd`, so no dry run ran anywhere. `git status` in the worktree showed only the uncommitted Task 2 edits.
- An operator with a leftover `.worktrees/reference` from 3.x will see it treated like any other worktree under `.worktrees/`. It has no owning phase, so a plain `cleanup` removes it, with no `--force` needed. The operator's machine has none: the main checkout's `.worktrees/` holds only `phase-52`. No migration is needed, per the no-migration rule.

## User Setup Required
None. No external service configuration is required.

## Next Phase Readiness
Ready for 52-03. Later cut plans append to `REMOVED_SUBCOMMANDS` and regenerate the help snapshot the same way.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*

## Self-Check: PASSED
- Files exist: `52-RUN-NOTES.md`, `phase7_cli.rs`. Both new test functions are present.
- Commits are ancestors of HEAD: `4c81863`, `4ca4aea`, `3446749`, `8ec62fd`, `73adca8`.
- `check evaluation-scope --plan 52-02 --commits-only` resolved the plan's commits.
- Stub scan of the added lines: none found.
