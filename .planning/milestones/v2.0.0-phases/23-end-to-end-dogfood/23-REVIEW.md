---
phase: 23-end-to-end-dogfood
reviewed: 2026-07-27T00:52:03Z
depth: standard
files_reviewed: 29
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/config_parse.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-cli/tests/gate_sweep_e2e.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-cli/tests/start_reachability_e2e.rs
  - crates/devflow-cli/tests/stop_e2e.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/events.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/registry.rs
  - crates/devflow-core/src/ship_evidence.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/tests/devflow_dir_gitignore.rs
  - scripts/scratch-dogfood-repo.sh
findings:
  critical: 0
  warning: 2
  info: 1
  total: 3
status: issues_found
---

# Phase 23: Code Review Report

**Reviewed:** 2026-07-27T00:52:03Z
**Depth:** standard (priority focus: `crates/devflow-cli/src/staleness.rs`, commit `c2e947a` / plan 23-16, given disproportionate attention per reviewer instruction)
**Files Reviewed:** 29 (listed above)
**Status:** issues_found

## Summary

This phase's newest change — `staleness.rs`'s `embedded_commit_is_stale` reverse-probe divergent arm — was the primary target of this pass and received line-by-line tracing plus adversarial scenario construction, not just a read-through. The rest of the file set (already covered by prior cross-AI plan review in `23-REVIEWS.md`) was scanned at standard depth for regressions and new defects introduced since that review; the four items that review flagged as BLOCKER/HIGH (the `workflow_finished`/`workflow_shipped` oracle, the 23-10 checkpoint ordering, masked verification chains, and registry lost-update risk) were spot-checked against current source and confirmed fixed (`ship_evidence.rs`'s `workflow_shipped` predicate at exactly one emission site with regression tests for both the shipped and `--until`-stopped paths; `registry.rs`'s per-file storage shape with a concurrent-registration test proving no lost update; `pipeline_outcomes.rs`'s `auto_response` scoped to exactly one call site with a negative regression test in `pipeline_gate.rs` proving the reopened finalization-retry gate never auto-approves) — not re-litigated below.

**On the priority question (does the divergent-arm content check over-permit a genuinely stale divergent build to Fresh?):** verified sound. `git diff --name-only <embedded> HEAD` is a plain two-commit tree-to-tree diff (space-separated commit args, not `A...B`), so it answers exactly the question that matters regardless of history shape: "does HEAD's current build-relevant file content differ from what this binary was literally compiled from?" A build-affecting change that exists only on the embedded side, or only on HEAD's side, always produces a path-level content difference and is caught (proven directly by `divergent_lineage_with_source_change_is_stale`). A change that nets to byte-identical content on both tips (e.g. made-then-reverted on one branch before its tip, or independently converged) is not staleness by any definition that matters here — a rebuild from HEAD would compile the same build-relevant bytes the running binary already reflects. I deliberately tried to construct a false-Fresh case (independent revert, divergent-but-converged Cargo.lock, orphaned file additions on either side) and could not — every construction either reduces to genuine content equality (correctly Fresh) or a genuine content difference (correctly caught). The fail-closed contract (`ancestry_range_affects_build`'s `.unwrap_or(true)`) and `affects_compiled_binary`'s single definition are both correctly shared, unforked, between the strict-ancestor arm (line 59) and the new divergent arm (line 85) — same function, same call shape. The stated-untouched invariants (empty-commit → `Indeterminate`, `Ahead` → warn-only never-blocks, the dirty-tree arm of `combined_staleness`, `is_self_dogfood_workspace`) are confirmed untouched by this commit's diff. The three new tests (`divergent_lineage_docs_only_range_is_fresh`, `divergent_lineage_with_source_change_is_stale`, `enforce_build_staleness_does_not_block_self_dogfood_on_divergent_docs_only_lineage`) construct genuine mutual non-ancestry — both `merge-base --is-ancestor` directions are runtime-asserted to exit 1 immediately before the real assertion, so a broken fixture (e.g. an accidental strict-ancestor shape) would fail loudly on the precondition rather than silently proving the wrong thing.

Two WARNING-level and one INFO-level finding below — none blocking.

## Warnings

### WR-01: Divergent-arm fail-closed path has no test exercising it at its own call site

**File:** `crates/devflow-cli/src/staleness.rs:85` (call site), regression test at `crates/devflow-cli/src/staleness.rs:1253` (`git_error_range_fails_toward_stale`)
**Issue:** The fail-closed contract ("a git failure in `ancestry_range_affects_build` must return `true`, never a false `Fresh`") is proven only through the strict-ancestor call site (line 59's arm, exercised by `git_error_range_fails_toward_stale`, which constructs a strict-ancestor fixture and deletes the embedded commit's tree object). The new reverse-probe/divergent call site at line 85 shares the exact same function, so the current test *does* prove the underlying logic is fail-closed by construction — but no test drives a git failure through a genuinely divergent fixture and asserts `embedded_commit_is_stale` still returns `Stale` via that specific branch. If a future change forks the divergent arm's content check (exactly the kind of drift this file's own comments warn against for `affects_compiled_binary`), nothing here would catch a regression where the divergent arm's fail-closed behavior silently breaks while the strict-ancestor arm's test keeps passing green.
**Fix:** Add a divergent-history variant of `git_error_range_fails_toward_stale` — construct a genuinely mutually-non-ancestor fixture (same shape as the three 23g tests, with the same runtime precondition assertions), delete the embedded-side tip commit's tree object, and assert `embedded_commit_is_stale` still returns `Staleness::Stale` through the reverse-probe branch specifically.

### WR-02: `write_atomic`'s crash-orphaned temp files are never cleaned up

**File:** `crates/devflow-core/src/registry.rs:214-221` (`write_atomic`), `crates/devflow-core/src/registry.rs:132-154` (`prune_missing_in`), `crates/devflow-core/src/registry.rs:229-251` (`load_roots_in`)
**Issue:** `write_atomic` writes to `path.with_extension("tmp.{pid}.{n}")` then renames over the target. If the process is killed (SIGKILL, OOM, power loss) between the `fs::write` and the `fs::rename`, the temp file is left behind permanently: it does not end in `.json`, so neither `load_roots_in`'s enumeration filter (`registry.rs:238`, `name.ends_with(".json")`) nor `prune_missing_in`'s cleanup filter (`registry.rs:141`, the same check) will ever see or remove it. `register_in` is called on every stage launch (`pipeline_launch.rs:137`), so a machine that experiences even occasional hard kills of `devflow` processes accumulates an unbounded number of orphaned temp files under `~/.cache/devflow/roots/` over the life of the machine. This is out of scope as a raw performance concern, but it is a genuine unbounded-growth defect in a module whose own doc comment explicitly claims "a corrupt or truncated entry costs one entry, never the whole registry" as a design goal — the crash case doesn't cost an entry, it leaks a file forever, silently, with no operator-visible symptom until someone notices the directory size.
**Fix:** Have `prune_missing_in` also remove leftover `roots/*.tmp.*` files (any file whose name contains `.tmp.` is by construction never a live registration), e.g.:
```rust
// in prune_missing_in, alongside the existing .json branch:
if name.contains(".tmp.") {
    let _ = std::fs::remove_file(&path);
    removed += 1;
    continue;
}
```

## Info

### IN-01: `ancestry_range_affects_build`'s doc comment describes the divergent arm with strict-ancestor terminology

**File:** `crates/devflow-cli/src/staleness.rs:98-110` (doc comment), `crates/devflow-cli/src/staleness.rs:30-33` (top-of-file doc comment)
**Issue:** Both doc comments describe `ancestry_range_affects_build` as checking whether "the committed range between `embedded_commit` and ... `HEAD` touches at least one build-affecting file." That phrasing is exact for the strict-ancestor arm (where the two-dot tree diff and the accumulated diff of the commits in between are provably identical), but is looser for the new divergent arm reusing the same function: `git diff --name-only <embedded> HEAD` there is a raw two-commit tree comparison with no single linear "range" of commits behind it — two divergent branches can each introduce and later net-cancel changes to the same path, and the diff reports only the final content delta, not "every file touched by either branch's commits along the way." The underlying check is still the right one to run (see Summary — content-equivalence, not history-path, is what determines whether a rebuild would differ), but a future reader relying on the doc comment's literal "committed range" language could mistakenly assume the function enumerates every file any commit unique to either side touched, which it does not.
**Fix:** Extend the doc comment's second sentence to note the divergent case explicitly, e.g.: "...for the divergent arm this is a plain two-commit tree diff, not an accumulated commit-range diff — two divergent branches that each touch and then net-cancel the same path will not appear here, which is correct: what matters is whether the two commits' build-relevant *content* actually differs, not the history path that produced it."

---

_Reviewed: 2026-07-27T00:52:03Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
