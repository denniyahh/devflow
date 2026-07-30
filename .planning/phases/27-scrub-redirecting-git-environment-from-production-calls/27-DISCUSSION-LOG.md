# Phase 27: Scrub Redirecting Git Environment From Production Calls - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-30
**Phase:** 27-scrub-redirecting-git-environment-from-production-calls
**Areas discussed:** GIT_DIR policy, build.rs scope, Acceptance target

---

## GIT_DIR policy

| Option | Description | Selected |
|--------|-------------|----------|
| Hard scrub, no escape hatch | Strip GIT_DIR/GIT_WORK_TREE and the rest of test_support's list unconditionally, no flag/config/env var to re-enable | ✓ |
| Scrub by default, explicit opt-in escape hatch | Same default, but a flag or env var lets an operator deliberately opt back in for an advanced case | |

**User's choice:** Hard scrub, no escape hatch (recommended option).
**Notes:** User asked for a plain-language explanation of what GIT_DIR is and why it matters before deciding — provided (git's repo-location override, set by hooks/rebase --exec/bisect, and the mechanism behind CR-01's bypass of `mutating_project_root`). User then confirmed going with the recommendation.

---

## build.rs scope

| Option | Description | Selected |
|--------|-------------|----------|
| Out of scope | build.rs runs at compile time under the builder's own control — different actor/threat model than an operator's runtime git-hook scenario | ✓ |
| In scope — scrub it too | A GIT_DIR set during `cargo build` would embed a wrong commit/dirty flag into the runtime staleness check, so scrub for full consistency | |

**User's choice:** Out of scope (recommended option).
**Notes:** Explained build.rs's `run_git` (embeds DEVFLOW_BUILD_COMMIT/DEVFLOW_BUILD_DIRTY via rustc-env at compile time) and why its risk (wrong staleness warning) differs in kind from CR-01's risk (wrong repository acted on at runtime). User asked for help deciding — provided the recommendation and rationale, user confirmed.

---

## Acceptance target

| Option | Description | Selected |
|--------|-------------|----------|
| Prove the scrubbing constructor itself | New tests directly on the constructor (hostile GIT_DIR, correct repo still resolved) plus the 37 currently-failing dirty-environment unit tests turning green | ✓ |
| Also stub a minimal root-resolution guard now | Build a placeholder guard anticipating 999.25's `mutating_project_root`, to close CR-01 provably today | |

**User's choice:** Prove the scrubbing constructor itself (recommended option).
**Notes:** Explained that `mutating_project_root` and `project_root_guard.rs` (what CR-01 names) don't exist on `develop` — confirmed by grep — and remain only on the unmerged `feature/phase-26`, tied to 999.25. Building a stand-in guard now would repeat the same "build ahead of where the real thing lives" coupling problem already ruled out for the resume-ledger (D-06a) during the 999.5 split discussion earlier this session. User asked for help deciding — provided the recommendation, user confirmed.

---

## Claude's Discretion

- Where the scrubbing constructor lives (module/function name) — `devflow-core`, mirroring `test_support::git_command`'s location, is the natural home but not fixed.
- One function vs. a small family (mirroring `test_support`'s `git_command`/`hermetic_command` split).
- Mechanical migration order/waves across the 7 files (`git.rs`, `version.rs`, `agent_result.rs`, `worktree.rs`, `commands.rs`, `staleness.rs`, `preflight.rs`).
- Whether `test_support`'s existing functions get refactored to delegate to the new production constructor, or stay independent.
- Exact scrubbed-variable list — start from `test_support::REPO_LOCAL_GIT_VARS` + `ALSO_REDIRECTING_GIT_VARS`, re-verify against the implementation machine's `git rev-parse --local-env-vars`.

## Deferred Ideas

- `build.rs`'s compile-time git calls — considered and excluded (see build.rs scope above). Worth its own backlog entry only if a real incident is ever observed.
- An operator-facing escape hatch for honoring GIT_DIR — considered and rejected as a standing feature (see GIT_DIR policy above). Not filed; re-derive from scratch if a genuine use case surfaces.
- `mutating_project_root` / `project_root_guard.rs` — not built here; remain 999.25's territory on `feature/phase-26` (see Acceptance target above).
