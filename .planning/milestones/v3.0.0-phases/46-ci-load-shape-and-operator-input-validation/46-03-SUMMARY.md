---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 03
subsystem: cli
tags: [validation, clap, positional-argument, tdd, operator-input, stop]

requires:
  - phase: 23
    provides: "`devflow stop` itself (23c), its `stop_e2e.rs` harness, and the eight `--root` call sites that serve as this plan's regression control"
provides:
  - "`Command::Stop` carries a `project: PathBuf` positional with `#[arg(default_value = \".\")]`, making its shape identical to `Resume` and `Status`"
  - "`root.unwrap_or(project)` precedence in the `Stop` dispatch arm — the flag wins, the both-supplied case is not an error, and no clap conflict attribute exists"
  - "Five new e2e cases (`PhaseId` 103-107) plus a widened `stop_help_documents_phase_flag`, covering acceptance, both-direction precedence, the wrong-root message, the empty-value edge and the non-UTF-8 edge"
  - "A MEASURED correction to this plan's own empty-root premise: clap rejects an empty value at the parser for BOTH spellings, so `project_root` is never reached and the predicted invisible-value message is unreachable via an empty argument"
affects: [46, VALID-02, devflow-stop]

actuals:
  tokens: 3610
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Root positional expressed as `root.unwrap_or(project)` in the dispatch arm, with the flag's own `unwrap_or_else` fallback DELETED so the positional's `default_value` is the single source of the default"
    - "Precedence proven in both orderings — a single direction is equally consistent with the opposite hypothesis and does not discriminate"
    - "Matched positive/negative grep pairs for every log-derived assertion, so a log that was never written fails both halves instead of passing one"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/stop_e2e.rs
    - OPERATIONS.md

key-decisions:
  - "No `conflicts_with` and no `overrides_with`. `conflicts_with` would make the both-supplied case an ERROR, which D-12 forbids outright; `overrides_with` relates a repeated flag to itself and has no bearing on a flag-versus-positional relationship. Both would also alter `--help`, whereas the plain shape adds only the `[PROJECT]` row."
  - "`project_root` was NOT touched. D-13 is a ROUTING fix — making the positional reach a message that already exists — and changing that function would alter every other root-resolving subcommand."
  - "The empty-root assertion was written from measurement, not from the plan's prediction, and the measurement overturned the prediction. Recorded as a deviation rather than forced into agreement."
  - "The eight `stop --root` call sites in `stop_e2e.rs` and the ninth in `reap_strays_e2e.rs:163` were left byte-for-byte unedited. Only two lines were removed from `stop_e2e.rs` in the entire plan, both doc-comment lines of the widened help test."

patterns-established:
  - "When a plan predicts a behaviour it never measured, measure it against an EXISTING surface of the identical shape before writing the assertion — `devflow status \"\"` gave the post-fix answer for `stop`'s positional before `stop` had one."

status: complete
---

# Phase 46 Plan 03: `stop`'s Positional Project Root Summary

`devflow stop` now takes its project root the way the rest of the CLI does, so a wrong root reads
as "stop rejected my input" rather than a clap usage error that names no offending argument.

## RED and GREEN, verbatim and adjacent

Both lines below were observed in this session and are quoted exactly as printed.

```
RED   (5023eeb, before main.rs changed):
test result: FAILED. 8 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.08s

GREEN (86f99d7, after main.rs changed):
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.08s
```

The RED run's complete `failures:` list:

```
failures:
    stop_accepts_the_project_root_as_a_positional_argument
    stop_against_a_nonexistent_positional_root_names_the_offending_path
    stop_help_documents_phase_flag
    stop_positional_root_survives_a_non_utf8_path
    stop_root_flag_takes_precedence_over_the_positional
    stop_with_an_empty_project_root_is_refused_identically_for_both_spellings
```

Six failures: the five new cases plus the widened help test. The eight unmodified pre-existing
tests all passed, which is what makes the RED an observation about the new cases rather than about
a broken fixture or a file that stopped compiling.

**The gate that checked this was itself given a negative control.** Grepping the `failures:` window
for the eight pre-existing names returned count `0`, exit `1`. Grepping the *whole log* for the same
eight names returned count `8`, exit `0`. The two disagree, so the zero is a real absence rather
than a dead pattern — the failure mode this repo's CLAUDE.md documents for `rg -c` piped into a
second matcher.

### Landmine 2 confirmed live, not merely anticipated

`stop_against_a_nonexistent_positional_root_names_the_offending_path` panicked at
`stop_e2e.rs:686`, which is the `project path does not exist:` stderr assertion. It did **not**
panic at the `!output.status.success()` assertion a few lines above it.

That is the demonstration, not a restatement of the plan's warning: today's clap usage error also
exits non-zero, so the exit-code half was **already green in RED** and would have stayed green
through the fix without ever discriminating. Only the two stderr assertions moved.

## Step 0: the empty-root measurement — and the finding that overturned the plan

The plan predicted both spellings would reach `project_root` and render
`project path does not exist: ` with nothing after the colon, and flagged the resulting
illegibility as an UNRESOLVED `verification: backstop` gap. **Measured against the binary, that is
not what happens.**

Against the unmodified binary (before any source change):

| Spelling | Exit | stderr, verbatim |
|---|---|---|
| `stop --phase 106 ""` (positional) | `2` | `error: unexpected argument '' found` — clap rejects the extra argument; the positional did not exist yet |
| `stop --phase 106 --root ""` (flag) | `2` | `error: a value is required for '--root <ROOT>' but none was supplied` |
| `stop --phase 106 --root=` (equals form) | `2` | identical to the flag row above |

Because `stop` had no positional yet, the post-fix positional behaviour was measured by proxy
against `status`, which already carries the *identical* field shape being copied:

| Probe | Exit | stderr, verbatim |
|---|---|---|
| `status ""` | `2` | `error: a value is required for '[PROJECT]' but none was supplied` |
| `status /tmp/definitely-not-here-46-03` | `1` | `error: project path does not exist: /tmp/definitely-not-here-46-03` |

After the fix, both spellings were measured **directly** on `stop` rather than left on the proxy:

| Spelling | Exit | stderr, verbatim |
|---|---|---|
| `stop --phase 106 ""` | `2` | `error: a value is required for '[PROJECT]' but none was supplied` |
| `stop --phase 106 --root ""` | `2` | `error: a value is required for '--root <ROOT>' but none was supplied` |

**Did the two spellings converge?** Partially, and the distinction matters. They converge on
*rejection*: same exit code `2`, same clap message template, both refused at the parser before
`project_root` runs. They **diverge** in which argument the message names — `'[PROJECT]'` versus
`'--root <ROOT>'`. That divergence is correct behaviour, not a defect: clap names the argument that
actually received the empty value, which is precisely what D-13 asks a refusal to do.

The negative control for this: grepping the empty-value stderr for `project path does not exist:`
returned count `0`, exit `1`, while the same grep over the nonexistent-path stderr matched. The
message is genuinely absent because the value genuinely never reaches `project_root`.

### Consequence for the empty-legibility gap — READ THIS CAREFULLY

The plan required this SUMMARY to state that the empty-value legibility gap remains OPEN and is not
covered by D-13. Stating it verbatim would now misreport what was measured, so both facts are given
rather than the sentence the plan expected:

- **D-13 does not cover the empty case, and no claim is made that it does.** `project_root` was not
  changed, and nothing in this plan makes `project path does not exist: ` render an empty value
  legibly. If that message is ever reached with an empty value by some other route, it will still
  print nothing after the colon.
- **But the route the plan described is unreachable.** An empty *argument* — in either spelling —
  cannot reach `project_root`, because clap refuses it first. So the specific illegibility the plan
  worried about does not occur for the input class the plan named, and the refusal that does occur
  names the offending argument.

The honest summary: the gap as *described* is vacuous for empty arguments; the underlying
`project_root` weakness is *untouched* and remains whatever it was for any other path that renders
empty. This was measured, not reasoned about, and it is a correction to the plan's premise rather
than a completion of it.

## What changed

Three files, three commits, 298 insertions and 7 deletions.

| Commit | Gate | Content |
|---|---|---|
| `5023eeb` | RED | Five new cases (`PhaseId` 103-107) plus the widened help test in `stop_e2e.rs`. Only two lines removed in the whole file, both doc-comment lines of the widened test. |
| `86f99d7` | GREEN | `main.rs`: `project: PathBuf` field on the `Stop` variant, revised `root` doc comment, dispatch arm rewritten to `root.unwrap_or(project)`. 12 lines. |
| `a5e3045` | docs | `OPERATIONS.md`'s `stop` signature cell, one line. |

The source change in full:

```rust
        /// Project root. Overrides the positional argument when supplied.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
```

```rust
        Command::Stop {
            phase,
            root,
            project,
        } => stop(&project_root(root.unwrap_or(project))?, phase),
```

The dispatch arm's old `unwrap_or_else(|| PathBuf::from("."))` is gone — the positional's own
`default_value` supplies it now, which is what makes the shape *identical* to `Resume`/`Status`
rather than merely similar. The `Evidence` arm immediately below keeps its own fallback closure;
converging that subcommand is deferred (see Deferred, below).

## Precedence, proven in both directions

`stop_root_flag_takes_precedence_over_the_positional` runs two invocations with the roles of the
two roots reversed, and makes four state assertions in total:

| Ordering | Invocation | Asserted stopped | Asserted NOT stopped |
|---|---|---|---|
| 1 | `--root A B` | A | B |
| 2 | `--root B A` | B | A |

Both directions are required. One direction alone is equally consistent with the *positional*
silently winning, so it does not discriminate between the two hypotheses — it is the negative
control for the precedence claim, not a redundant second case. Both roots carry their own saved
state before either invocation runs, so each resolves at depth zero and neither escapes upward
through `project_root`'s `.devflow` ancestor walk; the assertion measures precedence, not the walk.

Neither invocation asserts on stderr, because the both-supplied case must not be an error and the
success assertion is what covers that.

## Verification — every claim below was run in this session

| # | Command | Observed |
|---|---|---|
| 1 | `cargo test -p devflow --test stop_e2e` | `test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`, `cargo_exit=0` |
| 2 | `cargo test -p devflow --test reap_strays_e2e` | `test result: ok. 2 passed; 0 failed`, `cargo_exit=0` — the ninth `--root` call site, unedited |
| 3 | `cargo test --workspace --no-fail-fast` | `cargo_exit=0`; 29 `test result:` lines, all `ok`; `grep -c "test result: FAILED"` → count `0`, exit `1` |
| 4 | `cargo clippy -p devflow --all-targets -- -D warnings` | `clippy_exit=0` |
| 5 | `devflow stop --help` | `[PROJECT]` count `2` (usage line + Arguments row), exit `0`; conflict-clause count `0`, exit `1` |
| 6 | `OPERATIONS.md` greps | new signature count `1` exit `0`; old signature count `0` exit `1` |

Every `<verify>` block in this phase's plans is written for bash, and this executor's shell is zsh
where `${PIPESTATUS[0]}` expands to nothing. Every gate above was therefore run through
`bash -c '...'`; run under zsh they would have compared against an empty string and passed
vacuously.

**Gate 5 is a matched pair by design.** The first grep must find something and the second must find
nothing. They returned opposite verdicts, so the log was genuinely written — two greps agreeing
would have meant the assertion was not discriminating. The rendered help confirms it directly:

```
Usage: devflow stop [OPTIONS] --phase <PHASE> [PROJECT]

Arguments:
  [PROJECT]  Project root [default: .]

Options:
      --phase <PHASE>  Phase to stop
      --root <ROOT>    Project root. Overrides the positional argument when supplied
```

### What these checks do NOT establish

- The workspace suite ran **without** a CPU pin. The known pre-existing 2-CPU flake
  (`staleness::tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`) did not
  appear, but this run does not establish that it would not — it did not exercise the condition.
- `14 passed` is a single green run, not a reliability measurement. It bounds nothing about
  flakiness in the new cases.
- The `#[cfg(unix)]` case was compiled and run on Linux only. Its non-Unix exclusion is asserted by
  the attribute, not by a build on another platform.
- The non-UTF-8 case proves the argument *survived clap as raw bytes and reached the path check*.
  It asserts nothing about how the bytes render, deliberately: `Path::display` is lossy by design
  and pinning its output would pin a `std` implementation detail rather than DevFlow behaviour.

## D-12 regression control: nothing migrated

All nine pre-existing `stop --root` call sites pass unedited.

- **Eight in `stop_e2e.rs`** — verified structurally rather than by count. The whole-plan diff of
  that file removes exactly **two** lines, and both are doc-comment lines of the widened help test.
  No call-site line was touched. (A raw count of the `--root` idiom now returns 11, not 8, because
  the new precedence and empty-value tests add three more invocations of the same form — the count
  is therefore *not* the right measure here, and the removed-lines check is.)
- **One in `reap_strays_e2e.rs:163`** — the file is untouched by this branch
  (`git diff --name-only 917579d..HEAD -- <path>` is empty) and its 2 tests pass.

This phase carries **no breaking change** and no `BREAKING CHANGE` changelog entry, because nothing
migrated.

## Boundary check

Exactly three files changed across the plan: `OPERATIONS.md`, `crates/devflow-cli/src/main.rs`,
`crates/devflow-cli/tests/stop_e2e.rs`.

| Path | Touched | Why it must not be |
|---|---|---|
| `crates/devflow-cli/src/commands.rs` | 0 | Plan 02 owns it in the same wave |
| `crates/devflow-cli/tests/reap_strays_e2e.rs` | 0 | read-only D-12 control |
| `CLAUDE.md` | 0 | the pre-commit hook refuses it on a `feature/*` branch |
| `.planning/user/DEV-SETUP-CHECKLIST.md` | 0 | Plan 01 owns it this phase; none of this plan's files match the post-commit hook's `setup_changed` pattern |
| `CHANGELOG.md` | 0 | purely additive change |

`project_root` (`main.rs:718-738`) is unchanged: the `main.rs` diff contains zero lines matching
`project path does not exist`, `canonicalize`, or the `.devflow` ancestor-walk test.

The negative control for that whole table: `OPERATIONS.md` run through the identical loop reports
`touched_count=1`. The zeros are real absences, not a broken loop.

## Deviations from Plan

**1. [Rule 1 - Corrected premise] The plan's empty-root prediction was wrong, and the assertion was written from measurement instead**

- **Found during:** Task 1, Step 0 — which exists in the plan precisely to catch this.
- **Issue:** The plan stated both empty spellings would reach `project_root` and render
  `project path does not exist: ` with an invisible value, and recorded that illegibility as an
  UNRESOLVED gap requiring a `verification: backstop` marker. Neither spelling reaches
  `project_root`; clap refuses an empty value at the parser for both.
- **Action:** Followed the plan's own instruction — "assert the observed behaviour of each" — and
  wrote the test against the measured behaviour, including a `!contains("project path does not
  exist:")` negative control on each half so the test would fail if a future build ever *did* let
  an empty value through. The plan's required SUMMARY sentence was replaced with an accurate
  statement of both facts (see the boxed discussion above) rather than repeated verbatim.
- **Files modified:** `crates/devflow-cli/tests/stop_e2e.rs`
- **Commit:** `5023eeb`

No other deviations. No auth gates. No architectural (Rule 4) decisions arose.

## Deferred

`evidence` and `gate sweep` were **deliberately NOT converged** onto the positional form, per this
phase's Deferred Ideas. The `Evidence` dispatch arm keeps its own
`unwrap_or_else(|| PathBuf::from("."))` fallback and its flag-only shape, and `gate sweep` keeps
`--root PATH`. Both remain holdouts by decision, not by oversight.

The pre-existing `OPERATIONS.md` gap — the command table omits the positional project root for
`start`, `resume` and `status` too — was left alone. This phase's boundary is `stop`, and the plan
explicitly directs not to file the gap as work.

## Known Stubs

None. No placeholder values, no skipped tests, no unrun `<verify>` blocks — all six verification
gates were executed and their real output is recorded above.

## Self-Check: PASSED

- `crates/devflow-cli/src/main.rs` — FOUND, modified
- `crates/devflow-cli/tests/stop_e2e.rs` — FOUND, modified
- `OPERATIONS.md` — FOUND, modified
- Commit `5023eeb` — FOUND
- Commit `86f99d7` — FOUND
- Commit `a5e3045` — FOUND
