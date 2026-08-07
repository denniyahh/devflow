---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
plan: 06
subsystem: release-documentation
tags: [999.77, 999.78, 999.79, 999.84, 999.86, 999.87, HARDEN-01, HARDEN-05, D-08, breaking-change, changelog]
status: complete

requires:
  - "35-01: phase_commit_count -> Option<u32>, evaluate_layer3 re-pointed"
  - "35-03: classify_ssh_add_status and SigningStatus removed"
  - "35-04: Mode::should_gate widened; MAX_PHASE_VALIDATE_FAILURES, phase_failure_ceiling_reached, State::phase_validate_failures added"
  - "35-05: phase_verification_fingerprint and State::last_verification_fingerprint added"
provides:
  - "CHANGELOG.md 2.5.0 entry — the verified public-surface enumeration D-08 asks for"
  - "removal note at the deleted signing predictor's former site in git.rs"
  - "release-level note at phase_commit_count recording the return-type break"
affects:
  - "CHANGELOG.md"
  - "crates/devflow-core/src/git.rs"
  - "crates/devflow-core/src/agent_result.rs"

tech-stack:
  added: []
  patterns:
    - "derive an API delta mechanically from git at two revs, then corroborate with two further independent extraction methods"
    - "capture a grep's PRE-EDIT baseline before using its post-edit match as evidence"
    - "comment-stripped counting where the deliverable is itself comment prose naming the removed symbol"

key-files:
  created: []
  modified:
    - CHANGELOG.md
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/agent_result.rs

decisions:
  - "The API delta was derived from a comment-stripped pub-surface diff of devflow-core between 749a151 and HEAD, then corroborated by two independent methods — not transcribed from the plans"
  - "mode.rs and state.rs left untouched: all four items already carry full contracts from 35-04/35-05, and the plan forbids restating one"
  - "The two new State fields are filed as ADDITIVE, verified against source (#[non_exhaustive] + #[serde(default)]), not as breaking"
  - "evaluate_layer3 is filed as behaviour-only: its full signature is byte-identical across the phase"
  - "phase_verification_exists is recorded as an explicit non-event, not omitted"
  - "cargo doc's 35 warnings are pre-existing and out of scope — proven, not assumed"

metrics:
  duration: "17m"
  completed: 2026-08-07

actuals:
  tokens: 8600
  tasks: 2
  commits: 2
---

# Phase 35 Plan 06: Public-API Break Enumeration Summary

The `v2.5.0` public-surface change to `devflow-core` is now enumerated in `CHANGELOG.md` and
explained at each removal site in source. Every row was derived mechanically from git and
corroborated by two further independent methods — none was transcribed from the plans.

## How the enumeration was derived, and what that method cannot see

**The deliverable here is an enumeration, and the failure mode is enumerating from memory.** The
list was therefore produced from the tree, by diffing the `pub` surface of `devflow-core` between
the phase base (`749a151`) and `HEAD`.

**Method 1 — comment-stripped line grep.** For every `.rs` file under `crates/devflow-core/src`
at each rev: strip line comments (`sed 's|//.*||'`), keep lines matching a `pub` declaration
(deliberately excluding `pub(crate)`/`pub(super)`), normalise whitespace, sort, diff.
**441 declarations at base, 444 at HEAD.**

**Method 2 — full-declaration extraction, independent of method 1.** Same comment stripping, but
an `awk` pass joins continuation lines until parentheses balance, so a multi-line signature is
compared *in its entirety* rather than by its first line. **441 / 444, identical item set**, and
it additionally recovered the full new signatures.

**Method 3 — straight from the raw git diff, using neither extractor.**
`git diff 749a151..HEAD -- crates/devflow-core/src`, comment-stripped, filtered to `pub` lines.

**All three agree exactly: 9 changed lines — 2 removals, 2 changed pairs, 5 additions.** The
arithmetic is consistent (`441 − 2 + 5 = 444`).

**Why method 1 alone would not have been enough, stated because it is the trap this plan was
warned about.** Method 1 collapses a multi-line signature to `pub fn name(`. Both breaking
signature changes happened to move from single-line to multi-line, so method 1 caught them — but
it caught them by accident of formatting. Had `should_gate` already been line-wrapped, a
parameter change would have been **invisible** to method 1 and the enumeration would have been
short by one breaking entry. Method 2 exists precisely to close that, and it is why
`evaluate_layer3`'s absence from the diff is trustworthy: method 2 compares its *whole* signature,
not its first line.

### What this derivation does NOT establish

- **It covers declaration sites, not reachability.** A symbol whose visibility changed only via a
  `pub use` re-export in `lib.rs` would appear as a re-export line, which the extractors do capture
  — and no such line changed — but the method reasons about text, not about rustdoc's resolved
  public graph. A `cargo public-api`-style check against real rustdoc JSON would be strictly
  stronger and was not run.
- **It scopes to `devflow-core` only.** `devflow-cli` is a binary crate with no published API, so
  its many changes are correctly out of scope; that is an assumption about packaging, not something
  this derivation measured.
- **Comment stripping is line-based.** A `//` inside a string literal on a `pub` declaration line
  would truncate it. No such line exists here, but the extractor does not detect the case.
- **`#[cfg(test)]` items are not excluded by the extractor.** Checked: zero `pub fn/struct/enum`
  declarations follow a `#[cfg(test)]` marker in `state.rs`, and none of the 9 diff lines sits in a
  test module (each was confirmed by reading its surrounding context). So the *diff* is unaffected,
  even though the raw 441/444 totals would be inflated if such items existed.

## The verified surface record — per row, with the command that confirmed it

| Item | Classification | Verified by | Result |
|---|---|---|---|
| `agent_result::phase_commit_count` | changed, **breaking** | methods 1/2/3 | `-> u32` ⇒ `-> Option<u32>`; params unchanged |
| `mode::Mode::should_gate` | changed, **breaking** | methods 1/2/3 | `(self, stage, consecutive_failures)` ⇒ `+ phase_validate_failures` |
| `git::classify_ssh_add_status` | removed, **breaking** | methods 1/2/3; `grep -r … crates/` | absent — exit 1, **no matches at all** pre-edit |
| `git::SigningStatus` | removed, **breaking** | methods 1/2/3; `grep -r … crates/` | absent — exit 1, **no matches at all** pre-edit |
| `agent_result::evaluate_layer3` | changed, **behaviour only** | `git show 749a151:… \| sed -n '1971,1975p'` vs `sed -n '2024,2028p'` HEAD | **5-line signature byte-identical**; absent from all three diffs |
| `mode::MAX_PHASE_VALIDATE_FAILURES` | added | methods 1/2/3 | `pub const … = 10;` |
| `mode::phase_failure_ceiling_reached` | added | methods 1/2/3 | `pub fn …(u32) -> bool` |
| `agent_result::phase_verification_fingerprint` | added | methods 1/2/3 | `pub fn …(&Path, u32) -> Option<u64>` |
| `State::phase_validate_failures` | added, **non-breaking** | see classification checks below | `#[serde(default)]` at `state.rs:149` |
| `State::last_verification_fingerprint` | added, **non-breaking** | see classification checks below | `#[serde(default)]` at `state.rs:182` |
| `agent_result::phase_verification_exists` | **unchanged** | `grep -n 'fn phase_verification_exists'` at both revs | identical signature (base `:2654`, HEAD `:2720`) |
| `git::inline_key_fingerprint` | removed, **private** | `grep -r … crates/` | absent; not public surface, recorded so the deletion reads as deliberate |

### The classification checks — C1's correction was checked, not assumed

The review pass caught an earlier draft filing the two new `State` fields as breaking. Both
premises for filing them as **additive** were re-verified against source *before* the entry was
written, per the plan's explicit requirement:

- `grep -n -B2 'pub phase_validate_failures\|pub last_verification_fingerprint' crates/devflow-core/src/state.rs`
  → both fields preceded by `#[serde(default)]` (lines `149`/`150` and `182`/`183`).
- `sed -n '25,45p' crates/devflow-core/src/state.rs` → `#[non_exhaustive]` sits directly above
  `pub struct State`, with the comment two lines up stating the intent verbatim: *"Paying that cost
  once here makes every future field additive."*

So an external crate cannot construct `State` by literal at all, and existing
`.devflow/state-NN.json` files deserialize unchanged. **The breaking group in the shipped entry
names exactly two changed items and two removed ones. Neither new field appears in it.**

`evaluate_layer3`'s behaviour-only filing was verified the same way rather than argued: its full
signature is byte-identical across the phase (above), so no consumer's code changes even though its
observable classification does.

## The changelog entry

`CHANGELOG.md` gains a `## 2.5.0 — 2026-08-07` heading above the existing `2.4.0` entry, matching
its established shape (prose lead, `### What's new`, `### Fixed`, `### Known Issues`), plus a
`### Public API (devflow-core)` section carrying the enumeration in five groups: changed-breaking,
removed-breaking, changed-behaviour-only, added-non-breaking, and unchanged-despite-anticipation.

Each row gives the fully-qualified path and the backlog identifier the change closes, so the
reasoning is reachable rather than only the fact.

The entry states plainly that the release carries breaking `devflow-core` changes **without** a
major version bump, and why: no external consumers, so the compatibility risk is theoretical, and
the break is documented instead of versioned (D-08). It says so in those terms precisely so a
reader who finds a break under a minor bump does not have to guess whether it was an oversight.

**Per-symbol presence, measured** (`grep -o … | sort | uniq -c`):

| Symbol | Count | | Symbol | Count |
|---|---|---|---|---|
| `phase_commit_count` | 3 | | `phase_verification_fingerprint` | 1 |
| `should_gate` | 1 | | `evaluate_layer3` | 2 |
| `phase_validate_failures` | 2 | | `classify_ssh_add_status` | 1 |
| `last_verification_fingerprint` | 1 | | `SigningStatus` | 1 |
| `MAX_PHASE_VALIDATE_FAILURES` | 3 | | `phase_verification_exists` | 1 |
| `phase_failure_ceiling_reached` | 1 | | | |

**Negative control on that grep:** the same command against two symbols deliberately *not*
enumerated (`phase_commit_count_checked`, `SigningPredictor`) returns **0**. A grep that cannot
report zero is not evidence of presence.

## The removal notes

**`git.rs:724`** — a 28-line note directly under the `tag-signing viability` banner, at the exact
place the removed items used to sit (base `git.rs:728`/`:743`, immediately below that same banner).
It names both `SigningStatus` and `classify_ssh_add_status`, names the release, states that their
removal is breaking and points at `CHANGELOG.md`, gives the reason — viability was *predicted* from
`ssh-add -l`, and an agent identity listing cannot see private key material on disk, so it tested a
condition the real signing operation does not require and false-negatived on two release cuts — and
points at `check_signing_viability`'s probe as the replacement. It closes with the reason the note
exists at all: dead public API that still reads like the sanctioned way to judge signing viability
is how the predictor survived review twice, so a bare absence invites the mistake in reverse.

**`agent_result.rs:1863`** — an 18-line `# Changed in v2.5.0 — breaking` section on
`phase_commit_count`'s doc comment, recording the `u32 → Option<u32>` change and what the two states
now mean for someone updating a call site, and naming `.unwrap_or(0)` as precisely the wrong way to
restore the old form. It complements rather than repeats the 35-01 doc comment above it, which
explains the contract but not that it *changed in this release*.

**`mode.rs` and `state.rs`: nothing added, deliberately.** The plan says to add nothing if the
items already document their own contracts. They do, thoroughly — `MAX_PHASE_VALIDATE_FAILURES`
(`mode.rs:20-41`, including why it sits above `MAX_CONSECUTIVE_FAILURES` and that it must never
introduce an abort path), `phase_failure_ceiling_reached` (`:184-205`, including why it is a named
predicate), `should_gate`'s widening rationale (`:234-242`), and both `State` fields
(`state.rs:130-148` and `:151-181`, including both reset events and the `#[serde(default)]`
reasoning). Restating any of it would have been noise.

### The acceptance greps needed a pre-edit baseline, or they prove nothing

Both of this plan's acceptance greps **already matched before I edited anything.** Recording only
the post-edit match would have been a proxy measurement — the grep would have "passed" identically
had I written no note at all.

| Grep | Pre-edit | Post-edit | New match at |
|---|---|---|---|
| `999.86` in `git.rs` | **3** (`:1011`, `:2257`, `:2354`) | **4** | `:724` — the removal note |
| `999.77` in `agent_result.rs` | **1** (`:1842`) | **3** | `:1867`, `:1876` — the release note |

The **delta** is the evidence, not the match.

### A consequence the plan's own verify command does not survive, by design

Task 1's `<automated>` check is `rg -c 'phase_commit_count' CHANGELOG.md && ! rg -q
'classify_ssh_add_status|SigningStatus' crates/`. It **passed at Task 1** (count `3`; the removed
symbols absent from `crates/`, `grep` exit `1` — and notably with *no matches at all*, since 35-03
retired even the comments naming them).

**After Task 2 it necessarily fails**, because the removal note names both symbols in prose — which
is exactly what the plan's own `key_links` block requires (`pattern: SigningStatus`). This is
CLAUDE.md's recorded trap ("a grep over source counts comment prose") operating in reverse: the
deliverable *is* comment prose naming the removed symbol.

The correct post-Task-2 measurement is comment-stripped, and it is clean:

| Measure | Raw | Comment-stripped |
|---|---|---|
| `classify_ssh_add_status\|SigningStatus` across `crates/` | 2 (both in the note) | **0** |

**Stripper control:** the same stripped pass counts `check_signing_viability` at **13**, so the
stripper is not simply deleting everything. Both numbers are recorded rather than only the
flattering one.

## The setup-checklist check — performed, with its result

`CLAUDE.md` requires `.planning/DEV-SETUP-CHECKLIST.md` be updated in the same commit when a change
touches git policy, hooks, CI, devcontainer/toolchain pins, or GSD config.

**Assessed:** every file phase 35 changed, via `git diff --stat 749a151..HEAD`. The result is 12
files — eleven `crates/**/*.rs` sources and tests, plus `CHANGELOG.md`. The same diff restricted to
the checklist's scoped paths (`scripts/`, `.github/`, `Cargo.toml`, `rust-toolchain.toml`,
`.devcontainer/`, `.planning/config.json` — corresponding to checklist sections 1-8) returns
**empty**.

**Negative control, because an empty diff and a broken pathspec are otherwise
indistinguishable:** `git ls-files` over those same paths reports **19 tracked files**, and the
identical pathspec over a wider commit range (`HEAD~40..HEAD`) *does* report `Cargo.toml | 4 ++--`.
So the empty result means "unchanged", not "path does not exist".

**Conclusion: no checklist update required.** Nothing in this phase falls under its scope.

## Deviations from Plan

### [Rule 3 — out of scope, not fixed] `cargo doc -p devflow-core --no-deps` emits 35 pre-existing warnings

- **Found during:** Task 2 verification.
- **Issue:** the plan's acceptance criterion requires `cargo doc` to complete **without warnings**.
  It reports **35**, so the criterion as literally written is **not met**.
- **Established as pre-existing rather than assumed to be.** Two independent lines of evidence:
  1. **Warning locations vs. my edited ranges.** My additions are `agent_result.rs:1863-1880` (18
     lines) and `git.rs:724-751` (28 lines), per `git diff -U0`. Enumerating all 35 warning
     locations, **none falls inside either range** — the nearest in `agent_result.rs` are `:1768`
     and `:2536`, and the only `git.rs` warning is at `:129`. My `git.rs` note is a plain `//`
     comment (cannot produce rustdoc warnings) and my `agent_result.rs` note contains no intra-doc
     links at all.
  2. **Seven warnings sit in files this phase never opened** — `workflow.rs:81`, `workflow.rs:233`,
     `gates.rs:305`, `config.rs:246`, `agent.rs:352`, `agent.rs:355` (x2).
     `git diff --stat 749a151..HEAD` over those four files is **empty**, so those warnings
     definitively pre-date phase 35.
- **Not fixed**, per the executor's scope boundary: they are pre-existing warnings in unrelated
  files. They are all one class — public documentation linking to a private item.
- **Worth stating plainly:** `scripts/check.sh` (this repo's definition of green) runs
  fmt + clippy + test and **does not run `cargo doc` at all**, which is how a 35-warning rustdoc
  baseline persisted unnoticed. That is a real gap, but closing it is not this plan's job.

### [Recorded, not a fix] Task 1's automated verify cannot survive Task 2

Described in full above. Not a defect in either task — the two requirements genuinely conflict, and
the plan's `key_links` block is the one that governs. The comment-stripped measurement replaces it.

## Reconciliation against the four prior SUMMARYs

The planner's list was treated as a checklist to verify. Reconciled row by row against what actually
shipped:

- **35-01** deleted a planned deliverable (`devflow-core`'s `NoGitPath` guard) and re-pointed
  `evaluate_layer3`. **Neither affects the public enumeration** — `NoGitPath` was test-support in a
  crate-private module and never appeared in the `pub` surface at either rev (confirmed: absent from
  both extractions). `evaluate_layer3` is captured, as behaviour-only.
- **35-02** changed **no** `devflow-core` source at all — it added a test to `devflow-cli`. Nothing
  to enumerate. Its 999.84 closure is recorded in the changelog's *Fixed* group, not in the API
  section.
- **35-03** folded in a post-completion `setsid` fix *after* the plan was marked complete
  (`34aab4f`). Checked: it changed only the probe's `Command`, adding no public item. **Not in the
  API section**; its user-visible effect is described under *What's new*, and its untested status is
  carried into *Known Issues* as 999.88.
- **35-04** deviated on F-7's dry-run clause ordering. That is `devflow-cli` internal and produces
  no public surface. Its four public items are all captured.
- **35-05** deliberately kept `phase_verification_exists` with **zero callers** to avoid a third
  public-API break. **The changelog does not claim it was removed.** It is recorded in its own
  "Unchanged" group, stating that the anticipated third break did not materialise and that it
  currently has no in-workspace caller.

**Nothing in the planner's list was dropped, and nothing was added.** All eleven public rows plus
the one private removal match what the four SUMMARYs reported. The planner's list was accurate; it
is now also verified.

## Verification

| Check | Result |
|---|---|
| `scripts/check.sh all` | **`==> check.sh: all OK`** — fmt clean, clippy clean under `-D warnings` |
| suites with `test result: ok.` | **22** |
| suites reporting `N failed` (N≥1) | **0** |
| `cargo test -p devflow-core --lib` | **569 passed; 0 failed** |
| `cargo fmt --check` | clean (no output, exit 0) |
| `cargo doc -p devflow-core --no-deps` | 35 warnings, **all pre-existing** — see Deviations |
| pub-surface diff, methods 1/2/3 | all three agree: 2 removed, 2 changed, 5 added |
| `git diff -- Cargo.toml` | **empty**; version still `2.4.0`, left for the release cut |
| `git diff -- .planning/` | **empty** — `STATE.md`, `ROADMAP.md`, `REQUIREMENTS.md`, `PROJECT.md` untouched |
| `grep -n 'milestone' .planning/STATE.md` | still `v2.5.0`; milestone not renamed |
| `git status --short` after both commits | clean |

**Measured limits of the green run:** `scripts/check.sh all` is **n=1** here — it supports "this
change does not break the suite" and says nothing about flakiness. This plan adds no executable
code, so that is a weaker claim than usual but also a smaller one: the only way documentation could
break the suite is a doctest, and `Doc-tests devflow_core` reports `0 tests`.

## Known Stubs

None. This plan adds documentation only — no code, no placeholder values, no TODO/FIXME markers, no
`#[ignore]`d tests.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern or trust-boundary schema change. The
text names symbol paths and backlog identifiers only — no operator path, host detail, or key
material (T-35-28, accepted as planned). T-35-26, T-35-26b and T-35-27 are mitigated as specified:
the enumeration is verified against source (T-35-26), the breaking group is held to the two changes
and two removals a consumer must act on with `#[non_exhaustive]`/`#[serde(default)]` checked before
classifying (T-35-26b), and the removal site carries its reason and its replacement (T-35-27).
T-35-SC accepted — no packages installed, and this plan does not perform the release.

## Still open — needs the operator's word, not my assumption

1. **RESOLVED 2026-08-07 (operator, during 35-verify-work) — filed as backlog 999.95.**
   **`cargo doc` is not in this repo's definition of green, and carries a 35-warning baseline.**
   `scripts/check.sh all` runs fmt + clippy + test only. Every warning is the same class (public
   documentation linking to a private item), so it is likely a small mechanical fix, but adding a
   doc step to `check.sh` changes what every future commit must satisfy and is a policy decision,
   not mine. Out of scope here; worth a backlog entry.

   **Disposition: fix the warning class, do NOT add the gate yet — the two are separate decisions.**
   Gating is far easier to decide against a zero-warning baseline than against the current one, and
   bundling them would force a policy call as a side effect of a documentation cleanup. Both are
   scoped in 999.95, with the gating question (and whether `check.sh` or the pre-push container gate
   is the right home, given doc builds are slow) left explicitly open.

   **Re-measured during verify-work rather than trusting this figure:** `cargo doc --workspace
   --no-deps` reports **33** warnings today, all in `devflow-core`; `devflow-cli` contributes none.
   The single-class claim was re-checked with a negative control (filtering for any other warning
   kind returns only the summary line). The 35-vs-33 discrepancy is unexplained and recorded in
   999.95 as something to re-measure rather than inherit.
2. **The version step is deliberately NOT done.** `Cargo.toml` remains at `2.4.0` while
   `CHANGELOG.md` now has a `2.5.0` heading. That is what the plan requires — the version is set in
   two places at release time, with `devflow-core` publishing before `devflow-cli` — but it does
   mean the tree is momentarily inconsistent between now and the release cut, and anyone reading
   `Cargo.toml` in between will see the older number.

## Self-Check: PASSED

Files verified present on disk: `CHANGELOG.md`, `crates/devflow-core/src/git.rs`,
`crates/devflow-core/src/agent_result.rs`, and this SUMMARY.

Commits verified present in this worktree's history: `896c9fe`, `ba14166`.

`STATE.md` and `ROADMAP.md` were **not** modified — this plan ran as a worktree-isolated parallel
executor and the orchestrator owns those writes after the wave merges.
