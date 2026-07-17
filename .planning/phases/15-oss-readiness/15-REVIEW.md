---
phase: 15-oss-readiness
reviewed: 2026-07-17T17:34:21Z
depth: standard
files_reviewed: 10
files_reviewed_list:
  - .devcontainer/devcontainer.json
  - .github/workflows/devcontainer.yml
  - ARCHITECTURE.md
  - CONTRIBUTING.md
  - DEPENDENCIES.md
  - LICENSE-APACHE
  - README.md
  - SECURITY.md
  - docs/guides/configuration.md
  - docs/guides/quickstart.md
findings:
  critical: 0
  warning: 4
  info: 1
  total: 5
status: issues_found
---

# Phase 15: Code Review Report

**Reviewed:** 2026-07-17T17:34:21Z
**Depth:** standard
**Files Reviewed:** 10
**Status:** issues_found

## Summary

This phase's file set is entirely documentation, license, and CI/devcontainer
config — no application source. There is no security or crash risk in this
diff. However, several of the documentation files make claims that were
cross-checked against the actual `crates/` source and do **not** match it,
which directly undermines the stated goal of this phase (accurate OSS-facing
docs) and ARCHITECTURE.md's own instruction to "treat the named source files
as the source of truth if this drifts." All findings below are documentation
accuracy defects (Warning) plus one process-consistency note (Info); nothing
rises to Critical since no code, secrets, or executable logic is affected.

The `.devcontainer/devcontainer.json` base image tag
(`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`) was verified live
against the MCR tag list and does exist — no issue there. `LICENSE`,
`LICENSE-APACHE`, `OPERATIONS.md`, `scripts/install.sh`, and
`.github/workflows/ci.yml` (all referenced by these docs) exist and are
consistent with what's described. `--phase`, `--agent`, `--mode`, `--force`,
`--no-worktree`, `--dry-run`, and the `gate approve/reject` flag surfaces all
match the current `Command`/`GateCmd` enums in `main.rs`.

## Warnings

### WR-01: README.md and CONTRIBUTING.md name a nonexistent `Agent` trait — actual trait is `AgentAdapter`

**File:** `README.md:62`, `CONTRIBUTING.md:178`
**Issue:** README states "Claude, Codex, and OpenCode all implement the same
`Agent` trait" and CONTRIBUTING.md's "Adding a New Agent" checklist says to
implement "the `Agent` trait." Neither trait exists in the codebase — the
actual trait, defined in `crates/devflow-core/src/agents/mod.rs:11`, is
`AgentAdapter` (`pub trait AgentAdapter { ... }`). ARCHITECTURE.md correctly
names `AgentAdapter` in its own "Extension points" section
(ARCHITECTURE.md:395-396), so these two files have drifted from both the
source and from ARCHITECTURE.md itself. A contributor following either
checklist verbatim (`impl Agent for ...`) will get a compile error.
**Fix:**
```diff
- Agent-agnostic — Claude, Codex, and OpenCode all implement the same `Agent` trait.
+ Agent-agnostic — Claude, Codex, and OpenCode all implement the same `AgentAdapter` trait.
```
```diff
- 1. Add an adapter file in `crates/devflow-core/src/agents/` implementing the `Agent` trait
+ 1. Add an adapter file in `crates/devflow-core/src/agents/` implementing the `AgentAdapter` trait
```

### WR-02: README.md's "Shared prompts" claim contradicts the actual per-stage prompt implementation and cites a nonexistent function

**File:** `README.md:66`, `CONTRIBUTING.md:67`
**Issue:** README says: "Shared prompts — all agents receive the same prompt
via `phase_prompt()`. No agent-specific prompt logic." CONTRIBUTING.md
similarly says "The launch prompt (`agents::phase_prompt()`) reads
`.planning/ROADMAP.md` and `.planning/phases/NN-*/CONTEXT.md`."

Neither is accurate:
- There is no `phase_prompt()` function anywhere in the codebase (verified
  via `rg -n "fn phase_prompt|fn stage_prompt"` — only `stage_prompt` exists,
  in `crates/devflow-core/src/prompt.rs:148`, not in the `agents` module).
- The actual behavior is the opposite of "no agent-specific prompt logic":
  ARCHITECTURE.md (the doc that IS accurate here) explicitly documents that
  `stage_prompt(stage, phase)` builds **per-stage, not shared** prompts, and
  that Define/Plan, Validate, and Ship each get dedicated logic (idempotent
  re-run check, `verdict` field requirement, and the code-review gate before
  `/gsd-ship`, respectively) — see ARCHITECTURE.md:101-118.

This is a direct self-contradiction between README/CONTRIBUTING and
ARCHITECTURE.md within the same reviewed doc set, and it references a
function name that will not be found by anyone grepping the codebase.
**Fix:**
```diff
- **Shared prompts** — all agents receive the same prompt via `phase_prompt()`. No agent-specific prompt logic.
+ **Per-stage prompts** — `stage_prompt(stage, phase)` builds a dedicated prompt per pipeline stage (Define/Plan, Validate, Ship each have distinct completion contracts); all *agents* share the same prompt for a given stage.
```
```diff
- The launch prompt (`agents::phase_prompt()`) reads `.planning/ROADMAP.md` and
+ The launch prompt (`prompt::stage_prompt()`) reads `.planning/ROADMAP.md` and
```

### WR-03: README.md's rate-limit/cron-instructions description is stale — wrong filename and wrong scope

**File:** `README.md:86`
**Issue:** README states, under the general "Agent Protocol" section (which
describes `devflow start`'s evaluation layers): "Rate-limit detection: if an
agent's stdout contains rate-limit messages (429), DevFlow writes
`.devflow/cron-instructions.json` for rescheduling."

Two problems, both confirmed against `crates/devflow-core/src/ship.rs` and
`crates/devflow-cli/src/main.rs`:
1. **Wrong filename.** The current, per-phase record is written to
   `.devflow/cron-instructions-{phase:02}.json`
   (`ship.rs::cron_instructions_path`, and asserted by
   `phase7_cli.rs:502` — `"wrote .devflow/cron-instructions-07.json"`). The
   flat `.devflow/cron-instructions.json` name is explicitly the **legacy**
   single-slot path (`ship.rs:63-65`: "Per-phase since 14a
   (13-DEFERRED-CR-03): the old single-slot `cron-instructions.json` let one
   phase's rate-limit record clobber another's under `devflow parallel`"),
   kept only as an upgrade-path fallback, not the primary output.
2. **Wrong scope.** This behavior is wired into the `sequentagent` command
   specifically (`write_rate_limit_cron` is called from
   `main.rs:1483`, inside `run_sequentagent`), not into the generic
   `devflow start`/monitor path the surrounding README section describes.
   Placed where it is, the sentence implies any `devflow start` run detects
   429s and writes this file, which is not what the code does.
**Fix:**
```diff
- Rate-limit detection: if an agent's stdout contains rate-limit messages (429), DevFlow writes `.devflow/cron-instructions.json` for rescheduling.
+ Rate-limit detection (`sequentagent` only): if agent A's stdout contains rate-limit messages (429), DevFlow writes `.devflow/cron-instructions-NN.json` (per-phase) for a Hermes cron poller to resume the handoff later.
```

### WR-04: CONTRIBUTING.md's "Commit Conventions" section is contradicted by the project's own current git history

**File:** `CONTRIBUTING.md:110-115`
**Issue:** The doc states: "**Deprecated — June 2026.** The conventional
commits model is being replaced by a per-phase branching and merge scheme
(Phase 11). Until then, write descriptive commit messages that explain what
changed and why, without a required prefix format."

Today's date is 2026-07-17 — after the claimed June 2026 deprecation — yet
the repository's own recent commit history (including every commit in this
same phase 15 branch) still consistently uses conventional-commit-style
`type(scope): subject` prefixes:
```
docs(phase-15): re-confirm validation strategy (0 new gaps)
feat(15-04): add canonical LICENSE-APACHE for dual license
ci(15-03): add devcontainer.yml for container-parity CI test
fix: address Codex review — clippy, license dual, bug template, ARCHITECTURE link
```
A new contributor reading this section will be told the prefix format is
deprecated and unnecessary, while the actual, current project convention
(as demonstrated by every commit merging into this branch) still uses it.
This is either a stale doc that should be updated/removed, or the stated
deprecation was never actually enforced — either way it is misleading as
written.
**Fix:** Either update the section to reflect that conventional-commit
prefixes remain the de facto convention (drop or correct the deprecation
notice), or, if the intent genuinely is to move away from prefixes, actually
adopt that going forward and note the discrepancy in the phase's plan.

## Info

### IN-01: `.github/workflows/devcontainer.yml` duplicates CI's build/test/clippy jobs without running `cargo fmt --check`

**File:** `.github/workflows/devcontainer.yml:19-25`
**Issue:** This workflow runs on every push/PR to `main`/`develop` (same
triggers as `ci.yml`) and re-runs `cargo build --workspace`, `cargo test
--workspace`, and `cargo clippy --workspace -- -D warnings` inside the
devcontainer for "CI-parity checks," but omits `cargo fmt --check`, which
`ci.yml` and CONTRIBUTING.md's "Required checks" list both treat as a
required gate. Not a correctness bug (this workflow is presumably meant as a
devcontainer build smoke test, not a substitute for `ci.yml`), but the
"CI-parity" framing in the step name is one check short of actual parity.
**Fix:** Either rename the step to something less than "CI-parity" (e.g.
"devcontainer smoke test") or add `cargo fmt --check` to `runCmd` for true
parity with `ci.yml`'s three required jobs.

---

_Reviewed: 2026-07-17T17:34:21Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
