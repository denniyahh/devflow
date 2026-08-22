# Phase 27: Scrub Redirecting Git Environment From Production Calls - Pattern Map

**Mapped:** 2026-07-30
**Files analyzed:** 7 (all modified, 0 net-new files)
**Analogs found:** 7 / 7 (all against one canonical source: `test_support.rs`)

This is a migration phase, not a new-file phase. There is one canonical pattern
(the constructor) to reproduce in an always-compiled home, and 41 existing call
sites across 7 files to rewrite to use it. Line numbers below were re-verified
live against this worktree's HEAD and match `27-RESEARCH.md` and `27-CONTEXT.md`
exactly — **zero drift** found.

## File Classification

| Modified File | Role | Data Flow | Call Sites | Chokepoint? |
|---|---|---|---|---|
| `crates/devflow-core/src/git.rs` | service (git wrapper) + **new constructor's home** | request-response (subprocess) | 9 | Partial — 4 method wrappers (`git`, `git_output`, `git_raw`, `git_raw_combined`) cover much of the internal call graph; 5 sites (100, 175, 488, 497, 708) are free-standing and need direct edits |
| `crates/devflow-core/src/version.rs` | service (git-derived version resolution) | request-response | 10 | No — every site (120, 147, 160, 184, 216, 240, 305, 338, 399, 563) is a direct, independent `Command::new("git")` call; no internal wrapper exists |
| `crates/devflow-core/src/agent_result.rs` | service (agent commit/branch bookkeeping) | request-response | 3 | No — 3 independent sites (574, 583, 664), all `std::process::Command::new("git")` (fully qualified, no `use` import) |
| `crates/devflow-core/src/worktree.rs` | service (worktree list/create/remove) | request-response | 2 | Yes — `run()` at line 174 is the wrapper; line 121 (`list`) is a second, independent direct call that does NOT go through `run()` |
| `crates/devflow-cli/src/commands.rs` | controller (CLI command handlers) | request-response | 3 | No — 3 independent sites (91, 2886, 2892), but 2 of them (2886, 2892) also call into `staleness::run_git_stdout` cross-file, so migrating `staleness.rs`'s wrapper affects this file's *behavior* even though its own 3 direct sites still need individual edits |
| `crates/devflow-cli/src/staleness.rs` | service (build staleness detection) | request-response | 3 | **Yes — highest leverage in the whole phase.** `run_git_stdout` at line 124 is the wrapper; ~15 internal callers plus `commands.rs`'s 2 cross-file callers get scrubbed for free from that one edit. Lines 51 and 72 (inside `embedded_commit_is_stale`) are direct sites that do NOT route through `run_git_stdout` and need separate edits |
| `crates/devflow-cli/src/preflight.rs` | controller/service (pre-flight validation checks) | request-response | 11 | No — largest file, no internal wrapper; all 11 sites (146, 160, 183, 332, 345, 356, 371, 426, 457, 530, 778) are independent direct calls. 2 of them (356, 530) live inside closures (`is_ancestor` closure at 355-362; a second closure at ~525-537) — these will not surface via a function-signature search, only a literal `Command::new("git")` grep |

**Total: 41 call sites, matches CONTEXT.md/RESEARCH.md exactly.**

## Pattern Assignments

### The canonical constructor to reproduce — `crates/devflow-core/src/git.rs` (new code, no analog needed beyond mirroring `test_support.rs`)

**Analog:** `crates/devflow-core/src/test_support.rs` (verified lines 141-190, 192-260)

This is the one piece of genuinely new code in the phase. `test_support::git_command`/
`hermetic_command` already implement exactly this pattern for tests; CONTEXT.md's
discretion section and RESEARCH.md's primary recommendation both point at
mirroring it verbatim in `git.rs` (always-compiled — `test_support.rs` is gated
`#[cfg(any(test, feature = "test-support"))]` and devflow-cli's *production*
dependency on devflow-core does not enable that feature).

**Variable list constants** (test_support.rs:141-162, verified verbatim):
```rust
pub const REPO_LOCAL_GIT_VARS: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

// Variables that are not repository-local -- and so absent from
// `--local-env-vars` -- but still redirect where git reads or writes.
pub const ALSO_REDIRECTING_GIT_VARS: &[&str] =
    &["GIT_NAMESPACE", "GIT_DISCOVERY_ACROSS_FILESYSTEM"];
```

**Constructor pattern** (test_support.rs:164-190, verified verbatim):
```rust
/// A `git` command pinned to `repo` **and** stripped of every inherited
/// variable that could redirect it somewhere else.
///
/// Use this for every fixture git invocation instead of building
/// `Command::new("git")` directly. `GIT_EXEC_PATH` is deliberately left alone:
/// it only locates git's own helper binaries and cannot change which
/// repository git acts on.
///
/// Clearing `GIT_CONFIG_COUNT` is sufficient to neutralize any inherited
/// `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` pair -- git only reads those when
/// the count is set -- so they need no separate sweep.
pub fn git_command(repo: &Path) -> Command {
    hermetic_command("git", repo)
}

/// As [`git_command`], for a program that is not `git` itself but will shell
/// out to it -- `cargo`, whose build scripts invoke `git`, is the motivating
/// case. The redirecting variables are inherited all the way down a process
/// tree, so scrubbing only the direct `git` calls would leave that path open.
pub fn hermetic_command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir);
    for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
        cmd.env_remove(var);
    }
    cmd
}
```

**Test pattern to mirror** (test_support.rs:196-260, verified verbatim, 3 tests):
- `git_command_marks_every_redirecting_var_for_removal` — asserts every var in
  both lists is present in `cmd.get_envs()` with `value.is_none()` (marked for
  removal), not a behavioral subprocess test — cheap, no spawn.
- `git_command_preserves_git_exec_path` — negative assertion that `GIT_EXEC_PATH`
  is untouched.
- `local_env_vars_match_git` — runs `git rev-parse --local-env-vars` live and
  asserts (sorted) equality against `REPO_LOCAL_GIT_VARS`, so a git upgrade
  that adds a new repository-local variable makes this fail loudly instead of
  silently reopening the hole. This is the drift-check the "Don't Hand-Roll"
  guidance in RESEARCH.md calls out as already-solved — reuse it, don't
  reinvent a second copy.

**Placement recommendation (from RESEARCH.md, not re-derived here):** land these
in `git.rs` (already `use std::process::{Command, Stdio};`, already
unconditionally `pub`), then have `test_support::git_command`/`hermetic_command`
delegate to `crate::git::hermetic_command` to keep the two lists from drifting —
this is Claude's-discretion in CONTEXT.md, not a hard requirement, but is the
one both CONTEXT.md and RESEARCH.md converge on.

---

### `crates/devflow-core/src/git.rs` (service, 9 call sites, partial chokepoint)

**Free-standing site — plain builder shape** (git.rs:100-105, `branch_exists`-adjacent, verified):
```rust
Command::new("git")
    .args(["merge-base", "--is-ancestor", &branch, &self.config.develop])
    .current_dir(&self.root)
    .output()
    .map(|output| output.status.success())
    .unwrap_or(false)
```
Migration shape: `git_command(&self.root)` replaces `Command::new("git")` +
`.current_dir(&self.root)` becomes redundant (constructor already pins
`current_dir`) — drop the explicit `.current_dir()` call, keep everything else.

**Site with pre-existing extra config scoping** (git.rs:127-133, the `release_finish`
method — note: this method itself calls `self.git([...])`, an internal wrapper,
not a bare `Command::new("git")` directly, but demonstrates the pattern any
free-standing site with `-c` flags must preserve):
```rust
// `-c tag.gpgSign=false` scopes the override to this invocation only
// (never the user's global/repo config) -- without it, a global
// `tag.gpgsign=true` forces this lightweight tag into an
// annotated+signed one requiring a message, which blocks on
// `$EDITOR` in what must be a headless, unattended flow (Phase 13
// dogfood finding).
self.git(["-c", "tag.gpgSign=false", "tag", &format!("v{version}")])?;
```
Composable scrub-then-augment applies here per RESEARCH.md Pattern 1: `-c`
flags are `.args()` content, independent of the env scrub — no conflict,
no special handling needed beyond swapping the constructor at the wrapper's
own `Command::new("git")` call site (not shown above, but implied at
whichever of the 4 internal method wrappers builds the actual `Command`).

**Sites needing direct edits (no wrapper coverage):** lines 100, 175, 488, 497, 708.
**Sites likely covered via the 4 method wrappers** (`git`, `git_output`, `git_raw`,
`git_raw_combined` — migrate the wrapper's own `Command::new("git")` call, not
each caller): lines 387, 415, 438, 450. Confirm each of these 4 line numbers is
in fact inside one of the 4 wrapper method bodies before treating it as
"covered" — do not assume without checking.

**LC_ALL/LANG composability note** (git.rs:387-392 region, per RESEARCH.md
direct verification): 2 of these 9 sites already chain `.env("LC_ALL", "C")`
and `.env("LANG", "C")` after `.current_dir()` — these are different env keys
than the scrub list, so they compose without conflict; append them after
`git_command(&root)` the same way they're appended today.

---

### `crates/devflow-core/src/version.rs` (service, 10 call sites, no chokepoint)

No internal wrapper — every one of the 10 sites (120, 147, 160, 184, 216, 240,
305, 338, 399, 563) needs its own direct edit, swapping `Command::new("git")` +
`.current_dir(&root)` for `git_command(&root)`. Same builder shape as
git.rs:100-105 above; use that as the representative before-shape.

---

### `crates/devflow-core/src/worktree.rs` (service, 2 call sites, 1 partial chokepoint)

**Chokepoint** (worktree.rs:174-184, verified):
```rust
fn run(project_root: &Path, args: &[&str]) -> Result<(), WorktreeError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(WorktreeError::Command(stderr_or_status(&output)))
    }
}
```
Migrating this one function's `Command::new("git")` covers every caller of
`run()` (e.g. `prune`, `remove` at lines above 110) without a separate edit
per caller.

**Independent direct site — NOT covered by `run()`** (worktree.rs:121-124, verified):
```rust
pub fn list(project_root: &Path) -> Result<Vec<WorktreeInfo>, WorktreeError> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        return Err(WorktreeError::Command(stderr_or_status(&output)));
    }
    ...
```
This needs its own direct edit — `list()` does not call `run()`.

---

### `crates/devflow-cli/src/staleness.rs` (service, 3 call sites, 1 high-leverage chokepoint)

**Chokepoint — the single highest-leverage edit in the phase** (staleness.rs:120-133, verified):
```rust
/// Shell `git` in `project_root`, returning `None` on any failure (missing
/// binary, non-git directory, non-zero exit) -- same argv-array idiom as
/// `build.rs`'s `run_git`.
pub(crate) fn run_git_stdout(project_root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}
```
Migrating this function's `Command::new("git")` to `git_command(project_root)`
scrubs ~15 internal callers in `staleness.rs` (e.g. `ancestry_range_affects_build`
at line 112 calls `run_git_stdout` directly) PLUS 2 cross-file callers in
`commands.rs` (lines 2886, 2892-region — confirm exact call sites reference
`staleness::run_git_stdout`, not a separate direct `Command::new("git")`, when
implementing) — all from one edit.

**Direct sites NOT routed through the wrapper — need separate edits** (staleness.rs:51-54
and 72-75, inside `embedded_commit_is_stale`, verified):
```rust
let output = std::process::Command::new("git")
    .args(["merge-base", "--is-ancestor", embedded_commit, "HEAD"])
    .current_dir(execution_root)
    .output();
...
let reverse = std::process::Command::new("git")
    .args(["merge-base", "--is-ancestor", "HEAD", embedded_commit])
    .current_dir(execution_root)
    .output();
```
Same builder shape, fully-qualified `std::process::Command` (no `use` import
in this file, unlike `git.rs`/`worktree.rs` which import `Command` directly)
— match whichever import convention the file already uses when adding the
`git_command` call (likely `crate::...::git_command` or a re-exported path,
planner's call on module path).

---

### `crates/devflow-cli/src/commands.rs` (controller, 3 call sites, no internal wrapper)

3 independent direct sites (91, 2886, 2892), same `std::process::Command::new("git")`
fully-qualified shape as staleness.rs. 2 of these (2886, 2892) also call into
`staleness::run_git_stdout` per RESEARCH.md's structure map — migrating
`staleness.rs`'s wrapper affects this file's behavior for those cross-file
paths, but the 3 sites listed here are `commands.rs`'s own direct
`Command::new("git")` calls and still need individual edits regardless.

---

### `crates/devflow-cli/src/preflight.rs` (controller/service, 11 call sites, largest file, no wrapper)

**Representative direct site with soft-fail posture** (preflight.rs:332-337, verified,
part of `base_ref_currency`):
```rust
let fetch_ok = std::process::Command::new("git")
    .args(["fetch", "--quiet", ORIGIN, base])
    .current_dir(project_root)
    .output()
    .map(|out| out.status.success())
    .unwrap_or(false);
```

**Closure-embedded site — will NOT surface via a function-signature search**
(preflight.rs:355-362, verified, inside `base_ref_currency`):
```rust
let is_ancestor = |ancestor: &str, descendant: &str| {
    std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
};
```
This closure captures `project_root` by reference from its enclosing scope —
migrate identically to any other site (`git_command(project_root)` in place of
`Command::new("git")` + drop `.current_dir(project_root)`), but it will only be
found via a literal `Command::new("git")` grep, not a "function that takes
`project_root`" search. A second closure exists around preflight.rs:525-537
(not re-read here — RESEARCH.md already flags it at the same shape) with the
same caveat.

**Sites needing individual edits:** 146, 160, 183, 332, 345, 356 (closure), 371,
426, 457, 530 (closure), 778.

---

## Shared Patterns

### Composable scrub-then-augment (applies to all 41 sites)
**Source:** Modeled on `test_support.rs:164-190` + confirmed empirically against
`git.rs`'s existing `.env("LC_ALL", "C")`/`.env("LANG", "C")` chaining.
**Apply to:** Every migrated call site. The constructor returns a `Command`,
not a finished object — any additional `.env(...)`, `.args(...)`, `-c` config
flags, etc. that a site already chains continue to work unchanged; only the
`Command::new("git")` (or `std::process::Command::new("git")`) + separate
`.current_dir(...)` pair is replaced by a single `git_command(&root)` call.

### Chokepoint-first migration ordering (applies to 3 of 7 files)
**Source:** `staleness.rs::run_git_stdout` (line 124), `worktree.rs::run` (line
174), and `git.rs`'s 4 internal method wrappers (`git`, `git_output`, `git_raw`,
`git_raw_combined` — exact line numbers not enumerated here, confirm during
planning).
**Apply to:** `staleness.rs` (migrate `run_git_stdout` first — highest leverage
in the whole phase, ~17 callers covered by 1 edit), `worktree.rs` (migrate
`run` first, covers `prune`/`remove`, then handle `list` separately), `git.rs`
(migrate the 4 method wrappers before touching the 5 free-standing sites).
`version.rs`, `agent_result.rs`, `commands.rs`, `preflight.rs` have no internal
wrapper — every site in those 4 files needs an individual edit regardless of
order.

### Fully-qualified vs. imported `Command` (naming convention split)
**Source:** `git.rs`/`worktree.rs` use `use std::process::{Command, Stdio};` +
bare `Command::new("git")`; `agent_result.rs`, `commands.rs`, `staleness.rs`,
`preflight.rs` use fully-qualified `std::process::Command::new("git")` with no
`use` import.
**Apply to:** Match each file's existing convention when importing the new
`git_command`/`hermetic_command` function — do not introduce a `use
std::process::Command` import into a file that has deliberately avoided it, and
vice versa; this is a style-consistency note only, no functional difference.

## No Analog Found

None — every file in scope has a direct analog (`test_support.rs`, for the
constructor) or is itself the analog for its own migration (a call site's
"before" shape is its own closest match; there is no cross-file precedent
needed for a like-for-like rewrite).

## Metadata

**Analog search scope:** `crates/devflow-core/src/{git,version,worktree,agent_result,test_support}.rs`,
`crates/devflow-cli/src/{commands,staleness,preflight}.rs` — the 7 files
CONTEXT.md/RESEARCH.md scope plus the 1 canonical pattern source.
**Files scanned:** 8 (7 target + 1 analog source), all read directly, no re-reads
of overlapping ranges.
**Call-site count cross-check:** `rg -n 'Command::new\("git"\)'` run live against
each of the 7 files; all 41 line numbers matched RESEARCH.md/CONTEXT.md exactly
— zero drift found in this pass.
**Pattern extraction date:** 2026-07-30
</content>
