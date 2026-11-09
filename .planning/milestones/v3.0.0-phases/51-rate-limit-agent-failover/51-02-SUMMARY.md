---
phase: 51-rate-limit-agent-failover
plan: 02
subsystem: testing
tags: [rust, doc_check, source-scan, clap, negative-controls]

requires: []
provides:
  - "doc_check::the_removed_sequentagent_handoff_stays_dead: criterion 4 (D-14) as a permanent cargo test gate"
  - "Test-only helpers in doc_check.rs: strip_rust_comments, strip_string_literal_contents, subcommand_enums, subcommand_enum_names, rebase_in_findings, removed_verb_findings, removed_verb_attr_findings, removed_handoff_findings"
affects: [51-rate-limit-agent-failover, doc_check]

actuals:
  tokens: 6500
  tasks: 2
  commits: 3
plan_head_before: 490fa1c1dcc2bdc2e1882fc27e434fb8ba406168
plan_head_after: 62b77a5b813e82f30ae0c849e69e32ad50e6d63d

tech-stack:
  added: []
  patterns:
    - "One literal-aware Rust lexer with two outputs: comment-stripped code, and a byte-aligned literal-blanked mask. Structure is found in the mask and values are sliced from the code at the same offsets"
    - "Removed-token constants assembled with concat! so the scanning file's own code never holds them"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/doc_check.rs

key-decisions:
  - "The attribute scan reads every #[command(...)] in crates/devflow-cli/src/, not only those inside a Subcommand enum. A removed spelling set as name or alias anywhere in the CLI is a finding. The real tree has none besides the clean name = \"devflow\" and name = \"__monitor\""
  - "The Command variant count used for the non-vacuity check comes from the new subcommand_enums parser, not the older enum_variants helper, so the check pins the parser the scan actually uses"
  - "Task 2 had no RED: Task 1's action already specified the full matcher (multi-line bodies, every alias key, exact element equality). Four mutations of the matcher were run instead as a negative control (see TDD Gate Compliance)"

patterns-established:
  - "Source-scan tests compare declarations, not free text, and carry planted positives and decoys in the same test"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "A comment-stripped scan finds a reintroduced rebase_in identifier in any crate source, and a sequentagent or n verb in any Subcommand enum by variant name, name or any alias form, with exact element equality. Every planted positive gives exactly the stated findings and every decoy gives none"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::the_removed_sequentagent_handoff_stays_dead"
        status: pass
    human_judgment: false
  - id: D2
    description: "The real tree has zero findings. The walker finds sources under crates/ and crates/devflow-cli/src/, the finder returns command and gatecmd, and Command has more than five variants"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::the_removed_sequentagent_handoff_stays_dead"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow-core --lib (1120 passed, 2 ignored)"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-11-06
status: complete
---

# Phase 51 Plan 02: Removed Rebase Handoff Stays Dead Summary

**A `cargo test` gate now fails if `rebase_in` returns anywhere under `crates/`, or if a `sequentagent` or `n` verb returns in any clap `Subcommand` enum, whether as a variant, a `name` or any alias. It reads comment-stripped source and compares values whole, so `name = "__monitor"` does not trip it.**

## Performance

- **Duration:** about 6 min of execution (the clock started after the context reads)
- **Started:** 2026-11-06T16:01:38Z
- **Completed:** 2026-11-06T16:06:50Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- `doc_check::the_removed_sequentagent_handoff_stays_dead` covers criterion 4 (D-14) as a named test.
- One lexer handles strings with escapes, `r#"…"#` raw strings, `b`/`br` prefixes, char literals versus lifetimes, and nested block comments. It produces both the comment-stripped code and a byte-aligned mask with literals blanked.
- `subcommand_enums` finds every enum whose outer attributes derive `Subcommand`, bare or path-qualified. It skips visibility, reads each variant and records its line.
- The attribute scan handles multi-line `#[command(` bodies and reports the line the attribute opens on.

## Task Commits

1. **Task 1 (tracer) RED: failing scan with stub finders** - `973b48f` (test)
2. **Task 1 (tracer) GREEN: lexer, subcommand-enum finder, three finding fns** - `4163288` (feat)
3. **Task 2: remaining spellings, routes, decoys, line-number and stripper assertions** - `62b77a5` (test)

## Files Created/Modified
- `crates/devflow-core/src/doc_check.rs` - adds the test and its helpers. The change only appends to the file (one hunk after the last existing test). `doc_check` is `#[cfg(test)]`, so no public surface changes and semver-checks is not needed.

## Verification

| Check | Result |
|-------|--------|
| `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::the_removed_sequentagent_handoff_stays_dead)'` | 1 passed |
| `cargo nextest run ... -E 'test(=doc_check::no_monitor_pid_field_json_key_or_help_mention_remains)'` | 1 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test -p devflow-core --lib` | 1120 passed, 0 failed, 2 ignored |
| Real-tree negative control: planted into the real `main.rs`, then restored with `git checkout -- crates/devflow-cli/src/main.rs` and confirmed clean | a `Sequentagent` variant in `Command`, an `N` variant in `GateCmd`, and a multi-line `#[command(\n visible_alias = "n",\n)]` each reported once at the correct line (52, 414, 48) |

**What these checks do not establish.** The scan is lexical:
- A verb added under a different spelling that re-implements the rebase handoff would pass it.
- A subcommand enum built by a macro rather than `#[derive(Subcommand)]` would not be found.
- `#[clap(...)]` attributes (the older clap spelling) and `short_flag`/`long_flag` keys are not read. Neither appears in the tree today.

Criterion 4 asks that the named removed items stay removed, and this test does establish that.

## TDD Gate Compliance

- **RED (Task 1):** `973b48f`. The stub finding functions returned nothing. The run exited 100 and failed on `planted positive "fn rebase_in(base: &str) {}" gave []`, which is the planned assertion. The evidence is the nextest JUnit report passed through `gsd-tools check tdd-red-evidence`, which returned `RED_EVIDENCE_OK` (`target_test_failed`). The record is `target/red-evidence/51-02-task1.json` (gitignored). Semantic assessment: the target test ran and failed on a planted-positive assertion. This was not a build, load or discovery fault.
- **GREEN (Task 1):** `4163288` descends from `973b48f`.
- **Task 2:** has no RED. Every new case passed on its first run, because Task 1's action already required the full matcher. The plan anticipated this ("RED is expected only where Task 1's matcher misses"). To show the new cases bite, I mutated the matcher four times, restored it from a backup each time, and confirmed the final copy was byte-identical with `cmp`:
  - Substring match instead of whole equality: fails at `aliases = ["nn", "n"]` (two findings instead of one).
  - Reporting the value's line instead of the attribute's opening line: fails the multi-line assertion (`planted.rs:3` instead of `:2`).
  - Scanning only enums named `Command`: fails at the `GateCmd { List, N }` positive.
  - Not treating `"…"` as a literal: fails at `let s = "// x"; git.rebase_in(base)?;`.
- No REFACTOR commit was needed.

## Decisions Made
- The attribute scan covers every `#[command(...)]` under `crates/devflow-cli/src/`, not only those inside Subcommand enums, which is the wider reading of the truth.
- The non-vacuity variant count uses the new parser, not `enum_variants`.
- The planted fixtures build `enum {command} {{` through `format!`. The source therefore never contains the literal `enum Command {` marker that the existing `enum_variants(rust_source(), "Command")` matches with `split_once`, so it cannot capture a fixture.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] An extra positive: a call after a `//` inside a literal**
- **Found during:** Task 2
- **Issue:** The planned decoy `let s = "// rebase_in"; let t = s.len();` gives zero findings whether or not the stripper respects literals. If the stripper wrongly treated the `//` as a comment, the decoy would still give zero, so it cannot catch that defect.
- **Fix:** Added the positive `let s = "// x"; git.rebase_in(base)?;`, which must give exactly one finding. Mutation 4 confirms it discriminates.
- **Files modified:** crates/devflow-core/src/doc_check.rs
- **Committed in:** `62b77a5`

---

**Total deviations:** 1 auto-fixed (missing critical: negative control)
**Impact on plan:** One extra test case. No scope change.

## Issues Encountered
- The RED commit carried two dead-code warnings: `strip_string_literal_contents` and `REMOVED_VERB_SPELLINGS` were not yet used by the stubs. GREEN uses both, and clippy is clean at `4163288` and `62b77a5`.
- No DevFlow or GSD defect was hit during this plan, so there are no new 999.x candidates.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
Ready for the next plan in phase 51. This plan has no dependents.

## Self-Check: PASSED
- FOUND: crates/devflow-core/src/doc_check.rs (contains `the_removed_sequentagent_handoff_stays_dead`, `removed_handoff_findings`)
- FOUND: 973b48f, 4163288, 62b77a5 (all ancestors of HEAD)
- Commits measured from the ledger: 3 (`490fa1c..62b77a5`)

---
*Phase: 51-rate-limit-agent-failover*
*Completed: 2026-11-06*
