# Phase 40: Pi Dogfood - Context

**Gathered:** 2026-08-18
**Status:** Ready for planning

## Phase Boundary

Phase 40 proves the shipped Pi driver (`devflow start --agent pi`, delivered in v2.7.0 Phase 39)
survives a real run. It drives one real phase through Define→Plan→Code→Validate in **supervise
mode** so at least one live human gate fires and is honored. The Code-stage subject is **999.85**
(the two protected comments whose stated mechanisms Phase 34 deleted) — a small, real, comment-only
change that exercises the Code→Validate loop without touching the Pi driver itself.

Separately, the phase adds a small set of **Pi-transport regression tests** (stubbed `pi` binary) to
close any failure-mode gaps not already covered by the generic Layer 1/2/3 machinery.

## Implementation Decisions

### Dogfood subject
- **D-01: The Code-stage subject is 999.85 (MAINT-01).** Rewrite the two protected comments in
  `agent_result.rs` — `idle_timeout_result` (:1746-1750) and the test
  `stream_success_cannot_stand_against_nonzero_exit_code` (:6412-6417) — to cite the two structural
  defenses that now carry the `verdict: None` invariant (the enumerated status position from 34-03;
  the graft's status filter from 34-01), keeping the `verdict: None` instruction intact and
  unweakened. No production change — comment rewrites only.
- **D-02: MAINT-01 moves from Phase 45 → Phase 40.** 999.85 is now satisfied by this phase; Phase 45
  holds only DECN-01 (999.94). ROADMAP.md + REQUIREMENTS.md traceability updated to match.

### Run depth
- **D-03: Run through Validate only** — Define→Plan→Code→Validate. No Ship: Ship's terminal contract
  (version bump/tag/changelog) is agent-independent, already proven by the v2.5.0/v2.7.0 dogfoods,
  and cutting a real release for a comment-only change is unwanted. Phase 40's roadmap goal and
  PIDG-01 wording adjust from "Define→Ship" to "Define→Validate".

### Isolated-context dispatch
- **D-04: Re-filed, not built this phase.** The deferred isolated-context (process-spawning)
  dispatch — `--no-approve` child-argv patch + a Pi drain predicate — stays deferred. `@bacnh85`'s
  in-process model covers the shipped need; the process-spawning packages (`@dreki-gg`, `@smoose`)
  remain excluded on trust grounds.

### Hardening bar
- **D-05: Real supervised run + Pi-transport regression tests.** Evidence is two-part:
  1. **Real:** one clean Define→Validate run through `--agent pi` in supervise mode, with at least
     one live gate the operator answers — proving "gate decisions honored" through Pi, not just in
     auto mode.
  2. **Simulated (regression tests):** stubbed-`pi` fixtures proving the failure modes — a
     marker-less run does not advance, a non-zero exit does not advance, a hung Pi is detected. Only
     the Pi-transport delta is new; the generic marker/exit/liveness logic is already regression-tested
     (Phases 13/17/18) and must not be re-proven.

### Agent's discretion
- The exact gate point for the live gate (natural stage transition vs. a declared checkpoint in the
  plan) — planner's call.
- The precise set of regression-test gaps — determined by auditing existing generic coverage; add
  only what is genuinely Pi-transport-specific.

## Canonical References

### The dogfood subject
- `.planning/ROADMAP.md` § "Phase 999.85" (Backlog) — full F-34-01/F-34-02 detail, the proposed fix,
  and the "keep the instruction intact, do not treat the mechanism change as licence to relax it" constraint.
- `crates/devflow-core/src/agent_result.rs:1746-1750` — `idle_timeout_result` comment (F-34-01).
- `crates/devflow-core/src/agent_result.rs:6412-6417` — `stream_success_cannot_stand_against_nonzero_exit_code` comment (F-34-02).

### The Pi driver under test
- `.planning/milestones/v2.7.0-phases/39-pi-end-to-end/39-CONTEXT.md` — prior decisions
  D-01/D-02/D-02a/D-03/D-04 that Phase 40 carries forward (Legacy `-p --no-approve`, `@bacnh85`
  in-process dispatch, provider health, Define no-op / Plan un-gated).
- `crates/devflow-core/src/agents/pi.rs` — `PiDriver` (`-p --no-approve`, `Legacy` launch, provider health).
- `crates/devflow-core/src/agent_result.rs` — `parse_devflow_result` / `evaluate_layer1` /
  `evaluate_layer3` (the generic completion machinery the transport feeds).

### Regression-test pattern
- `crates/devflow-cli/tests/phase7_cli.rs` — stub-PATH + `ENV_MUTEX` integration-test pattern for
  stubbing an agent binary.
- `crates/devflow-core/src/agents/mod.rs` — the `AgentDriver` contract + shared conformance suite.

## Existing Code Insights

### Reusable Assets
- `parse_devflow_result` / `evaluate_layer1` (`agent_result.rs`) — generic marker detection that
  already covers Pi's plain-text completion; a `parse_pi_result` is likely unnecessary.
- `evaluate_layer3` typed outcomes + monitor liveness (Phase 17/18) — the non-zero-exit and
  hung-process machinery; regression-tested generically.
- Stub-PATH + `ENV_MUTEX` + `AlwaysFailAdapter` — how to fake a `pi` binary deterministically.

### Established Patterns
- Fail-closed capability detection (exact-package match) — Phase 39.
- "Marker-less must never advance" — Phases 13/17.
- Dogfood evidence: real run + UAT + verification, with negative controls — v2.5.0/v2.7.0 playbook.

### Integration Points
- `pipeline_launch.rs` `resolve_launch_shape` — Pi stays on `Legacy` (no change; a regression test
  already asserts the `claude_stream_launch_enabled(Pi)` precondition).
- `agents/pi.rs` `health` — provider probe (already fixed for `litellm` in Phase 39).

## Specific Ideas

None — standard dogfood; no particular references beyond the 999.85 entry.

## Deferred Ideas

- **Isolated-context (process-spawning) dispatch** — `--no-approve` child-argv patch + Pi drain
  predicate. Re-filed; not this phase (D-04).
- **DEN-95 (999.74) / DEN-98 (999.76) status sweep** — flagged in the 999.85 entry's "note for
  whoever picks this up." Out of scope; do not do the status sweep while fixing the comments.

---

*Phase: 40-Pi Dogfood*
*Context gathered: 2026-08-18*
