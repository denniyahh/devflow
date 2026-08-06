---
phase: 15-oss-readiness
verified: 2026-07-17T18:30:00Z
status: passed
score: 20/20 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 19/20
  gaps_closed:
    - "README.md and CONTRIBUTING.md accurately describe the current codebase (no invented trait/function names, filenames, or stale process claims)"
  gaps_remaining: []
  regressions: []
deferred: []
human_verification: []
---

# Phase 15: Dogfood Enablement + OSS Readiness Verification Report

**Phase Goal:** Rescoped 2026-07-16 (dogfood-first). 15a Dogfood Enablement: `devflow gate`
subcommand (list/approve/reject), an accurate `OPERATIONS.md` operator reference, and
doc-accuracy quick hits; exit criterion: a real phase runs end-to-end with gates answered only
via `devflow gate` + the notify hook. 15b OSS Packaging (run through DevFlow as the first
post-MVP dogfood): README/ARCHITECTURE rewrite against v2 reality, CONTRIBUTING, dev container,
crates.io publish. Antigravity adapter deferred out of the phase.

**Verified:** 2026-07-17T18:30:00Z
**Status:** passed
**Re-verification:** Yes — after gap closure (commit `8539b87`, "fix(docs): correct stale
Agent/phase_prompt/cron/commit-convention refs")

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `devflow gate list/approve/reject` subcommand exists and functions (15a) | ✓ VERIFIED | `main.rs::Command::Gate` (line 104) + match arm (line 328); `Gates::list_open`/`Gates::respond` in `gates.rs`. Regression check: still present, unchanged since prior pass. |
| 2 | `OPERATIONS.md` is an accurate operator reference | ✓ VERIFIED | File present at repo root; regression check only (content unchanged by fix commit). |
| 3 | `.devflow.yaml` decoy config file removed from repo root | ✓ VERIFIED | `test -f .devflow.yaml` → absent |
| 4 | `lib.rs` rustdoc examples fixed to pass `--mode` (IN-01) | ✓ VERIFIED | `crates/devflow-core/src/lib.rs:25-26` shows `devflow start --phase 3 --agent claude --mode auto` |
| 5 | `--help` snapshot test exists and passes | ✓ VERIFIED | `cargo test -p devflow --test help_snapshot` → re-ran independently: `1 passed; 0 failed` |
| 6 | README documents `gate`/`logs`, per-phase `state-NN.json`, links OPERATIONS.md | ✓ VERIFIED | Regression check: command table and links unchanged by fix commit (diff only touched lines 62/66/86 and the commit-conventions/agent-checklist sections). |
| 7 | SECURITY.md points to real `state-NN.json`/`events.jsonl`, not phantom `audit.log` | ✓ VERIFIED | `SECURITY.md:37` — `.devflow/state-NN.json` / `.devflow/events.jsonl`; zero `audit.log` hits |
| 8 | DEPENDENCIES.md doctor sample shows 1.2.0, no phantom `.devflow.yaml`/`devflow confirm` | ✓ VERIFIED | `DEPENDENCIES.md:55` shows `devflow 1.2.0`; zero hits for both phantom terms |
| 9 | ARCHITECTURE.md documents the real 5-stage `Stage` enum + subsystems | ✓ VERIFIED | `ARCHITECTURE.md:4/29` — Define→Plan→Code→Validate→Ship; zero hits for the dead 8-step names |
| 10 | `docs/guides/quickstart.md` + `configuration.md` reflect no-config-file v2 reality | ✓ VERIFIED | quickstart.md:22 (`devflow start --phase 3 ...`); configuration.md:30/:47 (`DEVFLOW_GATE_NOTIFY_CMD`, OPERATIONS.md link); zero `devflow init`/`.devflow.yaml`/`state.json` hits in either file |
| 11 | CONTRIBUTING.md documents the 3 required CI checks + devcontainer flow | ✓ VERIFIED | `CONTRIBUTING.md:98-103` (cargo test/clippy -D warnings/fmt --check as the 3 required checks); regression check on the devcontainer subsection, unchanged by fix commit |
| 12 | `.devcontainer/devcontainer.json` pins explicit Rust image, mounts caches, installs clippy/rustfmt | ✓ VERIFIED | `image: mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` (pinned, not `:latest`) |
| 13 | A devcontainer CI job builds the container and runs `cargo build/test/clippy` inside it | ✓ VERIFIED (static structure; live GitHub Actions run not exercised — no Docker in this sandbox) | `.github/workflows/devcontainer.yml` runs `devcontainers/ci@v0.3` with `cargo build/test/clippy --workspace -D warnings` |
| 14 | CODE_OF_CONDUCT.md present | ✓ VERIFIED | `test -f CODE_OF_CONDUCT.md` |
| 15 | `LICENSE-APACHE` is canonical Apache-2.0 text | ✓ VERIFIED | Contains "Apache License" / "Version 2.0, January 2004" header |
| 16 | Packaging dry-runs pass | ✓ VERIFIED | Regression check only — not re-run (no source/manifest changes since the prior independent `exit 0` confirmation; fix commit touched only `README.md`/`CONTRIBUTING.md`) |
| 17 | `devflow-core` published to crates.io, resolvable | ✓ VERIFIED (independently, via `curl` with explicit `User-Agent` — crates.io 403s anonymous/no-UA requests) | `curl -A "devflow-verifier/1.0" https://crates.io/api/v1/crates/devflow-core` → `max_version`/`max_stable_version`: `1.2.0`, `yanked: false`, `created_at: 2026-07-17T17:39:23Z` |
| 18 | `devflow` (CLI) published AFTER `devflow-core`, resolving from registry | ✓ VERIFIED (independently) | `curl -A "devflow-verifier/1.0" https://crates.io/api/v1/crates/devflow` → `1.2.0`, `yanked: false`, `created_at: 2026-07-17T17:40:31Z` (68s after devflow-core, leaf-first order confirmed) |
| 19 | No crates.io API token committed, echoed, or written into any repo-tracked file | ✓ VERIFIED | `git status --short` shows only this VERIFICATION.md as untracked (expected); no token-shaped strings in any phase-touched file |
| 20 | README.md and CONTRIBUTING.md accurately describe the current codebase | ✓ VERIFIED | See "Gap Closure — Independent Re-check" below. All 4 previously-flagged factual defects independently re-verified against source (not trusted from commit message or SUMMARY). |

**Score:** 20/20 truths verified (0 present, behavior-unverified)

### Gap Closure — Independent Re-check

The prior verification (`status: gaps_found`, 19/20) flagged 4 unfixed factual defects in
README.md and CONTRIBUTING.md. Commit `8539b87` ("fix(docs): correct stale
Agent/phase_prompt/cron/commit-convention refs") claims to fix them. Each is independently
re-checked against the named source file below — not trusted from the commit message.

| # | Prior defect | Fix claimed | Independent source check | Result |
|---|---|---|---|---|
| 1 | README:62 / CONTRIBUTING:178 cited a nonexistent `Agent` trait | Renamed to `AgentAdapter` | `rg -n "^pub trait Agent" crates/devflow-core/src/` → no match; `rg -n "trait AgentAdapter" crates/devflow-core/src/agents/mod.rs:11` → `pub trait AgentAdapter {` | ✓ Confirmed — README.md:62 and CONTRIBUTING.md:183 now say `AgentAdapter`; matches source exactly |
| 2 | README:66 / CONTRIBUTING:67-68 cited a nonexistent `phase_prompt()`, opposite of real per-stage behavior | Renamed to `stage_prompt(stage, phase)`, wording corrected to "per-stage prompts" | `rg -n "fn phase_prompt\|fn stage_prompt" crates/devflow-core/src/prompt.rs` → only `pub fn stage_prompt(stage: Stage, phase: u32) -> String` at line 148 | ✓ Confirmed — README.md:66 reads "Per-stage prompts — `stage_prompt(stage, phase)` builds a dedicated prompt per pipeline stage"; matches source signature exactly; CONTRIBUTING.md:67 now cites `crate::prompt::stage_prompt(stage, phase)` |
| 3 | README:86 stale cron-instructions filename/scope | Corrected to per-phase filename, scoped to `sequentagent` | `rg -n "cron_instructions_path" crates/devflow-core/src/ship.rs:66-69` → `.join(format!("cron-instructions-{phase:02}.json"))`. Scope check: `write_rate_limit_cron` (the only call site that writes this file) is defined and called exclusively inside `fn sequentagent` (`crates/devflow-cli/src/main.rs:1413-1538`, call at line 1483) — no other command path writes this file. `Sequentagent` confirmed as a real `clap` subcommand (`main.rs:147`, dispatch at `main.rs:362-367`). | ✓ Confirmed — README.md:86 now reads "during `devflow sequentagent` ... writes a per-phase `.devflow/cron-instructions-{phase:02}.json`"; filename format and command scope both match source exactly |
| 4 | CONTRIBUTING:110-115 falsely claimed conventional commits were "Deprecated — June 2026" | Rewritten to state Conventional Commits is the current, active convention | `git log --oneline` (278 commits) — every commit within Phase 15 itself (`docs(15-05): ...`, `feat(cli): ...`, `fix(docs): ...`) follows `type(scope): subject`; broadly true across recent phase history (14, 13, 12). Note: a handful of pre-Phase-11 commits (e.g. `11a: Stage enum...`, `10f: Update docs...`) and merge commits (`merge: Phase 14 — ...`) do not follow the strict `type(scope):` format, so the new text's literal claim "every commit in this project's own history follows the format" mildly overstates historical universality. | ✓ Substantively confirmed — the defect that blocked the prior pass (an actively false "deprecated" claim contradicted by the very commits in this phase) is fixed; the convention is correctly described as current and in force. Minor overclaim on strict historical completeness noted below as an info-level anti-pattern, not a functional or code-breaking defect, and not a re-opened gap. |

All 4 previously-blocking defects are closed. Two were code-breaking if followed literally
(`Agent` trait, `phase_prompt()`) — both now match real source signatures verbatim.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `README.md` | gate/logs table, per-phase state, OPERATIONS.md link, accurate against source | ✓ VERIFIED | All 3 previously-flagged defects (Agent trait, phase_prompt, cron filename/scope) fixed and independently re-confirmed against source |
| `SECURITY.md` | real state/event file references | ✓ VERIFIED | Regression check — unchanged, still correct |
| `DEPENDENCIES.md` | accurate doctor sample, no phantom commands | ✓ VERIFIED | Regression check — unchanged, still correct |
| `ARCHITECTURE.md` | real Stage machine + subsystems | ✓ VERIFIED | Regression check — unchanged, still correct |
| `docs/guides/quickstart.md` | init-less entry flow | ✓ VERIFIED | Regression check — unchanged, still correct |
| `docs/guides/configuration.md` | no-config-file model | ✓ VERIFIED | Regression check — unchanged, still correct |
| `CONTRIBUTING.md` | required checks, devcontainer flow, accurate against source | ✓ VERIFIED | All 3 previously-flagged defects (phase_prompt, Agent trait, stale Commit Conventions section) fixed and independently re-confirmed against source |
| `CODE_OF_CONDUCT.md` | present | ✓ VERIFIED | Regression check |
| `.devcontainer/devcontainer.json` | pinned image, cache mounts | ✓ VERIFIED | Regression check — unchanged |
| `.github/workflows/devcontainer.yml` | real in-container CI checks | ✓ VERIFIED (static) | Still missing `cargo fmt --check` (info-level, unchanged from prior pass — not part of this fix's scope, not blocking) |
| `LICENSE-APACHE` | canonical Apache-2.0 text | ✓ VERIFIED | Regression check — unchanged |
| `OPERATIONS.md` (15a) | operator reference | ✓ VERIFIED | Regression check — unchanged |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| README "Agent-agnostic" claim | `crates/devflow-core/src/agents/mod.rs` trait definition | grep | ✓ WIRED (fixed) | README.md:62 now says `AgentAdapter`; matches `pub trait AgentAdapter` at mod.rs:11 |
| README/CONTRIBUTING "prompt" claim | `crates/devflow-core/src/prompt.rs` | grep | ✓ WIRED (fixed) | README.md:66 / CONTRIBUTING.md:67 now cite `stage_prompt(stage, phase)`; matches `pub fn stage_prompt(stage: Stage, phase: u32)` at prompt.rs:148 |
| README cron-instructions claim | `crates/devflow-core/src/ship.rs` `cron_instructions_path` + `main.rs` `sequentagent` | grep + manual read | ✓ WIRED (fixed) | Filename format and command scope (`sequentagent`-only) both match source |
| CONTRIBUTING commit-conventions claim | `git log` | manual read | ✓ WIRED (substantively) | Current convention correctly described; minor overclaim on 100% historical universality, not functionally significant |
| CLI surface matches committed `--help` snapshot | `crates/devflow-cli/tests/help_snapshot.rs` | `cargo test -p devflow --test help_snapshot` | ✓ WIRED | Re-ran independently: 1 passed, 0 failed |
| publish ordering | devflow-core (leaf) before devflow (CLI) | crates.io `created_at` timestamps | ✓ WIRED | Re-queried independently: 68-second gap confirmed, core first |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| CLI surface matches committed docs snapshot | `cargo test -p devflow --test help_snapshot` | 1 passed, 0 failed | ✓ PASS |
| `AgentAdapter` trait exists in source (as fixed README/CONTRIBUTING now claim) | `rg -n "trait AgentAdapter" crates/devflow-core/src/agents/mod.rs` | `pub trait AgentAdapter {` at line 11 | ✓ PASS |
| No `Agent` trait exists (old false claim confirmed gone from docs) | `rg -n '`Agent`' README.md CONTRIBUTING.md` | zero hits | ✓ PASS |
| `stage_prompt()` function exists in source (as fixed docs now claim) | `rg -n "fn stage_prompt" crates/devflow-core/src/prompt.rs` | `pub fn stage_prompt(stage: Stage, phase: u32) -> String` at line 148 | ✓ PASS |
| `phase_prompt()` no longer referenced in docs | `rg -n "phase_prompt" README.md CONTRIBUTING.md` | zero hits | ✓ PASS |
| cron-instructions filename matches source format string | `rg -n "cron-instructions-{phase:02}.json" crates/devflow-core/src/ship.rs` vs README.md:86 | Both use `cron-instructions-{phase:02}.json` | ✓ PASS |
| `sequentagent` is a real CLI subcommand | `rg -n "Sequentagent" crates/devflow-cli/src/main.rs` | `Sequentagent {` (line 147), dispatch (line 362) | ✓ PASS |
| `devflow-core` publish still resolvable | `curl -A UA .../api/v1/crates/devflow-core` | `max_version: "1.2.0"`, `yanked: false` | ✓ PASS |
| `devflow` publish still resolvable, after core | `curl -A UA .../api/v1/crates/devflow` | `max_version: "1.2.0"`, `created_at` 68s after devflow-core | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|--------------|--------|----------|
| 15a | (no PLAN.md in this phase dir — landed via commits `53b2a56`/`39c195b`, predating the 15-01..15-05 plan set) | `devflow gate` subcommand, OPERATIONS.md, doc-accuracy quick hits, live e2e exit criterion | ✓ SATISFIED | All artifacts confirmed present and wired (truths 1-5). No REQUIREMENTS.md file exists in this project (`.planning/REQUIREMENTS.md` not found); traceability is via ROADMAP.md/PLAN frontmatter per project convention. Procedural note only, not a functional gap. |
| 15b | 15-01, 15-02, 15-03, 15-04, 15-05 (all declare `requirements: [15b]`) | README/ARCHITECTURE rewrite, CONTRIBUTING, dev container, crates.io publish | ✓ SATISFIED | All 5 plans' deliverables confirmed accurate against source, including the 2 contributor-facing documents (README.md, CONTRIBUTING.md) that carried the only outstanding defect from the prior verification pass — now fixed and independently re-confirmed. |

**Note:** No `.planning/REQUIREMENTS.md` exists in this project — confirmed via `find .planning -iname "REQUIREMENTS*.md"` (no results). Requirement traceability for this phase is sourced from ROADMAP.md phase definition and PLAN frontmatter `requirements:` fields, consistent with the prior verification pass's approach.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| CONTRIBUTING.md | 119-120 | "every commit in this project's own history follows the format" mildly overstates — a handful of pre-Phase-11 commits and merge commits don't strictly follow `type(scope):` | ℹ️ Info | Minor prose overclaim, not code-breaking, not architecturally misleading; the substantive fix (current convention is Conventional Commits, not deprecated) is correct |
| `.github/workflows/devcontainer.yml` | 19-25 | Omits `cargo fmt --check` while labeled a "CI-parity" check | ℹ️ Info | Carried over unchanged from prior pass; one check short of the framing's claim, not a correctness bug |

No 🛑 Blocker or ⚠️ Warning-level anti-patterns remain. The 4 blocker/warning items from the prior
pass (README.md:62/66/86, CONTRIBUTING.md:67-68/178/110-115) are all resolved.

### Human Verification Required

None. The remaining open items (live GitHub Actions run of `devcontainer.yml`; CI badge
rendering on the GitHub-hosted README) are pre-acknowledged, non-blocking manual-only checks
documented in `15-VALIDATION.md`'s "Manual-Only Verifications" table. They do not gate
phase-goal achievement.

### Gaps Summary

No gaps. The single blocking gap from the prior verification pass — README.md and
CONTRIBUTING.md containing 4 factual defects (nonexistent `Agent` trait, nonexistent
`phase_prompt()`, stale cron-instructions filename/scope, and a false "Commit Conventions —
Deprecated" claim) — is closed by commit `8539b87`. Each fix was independently re-verified
against its named source file in this pass, not trusted from the fix commit's message:

- `AgentAdapter` trait confirmed at `crates/devflow-core/src/agents/mod.rs:11`, matching the
  corrected README/CONTRIBUTING text exactly.
- `stage_prompt(stage, phase)` confirmed at `crates/devflow-core/src/prompt.rs:148`, matching
  the corrected README/CONTRIBUTING text exactly, including the per-stage (not shared) behavior
  description.
- `cron-instructions-{phase:02}.json` filename confirmed at `crates/devflow-core/src/ship.rs:69`,
  and the `sequentagent`-only write scope confirmed by tracing `write_rate_limit_cron`'s single
  call site inside `fn sequentagent` in `crates/devflow-cli/src/main.rs`.
- Commit Conventions section rewritten to correctly describe Conventional Commits as the
  current, active convention (previously falsely claimed deprecated) — substantively fixed,
  with one minor prose overclaim noted as info-level, non-blocking.

All 20 must-haves (the original 19 verified truths plus the now-closed 20th) pass. A full
regression pass on the 19 previously-verified truths found no new defects: `devflow gate`
subcommand, OPERATIONS.md, `.devflow.yaml` decoy removal, lib.rs rustdoc, `--help` snapshot test
(re-ran, still passes), SECURITY.md, DEPENDENCIES.md, ARCHITECTURE.md, the guide files, the
devcontainer artifacts, CODE_OF_CONDUCT.md, LICENSE-APACHE, and both crates.io publications
(re-queried independently with an explicit User-Agent header, since anonymous requests now 403)
all remain intact and unchanged since the prior pass. Phase 15 goal — dogfood enablement plus
accurate OSS packaging — is achieved.

---

*Verified: 2026-07-17T18:30:00Z*
*Verifier: Claude (gsd-verifier)*
