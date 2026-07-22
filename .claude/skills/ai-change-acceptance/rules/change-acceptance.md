# AI Change Acceptance — Requirements

**Applies to:** DevFlow

The five requirements a diff must satisfy before an AI-authored change is
accepted (D-19). Each requirement is stated as an assertion about the diff,
the check a reviewer performs to decide it, and the observable signature of a
failure. All five must hold; none is optional.

## 1. A regression test that fails before the change

**Requirement:** The diff includes a test that, run against the code as it
stood *before* this change, fails.

**Check:** Read the test. Confirm it exercises the behavior the change claims
to add or fix. If the PR/commit history doesn't demonstrate a RED run, mentally
(or actually) revert the implementation hunk and ask whether the test would
still fail for the right reason.

**Failure signature:** A test that would have passed against the old code too
— it was never red, so it proves nothing about this change. Also a
red flag: a test added in the same commit as the fix with no evidence it was
ever run against the pre-fix state.

## 2. At least one assertion at a public or otherwise stable boundary

**Requirement:** At least one assertion in the diff's tests exercises a
public function, a CLI invocation, a file/JSON contract, or another boundary
that callers outside the test actually depend on — not only a private helper
three layers removed from anything a caller touches.

**Check:** Identify what the assertion calls. If every assertion in the diff
targets a `fn` that is `pub(crate)` or narrower and has no path to an
externally observable effect, this requirement is not met.

**Failure signature:** A test suite that is 100% green while the public
behavior it's supposed to protect has visibly regressed, because every
assertion stopped one layer short of anything observable.

## 3. Evidence the test fails for the intended reason

**Requirement:** When the regression test was run RED (requirement 1), it
failed because the *behavior under test* was absent or wrong — not because of
a setup error, a compile failure, a panic in unrelated code, or a
misconfigured fixture.

**Check:** Read the actual RED failure output (or reproduce it). The
assertion that fails, and the message it produces, must match the behavior
the change is supposed to add.

**Failure signature:** A "RED" that was actually a compile error, a missing
import, or a fixture that panicked before reaching the assertion under test —
none of which say anything about whether the intended behavior existed.

## 4. Full affected-package tests, clippy, and fmt — all clean

**Requirement:** The affected package's full test suite passes, `cargo clippy
--workspace --all-targets -- -D warnings` is clean, and `cargo fmt --check` is
clean.

**Check:** Actually run the commands and read the reported counts and exit
status — do not trust a summary claim in a commit message or PR description.

**Failure signature:** A claimed "all tests pass" that, on rerun, reports a
different count, or a clippy/fmt invocation that was run with a narrower scope
than `--workspace --all-targets` (silently skipping `#[cfg(test)]` code — see
`CONTRIBUTING.md` § PR Process and `crates/devflow-cli/tests/
devcontainer_ci_failfast.rs`).

This project has been bitten by two specific false-green traps that a
reviewer must know before accepting a green result here:

- **`cargo test --exact` with a bare test name matches nothing and still
  exits 0.** A reviewer accepting a claimed pass must read the *reported pass
  count* (e.g. `12 passed; 0 failed`), not just the exit status — `0 passed;
  0 failed` exits 0 and proves nothing ran.
- **The package is `devflow`, not `devflow-cli`.** A `-p devflow-cli`
  invocation does not run the CLI's own unit tests; the binary crate's Cargo
  package name is `devflow`. An invocation naming the wrong package can exit
  0 while silently testing zero of the intended code.

Also record a confirmed dead end (STATE.md, 18-01 decision entry): `cargo test
-p devflow --lib` does not work on this crate — `devflow` (the `devflow-cli`
package) is binary-only, so `--lib` hard-errors (`no library targets found`)
rather than filtering tests. Use the bare form, `cargo test -p devflow
<name>`, instead.

## 5. Independent review of both the implementation and the test signal

**Requirement:** The reviewer evaluates the test itself — not only whether
the implementation looks correct — asking whether the test could ever fail
given a wrong implementation.

**Check:** For each new or changed assertion, ask: "if the implementation
were subtly wrong in the way this change is supposed to prevent, would this
assertion catch it?" If reviewing only the implementation diff and treating
green tests as sufficient, this requirement is not met.

**Failure signature:** A test that cannot fail slipping through because the
reviewer's attention went entirely to the implementation hunk and treated the
test hunk as a formality. See `rules/test-signal-rejection.md` for the
concrete shapes such a test takes.
