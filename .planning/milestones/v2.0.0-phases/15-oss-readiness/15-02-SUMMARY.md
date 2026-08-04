---
phase: 15-oss-readiness
plan: 02
subsystem: docs
tags: [documentation, architecture, rust, cli, gate-protocol]

# Dependency graph
requires:
  - phase: 15-oss-readiness (15-01)
    provides: README.md/SECURITY.md/DEPENDENCIES.md accuracy pass (no file overlap with this plan)
provides:
  - Accurate ARCHITECTURE.md tracing the real Stage enum, hooks, per-phase state + two-level locking, events.jsonl, gate protocol, and monitor ownership to source
  - Corrected docs/guides/quickstart.md (no init step, per-phase state, gate-driven Ship)
  - Corrected docs/guides/configuration.md (no config file, CLI flags + env-var tuning, links OPERATIONS.md)
affects: [16-hermes-support, future-contributor-onboarding]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Docs-as-source-of-truth: every architectural claim traced to a named source file/line before being written"

key-files:
  created: []
  modified:
    - ARCHITECTURE.md
    - docs/guides/quickstart.md
    - docs/guides/configuration.md

key-decisions:
  - "Corrected the Agent model and Completion evaluation sections beyond the plan's explicit read_first list — both had also drifted (Agent trait renamed to AgentAdapter in Phase 12-11; prompts are per-stage via prompt.rs::stage_prompt, not a single shared template; the Layer 2 commit gate is scoped to Plan/Code only, not all stages) — left as-is these would have reintroduced the same class of stale-doc bug this plan exists to fix"
  - "Documented that GitFlow::release_start/release_finish exist in git.rs but are not called from any production CLI path — the real Ship flow is gate-driven (devflow gate approve/reject --stage ship), not a release-branch-and-PR command"
  - "Left CONTRIBUTING.md's 'Adding a New Agent' section untouched (out of this plan's files_modified scope) despite discovering it already duplicates the extension checklist inline (not just an anchor pointer) and references the stale 'Agent' trait name — flagged as a pre-existing issue for a future plan, not fixed here"

requirements-completed: [15b]

coverage:
  - id: D1
    description: "ARCHITECTURE.md rewritten to describe the real 5-stage Stage enum, hooks, per-phase state + two-level locking, events.jsonl, gate protocol, and monitor ownership, each traced to source; dead 8-step machine and phantom .devflow.yaml/devflow init/confirm/rejectpr references removed"
    requirement: "15b"
    verification:
      - kind: other
        ref: "rg verification: 0 hits for dead-machine terms (Branching/Executing/Docsing/Shipping), 0 hits for .devflow.yaml/devflow init/rejectpr; >=1 hit each for events.jsonl, Define/Validate, state- naming, gate"
        status: pass
    human_judgment: false
  - id: D2
    description: "docs/guides/quickstart.md corrected to the real init-less entry flow and per-phase state"
    requirement: "15b"
    verification:
      - kind: other
        ref: "rg verification: 0 hits for devflow init/.devflow.yaml/state.json; >=1 hit for 'devflow start --phase'"
        status: pass
    human_judgment: false
  - id: D3
    description: "docs/guides/configuration.md rewritten to the no-config-file / CLI-flags + env-vars model, linking OPERATIONS.md"
    requirement: "15b"
    verification:
      - kind: other
        ref: "rg verification: 0 hits for .devflow.yaml; >=1 hit each for --mode, DEVFLOW_GATE_NOTIFY_CMD, OPERATIONS.md"
        status: pass
    human_judgment: false

duration: 40min
completed: 2026-07-17
status: complete
---

# Phase 15 Plan 02: ARCHITECTURE.md + guides rewrite Summary

**Full rewrite of ARCHITECTURE.md against current source (Stage enum, hooks, two-level locking, events.jsonl, gate protocol, monitor ownership), plus accuracy passes on both docs/guides files to remove `.devflow.yaml`/`devflow init`/`confirm`/`rejectpr` phantoms.**

## Performance

- **Duration:** ~40 min
- **Completed:** 2026-07-17T14:20:30Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- ARCHITECTURE.md now documents the real `Stage` enum (`crates/devflow-core/src/stage.rs`: Define → Plan → Code → Validate → Ship), with dedicated sections for hooks (`hooks.rs`), per-phase state + two-level locking (`workflow.rs`/`lock.rs`), the events log (`events.rs`), the gate protocol (`gates.rs`), and monitor ownership (`monitor.rs`) — each claim traced to a source file
- Configuration section replaced with the real no-config-file model (`devflow start` CLI flags); Git/ship model replaced with the gate-driven flow (`devflow gate approve <phase> --stage ship`)
- Extension-points/adding-an-agent checklist kept authoritative in ARCHITECTURE.md (only its trailing docs-file list updated); CONTRIBUTING.md's own copy left untouched (out of this plan's scope)
- `docs/guides/quickstart.md` no longer instructs readers to run a nonexistent `devflow init`; corrected to per-phase `state-NN.json` and the real gate-driven Ship flow
- `docs/guides/configuration.md` rewritten from a `.devflow.yaml` schema reference to the real CLI-flags + environment-variable model, pointing at OPERATIONS.md as the authoritative operator reference

## Task Commits

Each task was committed atomically:

1. **Task 1: ARCHITECTURE.md full rewrite against source** - `a0e3442` (docs)
2. **Task 2: docs/guides/quickstart.md accuracy pass** - `8c4a6a2` (docs)
3. **Task 3: docs/guides/configuration.md accuracy pass** - `4260dfb` (docs)

**Plan metadata:** committed alongside STATE.md/ROADMAP.md updates (see final commit)

## Files Created/Modified

- `ARCHITECTURE.md` - Full rewrite: Stage machine, hooks, per-phase state/locking, events log, gate protocol, monitor daemon, worktree model, gate-driven Git/ship model, no-config-file Configuration, corrected Agent model + Completion evaluation, updated Logging instrumentation list, extension-points checklist with corrected trait name
- `docs/guides/quickstart.md` - Removed the `devflow init`/`.devflow.yaml` step; corrected to per-phase state and gate-driven Ship
- `docs/guides/configuration.md` - Replaced the `.devflow.yaml` schema doc with the real CLI-flags + env-var model, linking OPERATIONS.md

## Decisions Made

- Extended the rewrite beyond ARCHITECTURE.md's explicitly-flagged stale sections (State machine, Configuration, Git/ship model) to also correct the Agent model and Completion evaluation sections, which PATTERNS.md had classified as "already accurate, reuse verbatim" but which direct source verification showed had also drifted (see key-decisions in frontmatter for specifics). Both are directly load-bearing for the "every architectural claim traced to source" threat mitigation this plan exists to satisfy — leaving them as-is would have shipped a doc that still lied about the codebase.
- Kept the extension-points checklist changes scoped to ARCHITECTURE.md only, per the plan's explicit prohibition against touching CONTRIBUTING.md's copy in this task.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected Agent model section (AgentAdapter trait, per-stage prompts)**
- **Found during:** Task 1 (ARCHITECTURE.md rewrite)
- **Issue:** The pre-existing "Agent model" section (classified "already accurate" in 15-PATTERNS.md) referenced a trait named `Agent` with a `phase_prompt(phase)` shared-template function. Source verification (`crates/devflow-core/src/agents/mod.rs`, `crates/devflow-core/src/prompt.rs`) showed the trait was renamed `AgentAdapter` in Phase 12-11, its methods now include `extra_writable_roots`/`extra_env()`, and prompts are built per-stage by `stage_prompt(stage, phase)` (idempotent Define/Plan, verdict-requiring Validate, review-gated Ship) rather than one shared instruction template.
- **Fix:** Rewrote the Agent model section against `agents/mod.rs` and `prompt.rs`.
- **Files modified:** ARCHITECTURE.md
- **Verification:** Cross-checked trait signature and all three per-stage prompt builders directly in source.
- **Committed in:** a0e3442 (Task 1 commit)

**2. [Rule 1 - Bug] Corrected Completion evaluation's Layer 2 commit-gate scoping**
- **Found during:** Task 1 (ARCHITECTURE.md rewrite)
- **Issue:** The pre-existing section implied the "zero commits → failed" gate applied to every stage's `exit == 0` case. `crates/devflow-core/src/agent_result.rs::evaluate_layer2` scopes that gate to `Stage::Plan`/`Stage::Code` only (via an explicit `matches!`, not `is_agent_stage()`, since that also covers `Define`, which legitimately produces zero commits) — Define/Validate/Ship succeed on `exit == 0` regardless of commit count.
- **Fix:** Rewrote the Layer 2 description with the stage-scoped decision matrix from source.
- **Files modified:** ARCHITECTURE.md
- **Verification:** Read `evaluate_layer2`'s full decision matrix and doc comment directly.
- **Committed in:** a0e3442 (Task 1 commit)

**3. [Rule 3 - Blocking] Plan's literal verify command incompatible with installed ripgrep version**
- **Found during:** Task 1 verification
- **Issue:** The plan's `<verify>` block uses `test "$(rg -c PATTERN FILE)" = "0"` to assert zero matches. This repo's ripgrep (15.2.0) prints nothing and exits 1 on zero matches (rather than printing `0`), so `$(...)` captures an empty string and the literal test can never pass, independent of file content.
- **Fix:** Verified the same intent directly — `rg -q` presence checks and manual review confirmed zero occurrences of every prohibited term (dead 8-step machine words, `.devflow.yaml`, `devflow init`, `rejectpr`) and required-presence of `events.jsonl`, `Define`, `state-` naming, and gate coverage. Also reworded three sentences that legitimately needed to explain what was removed (e.g. "no config file... no `devflow init`") so they don't contain the literal banned substrings themselves, satisfying both the letter and the spirit of the check.
- **Files modified:** ARCHITECTURE.md (wording only, no factual change)
- **Verification:** Re-ran all four acceptance-criteria greps individually with `-q`/`-n`; all pass.
- **Committed in:** a0e3442 (Task 1 commit)

**4. [Rule 3 - Blocking] Plan's `<verification>` cargo package name was wrong**
- **Found during:** end-of-plan verification
- **Issue:** The plan's `<verification>` block specifies `cargo test -p devflow-cli --test help_snapshot`. The CLI crate's Cargo package name is `devflow` (binary crate `devflow-cli/Cargo.toml` declares `name = "devflow"`), not `devflow-cli` (that's the directory name). `-p devflow-cli` fails with "package ID specification did not match any packages."
- **Fix:** Ran `cargo test -p devflow --test help_snapshot` instead — passes (1 test, `help_output_matches_committed_snapshot`), confirming no CLI source drifted during this docs-only plan.
- **Files modified:** None (verification-only)
- **Verification:** `cargo test -p devflow --test help_snapshot` → 1 passed, 0 failed
- **Committed in:** N/A (no file change required)

---

**Total deviations:** 4 auto-fixed (2 bug corrections beyond the plan's explicit read_first scope, 2 blocking-verification-tooling issues)
**Impact on plan:** All four were necessary either for ARCHITECTURE.md's core correctness contract (traced-to-source claims) or to confirm the plan's acceptance criteria were actually satisfied despite literal-script quirks in this environment. No scope creep — no CLI/core source was touched, only the three docs files named in `files_modified`.

## Issues Encountered

- CONTRIBUTING.md's "Adding a New Agent" section (`CONTRIBUTING.md:156-171`) was found to already duplicate the extension checklist inline (not merely an anchor pointer, contradicting 15-PATTERNS.md's characterization) and it references the stale `Agent` trait name. This plan's `files_modified` scope excludes CONTRIBUTING.md, so it was left untouched and is flagged here for a future cleanup pass rather than fixed opportunistically.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- ARCHITECTURE.md and both `docs/guides/*.md` files are now accurate against current source; no phantom `.devflow.yaml`/`devflow init`/`confirm`/`rejectpr` references remain in files this plan touched.
- Remaining known doc-drift, deliberately out of scope here: CONTRIBUTING.md's duplicated + stale extension checklist (flagged above).
- Ready for the remaining 15b waves (`.devcontainer/`, `LICENSE-APACHE`, crates.io publish prep).

---
*Phase: 15-oss-readiness*
*Completed: 2026-07-17*

## Self-Check: PASSED

- FOUND: ARCHITECTURE.md
- FOUND: docs/guides/quickstart.md
- FOUND: docs/guides/configuration.md
- FOUND: .planning/phases/15-oss-readiness/15-02-SUMMARY.md
- FOUND: a0e3442 (Task 1 commit)
- FOUND: 8c4a6a2 (Task 2 commit)
- FOUND: 4260dfb (Task 3 commit)
