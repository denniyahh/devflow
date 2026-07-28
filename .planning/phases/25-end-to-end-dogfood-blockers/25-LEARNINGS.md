---
phase: 25
phase_name: "end-to-end-dogfood-blockers"
project: "DevFlow"
generated: "2026-07-28"
counts:
  decisions: 10
  lessons: 10
  patterns: 7
  surprises: 7
missing_artifacts: []
extraction_notes: >
  All 19 PLAN.md and 18 SUMMARY.md files, 25-UAT.md, and 25-VERIFICATION.md were available.
  First pass read 25-VERIFICATION.md at its COMMITTED state (git HEAD, the round-4
  re-verification block) rather than from the working tree, because a round-5 gsd-verifier run
  was rewriting that file concurrently and a working-tree read risked a torn file. That verifier
  has since landed with status `passed` (10/10), and this file was AMENDED to incorporate its
  findings — see the final Lesson and final Surprise, both sourced to the round-5 verification
  block. No earlier item was altered by the amendment.
---

# Phase 25 Learnings: end-to-end-dogfood-blockers

Phase 25 ran to 19 plans across five gap-closure rounds — an unusually long tail for a phase
whose production scope was four blockers. The most transferable material here is not the fixes
themselves but the repeated pattern of *incomplete enumeration*, and the review/verify machinery
that eventually caught it.

## Decisions

### Derive version from the highest *reachable* semver tag, not raw tag count
`compute_version`'s three inputs were replaced with a baseline from the highest semver tag
reachable from HEAD plus conventional-commit classification of commits since that baseline.
Unreachable baselines refuse outright rather than guessing.

**Rationale:** The prior algorithm computed `~1.11.359` against a real `1.8.1`. Tag *count* is not
a version.
**Source:** 25-01-SUMMARY.md

### Empty-string sentinel for "no baseline tag" in `classify_range_bump`
`range_start = ""` means "classify all history reachable from HEAD" rather than adding a second
function signature for the no-tag case.

**Rationale:** Lets the "no tag at all → baseline 0.0.0" behaviour fall out of the same
classification path every other case uses, instead of a special-cased branch.
**Source:** 25-01-SUMMARY.md

### D-18a — base ref repair is fetch, compare, fast-forward when safe, else refuse loudly
Operator-adjudicated before execution began, so the plan carried no `checkpoint:decision`.

**Rationale:** `Behind` already establishes strict ancestry, so a fast-forward is lossless; any
other state must refuse rather than guess.
**Source:** 25-05-SUMMARY.md

### `git update-ref <ref> <new> <old>` — the only ref-write primitive that is a compare-and-swap
`git branch -f` was explicitly rejected, and the repository-wide checked-out predicate built by
hand rather than delegated to `branch -f`'s own refusal.

**Rationale:** `branch -f` has no `<oldvalue>` parameter. Using it would have closed the
checked-out defect while leaving the lost-update defect open.
**Source:** 25-14-SUMMARY.md

### The scan-to-swap window is accepted as a documented residual, not eliminated
A worktree can still check out `<base>` between the repository-wide scan and the compare-and-swap.

**Rationale:** Bounded to two subprocess invocations; the CAS still prevents a lost update inside
it; the only alternative that closes it (`branch -f`) reopens a higher-severity defect. Recorded
in the doc comment and in `must_haves.truths` as a backstop rather than hidden.
**Source:** 25-14-SUMMARY.md

### The stray-reap safety set is machine-wide and `--root` unions into it, never narrows it
Explicitly refused the `missing` list's own parenthetical suggestion to scope the reachable-pid
set to `--root`.

**Rationale:** Narrowing the *safety* set while the sweep stays machine-wide protects less and
reaps the same — strictly more dangerous than the pre-fix code. Stated in doc comments, in a
terminal warning, and in CLI help so a reader of any one of them learns it.
**Source:** 25-15-SUMMARY.md

### `lock::holder_identity` over `lock::holder` on `doctor`'s path
The review's own sketch called `lock::holder`, which deletes an empty lock file.

**Rationale:** `doctor` is contractually read-only; the sketch would have made it mutate. Verified
against `lock.rs:177-210` before writing the fix rather than trusting the sketch.
**Source:** 25-15-SUMMARY.md

### `ReapMonitorOnDrop` captures the pid by value, not as `&'a State`
Guard holds `Option<u32>`.

**Rationale:** Avoids tying the guard's lifetime to the `State` binding's scope. (Note: the
originally-recorded rationale — that a `&State` borrow would conflict with the launch's
`&mut state` — was corrected by the plan checker as technically false, since the guard binds after
that borrow ends. The by-value design is right; the first stated reason was not.)
**Source:** 25-17-SUMMARY.md, corrected per plan-checker warning on 25-17-PLAN.md

### Plan 25-10 was superseded, not re-run
Its objective's "structurally removed" premise was falsified by a 2/2 reproduction, and its trial
design targeted tests that D-13 had already retargeted away from the real risk.

**Rationale:** Re-running it unchanged would have produced evidence about the wrong tests. No
`25-10-SUMMARY.md` will ever exist; this is by design, not an omission.
**Source:** 25-13-SUMMARY.md, 25-10-PLAN.md frontmatter

### Never drive the real, unscoped destructive reap path in any test
Every destructive-path test acts only on a pid the test itself spawned, filtered before any signal.

**Rationale:** A live census taken before writing the tests found ~18 real active monitor/advance
processes belonging to concurrent sibling worktree agents. An unscoped non-dry-run reap would have
signalled all of them.
**Source:** 25-07-SUMMARY.md

---

## Lessons

### `cargo test --lib` fails on this binary-only CLI crate, and a filter matching nothing exits 0
`devflow` has only `main.rs`. `cargo test -p devflow --lib` errors with "no library targets found";
the correct invocation is `--bin devflow`. Separately, a test filter that matches nothing still
exits 0.

**Context:** These two traps combined produced at least three false greens across this phase — one
in 25-03's plan-specified verify command, and two during orchestration when a probe reported
success while running no tests at all. The durable fix is to assert on the printed `N passed` line,
never on the exit code.
**Source:** 25-03-SUMMARY.md; orchestrator probes during rounds 4–5

### `rg -c 'fn some_name'` matches substrings, so test names collide with acceptance-criteria greps
Naming new tests with the production function's full name inflated an expected count of 1 to 10.

**Context:** Fixed by shortening test prefixes (`base_ref_currency_*` → `currency_*`), which became
a repeated convention. A related instance: `rg -A60` searches *after* the matched line, so doc
comments preceding a `fn` are never captured by an acceptance grep anchored on it.
**Source:** 25-05-SUMMARY.md, 25-06-SUMMARY.md, 25-07-SUMMARY.md

### `SIGKILL` delivery is asynchronous relative to the syscall returning
An immediate `agent_running` check right after `libc::kill(pid, SIGKILL)` intermittently reported a
just-killed process as alive.

**Context:** Fixed with a second bounded poll (1s) after escalation, mirroring the pre-escalation
`SIGTERM` wait. "Uncatchable" does not mean "synchronous."
**Source:** 25-02-SUMMARY.md

### `process_age` genuinely reads `Duration::ZERO` within the first USER_HZ tick
Measured deterministically 5/5 in isolation, not a flake — the 10ms granularity its own doc comment
already caveats, applied to a process asking about itself.

**Context:** The test was changed to sleep 20ms first so it asserts "age advances," which is what
its stated intent actually required.
**Source:** 25-12-SUMMARY.md

### A leaked-process delta of zero is not evidence that no leak exists
Six isolated runs, whole-workspace runs on both unfixed and fixed trees, and an independent
orchestrator probe all measured delta 0 — yet the leak was real.

**Context:** The spawned wrapper self-exits in under a millisecond (the stubbed agent returns
instantly, and the wrapper's trailing `devflow advance` resolves `current_exe()` to the test binary,
which rejects the argument shape and exits). An after-the-fact process count structurally cannot
observe it. This misled both an executor and the orchestrator before source-tracing settled it.
**Source:** 25-16-SUMMARY.md, .planning/WINDOWS.md item 2, 25-VERIFICATION.md

### Enumerating by call site finds fewer sites than enumerating by reachability
The single highest-cost lesson of this phase, and it recurred three times.

**Context:** 25-16 searched for direct `launch_stage`/`launch_stage_inner` calls and missed two
tests that reach the spawn through `run_preflight`'s `Advance`/`LoopBack` arms. 25-18 extended the
search to those arms, found a third site — then asserted in its SUMMARY that no fourth path existed.
That statement was false: `resume()` is a fourth wrapper, and its test leaked. Each round's
enumeration was correct for the pattern it searched and wrong about its own completeness.
**Source:** 25-16-SUMMARY.md, 25-18-SUMMARY.md, 25-VERIFICATION.md

### A `Drop` that panics during an in-flight unwind calls `abort()` — and `eprintln!` can panic
The RAII guard's "safe" fallback branch used `eprintln!`, which routes through `std::io::_eprint`
and panics on a failed stderr write.

**Context:** The failure mode would have been: assertion fails → guard drops → wrapper survived →
second panic → whole test binary aborts, reporting nothing about the other ~695 tests. Strictly
worse than the leak it replaced, and triggered by exactly the scenario the guard existed for. Fixed
with `let _ = writeln!(std::io::stderr(), ...)`.
**Source:** 25-REVIEW.md CR-01, commit c2f5080

### `scripts/check-in-container.sh` cannot run cleanly from inside a linked git worktree
The script mounts only `git rev-parse --show-toplevel`; a linked worktree's `.git` is a *file*
pointing at a gitdir outside that mount, so any test resolving the real repository fails with
"fatal: not a git repository".

**Context:** Worked around with an equivalent docker invocation and the gap documented explicitly,
rather than silently claiming the literal acceptance criterion was met.
**Source:** 25-11-SUMMARY.md

### A plan-specified test fixture can be structurally impossible, not merely wrong
Two independent instances in one phase.

**Context:** 25-06's "pre-create the tag `compute_version` will produce, to force a collision" can
never collide — the new algorithm always bumps strictly past the highest reachable tag, so any
pre-created tag simply becomes the next baseline. 25-09's fixture produced a `git rev-list
--ancestry-path` ordering that made the test FAIL pre-fix when the plan required it to PASS. Both
were caught by executors following the plan's own contingency instructions rather than forcing the
literal steps.
**Source:** 25-06-SUMMARY.md, 25-09-SUMMARY.md

### Enumerate by the scarce prerequisite, not by the call graph — the call-graph list was itself incomplete
Round 5 closed the leak hunt, but only because an *orthogonal* method was used to confirm it.

**Context:** The orchestrator's sweep enumerated eight functions that reach `launch_stage` and
cross-referenced them against tests. The verifier instead grepped for the two helpers that place a
stub agent binary on `PATH` (`stub_agent_binary`, `agent_free_dir_with_agent_stub`) and classified
every caller — reaching the same 7 sites by a different route. It then found a **ninth** wrapper
(`commands::start`, `commands.rs:113/302`) that the eight-entry list had missed. That ninth entry
happened not to expand the leak count, because no in-process test in `commands.rs` uses either stub
helper — but the call-graph enumeration was incomplete *again*, on the very pass that was supposed
to close it. A spawn requires a stubbed binary on `PATH`; that prerequisite is scarce, greppable,
and cannot be reached around, whereas the set of wrappers reaching a spawn is open-ended. Enumerate
on the scarce prerequisite.
**Source:** 25-VERIFICATION.md (round-5 block)

---

## Patterns

### RAII `Drop` guard for test teardown that must survive assertion panics
A trailing cleanup call runs only on the success path — the path where cleanup matters least. Bind
a `Drop` guard *before* the panicking assertions instead.

**When to use:** Any test that acquires a real external resource (process, file lock, remote
handle) and then asserts on it. Two failure modes are silent and must both be checked: bound too
early it captures nothing; bound too late it is the original bug.
**Source:** 25-17-SUMMARY.md, 25-REVIEW.md WR-06

### Matched positive + control test pair to prove a fix is non-vacuous
Ship the new mechanism and the old one side by side, differing in exactly one thing, with opposite
expected outcomes.

**When to use:** Whenever a test could pass for the wrong reason. The criterion for
non-vacuousness is that swapping the two mechanisms would flip *both* outcomes — which is
checkable, unlike "does this test look right."
**Source:** 25-17-SUMMARY.md, verified independently in 25-VERIFICATION.md

### Injectable zero-I/O core plus a thin live caller
`reap_stray_candidates` was split out from live discovery so escalation, identity-refusal and
dry-run logic are unit-tested against real test-owned processes without ever routing through an
unscoped live census.

**When to use:** When a code path is genuinely dangerous to exercise end-to-end but its logic still
needs real behavioural coverage. Mirrors the existing
`reconcile_planning_docs`/`collect_planning_doc_findings` pair.
**Source:** 25-07-SUMMARY.md

### One shared filter interposed between a data source and every consumer surface
`doctor` and `gate sweep --reap-strays` both route through a single
`unreachable_stray_candidates` composition.

**When to use:** When two surfaces make claims about the same underlying data and must not drift.
The single composition is what makes "`doctor`'s claim and the sweep's action cannot disagree" a
structural property rather than a convention.
**Source:** 25-15-SUMMARY.md

### Compare-and-swap ref write behind a repository-wide predicate
Pair a conditional write (`update-ref` with `<oldvalue>`) with a precondition evaluated across
*every* worktree via `git worktree list --porcelain`, not just the current one.

**When to use:** Any automated write to a shared git ref. `git update-ref` carries no checked-out
protection of its own, so the predicate is the only guard — and a `project_root`-scoped probe
cannot see linked worktrees.
**Source:** 25-14-SUMMARY.md

### Convert an inferred premise into a runtime-verified assertion
Rather than trusting source-reading that a test reaches a spawn, assert
`state.monitor_pid.is_some()` so a silent no-op fails loudly.

**When to use:** When a fix's correctness depends on a premise established only by reading code. If
the premise ever stops holding, the suite says so instead of quietly doing nothing.
**Source:** 25-18-SUMMARY.md, 25-19-SUMMARY.md

### Retain the RED commit as evidence, don't squash it
A cross-AI reviewer suggested not committing failing tests; the plan's disposition rejected it.

**When to use:** This repository's established practice, and load-bearing when a truth previously
shipped on an unverified premise — the RED commit is the proof the test discriminates.
**Source:** 25-11-SUMMARY.md, 25-08-SUMMARY.md

---

## Surprises

### The mandated re-derivation found a leak site the plan hadn't declared
25-18's verification step 6 required re-enumerating reachability rather than confirming the two
declared sites. It surfaced a third live leak, which the executor fixed in the same plan and
recorded as its own ledger entry.

**Impact:** Validated the methodology change over the outcome — a step written to check the *method*
found what the method's predecessor had missed.
**Source:** 25-18-SUMMARY.md

### The verifier then falsified that same plan's completeness claim
25-18-SUMMARY.md stated no path to `spawn_monitor` existed beyond the three functions its grep
named. The verifier found `resume()` — a fourth wrapper — and confirmed a real spawn empirically
(observed monitor pid under `--nocapture`).

**Impact:** Held the phase at `human_needed` and triggered a fifth gap-closure round. The lesson is
narrower than "the plan was wrong": every round's enumeration was correct within its own search
pattern and wrong about its own completeness, three times running.
**Source:** 25-VERIFICATION.md

### The sixth site needed a *different* fix shape than the five before it
`resume(root, phase)` takes no state — it loads its own from disk and never writes the spawned pid
back into the caller's local binding. The obvious form, correct at all five prior sites, would have
captured `None` and reaped nothing while passing every assertion.

**Impact:** The natural instinct (copy the sibling sites) produced a silently-broken fix. Closing it
required reading the pid back from disk plus an assertion that the spawn was real.
**Source:** 25-19-SUMMARY.md

### A pre-created tag can never collide with `compute_version`'s output
The plan's fixture instruction assumed a fixed point existed. It does not: any tag reachable from
HEAD becomes the new baseline, and the algorithm always bumps strictly past it.

**Impact:** A general structural property (the function's image is disjoint from any already-
reachable tag), so no alternative tag value would have worked either. The fixture was rebuilt
around a D-10 refusal via an orphan-commit tag instead.
**Source:** 25-06-SUMMARY.md

### `devflow doctor` reported ~18 live sibling-agent processes as "stray"
A smoke test after implementation correctly — and alarmingly — listed every concurrent
worktree agent's monitor as an orphan.

**Impact:** Directly motivated the CR-01 registry-reachability filter, and stands as the concrete
demonstration of why the documented repair (`gate sweep --reap-strays`) was unsafe to run at all
before that filter landed.
**Source:** 25-07-SUMMARY.md

### The test suite was manufacturing the exact defect class the phase existed to eliminate
Six tests spawned real detached monitor wrappers and let `TempDir` unlink the project root out from
under them — 999.44's reproduction shape, produced by the phase's own suite.

**Impact:** Two independent counts found 21 and 22 live wrappers on the development machine. Closing
this consumed rounds 4 and 5 (plans 25-16, 25-17, 25-18, 25-19) — more plans than several of the
phase's production units.
**Source:** 25-16-SUMMARY.md, 25-REVIEW.md WR-03/WR-05/WR-06, .planning/WINDOWS.md

### The enumeration closed only when a third method agreed with the second
Four rounds asserted completeness; three were wrong.

**Impact:** 25-16 (call sites) missed two. 25-18 (call sites + `run_preflight` arms) missed one and
asserted none remained. The orchestrator's eight-entry call-graph sweep found the last leak but
itself missed a ninth wrapper. Only the verifier's stub-helper grep — an independent discriminator,
run by a party that had not authored any of the fixes — produced a completeness claim that survived
scrutiny, and it did so by agreeing with the previous method's *site list* while disagreeing with
its *wrapper list*. The durable practice is not "enumerate more carefully" but "confirm an
enumeration with a method that could fail differently, run by someone who did not write the fix."
**Source:** 25-VERIFICATION.md (round-5 block), 25-16-SUMMARY.md, 25-18-SUMMARY.md
