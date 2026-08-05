---
phase: 33
phase_name: "Loop-Back Correctness for Multi-Wave Validate→Code Cycles (999.65 + 999.66)"
project: "DevFlow"
generated: "2026-08-05"
counts:
  decisions: 6
  lessons: 6
  patterns: 5
  surprises: 5
missing_artifacts:
  - "33-UAT.md"
---

# Phase 33 Learnings: Loop-Back Correctness for Multi-Wave Validate→Code Cycles

## Decisions

### D-01 — File existence of `{N}-VERIFICATION.md` alone is the mid-arc discriminator
No `{N}-VERIFICATION.md` → issue plain `/gsd-execute-phase {N}`; file exists → issue `--gaps-only`, unchanged. No parsing of VERIFICATION.md's internal finding-type breakdown.

**Rationale:** matches the phase's own success criteria literally, and needs no parser for a document whose internal shape could drift. Reversible — a pure decision-function change with no persisted-state shape involved.
**Source:** 33-CONTEXT.md

### D-02 — Scope the fix to the three call sites inside `handle_validate_outcome`
The fourth `FixType::GapsOnly` call site, inside `handle_ship_outcome`, is explicitly out of scope.

**Rationale:** by the time Ship runs, the phase is by definition not mid-arc, so `--gaps-only` is already correct there. Narrowing now does not block widening later if a real Ship-loop-back defect turns up.
**Source:** 33-CONTEXT.md

### D-03 — The forward-progress signal was deferred to the phase researcher, not locked at discuss time
Two candidates were surfaced during discussion and neither was picked outright.

**Rationale:** the choice depended on evidence the discussion did not have. Deferring a genuinely open technical question to the stage equipped to answer it beats guessing at discuss time and then defending the guess.
**Source:** 33-CONTEXT.md

### D-04 — The two adjacent root-consuming reads are deliberately on *different* roots
`phase_verification_exists` follows the agent's cwd (the phase worktree); `phase_commit_count` keeps reading the main checkout. Recorded as a hard prohibition against "fixing" the asymmetry.

**Rationale:** `.planning/` is tracked content, so an in-flight phase's artifacts exist only on its own branch inside the worktree — but git refs and the object database are *shared* across a repository's worktrees, so a worktree commit is already visible from the main checkout. Retargeting the commit count would fix nothing and would break the 999.66 wiring.
**Source:** 33-05-PLAN.md (prohibitions), 33-VERIFICATION.md

### D-05 — Use the plain worktree-fallback form, not the `.exists()`-filtered variant
`hook_context_root`'s filtered variant sits ~180 lines away in the same file and was explicitly rejected.

**Rationale:** the two answer different questions. `hook_context_root` picks a directory to **write** into, where a vanished worktree must degrade to somewhere writable. This picks a root to **probe** for evidence, where a vanished worktree means the evidence is gone with it — silently falling back to the main checkout would resurrect a stale or other-branch artifact as if it were this phase's. All four in-repo precedents use the plain form.
**Source:** 33-05-PLAN.md (prohibitions)

### D-06 — Route review findings to backlog rather than widening the phase
Six entries filed (999.76–999.81); only findings against *this phase's own new code* were fixed in-phase, as 33-06.

**Rationale:** the pre-existing defects predate the phase's merge-base and carry their own scope (999.76 needs a test rewritten, not a one-line root swap). Fixing them here would half-do planned work. The in-phase five were fixed because three of them were the *same failure class* the phase existed to eliminate.
**Source:** 33-VERIFICATION.md (deferred), 33-REVIEW.md

---

## Lessons

### A green suite can certify an inverted decision
Every one of the phase's eight original loop-back tests left `state.worktree_path` at its `State::new()` default of `None` — which makes `project_root` and "the root the agent actually wrote to" identical by construction, the one condition under which the defect is invisible.

**Context:** the tests were correct, comprehensive, and passing, and the code they covered dispatched the wrong command in DevFlow's default operating shape. Coverage of a decision is not coverage of the decision's *inputs*.
**Source:** 33-05-PLAN.md (objective)

### The `cargo test --exact` false-green trap recurs even inside a plan that warns about it
Bare test names match nothing under `--exact` and still exit 0. Observed live at `0 passed; 270 filtered out`, exit 0 — inside a plan whose own frontmatter documented the trap.

**Context:** the plan's verify loop grepped for a positive `1 passed` with a non-zero `filtered out`, so it failed safe rather than passing falsely. Writing the warning down did not prevent the mistake; the *assertion shape* did.
**Source:** 33-05-SUMMARY.md

### `cargo test -p devflow --lib` hard-errors — the CLI package is binary-only
`error: no library targets found in package 'devflow'`, exit 101. Use `--bins`.

**Context:** recurred in three separate plans (33-03, 33-05, 33-06) despite being documented in both CLAUDE.md and the `ai-change-acceptance` skill. It also reached the phase's own VALIDATION.md, whose every quick-run command was unrunnable until the nyquist audit corrected them.
**Source:** 33-03-SUMMARY.md, 33-06-SUMMARY.md, 33-VALIDATION.md

### A test that seeds `consecutive_failures` directly can launch a real agent
Seeding the counter bypasses the mechanism that would have recorded the forward-progress baseline, leaving `None`. That takes the reset arm, drops off the gated path, and falls through to `launch_stage` → a real `claude` process with the developer's inherited credentials.

**Context:** found when three pre-existing tests broke under 33-03's rewiring, and again when 33-01's own new tests omitted PATH neutralization on first draft. The failure is silent and expensive rather than loud.
**Source:** 33-01-SUMMARY.md, 33-03-SUMMARY.md

### Fixing the instance leaves the class open
33-05 corrected the caller and left `phase_verification_exists` still naming its parameter `project_root` — a public signature that is the contract a future caller reads first, and the exact mislabeling that invited the original defect.

**Context:** the review caught it as WR-07/WR-08 and 33-06 closed it. Worth generalising: after fixing a defect caused by a misleading name or a duplicated expression, check whether the *invitation* survives at the callee or in the remaining copies.
**Source:** 33-REVIEW.md, 33-06-PLAN.md

### An external reviewer's clean verdict can rest on an incomplete case split
Gemini 3.1 Pro rated the 999.66 counter logic clean, having analysed `current = 0` against `previous = Some(0)` — two *consecutive* git failures. It never examined failure-then-success, which is the actual defect.

**Context:** the verdict was not wrong about what it examined. Peer review adds most value when its case coverage is checked, not just its conclusion. Recorded on 999.77 so the next reader does not treat that AGREE as clearing the finding.
**Source:** 33-REVIEW.md (WR-03), external peer review

---

## Patterns

### Negative control on every measurement
Pair each check with a case that must produce the *opposite* result. If both agree, the measurement is broken rather than the subject.

**When to use:** any verification claim. Concretely used here for: a fabricated test name proving `--exact` runs matched real tests; `git ls-tree -r` on `develop` vs `HEAD` proving `.planning/` artifacts are branch-local (the non-recursive form returns 0 for *every* ref and proves nothing); a positive control proving an `rg -c` zero was real and not a broken pattern.
**Source:** 33-05-SUMMARY.md, 33-VALIDATION.md, 33-VERIFICATION.md

### One test per discriminating scenario
Scenario B — the artifact present in the main checkout only — is the workspace's sole test that fails a "probe both roots and OR them" implementation. Packed into the same `#[test]` as scenario A, an A failure aborted before B ever asserted.

**When to use:** whenever one case is the only thing standing between the suite and a specific plausible wrong implementation. Give it its own test and a name that says so.
**Source:** 33-REVIEW.md (IN-06), 33-06-SUMMARY.md

### RAII guard over trailing-statement cleanup
`NeutralPath`, modeled on the file's existing `ReapMonitorOnDrop`, restores `PATH` in `Drop`.

**When to use:** any test region that mutates process-global state. Rust abandons remaining statements the instant a panic unwinds, so a trailing restore is skipped exactly when it matters — and here that stranded `PATH` at a tempdir the unwind then deleted, plus poisoned `ENV_MUTEX`, turning one legible failure into a ~15-failure cascade. As the existing guard's own doc comment puts it: it is the language's `Drop` guarantee, not a call-ordering convention, that makes cleanup unconditional.
**Source:** 33-REVIEW.md (WR-05), 33-06-SUMMARY.md

### Bind once, consume many — an owned value sidesteps the borrow that forced duplication
The evidence-root resolution was triplicated because a *borrowed* binding could not outlive the later `&mut state` calls. An owned `PathBuf` holds no borrow and hoists cleanly.

**When to use:** when a repeated expression is justified by "the borrow checker forces it." Check whether taking ownership dissolves the constraint — three copies make correctness a matter of attention; one binding makes it structural.
**Source:** 33-REVIEW.md (WR-08), 33-06-SUMMARY.md

### Transcribe the idiom, don't invent it
The worktree-fallback expression was taken from four existing in-repo sites, and the parameter name `evidence_root` from an existing function that already takes both roots as distinctly-named parameters.

**When to use:** by default. The plan recorded where each borrowed element came from, which made the review's job checking *fidelity to precedent* rather than adjudicating a novel design.
**Source:** 33-05-PLAN.md

---

## Surprises

### Tests spawned a real `claude` CLI during `cargo test`
Not a hypothetical: it happened on the first run of three new tests, and separately broke three pre-existing tests under rewiring.

**Impact:** an unattended agent launched from a unit test runs with the developer's inherited credentials and burns real quota. Drove an entire gap-closure plan (33-04) and a follow-up backlog entry (999.80).
**Source:** 33-01-SUMMARY.md, 33-03-SUMMARY.md

### The phase needed three gap-closure plans past its original three
33-04 (bypassed regression tests), 33-05 (the evidence-root defect), 33-06 (five findings against the phase's own new code).

**Impact:** doubled the plan count. Every one came from a review or verification pass rather than from execution failure — the plans themselves all self-reported success.
**Source:** 33-VERIFICATION.md, 33-REVIEW.md

### One `index.lock` collision cascaded into ~15 unrelated failures
A pre-existing concurrency flake in `concurrent_ship_advances_finish_both_phases_independently` poisoned `ENV_MUTEX`, and every subsequent `ENV_MUTEX.lock().unwrap()` panicked with `PoisonError`.

**Impact:** turned a single legible failure into a suite-wide noise event. Observed 1 failure in 6 runs — non-determinism established, but 5 passes is a weak bound and the pre-change commit was never re-run under equivalent load, so "this change did not worsen it" remains unproven rather than established.
**Source:** 33-06-SUMMARY.md

### Two unexplained `test result:` lines in the bin log turned out to be benign
Investigated rather than waved off: two pre-existing tests re-invoke the test binary as a subprocess, so their child output lands in the parent's log.

**Impact:** none, but the investigation is the point — an unexplained line in a test log is either a finding or a fact you can name, and "probably fine" is neither.
**Source:** 33-05-SUMMARY.md

### The defect was in DevFlow's *default* operating shape, not an edge case
`FixType::GapsOnly` was unreachable on the Validate path whenever a worktree was configured — which is how DevFlow normally runs. ROADMAP criterion 1 landed on the right output by accident; criterion 2 landed on the wrong one.

**Impact:** reframed the finding from "an edge case we missed" to "the mainline path was wrong and the tests could not see it." Also produced the phase's most useful generalisation: the same wrong-root class was then found one file over in `evaluate_layer0`, filed as 999.76.
**Source:** 33-05-PLAN.md, 33-REVIEW.md
