---
phase: 39-pi-end-to-end
plan: 39
subsystem: agents
tags: [pi, agent-driver, provider-auth, subagent-dispatch, devflow]

# Dependency graph
requires:
  - phase: 38-driver-contract-completion
    provides: AgentDriver trait, driver-driven InteractivityMode gate
  - phase: 37.1-pi-subagent-extension-spike
    provides: VIABLE verdict for @bacnh85/pi-subagent (in-process, synchronous)
provides:
  - PiDriver health that probes the provider a launch actually uses (settings.json defaultProvider)
  - Pi pinned to MonitorLaunch::Legacy (regression test with a real precondition)
  - Capability detection matching only the vetted @bacnh85/pi-subagent package
affects: [verify-work, validate-phase]

# Actuals (#2632)
actuals:
  tokens: 5300
  tasks: 6
  commits: 5

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "AgentDriver::health reads the agent's own settings.json for the active provider (not models.json, not a hardcoded provider)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agents/pi.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/commands.rs
    - docs/guides/pi-subagent-dispatch.md
    - ARCHITECTURE.md

key-decisions:
  - "health probes settings.json defaultProvider, falling back to Pi's --provider default (google); never any-ready models.json provider"
  - "Capability detection matches the vetted @bacnh85/pi-subagent name, not a *subagent* substring (unsafe/deferred packages excluded)"
  - "subagent_dispatch is reported-only (devflow doctor); no routing/launch consumer yet"

patterns-established:
  - "Provider-aware health: probe what build_command will use, not a catalog of what could work"

requirements-completed: []

# Coverage metadata (#1602)
coverage:
  - id: D1
    description: "PiDriver health probes the active provider (settings.json defaultProvider) with google fallback; no models.json hard-refuse, no any-ready false-green"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/pi.rs#preflight_invokes_pi_auth_check_and_accepts_ready"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/pi.rs#preflight_falls_back_to_google_when_no_default_provider"
        status: pass
    human_judgment: false
  - id: D2
    description: "Pi resolves to MonitorLaunch::Legacy in resolve_launch_shape (never PipeOwning), asserted with a real claude_stream_launch_enabled precondition"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pi_resolves_to_legacy_launch"
        status: pass
    human_judgment: false
  - id: D3
    description: "Capability detection matches the vetted @bacnh85/pi-subagent and excludes unsafe/deferred subagent-named packages"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/pi.rs#pi_capabilities_exclude_unvetted_subagent_packages"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#pi_subagent_dispatch_check_renders_both_arms"
        status: pass
    human_judgment: false
  - id: D4
    description: "Stage-2 subagent dispatch completes under Legacy + DEVFLOW_RESULT (live e2e run with @bacnh85/pi-subagent)"
    verification: []
    human_judgment: true
    rationale: "The in-repo e2e evidence is a bash side-effect proxy (not a captured session transcript showing toolCall: subagent), and the smoke ran on deepseek/openrouter rather than litellm. A re-run that captures the session transcript is required before this deliverable can pass."

# Metrics
duration: ~2h (incl. adversarial review + FIX-FIRST close)
completed: 2026-08-18
status: complete
---

# Phase 39: Pi End-to-End Summary

**Pi driver health reads the active provider from settings.json (google fallback), Pi pinned to Legacy launch with a discriminating regression test, and capability detection matching only the vetted @bacnh85/pi-subagent — with a FIX-FIRST adversarial review closed.**

## Performance

- **Duration:** ~2h across two sessions (execution + adversarial review + fix close)
- **Tasks:** 6 (T-39-01 … T-39-06, both waves)
- **Files modified:** 9

## Accomplishments

- Stage 1 baseline: `PiDriver::health` now probes `settings.json`'s `defaultProvider` (this machine: `litellm`) instead of a hardcoded `google`; the first cut (read `models.json`) was rejected by adversarial review for false-rejecting standard installs and false-greening any-ready providers, and was rewritten.
- Pi is pinned to `MonitorLaunch::Legacy` with a regression test whose precondition asserts `!claude_stream_launch_enabled(Pi, …)` so the test discriminates a broken predicate rather than re-asserting its own input.
- Stage 2 dispatch: capability detection matches the vetted `@bacnh85/pi-subagent` package specifically (not `*subagent*`), so the unsafe/deferred `@mystilleef`/`@dreki-gg`/`@smoose` are not reported available.
- `subagent_dispatch` is surfaced as **reported-only** in `devflow doctor`; docs were corrected from "routing/two arms" to "no consumer yet".

## Task Commits

Tasks were bundled into two feature commits plus a review-fix commit (not one commit per task):

1. **T-39-01..03 (Stage 1: provider fix + Legacy pin + completion detection)** — `b91b37b` (feat)
2. **T-39-04..06 (Stage 2: capability detection + e2e smoke)** — `e4f0bb6` (feat)
3. **FIX-FIRST review close (provider rewrite + detection fix + test hardening)** — `66e5c4a` (fix)
4. **Review record + doc fixes** — `04eaa0f`, `b2e5b0c` (docs)

## Files Created/Modified

- `crates/devflow-core/src/agents/pi.rs` — provider-aware health, vetted-package detection, 13 unit tests
- `crates/devflow-cli/src/pipeline_launch.rs` — `pi_resolves_to_legacy_launch` precondition
- `crates/devflow-cli/src/commands.rs` — extracted `pi_subagent_dispatch_check` + unit test
- `docs/guides/pi-subagent-dispatch.md` — "reported only, no consumer yet"
- `ARCHITECTURE.md` — same correction
- `.planning/phases/39-pi-end-to-end/39-{CONTEXT,PLAN,E2E-SMOKE}.md` — re-scope + evidence correction

## Decisions Made

- Provider probing targets the single provider `build_command` will actually use (`defaultProvider`), with a `google` fallback — not "any ready `models.json` provider" (the false-green vector) and not a hard-refuse when `models.json` is absent (the false-reject vector).
- Capability detection is name-based on the exact `@bacnh85/pi-subagent` package; a bare `*subagent*` substring match would admit packages the phase itself ruled unsafe.
- `subagent_dispatch` is honest as diagnostic-only: nothing in launch/prompt/advance consumes it yet.

## Deviations from Plan

- **The provider fix's first implementation was wrong.** The plan's open question ("read `models.json` or drop `--provider`") was resolved to `models.json`, which the adversarial review proved both false-rejects standard installs (no `models.json`) and false-greens (any ready provider). Rewritten to read `settings.json`'s `defaultProvider`. This is the finding-1 BLOCKER, now closed in `66e5c4a`.
- **The e2e smoke's evidence is a proxy, not proof.** The recorded dispatch evidence is a bash side-effect file, which the parent's own `bash` tool could produce without invoking `subagent`. The discriminating session transcript (`toolCall: subagent`) was observed at review time but not captured into the repo, and the smoke ran on `deepseek`/`openrouter` rather than `litellm`.

## Issues Encountered

- **Stage-2 e2e acceptance is NOT met.** See coverage D4: the transcript is not captured in-repo and the smoke exercised the wrong provider. A re-run against a `litellm` profile (or a corrected provider claim) with the transcript committed is required before verify/close.
- The FIX-FIRST review (claude/codex/antigravity) surfaced five convergent findings; all five are closed, with the e2e-evidence one reduced to a documented follow-up rather than a code fix.

## Next Phase Readiness

- Ready for verify-work: deliverables D1–D3 auto-pass (unit tests green, 13 `agents::pi` tests + full workspace + clippy `-D warnings` clean); D4 routes to a human for the e2e re-run.
- Blocker for phase close: the Stage-2 e2e re-run with a captured `toolCall: subagent` transcript (see D4).

---
*Phase: 39-pi-end-to-end*
*Completed: 2026-08-18*
