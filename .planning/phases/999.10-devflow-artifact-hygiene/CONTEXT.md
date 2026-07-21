---
status: backlog
source: 17-REVIEW.md WR-01 + WR-02, triaged 2026-07-20 and re-verified still present at HEAD
severity: highest of the WR batch — affects downstream users' repositories, not just this one
---

# Backlog: `.devflow/` Artifact Hygiene (WR-01 + WR-02)

Two findings grouped because they compose into one threat: **WR-02 writes
personally-identifying data into a `.devflow/` artifact, and WR-01 commits that
artifact into the user's repository.** Either alone is minor; together they
publish a developer's home path and OS username into a target project's git
history. They have independent fixes and can land separately.

## WR-01 — `docs_update` can sweep `.devflow/` into a user's commit

**Verified at HEAD 2026-07-20:** `crates/devflow-core/src/hooks.rs:184` still calls
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

**Verified at HEAD 2026-07-20:** `crates/devflow-cli/src/main.rs:843` still emits
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

## Relationship to Phase 18

Phase 18 does **not** fix either. Its plans (18-01, 18-03, 18-06) cite WR-02 in
their threat models as a *prevention* constraint — "do not introduce new instances
of this leak class" — and 18-06 explicitly reasons about why its own path-bearing
message is distinct. That is correct scoping; it means the existing emission at
`main.rs:843` survives Phase 18 untouched.

Note `main.rs:5237` has a test asserting `payload["exe_path"].is_string() ||
.is_null()`, which will need updating alongside the fix.

**Update (18-fix, Phase 18 review-fix batch):** a THIRD WR-02 instance —
`enforce_build_staleness`'s `self_dogfood_stale_blocked` event, which
persisted `truncate_reason(&message)` (the full staleness-block message,
including `execution_root.display()`) into `events.jsonl` — was found and
fixed in Phase 18's code-review-fix pass. It now persists a bare
`"stale_build_blocked"` label plus a path-free `stage`/`worktree` payload;
the full path-bearing message is unchanged everywhere else (terminal notify,
returned `CliError`). That instance is closed and out of this backlog's
scope. The two instances documented above — `exe_path` in `workflow_started`
and `docs_update`'s `git add .` sweep — are the ORIGINAL findings and remain
this phase's scope; they are unaffected by the 18-fix change.

## Notes

Do this before any wider release push — it is the only finding in the WR batch
whose blast radius extends to other people's repositories. Cheap to fix; the
`.devflow/.gitignore` approach is a few lines in `ensure_devflow_dir`.

Promote with `/gsd-review-backlog` when ready.
