---
status: active
milestone: v2.0.0
target_version: v1.6.0
promoted: 2026-07-21
promoted_from: 999.10, 999.11, 999.8, 999.16
source: /gsd-review-backlog promotion — scoping session 2026-07-21
---

# Phase 19: Release Integrity + `main.rs` Decomposition

**Goal:** Close the two release-integrity defects whose blast radius reaches
outside this repository, then decompose `crates/devflow-cli/src/main.rs` as a
pure-move refactor so subsequent phases stop paying the near-serial wave tax.
Adds an AI change acceptance contract on a parallel, source-conflict-free track.

**Targets v1.6.0**, not v2.0.0. Nothing here is breaking and — apart from the
PII fix — almost nothing is user-visible. The v2.0.0 milestone closes at Phase
20, which carries the operator-facing set (999.6 `--until`, 999.7 manual ship
override, 999.13 release-cut automation, likely 999.3 `gate show`). Those all
land in `main.rs` and are exactly what this phase's split makes plannable as a
single phase instead of two.

## Units

| Unit | Source | Pri | Size | Notes |
|---|---|---|---|---|
| 19a | 999.10 `.devflow/` artifact hygiene | Urgent | S | PII leak into *other people's* repos |
| 19b | 999.11 `commit_path` empty commits | High | S | a release tag can sit on an empty commit |
| 19c–19f | 999.8 split `main.rs` | High | L | pure move, zero behavioral change |
| 19g | 999.16 AI change acceptance contract | High | M | no Rust source — fully parallel track |

## Sequencing (load-bearing)

19a and 19b land **before** the split, so they are small diffs against the file
everyone already knows rather than against seven new modules. 19g has no source
overlap with anything and can run in any wave.

## Re-verification at HEAD (2026-07-21, during promotion)

Every cited claim was re-checked before promoting; all four still hold.

- `hooks.rs:184` — `git.commit_all("docs: update generated docs")`, still the
  only remaining `commit_all` caller.
- `main.rs:902` — `exe_path` emission (the backlog item said `:843`; drifted).
  Its test assertion is now at `main.rs:6879`.
- `git.rs:312` and `git.rs:336` — both `--allow-empty` sites present.
- `main.rs` is now **8,467 lines** (4,025 production + a 4,442-line test module,
  106 tests) — the 999.8 item was written at 6,239 and is materially stale. It
  is 3.4x the next largest file (`agent_result.rs`, 2,505). `ENV_MUTEX` has
  **18 `.lock()` sites / 63 total references**, not the 22 the item recorded.
  Cluster line boundaries recorded in 999.8 are stale for the same reason and
  must be re-measured at plan time.

---

# Unit 19a — `.devflow/` Artifact Hygiene (WR-01 + WR-02)

> Promoted from `999.10-devflow-artifact-hygiene`. Source: 17-REVIEW.md WR-01 +
> WR-02, triaged 2026-07-20. Severity: highest of the WR batch — affects
> downstream users' repositories, not just this one.

Two findings grouped because they compose into one threat: **WR-02 writes
personally-identifying data into a `.devflow/` artifact, and WR-01 commits that
artifact into the user's repository.** Either alone is minor; together they
publish a developer's home path and OS username into a target project's git
history. They have independent fixes and can land separately.

## WR-01 — `docs_update` can sweep `.devflow/` into a user's commit

`crates/devflow-core/src/hooks.rs:184` still calls
`git.commit_all("docs: update generated docs")`. It is the **only remaining
`commit_all` caller**, and it runs `git add .` at `ctx.project_root` — the *user's*
repo, not DevFlow's.

`.devflow/phase-NN-stdout` is raw, unredacted agent stdout. If a target project's
`.gitignore` lacks `.devflow/`, the Validate→Ship `DocsUpdate` hook sweeps it into
a commit that `Merge` then pushes. Reproduced in a scratch repo (17-REVIEW.md):

```
$ git add . && git commit -qm init && git log -1 --name-only --pretty=format:
.devflow/events.jsonl
.devflow/phase-01-stdout
README.md
```

The assumption that `.devflow/` is gitignored is *asserted in test fixtures*
(`hooks.rs:489-491`, `:552-554` — "gitignored in every real project") but
**enforced nowhere**. Both existing guards (`gitignore_coverage.rs`,
`doc_check.rs:283`) only cover DevFlow's own repo.

Pre-existing, but Phase 17 introduced `commit_path` specifically to avoid this
sweep for `ChangelogAppend`/`VersionBump` and left `docs_update` behind.

**Preferred fix:** have `lock::ensure_devflow_dir` write a `.devflow/.gitignore`
containing `*` on creation. Self-ignoring, requires no change to the user's root
`.gitignore`, and closes it for every constructor at once. Verified 2026-07-20
that `lock.rs` contains no gitignore logic today. Alternative: scope `docs_update`
through `commit_path`, which fixes this call site only.

## WR-02 — `exe_path` writes an absolute home path and OS username into `events.jsonl`

`crates/devflow-cli/src/main.rs:902` still emits
`"exe_path": std::env::current_exe().ok().map(|p| p.display().to_string())`,
resolving to e.g. `/var/home/<user>/.../target/debug/devflow`. Appended on every
`devflow start`.

`OPERATIONS.md` advertises `events.jsonl` as a file to "tail from any tool", so it
is routinely read and pasted. Combined with WR-01 it becomes committable in a
target project. Gitignored in DevFlow's own repo (`git ls-files .devflow` → 0
tracked), which is exactly why it went unnoticed here.

**Fix:** emit only `current_exe().file_name()`, or a path relative to
`project_root`. `DEVFLOW_BUILD_COMMIT` / `DEVFLOW_BUILD_DIRTY` already carry the
diagnostic value, so nothing is lost.

Note `main.rs:6879` has a test asserting `payload["exe_path"].is_string() ||
.is_null()`, which will need updating alongside the fix.

## Relationship to Phase 18

Phase 18 did **not** fix either. Its plans (18-01, 18-03, 18-06) cite WR-02 in
their threat models as a *prevention* constraint — "do not introduce new instances
of this leak class" — and 18-06 explicitly reasons about why its own path-bearing
message is distinct. That is correct scoping; it means the existing emission
survives Phase 18 untouched.

**Update (18-fix, Phase 18 review-fix batch):** a THIRD WR-02 instance —
`enforce_build_staleness`'s `self_dogfood_stale_blocked` event, which
persisted `truncate_reason(&message)` (the full staleness-block message,
including `execution_root.display()`) into `events.jsonl` — was found and
fixed in Phase 18's code-review-fix pass. It now persists a bare
`"stale_build_blocked"` label plus a path-free `stage`/`worktree` payload;
the full path-bearing message is unchanged everywhere else (terminal notify,
returned `CliError`). That instance is closed and out of scope. The two
instances documented above are the ORIGINAL findings and remain in scope; they
are unaffected by the 18-fix change.

---

# Unit 19b — `commit_path`'s `--allow-empty` Creates Spurious Empty Commits

> Promoted from `999.11-commit-path-empty-commits`. Source: 17-REVIEW.md WR-03,
> triaged 2026-07-20.

## Problem

In `crates/devflow-core/src/git.rs` (`commit_path`, `--allow-empty` at `:336`;
the sibling `commit_all` site is `:312`):

```rust
/// Returns Ok(()) whether or not the path had changes to commit.
...
match self.git_raw(&["commit", "--allow-empty", "-m", message, "--", relative_path]) {
    Ok(()) => Ok(()),
    Err(GitError::Command(ref msg)) if msg.contains("nothing to commit") => Ok(()),
    Err(e) => Err(e),
}
```

Two defects, both from the same line:

1. **The doc comment is wrong about what happens.** It says "Ok(()) whether or not
   the path had changes to commit", implying a skip. `--allow-empty` does not skip
   — it **commits**. Verified in an isolated repo (17-REVIEW.md):
   ```
   $ git commit --allow-empty -m "empty test" -- a.txt   # a.txt unchanged
   [master 7f1c5c8] empty test
   ```

2. **The `nothing to commit` arm is dead code.** `--allow-empty` suppresses that
   failure entirely, so the guard can never be taken — while reading exactly as if
   it were the genuine skip path.

## Why it matters

If `version_bump` (`hooks.rs:239-243`) re-runs after a fail-fast retry of the
terminal hook batch and `write_version` produces byte-identical content, an empty
`chore: bump version to X` commit lands on develop — and the **release tag is
placed on a commit containing nothing**.

This is reachable: Phase 16 established that a failed Merge stops the batch,
preserves state, and reopens the Ship gate, which means terminal-batch retries are
a designed, exercised path rather than a theoretical one.

## Fix

Either:
- Drop `--allow-empty` and keep the `nothing to commit` arm as the genuine no-op
  path (restores the doc comment's stated contract and revives the dead arm); or
- Check `git diff --cached --quiet -- <path>` before committing.

The first is smaller and makes the existing code honest.

Related: `git.rs:659` has a test comment noting "the property that distinguishes
commit_path from commit_all" — check whether existing `commit_path` tests pin the
empty-commit behavior, since fixing this changes observable output.

**Scope note:** decide explicitly whether the `commit_all` site at `git.rs:312`
is in scope. The finding is written against `commit_path`, but the same
`--allow-empty` reasoning applies one function over.

---

# Units 19c–19f — Split `crates/devflow-cli/src/main.rs`

> Promoted from `999.8-split-main-rs`. Surfaced 2026-07-20 during Phase 18
> planning — the same-wave zero-file-overlap rule forced 6 near-serial waves
> because 6 of 7 plans touched `main.rs`. Was blocked on Phase 18; **now
> unblocked** (Phase 18 shipped as v1.5.0, 2026-07-21).

## Problem

`main.rs` is **8,467 lines** — 4,025 production plus a 4,442-line `#[cfg(test)]`
module (starting at `:4026`, 106 tests). It is the single largest file in the
workspace by **3.4x** (next is `agent_result.rs` at 2,505).

> The figures below marked *(as recorded 2026-07-20)* were measured when the
> file was 6,239 lines. The file has grown **+35%** since. Re-measure cluster
> boundaries at plan time — the old line ranges are stale, though the cluster
> *identities* are expected to hold.

Its size is now the binding constraint on execution parallelism. GSD's same-wave
zero-file-overlap rule keys on file path, so any two plans touching `main.rs`
cannot share a wave regardless of whether they touch disjoint functions. Phase 18
was forced into **6 near-serial waves** for 7 plans purely on this basis — the
planner flagged it explicitly as "serial by necessity, not by choice."

## Measured cluster boundaries *(as recorded 2026-07-20 — line ranges now stale)*

The production half already decomposes cleanly:

| Cluster | Lines | Representative functions | Phase 18 plans touching it |
|---|---|---|---|
| env/config parse | 30–53 | `parse_gate_timeout`, `checkout_lock_timeout` | — |
| dispatch | 329–520 | `main`, `run`, `resolve_gate_target` | — |
| preflight | 671–834 | `run_preflight`, `preflight_gh_auth_check`, `ensure_agent_binary` | 18-07 |
| staleness/provenance | 835–1136 | `enforce_build_staleness`, `combined_staleness`, `embedded_commit_is_stale` | 18-06 |
| pipeline state machine | 1137–1900 | `launch_stage`, `advance`, `transition`, `handle_*_outcome`, `run_gate` | 18-04, 18-05, 18-07 |
| parallel/sequentagent | 2018–2400 | `parallel`, `sequentagent`, `run_agent_blocking` | — |
| commands/display | 2470–3160 | `status`, `doctor`, `logs`, `gate_list`, `recover_cmd`, `list` | 18-01, 18-03 |

**Projected gain:** 6 waves → 3.

## The `ENV_MUTEX` risk — the core of this phase

**`ENV_MUTEX` has 18 `.lock()` sites and 63 total references in `main.rs`.** It is
a process-global mutex serializing env-var mutation across tests. Redistributing
106 tests across new module boundaries while preserving those serialization
guarantees is precisely the failure class this project has the worst track record
with:

- **19i** — process-global `PATH` race via `set_var`; hit **2/2 in CI** on the
  v1.4.0 release PR after passing locally most of the time.
- **GAP-2** — concurrent-ship gate-poll hang; ~33–40% of isolated runs.
- **999.4** — version-tag contention; caught only by instrumentation, both phases
  inside `version_bump` ~1.8ms apart.

All three were invisible on a dedicated workstation and expensive to diagnose.

`ENV_MUTEX` is therefore a *repeat* root cause across three separate
expensive-to-diagnose failures. **The scrutiny during this split should be on
whether its serialization guarantees can actually survive being distributed
across module boundaries — not just on relocating code cleanly. If they cannot
be preserved without a structural change to how tests serialize env mutation,
that is a finding worth surfacing on its own, not something to patch around
silently mid-refactor.** This phase may legitimately end with the split partially
done and that design question open; that is an acceptable outcome.

## Proposed shape

- **Pure-move refactor, zero behavioral change.** No logic edits bundled in — that
  is what makes the existing test suite valid as the equivalence proof.
- **Split the test module in the same operation.** Leaving 4,442 test lines in
  `main.rs` defeats the purpose. Rust unit tests reach parent-module private items,
  so each cluster's tests move with its code; tests spanning clusters need explicit
  handling. `ENV_MUTEX` must become a shared item whose serialization still holds
  across modules — this is the part to review hardest.
- **Verify on a branch with CI.** Feature-branch CI runs on every push (as of
  `f25c670`), and 19i is direct evidence that CI's shared runners widen race windows
  relative to this workstation. **Do not accept local-green as sufficient.**
- Keep `main.rs` as thin dispatch (`main`, `run`, arg routing) only.

## Open questions for discuss-phase

- Module layout: flat siblings (`preflight.rs`, `staleness.rs`, `pipeline.rs`,
  `commands.rs`) vs. a `commands/` subdirectory with one file per subcommand.
- Whether any of these clusters belong in `devflow-core` rather than `devflow-cli`
  — `staleness` and `preflight` are arguably core logic currently living in the CLI
  crate. Moving them changes the public API surface and is a larger decision than
  the file split itself; may warrant deferring to keep this a pure move.
- Can `ENV_MUTEX`'s guarantees survive the split at all? (see above)

## Mechanical follow-up owned by this phase

`.planning/codebase/TESTING.md` is already stale (still cites the now-deleted
`devflow_ignores_stray_devflow_yaml` as its example test for `main.rs`).
Regenerate or update it once the split lands — not worth its own backlog number,
but do not let it slide past this phase's completion.

---

# Unit 19g — AI Change Acceptance Contract

> Promoted from `999.16-ai-change-acceptance-contract`. Source:
> TEST-SUITE-QA-REVIEW.md (Codex, 2026-07-21), P0 recommendation #2.

## Problem

The 2026-07-21 QA review found tests that only reproduce the production
algorithm, compare a function's output with itself, or exercise entirely
test-invented behavior with no production analog (the `ReviewerSetTestAdapter`
found and removed in that session is the concrete example — a fake
`AgentAdapter` whose `preflight()` logic existed only in the test). Nothing
currently requires an AI-generated change to prove its own test actually
exercises real behavior.

## Proposed shape

Require every AI-generated behavioral change to include:

1. A regression test that fails before the implementation change.
2. At least one assertion at a public or stable domain boundary.
3. Evidence that the test fails for the intended reason (not just that a test
   exists).
4. Full affected-package tests, clippy with warnings denied, and formatting.
5. Independent review of both implementation and test signal.

Reject tests that only assert constants, reproduce the production algorithm,
compare a function call with itself, or grep implementation text without a
runtime contract.

## Where this should live

Not just prose in `CONTRIBUTING.md` — this project already runs
`/gsd-code-review` before Ship and refuses to ship on Critical findings. The
contract has teeth only if woven into that review's actual criteria, not left
as a checklist nobody consults. Open question for discuss-phase: does this
belong in the `gsd-code-review` prompt, a separate lint pass, or both.

## Why it is in this phase

It touches no Rust source, so it has zero file-overlap conflict with the split
and can run on a fully parallel track. It also directly hardens against the
defect class that this phase's own refactor is most likely to produce — a
pure-move refactor verified by tests is exactly the situation where a test that
does not really exercise anything is most dangerous.
