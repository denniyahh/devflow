---
phase: 21
phase_name: "Operator Legibility & Observability"
project: "DevFlow"
generated: "2026-07-23"
counts:
  decisions: 7
  lessons: 4
  patterns: 4
  surprises: 3
missing_artifacts:
  - "21-UAT.md"
---

# Phase 21 Learnings: Operator Legibility & Observability

## Decisions

### Recut the phase from "Usability & Release Execution" to "Legibility & Observability"
Removed the release-cut executor (999.25 → its own phase) and `--base` override (999.28 → Phase 22), and backfilled with legibility/observability units. Every unit is required to be single-writer, reversible or detection-only, and dogfood-testable with no irreversible side effect.

**Rationale:** No phase can build on an unmerged predecessor, and the release-executor/`--base` work needed its own design pass; the operator wanted a phase that improves legibility without shipping risk. (Operator decision, not `/gsd-review-backlog`-promoted.)
**Source:** 21-CONTEXT.md (D-01, D-02)

### Doctor drift check is detection-only and shares the existing reporter
The new planning-doc staleness check flags ROADMAP/STATE-vs-git-tag drift but never auto-edits prose, and integrates as a new `Check` in the existing `doctor` path so human and `--json` output stay consistent — no forked second reporter.

**Rationale:** Same discipline 18a's reconciliation already follows; auto-correcting narrative prose is irreversible and risks masking the very drift being reported.
**Source:** 21-CONTEXT.md (D-04, D-05); 21-03-SUMMARY.md

### Re-scoped 21c to only `sequentagent`'s second agent before planning
The "monitor unrecorded" half of the original "one process per phase" item had already shipped in v1.5.0 (18b), so 21c was narrowed to the one remaining gap: `sequentagent`'s second agent, which runs off the stage machine and had no tracked record.

**Rationale:** Planning against the original (already-half-done) scope would have re-implemented shipped work; re-scoping first kept the plan honest.
**Source:** 21-CONTEXT.md (D-06); 21-04-SUMMARY.md

### Sequenced 21d (staleness content-awareness) first
Ordered the dogfood-staleness fix ahead of the other three units even though it's the smallest.

**Rationale:** The staleness guard hard-blocks the phase's *own* Plan/Code/Validate stages after every `.planning/` commit, so fixing it first removed the no-op-rebuild tax for the rest of the phase.
**Source:** 21-CONTEXT.md (D-07)

### Reuse `affects_compiled_binary` verbatim rather than fork a second matcher
21d's content-aware arm filters `git diff --name-only <embedded> HEAD` through the *existing* predicate; verified there is exactly one new helper and `BUILD_AFFECTING_FILES` keeps its single definition + usage.

**Rationale:** A forked file-extension matcher would be a second source of truth that could silently drift from the dirty-tree arm it mirrors.
**Source:** 21-01-SUMMARY.md (D-07)

### Path-free two-line text record for the sequentagent slot, not JSON
The slot record is a plain `slot\nagent\n` text format written by a path-free API.

**Rationale:** A two-field struct has no genuine serialization failure mode; JSON would have forced either an `.expect()` (banned outside tests in this codebase) or needless error-plumbing. Text sidesteps both and parses defensively.
**Source:** 21-04-SUMMARY.md

### Observability writes are best-effort, never fatal
`run_agent_blocking` warns and continues if the slot write fails rather than propagating a `CliError`; `recovery_hints` only appends the `advance` verb when `gate_pending` is true (narrower than "every stuck phase").

**Rationale:** An observability write must never block or fail the real agent run (honors 21c's "MUST NOT change sequentagent's behavior" prohibition and T-21c-03); suggesting `advance` where the source path doesn't prove it's correct would mislead.
**Source:** 21-04-SUMMARY.md, 21-02-SUMMARY.md

---

## Lessons

### A bare-substring `cargo test` filter can pass vacuously on zero tests
21-03's plan specified `cargo test --workspace commands::tests::doctor_json` as its verify gate, but the `doctor_json_*` tests live in nested `mod doctor_reconciliation` / `mod planning_doc_staleness` blocks, so the module segment falls between `tests::` and `doctor_json` and the substring filter matches **0 tests** — the `rg "test result: ok"` grep then passes vacuously on `0 passed`.

**Context:** Caught by running the real test paths plus the full `commands::` (65) and workspace (524) suites. This is the recurring DevFlow false-green trap — always assert on a non-zero `N passed`, never trust a bare filtered `ok`.
**Source:** 21-03-SUMMARY.md

### The sandbox's global `tag.gpgsign true` breaks lightweight `git tag` in fixtures
With `tag.gpgsign true` set globally, a lightweight `git tag <name>` becomes a signed-annotated-tag attempt that fails non-interactively with `fatal: no tag message?`.

**Context:** Git-tag fixtures must add `git config tag.gpgsign false` — an idiom already present at `git.rs:1045`, `agent_result.rs:1150`, `version.rs:580`. Not a new pattern, just one this plan's fixtures hadn't yet needed.
**Source:** 21-03-SUMMARY.md

### Isolate a git-failure test by deleting the root TREE object, not the COMMIT
To force `git diff --name-only <embedded> HEAD` to fail while keeping `git merge-base --is-ancestor` succeeding, delete only the embedded commit's root tree object: ancestry is pure commit-graph traversal (still works) while diff needs the tree (fails).

**Context:** This surgically proves the new ancestry-range diff fails toward `Stale` without also breaking the ancestry check that gates it.
**Source:** 21-01-SUMMARY.md

### A "reuse verbatim" plan instruction and DRY pull in opposite directions
21-02's plan told the executor to copy `gate_respond`'s stage-resolution block into `gate_show` verbatim (surgical diff); the code reviewer then flagged that same copy-paste (WR-01) because its doc comment claims the two "can never drift."

**Context:** Both are defensible — the surgical diff kept the plan honest, and the executor's own Decisions note pre-registered "a third caller would be the trigger to extract a shared helper." The lesson: a doc comment asserting a guarantee the code doesn't enforce is the real defect, not the duplication itself. Deferred to backlog 999.30.
**Source:** 21-02-SUMMARY.md, 21-REVIEW.md

---

## Patterns

### Content-aware guard narrowing via an existing predicate, failing toward the safe state
Narrow a guard's false positives by filtering a `git diff` file list through an already-trusted predicate, and fail toward the *conservative* verdict (here `Stale`) on any git error — reuse the predicate, don't fork it.

**When to use:** Retiring false positives in a blocking guard without weakening its real-change detection (mixed docs+source ranges must still block).
**Source:** 21-01-SUMMARY.md (D-07)

### Detection-only reconciliation as a new Check in an existing diagnostic
Add drift detection as a new finding inside an existing `doctor`/diagnostic path (shared human + `--json` reporter), with a numeric cutoff to suppress known legacy noise — never auto-correct the source it reconciles.

**When to use:** Surfacing state/narrative drift (ROADMAP/STATE vs git tags) where auto-editing would be irreversible and could mask the drift.
**Source:** 21-03-SUMMARY.md (D-04, D-05)

### Best-effort observability with RAII cleanup that can't change host behavior
Record a lightweight, path-free slot; write it best-effort (warn-and-continue); clear it via an RAII guard (`Drop`) bound before the work starts so it clears on every exit path (success + all errors); route nothing through the host's state machine.

**When to use:** Making a process observable while guaranteeing zero change to that process's own execution/integration behavior.
**Source:** 21-04-SUMMARY.md (D-06)

### Update-the-fixture (don't loosen the assertion) when your own change breaks an exact-match test
When an intended behavior change breaks a pre-existing exact-match test, narrow the *old* fixture so it still isolates the original behavior (e.g. set `retry_after=""`) and add dedicated new tests for the new behavior — rather than loosening the old assertion to accommodate both.

**When to use:** A real, intended change collides with a strict test that was correct for its original intent.
**Source:** 21-02-SUMMARY.md

---

## Surprises

### The plan's own verify command tested nothing
21-03's literal `<verify>` command matched 0 tests (nested-module namespacing) yet its success grep passed — a verify gate that would have reported green while exercising no assertions at all.

**Impact:** Would have shipped an unverified doctor check on a naive run; caught only because the executor cross-checked against the real test paths and full-suite counts. Flagged for a future revisit of `doctor`'s test-module layout.
**Source:** 21-03-SUMMARY.md

### The narrowing exposed fixtures that silently relied on the old over-broad behavior
Two pre-existing 21d staleness fixtures asserted `Stale` using non-build files (`b.txt`, `trunk2.txt`); once the ancestry arm became content-aware they had to be retargeted to `.rs` files to keep their `Stale` intent — the fixtures had been passing *because* the guard over-blocked, not because they tested a build-relevant change.

**Impact:** A reminder that tightening a guard can invalidate tests that were green for the wrong reason; the fix added an explicit mixed docs+source range test to lock the real intent.
**Source:** 21-01-SUMMARY.md

### The only clippy fix of the phase was a hidden O(n) iterator walk
21-02's `latest_stage_launched_ts` used `.filter_map(...).last()` on a `DoubleEndedIterator`, which clippy's `double_ended_iterator_last` flagged as walking the whole iterator instead of `.next_back()`.

**Impact:** Trivial mechanical fix (identical semantics), but a reminder that `.last()` on a double-ended iterator is a silent full traversal — relevant given IN-01 already flagged a per-phase rescan in the same function.
**Source:** 21-02-SUMMARY.md
