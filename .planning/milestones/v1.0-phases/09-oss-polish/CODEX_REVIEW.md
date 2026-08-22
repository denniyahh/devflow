# Codex Review - Phase 9 OSS Polish Plan

## Verdict

The original Phase 9 plan is directionally correct and already removes the largest prior scope problem by deferring audit log and release workflow work. It still needed refinement in four areas:

1. Some acceptance checks were too broad and would fail on historical `.planning/` files.
2. Several tasks were factually stale against the current codebase.
3. The ship branch fix needed to be moved earlier so architecture/docs describe the fixed behavior.
4. Public docs need stronger correctness gates because README, CONTRIBUTING, CHANGELOG, and the Hermes skill disagree with source.

The refined `PLAN.md` keeps Phase 9 limited to OSS polish and the explicitly requested ship safety bug.

## Source Evidence Checked

| Area | Evidence |
|---|---|
| Workspace/version | `Cargo.toml` has workspace version `1.0.1`, crates `devflow-core` and `devflow-cli` |
| Agents | `state.rs` supports `Claude`, `Codex`, `OpenCode`; OMX is commented out but still present in `agents/omx.rs`, `agents/mod.rs`, tests, and docs |
| Prompt contract | `agents/mod.rs::phase_prompt()` requires `.planning/ROADMAP.md` and `.planning/phases/{phase}/CONTEXT.md` |
| Ship bug | `git.rs::release_start()` checks out `develop` before creating `release/{version}` |
| CLI commands | `main.rs` includes `doctor`, `confirm`, `rejectpr`, `parallel`, `sequentagent`, `reference`, `verify`, `lint`, `docs` |
| Config schema | `config.rs::GitFlowConfig` has `main`, `develop`, `feature_prefix`; no `enabled` field |
| Monitor | `monitor.rs` uses a detached shell process that owns the agent, captures stdout/exit/PID, then runs `devflow check` |
| Agent result | `agent_result.rs` implements DEVFLOW_RESULT, exit-code-plus-commit, and commit fallback layers |
| CI | `.github/workflows/ci.yml` uses `pull_request`, not `pull_request_target`, and runs test/clippy/fmt |
| Missing infra | No root `ARCHITECTURE.md`, no `.devcontainer/devcontainer.json`, no `rust-toolchain.toml` |
| Local artifacts | `distrobox.ini` is tracked; `.omx/` contains runtime logs/state and is ignored |

## Findings

### High - Repo-wide zero-OMX grep is not a valid acceptance check

Historical `.planning/` docs and previous phase summaries contain OMX references. Requiring zero matches across the entire repo would force noisy history rewriting of planning artifacts. The refined plan scopes zero-OMX verification to active source and public contributor docs.

### High - `.planning/` must be resolved as a real convention

The current prompt code requires `.planning/` files. The plan cannot tell contributors `.planning/` is merely private or unrelated while DevFlow asks agents to read it. The refined plan chooses the current behavior: document `.planning/` as the phase-plan convention and narrow `.gitignore` if needed.

### High - Ship branch fix should precede architecture/docs

`release_start()` currently branches from `develop`, which conflicts with the desired behavior and the stated Phase 8 incident. The refined order fixes and tests this before writing final architecture and public docs.

### Medium - `doctor` exists now, but stale-binary detection is not in scope unless explicitly chosen

Earlier review notes said no `doctor` command existed. Current `main.rs` does include `doctor`, but adding stale-binary detection would be feature work beyond the user's requested Phase 9 scope. The refined plan keeps docs accurate and leaves new doctor behavior out unless separately approved.

### Medium - CHANGELOG is over-optimistic

`CHANGELOG.md` already claims removed OMX docs, ship fix, and stale-binary detection. Source/docs do not fully support those claims yet. The refined plan requires either making claims true during Phase 9 or correcting the changelog.

### Medium - README has implementation mismatches

README still says adding an agent takes 3 changes, includes `git_flow.enabled`, and describes Layer 3 as stdout-existence based. Source requires a larger adapter checklist, has no `enabled` field, and Layer 3 is commit fallback.

### Low - CI mostly exists

CI already has the core jobs and fork-safe trigger shape. Phase 9 should polish this with a badge and toolchain policy rather than rebuild CI from scratch.

## Refined Plan Changes

- Added an explicit preflight/scope-fence section.
- Replaced repo-wide OMX grep with active-source/public-doc grep.
- Added exact source evidence and source-of-truth references.
- Added tasks for disabled OMX comments/tests/exports, not just active enum/match arms.
- Chose the `.planning/` convention path instead of leaving it ambiguous.
- Moved ship branch fix before architecture and docs.
- Added public docs gates for README command list, config schema, changelog accuracy, and completion-evaluation wording.
- Added devcontainer and CI/toolchain work as polish, with `gh` explicitly optional.
- Kept audit log, release workflow, and Dockerfile deferred to Phase 11.

## Validation Notes

No source code was changed during this review. Only planning artifacts were written:

- `.planning/phases/09-oss-polish/PLAN.md`
- `.planning/phases/09-oss-polish/CODEX_REVIEW.md`

The existing `.planning/phases/09-oss-polish/CODEX-REVIEW.md` was left untouched because the user specifically requested the underscore path.
