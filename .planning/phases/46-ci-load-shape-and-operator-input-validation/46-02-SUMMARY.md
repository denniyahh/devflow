---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 02
subsystem: cli
tags: [validation, git-plumbing, show-ref, check-ref-format, tdd, error-messages]

requires:
  - phase: 45
    provides: "`ensure_base_is_a_local_branch`, its single guarded call site, `base_branch_fixture`, and the existing reject-arm test this plan extends in place"
provides:
  - "`ensure_base_is_a_local_branch` refuses revision-expression base branches (`~1`, `@{0}`, `^{}`) via `git show-ref --verify` instead of the revision parser `git rev-parse --verify`"
  - "Two distinguishable refusal messages, classified by `git check-ref-format`: a missing-branch arm keeping today's actionable advice, and a malformed arm that names the offending value and withholds advice that would be nonsense there"
  - "An in-place extension of `ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch` whose `contains`/`!contains` pair pins the two messages as distinguishable"
affects: [46, VALID-01, devflow-start]

actuals:
  tokens: 3037
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Ref existence checked with the ref LOOKUP (`show-ref --verify`), never the revision PARSER (`rev-parse --verify`)"
    - "Ref-name validity delegated wholly to `git check-ref-format` rather than a hand-rolled denylist"
    - "Deliberately asymmetric subprocess fallbacks: fail-closed on the decision call, fail-open toward the pre-existing message on the classifier"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "Two ORDERED calls, not one substitution: `show-ref` decides accept-versus-reject, `check-ref-format` only labels an already-decided refusal. Reversing them accepts and rejects nothing correctly, because `check-ref-format` exits 0 for a well-formed name that does not exist (D-07/D-08)."
  - "Both git calls receive one shared `refs/heads/{base}` binding. The raw value reaches neither: it is option-shaped for a base beginning with `-` and misclassifies one-level branch names without `--allow-onelevel`."
  - "`.unwrap_or(false)` on existence, `.unwrap_or(true)` on the classifier. An unspawnable git still refuses (unchanged behaviour) but never emits a new malformed claim it has no evidence for."
  - "The empty-base arm was MEASURED before being asserted, not predicted. It lands in the malformed arm."
  - "Panic messages name the offending spelling: the arms assert `is_err()` first and unwrap afterwards, because a bare `.unwrap_err()` panics with `called Result::unwrap_err() on an Ok value: ()` and does not say which spelling failed."

patterns-established:
  - "Prove-the-bypass-first on the QUALIFIED form when the bypass only exists after a prefix is applied — a bare probe would assert a different fact."

status: complete
---

# Phase 46 Plan 02: Operator Base-Branch Validation Summary

`ensure_base_is_a_local_branch` now refuses revision expressions, because it asks git for a ref
instead of asking git to parse a revision — and when it refuses, it says which of the two possible
reasons applies.

## What changed

`crates/devflow-cli/src/commands.rs` only. Two commits, no new public symbol, no signature change.

| Commit | Gate | Content |
|---|---|---|
| `aa0707e` | RED | Extended the existing test in place with the suffix arms, the empty-base arm, and both halves of the message discriminator. 95 insertions, **0 deletions**. |
| `1272584` | GREEN | Body of `ensure_base_is_a_local_branch` replaced with the two-call implementation; doc comment and the stale call-site comment updated. |

There is no REFACTOR commit — the GREEN body is the final shape.

## Step-0 measurement: the empty base (required by the plan, and it matched the prediction)

Measured in a hand-built faithful replica of `base_branch_fixture`, **git 2.55.0**, before any
assertion about the empty case was written:

```
show-ref_refs/heads/_exit=1
check-ref-format_refs/heads/_exit=1
```

Both exit 1, so **an empty `base_branch` lands in the MALFORMED arm**. This matches the plan's
stated prediction (`check-ref-format` rejects the trailing slash); there is no divergence to report.

The same probe run reproduced the full RESEARCH truth table exactly, and it carried its own negative
control — `nonexistent-xyz` returns `check-ref-format=0` where the empty base returns `1`, so the
classifier column is genuinely discriminating rather than constant:

| base | `rev-parse --verify -q` | `show-ref --verify -q` | `check-ref-format` |
|---|---|---|---|
| `` (empty) | 1 | 1 | **1** |
| `develop` | 0 | 0 | 0 |
| `workspace/example` | 0 | 0 | 0 |
| `nonexistent-xyz` | 1 | 1 | **0** |
| `workspace/example~1` | **0 — live bug** | 1 | 1 |
| `develop@{0}` | **0 — live bug** | 1 | 1 |
| `develop^{}` | **0 — live bug** | 1 | 1 |
| `origin/main` | 1 | 1 | 0 |
| `refs/heads/main` | 1 | 1 | 0 |
| `HEAD` | 1 | 1 | 0 |

## RED, quoted verbatim

```
test commands::tests::ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch ... FAILED

thread 'commands::tests::ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch' (2907843) panicked at crates/devflow-cli/src/commands.rs:5542:13:
`workspace/example~1` is revision syntax surviving the `refs/heads/` prefix, not a local branch, and must be refused

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 362 filtered out; finished in 0.07s
cargo_exit=101
```

`362 filtered out` is non-zero, so the test name matched something rather than cargo exiting without
running anything.

### Each RED arm observed individually, not inferred

Rust fail-fast means one run only proves the *first* arm. Each remaining arm was isolated and run,
so no arm is claimed RED on the strength of a derivation:

| Isolated arm | Observed |
|---|---|
| `develop@{0}` | `panicked at commands.rs:5542:13` / `test result: FAILED` |
| `develop^{}` | `panicked at commands.rs:5542:13` / `test result: FAILED` |
| `""` (suffix loop neutralised) | `test result: FAILED`, panicking on the empty arm |

The empty arm's RED output is the best single illustration of why D-08 exists — today's one-size
message told the operator to run `git branch  origin/`:

```
panicked at crates/devflow-cli/src/commands.rs:5569:9:
an empty base is not a well-formed ref name and must land in the malformed arm, which carries no create-a-branch advice: configured base branch `` is not a local branch in this repository. DevFlow forks phase worktrees from it and merges phase work back into it, so it must be a branch — not a remote-tracking name, a `refs/heads/` path, `HEAD`, or a commit SHA. Create it locally (e.g. `git branch  origin/`) and re-run.
```

**A broken probe was caught and discarded during this step.** The first attempt at the
suffix-loop-neutralised run emitted *no* `panicked at` and *no* `test result:` line. That is not a
pass — a shell-escaping error had written invalid Rust (`Vec::<\&str>::new()`), so the target never
compiled and the grep matched nothing. It was re-run correctly. Recorded because an empty
measurement reading as a clean result is exactly the failure mode this repo's rules target.

## GREEN, quoted verbatim

```
test commands::tests::ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 362 filtered out; finished in 0.09s
cargo_exit=0
```

`1 passed` with `362 filtered out` — a real single-test pass, not a name that matched nothing.

### Both refusal messages rendered, not inferred from `format!`

Forced to print by temporarily inverting each assertion (source restored byte-identically
afterwards, verified with `cmp`):

- **Malformed, `workspace/example~1`:** `configured base branch `workspace/example~1` is not a
  well-formed branch name — git rejects `refs/heads/workspace/example~1` as a ref. DevFlow forks
  phase worktrees from the base and merges phase work back into it, so it must name a branch, not a
  revision expression such as `~1`, `@{0}` or `^{}`. Set the base to the branch itself and re-run.`
- **Malformed, empty base:** same text with `` `` `` and `refs/heads/` substituted — it still reads
  sensibly for an empty value, which was an explicit plan requirement.
- **Missing, `nonexistent-xyz`:** today's text, byte-identical, advice included (asserted by
  `contains("git branch nonexistent-xyz origin/nonexistent-xyz")`).

Neither malformed message contains the substring `git branch`; the missing message does. That
`contains` / `!contains` pair is the D-08 discriminator, and both halves are present.

## Verification

Every claim below was run in this session. Because the executor's shell is **zsh**, where
`${PIPESTATUS[0]}` silently expands to nothing, every exit-code assertion was run through
`bash -c`. This was verified directly first (`echo x | cat; echo "[${PIPESTATUS[0]}]"` printed
`[]` under the default shell) rather than assumed.

| # | Check | Result |
|---|---|---|
| 1 | `cargo test -p devflow --bin devflow ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch` | `1 passed`, `362 filtered out`, `cargo_exit=0` |
| 2 | `cargo test -p devflow --bin devflow no_worktree_start_forks_the_feature_branch_from_the_configured_base` | `1 passed`, `362 filtered out`, `cargo_exit=0` — the fixture's second consumer is unaffected |
| 3 | `cargo test -p devflow --bin devflow` (whole binary) | `363 passed; 0 failed`, `cargo_exit=0` |
| 4 | `cargo test --workspace --no-fail-fast` | `cargo_exit=0`, every `test result:` line `ok`, zero failures across all targets |
| 5 | `cargo fmt --check -p devflow` | `fmt_exit=0` |
| 6 | `cargo clippy -p devflow --bin devflow --all-targets -- -D warnings` | `clippy_exit=0` |
| 7 | `git diff --name-only \| grep -c devflow-core` | count `0`, exit `1` — both printed and read together, not piped into a pattern match |
| 8 | RED commit hunk headers | `@@ -5457,0 +5458,30 @@` and `@@ -5494,0 +5525,65 @@` — pure insertions, `0` deletions, which is what proves `base_branch_fixture`, the accept-arm negative control and the four-spelling reject loop are byte-for-byte unchanged |

The known 2-CPU-pinned flake `staleness::tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`
(deferred as D-46-01-A in plan 01) **passed** in every run here. These runs were unpinned, so that
is not evidence about its pinned behaviour either way.

### What this verification does NOT establish

- **Nothing was pushed and no CI ran.** `scripts/check-in-container.sh all` and the pinned-container
  pre-push gate were not exercised; plan 01 recorded that gate as currently exiting 101 on the
  pre-existing D-46-01-A failure. Green here is a host-toolchain result only.
- **The two messages are pinned as *distinguishable*, not as *well-worded*.** The test asserts one
  substring's presence and another's absence. No check establishes that an operator finds the
  malformed message actionable.
- **Behaviour is pinned at the helper, one level below the CLI.** No test drives `devflow start`
  with a revision-expression `base_branch` end-to-end, so the caller-side scoping at the single call
  site is verified by inspection (`if resolved_base.source != config::BaseBranchSource::Default` is
  unchanged) rather than by execution.
- **`git check-ref-format`'s own rules are inherited, not tested.** Coverage of `..`, trailing
  `.lock`, control characters and so on is asserted about git, by delegation. This plan tests the
  three spellings it names plus the empty string; it does not enumerate git's rule set.
- **One git version.** All plumbing exit codes were measured under git 2.55.0 on this host.

## R-01 remains OPEN — this plan does not close it

**VALID-01 does not close the validator/consumer ref-spelling asymmetry.**
`ensure_base_is_a_local_branch` qualifies `refs/heads/{base}`, while
`crates/devflow-core/src/worktree.rs:64-83` forwards the **raw** `{base}` to `git worktree add` as a
start point. A value that is simultaneously a legal literal ref name *and* a revision expression —
`@` is the demonstrated case: `git branch '@'` succeeds, `git show-ref --verify refs/heads/@` exits
0, and raw `@` resolves to HEAD — passes validation here and then forks from something else.

That gap is pre-existing (today's `rev-parse` path prefixed `refs/heads/` too), is **not narrowed**
by the switch to `show-ref`, and closing it requires validator and consumer to agree on one
spelling — a devflow-core change outside this phase's boundary. It is recorded in the function's own
doc comment and tracked as R-01 in `46-REVIEWS.md`. No file under `crates/devflow-core/` was
touched (verified, check 7 above).

## ROADMAP Success Criterion 2: a deliberate, stronger substitution

Criterion 2 names `refs/heads/develop~1`, `develop@{0}` and `develop^{}`. The arms shipped are
**`workspace/example~1`**, `develop@{0}` and `develop^{}`. Recorded here so an auditor reading the
criterion literally is not surprised:

- **`develop~1` would prove nothing.** `base_branch_fixture` (`commands.rs:5414`) creates `develop`
  at the fixture's **root commit**, so `develop~1` has no parent to resolve. Measured: it exits 1
  under today's `rev-parse --verify` check — it is *already* refused, and an arm using it would pass
  identically before and after the fix. `workspace/example` is the only fixture branch with a
  parent, so `workspace/example~1` is the spelling that actually flips from accept to reject.
- **The `refs/heads/` spelling class is already covered.** The pre-existing `refs/heads/main`
  reject-arm covers it, and D-07 records that the double prefix (`refs/heads/refs/heads/develop~1`)
  already defeated that spelling before this change. Nothing there was open.

The plan's prohibition against a `develop~1` arm stands and the arm list was not changed to match
the criterion's literal text.

## Deviations from Plan

**One, minor, within the plan's stated discretion.**

**1. [Rule 2 — diagnosability] Arms assert `is_err()` before unwrapping**

- **Found during:** Task 1, on the first RED run.
- **Issue:** The RESEARCH skeleton's `.unwrap_err()` panics with
  `called `Result::unwrap_err()` on an `Ok` value: ()` — which does not name the offending
  spelling. The plan's own acceptance criterion requires "the panic naming at least one of the three
  revision-suffix spellings", so the skeleton as written could not satisfy it.
- **Fix:** Each arm now asserts `is_err()` with a message naming the spelling, then unwraps. This
  also avoids clippy's `expect_fun_call` lint that `.expect_err(&format!(...))` would have tripped.
- **Files modified:** `crates/devflow-cli/src/commands.rs` (test module only).
- **Commit:** `aa0707e`.

No architectural deviations, no Rule 4 escalations, no authentication gates, no checkpoints.

## Known Stubs

None. No placeholder, no `TODO`, no skipped test, and no `<verify>` block left unrun.

## Threat Flags

None. The plan's threat register (T-46-06 through T-46-09, T-46-SC) is unchanged by what shipped:
no new network surface, no new dependency, no schema change. T-46-06's mitigation is implemented and
covered by acceptance ("the raw `base` is passed to neither" — both calls take `qualified.as_str()`).
T-46-09's mitigation is implemented by delegation to `check-ref-format` with no denylist. T-46-07
(R-01) remains `accept`, as recorded above.

## Self-Check: PASSED

Ran after writing this file, in-session:

- `crates/devflow-cli/src/commands.rs` — FOUND
- `.planning/phases/46-ci-load-shape-and-operator-input-validation/46-02-SUMMARY.md` — FOUND
- commit `aa0707e` — FOUND in `git log --all`
- commit `1272584` — FOUND in `git log --all`
- Both git calls take `qualified.as_str()` (`commands.rs:181`, `:198`); the raw `base` reaches
  neither.
- The existence call uses `.unwrap_or(false)` and the classifier `.unwrap_or(true)`, in that order.
