---
status: backlog
source: 17-REVIEW.md WR-03, triaged 2026-07-20 and re-verified still present at HEAD
---

# Backlog: `commit_path`'s `--allow-empty` Creates Spurious Empty Commits

## Problem

**Verified at HEAD 2026-07-20** in `crates/devflow-core/src/git.rs` (`commit_path`):

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

## Notes

Not covered by Phase 18. Distinct subsystem from 999.10 (git commit semantics
rather than artifact leakage), so tracked separately despite both originating in
the same review round.

Promote with `/gsd-review-backlog` when ready.
