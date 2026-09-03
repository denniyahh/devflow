### [CONFIRMED] Green full-suite verification exits non-zero

**Where:** `45-01-PLAN.md:515-516`, `45-02-PLAN.md:342-343`, `45-03-PLAN.md:354-355`

**Evidence:** On a green file, `grep -c '^test result: FAILED' file` prints `0` and exits `1`. The command’s final status is therefore non-zero despite `cargo_exit=0`.

**Impact:** Every clean workspace-suite verification is reported as failed.

**Fix:** Capture `cargo`’s status separately and use `grep ... || true`, or use an `awk`/shell assertion that exits zero when the count is zero.

### [CONFIRMED] Clippy verification passes when Clippy fails

**Where:** `45-01-PLAN.md:518-519`, `45-02-PLAN.md:345-346`, `45-03-PLAN.md:357-358`

**Evidence:** `cargo clippy ... | rg 'Finished|error'` exits zero for either `Finished` or `error`. I verified the equivalent pipeline returns zero for `error: boom`.

**Impact:** Compilation/lint errors can satisfy the automated check.

**Fix:** Capture Clippy’s exit status, then separately assert no error lines and a successful status.

### [CONFIRMED] `--no-worktree` still ignores the configured base

**Where:** `crates/devflow-cli/src/commands.rs:343-347`

**Evidence:** The planned conversions name `commands.rs:672,2052,2072` but omit the `start` branch-creation path. This path still calls `GitFlow::new`, whose constructor hardcodes `GitFlowConfig::default()` at `crates/devflow-core/src/git.rs:119-123`.

**Impact:** `devflow start --no-worktree` checks the configured base, then creates the feature branch from `develop`.

**Fix:** Use the same project-resolved `GitFlow` for the no-worktree feature-start path.

### [CONFIRMED] Artifact preflight still hardcodes `develop`

**Where:** `crates/devflow-cli/src/commands.rs:90-100`, `crates/devflow-cli/src/commands.rs:293`, `crates/devflow-cli/src/commands.rs:303`, `crates/devflow-cli/src/preflight.rs:618`

**Evidence:** `phase_artifact_on_develop` passes literal `"develop"` to `git ls-tree`; both `start` and `preflight_interactivity_check` call it. Plan 45-01 only retargets reachability/currency/worktree calls at `:270-280`.

**Impact:** A configured planning branch containing `CONTEXT.md`/`PLAN.md` is invisible to these checks, producing incorrect refusal/warning behavior for drivers using `RequiresExistingArtifact`.

**Fix:** Add the resolved base as a parameter to the helper and all callers; update messages and comments accordingly.

### [CONFIRMED] Base validation permits commit-ish aliases and production refs

**Where:** `45-01-PLAN.md:243-245`; `crates/devflow-cli/src/preflight.rs:151-157`; `crates/devflow-core/src/worktree.rs:76-79`

**Evidence:** The proposed validator rejects only blank values, leading `-`, and exact `main`. Reachability uses `git rev-parse --verify <base>`, and `worktree::add` passes the raw value as `<start_point>`. Values such as `HEAD`, `origin/main`, or `refs/heads/main` therefore pass the proposed checks.

**Impact:** The “never fork from production” guard is bypassable, and non-branch commit-ish values can lead to detached or otherwise invalid integration behavior.

**Fix:** Require a valid local branch ref (`refs/heads/<name>`), then compare the canonical branch name against `MAIN` before mutation.

### [CONFIRMED] The scoped staleness rule contradicts its own `..` negative control

**Where:** `45-02-PLAN.md:209-212`, `45-02-PLAN.md:287-295`, `crates/devflow-cli/src/staleness.rs:183-194`

**Evidence:** The specified scoped rule is “starts with `crates/` and ends in `.rs`.” `crates/../foo.rs` satisfies both conditions, but the test requires it to return `false`.

**Impact:** The executor must invent an unstated normalization/rejection rule or cannot satisfy both the action and tests.

**Fix:** Either explicitly reject `..` path segments and test that rule, or remove the contradictory case.

### [CONFIRMED] Required RED test cannot be produced by deleting the match arm

**Where:** `45-01-PLAN.md:353-355`, `45-01-PLAN.md:407`; `crates/devflow-cli/src/preflight.rs:538-547`

**Evidence:** `BaseRefCurrency` is fully matched. Deleting `BaseRefCurrency::Undeterminable => Ok(())` produces a non-exhaustive-match compiler error, not a runtime `test result: FAILED`.

**Impact:** The acceptance criterion explicitly demands evidence that the plan’s prescribed mutation cannot produce.

**Fix:** Temporarily replace the arm with `Err(...)` or `panic!`, run the test, then restore the implementation.

### [CONFIRMED] The fail-open warning is not actually pinned

**Where:** `45-01-PLAN.md:34`, `45-01-PLAN.md:364-369`; `crates/devflow-cli/src/preflight.rs:540-545`

**Evidence:** The required behavior includes a warning containing `fail-open`, but the proposed test may assert only `Ok(())` when no injectable writer is available. Removing or changing the warning would leave that test green.

**Impact:** The warning contract can regress silently while acceptance still passes.

**Fix:** Extract the disposition/message into a testable helper or add an injectable output path and assert the emitted warning text.

### [SUSPECTED] Monitor commit enumeration may resolve config from the wrong root

**Where:** `crates/devflow-core/src/monitor.rs:1190`, `crates/devflow-core/src/monitor.rs:1245-1251`; `45-01-PLAN.md:482-486`

**Evidence:** `enumerate_phase_commits` receives only `workdir`. The plan says to replace the default config but does not specify a canonical project root. Resolving from a phase worktree can miss a `devflow.toml` that exists only in the main checkout, silently falling back to `develop`.

**Impact:** Idle-timeout evidence can enumerate the wrong base range for configured projects.

**Fix:** Thread the canonical `project_root` separately and resolve the config from it, or resolve once in the caller and pass the `GitFlowConfig`.

### [CONFIRMED] Operator output still says branches are behind `develop`

**Where:** `crates/devflow-cli/src/commands.rs:2083`; `45-01-PLAN.md:488-499`

**Evidence:** The plan updates `GitFlow::new` at `:2072` but does not specify changing the literal `format!(" ({} behind develop)", b.behind)`.

**Impact:** `devflow list`/open-branch output reports a false comparison after configuring another base branch.

**Fix:** Interpolate the resolved base branch in that message.

### [SUSPECTED] The required `main` refusal is bypassed by the resolver’s fail-soft fallback

**Where:** `45-01-PLAN.md:234-245`, `45-01-PLAN.md:323-326`

**Evidence:** The resolver is instructed to validate each source and “fall through” on invalid values. Thus a file or environment value of `main` falls back to `develop`; the later `start` validation never sees `main`, so no refusal naming `main` is emitted.

**Impact:** The documented “configured `main` is refused” behavior is not observable for the most direct configuration.

**Fix:** Treat an explicitly supplied invalid base as a hard configuration error, or return source/provenance information so `start` can refuse it explicitly.

## Verdict

**REJECT.** The plans contain executable verification commands that fail on success or pass on errors, miss functional AUTO-01 call sites, and contain contradictory staleness requirements. The 45-03 plan is honest about its upstream competing instruction and does route one policy constant through both renderers; that residual limitation is not the reason for rejection.

Distinct `file:line` locations cited: **33**.
