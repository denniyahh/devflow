# AI Change Acceptance — Rejected Test Signal Shapes

**Applies to:** DevFlow

The four test shapes rejected outright under D-19, independent of whether
they currently pass. Each is named, explained (why it produces a test that
cannot fail), given a diff-recognition rule, and illustrated with a worked
example — a real instance from this repository's history where one exists,
otherwise a labeled synthetic example.

## 1. Tests that only assert constants

**Pattern:** The test's assertion compares two literal or hardcoded values,
neither of which is produced by the code path under test — the assertion
would hold no matter what the implementation did.

**Why it can't fail:** The value being checked never depends on the
implementation. Changing the implementation to something wrong leaves the
assertion passing, because it was never wired to the implementation's output.

**How to recognize it:** Look at what feeds each side of the assertion. If
neither side traces back through a function call that the change touches, the
test is asserting a constant against itself.

**Worked example (synthetic — no in-repo instance found):**

```rust
// Rejected: asserts a literal against a literal. Passes even if
// `Stage::Ship.as_wire_str()` were deleted, since the right-hand
// side never calls it.
#[test]
fn ship_stage_name_is_ship() {
    let expected = "ship";
    assert_eq!("ship", expected);
}
```

The fix is to assert the actual function's output against the expected
constant: `assert_eq!(Stage::Ship.as_wire_str(), "ship")`.

## 2. Tests that reproduce the production algorithm inside the test body

**Pattern:** The test computes its own copy of the logic under test — or
invents behavior that has no production counterpart at all — instead of
calling the real implementation and checking its output against an
independently-derived expected value.

**Why it can't fail:** If the test's logic and the implementation's logic
change together (because they're the same logic, duplicated), or if the
"production" side being tested was never wired to anything real, the test and
the code agree by construction. A regression in the real implementation has
no counterpart in the test to disagree with.

**How to recognize it:** Ask whether the test defines behavior (a struct, a
match arm, a computed value) that exists *only* inside the test module, with
no non-test code implementing the same contract.

**Worked example — real, from this repository's history
(`.planning/TEST-SUITE-QA-REVIEW.md`, deleted in commit `4e8bf9c`):**

`agents::ReviewerSetTestAdapter` was a fake `AgentAdapter` whose `preflight()`
method invented reviewer-receiver-set validation logic purely for the test:

```rust
impl AgentAdapter for ReviewerSetTestAdapter {
    fn preflight(&self, _state: &crate::state::State) -> Result<(), String> {
        if self.reviewers.is_empty() {
            Err("reviewer receiver set is empty".to_string())
        } else {
            Ok(())
        }
    }
}
```

No production `AgentAdapter` at the time had a `reviewers` field or this
validation rule at all — the test asserted that the test's own invented logic
behaved the way the test's own invented logic was written to behave. It was
removed because it protected nothing shipped.

## 3. Tests that compare a function call with itself

**Pattern:** The assertion calls the same function twice with the same
inputs and asserts the two results are equal to each other, rather than
asserting either result against an independently-known expected value.

**Why it can't fail:** A pure function called twice with identical inputs
always returns the same output as itself, regardless of whether that output
is *correct*. The assertion is a tautology — it tests that the function is
deterministic (usually true by construction in Rust, absent explicit
randomness or shared mutable state), not that it computes the right thing.

**How to recognize it:** The left-hand and right-hand side of an `assert_eq!`
are two invocations of the same function with the same arguments.

**Worked example — real, from this repository's history (deleted in commit
`4e8bf9c`):**

```rust
#[test]
fn decide_action_is_deterministic() {
    for stage in [Stage::Define, Stage::Plan, Stage::Code, Stage::Validate, Stage::Ship] {
        for outcome in [AgentStatus::Success, AgentStatus::Failed, /* ... */] {
            assert_eq!(decide_action(stage, outcome), decide_action(stage, outcome));
        }
    }
}
```

This proves `decide_action` has no hidden state — a real property, but not a
correctness check. It would pass even if `decide_action` always returned the
same wrong `Action` for every input. It was removed in favor of tests that
assert specific `(stage, outcome) -> Action` pairs against the documented
policy table.

## 4. Tests that grep implementation text without asserting a runtime contract

**Pattern:** The test reads source or config text as a string and asserts a
substring or pattern is present, as a stand-in for checking what the program
actually does when run.

**Why it can't fail (as a substitute):** A source-grep can pass while the
runtime behavior it's meant to protect is wrong — the exact string can be
present but unreachable, present in a comment, or present but bypassed by a
code path the grep doesn't see.

**How to recognize it:** The test reads a `.rs`, `.yml`, or other source file
with `std::fs::read_to_string` and asserts `.contains(...)` against literal
implementation text, with no process spawned and no runtime output captured.

**Worked example — real, current in-repo instance
(`crates/devflow-cli/tests/devcontainer_ci_failfast.rs`,
`devflow_test_clippy_matches_ci_scope`):**

```rust
let src = std::fs::read_to_string(&path).unwrap();
let has_narrow_form = src.contains("\"cargo clippy -- -D warnings\"");
assert!(!has_narrow_form, /* ... */);
assert!(
    src.contains("cargo clippy --workspace --all-targets -- -D warnings"),
    /* ... */
);
```

This asserts `main.rs`'s *source text* contains the wide clippy invocation
string. It would pass even if that string appeared in a comment, in dead
code, or in a code path `devflow test` never actually executes — it does not
run `devflow test` and observe the real clippy invocation's argv.

## Boundary: not every source-grep is rejected

A test that reads source text is not automatically rejected when the
property genuinely under test *is* a source property — for example, an
assertion that a specific argument is absent from a specific function body,
or that a workflow file's first command line is literally `set -e` (see
`devcontainer_runcmd_fails_fast_before_any_cargo_invocation` in the same
file: the CI contract it protects — command *ordering* inside a YAML block
scalar — is a property of the file's text, not of a runtime process, so
reading the text *is* the correct check).

What is rejected is grepping as a **substitute** for a runtime contract that
could have been asserted directly — as in the `devflow_test_clippy_matches_ci_scope`
example above, where the actual claim ("`devflow test` runs this clippy
invocation") is a runtime fact that a source-grep only approximates.
