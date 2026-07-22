# AI Change Acceptance

**Applies to:** DevFlow

Governs what a reviewer requires before accepting a change to this repository,
and which test shapes a reviewer must reject regardless of whether the suite
is green.

## When this applies

Effectively every change to this repository is authored or substantially
modified by an AI agent. This is not a special case — it is the default. The
acceptance bar below is written down and applied by
[`/gsd-code-review`](https://github.com/denniyahh/devflow) before Ship, rather
than assumed or left to individual reviewer judgment. There is deliberately no
new lint pass, CI job, or script enforcing this — enforcement is the existing
review-before-Ship gate, which already refuses to ship on Critical findings.
This skill supplies the criteria that gate applies.

## The five acceptance requirements

A change is not accepted unless all five hold. Full detail, the check a
reviewer performs for each, and the observable failure signature live in
`rules/change-acceptance.md`.

1. A regression test that fails before the change.
2. At least one assertion at a public or otherwise stable boundary.
3. Evidence the test fails for the intended reason — not a setup error, a
   compile failure, or an unrelated panic.
4. Full affected-package tests, `cargo clippy --workspace --all-targets -- -D
   warnings`, and `cargo fmt --check` — all clean.
5. Independent review of both the implementation and the test signal.

## The four rejection patterns

A test exhibiting any of these is rejected outright, independent of whether it
currently passes. Full detail, recognition guidance, and worked examples
(including a real instance from this repository's own history) live in
`rules/test-signal-rejection.md`.

1. Tests that only assert constants.
2. Tests that reproduce the production algorithm inside the test body.
3. Tests that compare a function call with itself.
4. Tests that grep implementation text without asserting a runtime contract.

## Where to look next

- `rules/change-acceptance.md` — the five requirements, one section each, with
  a reviewer check and a failure signature. Also records this project's two
  specific false-green traps for requirement 4 (`cargo test --exact` matching
  nothing yet exiting 0; `-p devflow-cli` not running the CLI's own tests).
- `rules/test-signal-rejection.md` — the four rejection patterns, one section
  each, plus the boundary that separates a legitimate source-property test
  from a grep used as a substitute for a runtime contract.

If this file and `CONTRIBUTING.md`'s `## AI Change Acceptance` section ever
disagree, this skill (and its `rules/` files) win — `CONTRIBUTING.md` is a
summary for human contributors, not a second copy of the contract.
