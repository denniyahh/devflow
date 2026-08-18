# Feature Landscape — Agent Harness Support (Milestone v2.8.0)

**Domain:** AI coding-agent harness integration (DevFlow's `AgentDriver` contract)
**Researched:** 2026-08-18

This milestone is not a greenfield feature set — it is *coverage completion* across an existing
contract. "Feature" here means **"a harness is fully supported"**, and that decomposes into the
`AgentDriver` trait surface below (read from `agents/mod.rs`).

## Table Stakes — what "fully supported" requires

Every supported agent must implement these or justify a default. Missing = the harness is a stub,
not supported.

| Capability | Trait method | Antigravity | Hermes | OpenCode (current: STUB) |
|------------|--------------|-------------|--------|--------------------------|
| Name | `name()` | ✅ new | ✅ new | ✅ (present) |
| Prompt rendering | `render_prompt()` | ✅ new (Claude-style or workflow-style) | ✅ new | ✅ (Claude-style, present) |
| Headless argv | `build_command()` | ✅ new (`-p` + `--output-format`) | ✅ new (`-z --yolo`) | ⚠️ has `run "<prompt>"` but **missing `--auto` + `--format json`** |
| Completion parsing | `parse_completion()` | Claude-style stream parse | process-exit (`None` default) + marker contract | ⚠️ **missing** (`--format json` JSONL unparsed) |
| Pre-launch health | `health()` | ✅ new | ✅ new | ❌ **missing** (default `Ok(())`) |
| Capability discovery | `capabilities()` / `discover()` | subagent dispatch (if any) | subagent dispatch (if any) | ❌ **missing** |
| Env for agent tree | `environment()` | as needed | as needed | ❌ **missing** |
| Interactivity gating | `interactivity_mode()` | Define/Plan headless-safe? | Define/Plan headless-safe? | ❌ **missing** (defaults `HeadlessSafe`) |
| Workflow-reference root | `workflow_root()` | `~/.codex/...` default or own | own install | ❌ **missing** (defaults Codex) |
| Conformance suite | `test_contract()` | ✅ inherited | ✅ inherited | ✅ inherited |

**Codex** (already a native driver, 37-03) is *not* a stub: it has all the above. Its milestone
work is **verification/hardening end-to-end** (dogfood a real phase through `--agent codex`, close
any gaps the run surfaces) — not new feature code.

## Differentiators — the parts that are hard, not checklist

| Differentiator | Value | Complexity | Notes |
|----------------|-------|------------|-------|
| Antigravity stream-json launch | Same pipe-owning monitor path Claude already uses (Phase 31) | Med | `--input-format stream-json` (1.1.14 only) |
| OpenCode `--format json` verdict/result parsing | Detect completion + Layer-1 verdict instead of blind process-exit | Med | mirror `parse_codex_event_result` |
| Pi dogfood (Phase 40) | Prove the *newest* driver holds up under a real Define→Ship run | High | also exercises the deferred isolated-context Pi dispatch |
| Hermes `--pass-session-id` / `--resume` | Cheap session-resume parity with Claude's `session_id` arc | Low | opportunistic; not required for v2.8.0 |

## Anti-Features — explicitly do NOT build

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Fake capability detection (name-substring match) | Phase 39 lesson: `*subagent*` matched unsafe/deferred packages | Exact vetted-package match, fail-closed |
| Shell-interpolated launch argv | Phase 12 security invariant | Positional argv only (as all existing drivers) |
| Fake completion signals | A marker-less run must not silently advance a stage (Phase 13 lesson) | process-exit transport + `DEVFLOW_RESULT` prompt contract, or a real parsed stream |
| Auto-fallback on parse failure | Invisible-degradation class (Phase 31 D-11) | Fail loud, keep the honest transport |

## Feature Dependencies

```
Antigravity driver → ClaudeDriver pattern (stream-json argv + parser reuse)
Hermes driver → PiDriver pattern (positional argv + process-exit + prompt contract)
OpenCode completion → parse_codex_event_result (JSONL) pattern
Pi dogfood (Phase 40) → nothing new; depends on Pi driver already shipped (v2.7.0)
999.94 → independent (checkpoint/gate logic, not driver code)
999.85 → independent (comment cleanup)
```

## MVP Recommendation

Prioritize (this milestone's committed scope):
1. Antigravity driver (new harness — Claude-style, highest reuse)
2. Hermes driver (new harness — Pi-style positional)
3. OpenCode driver completion (finish the existing stub: `--auto`, `--format json`, health)
4. Phase 40 Pi dogfood (prove the newest driver end-to-end)
5. Codex end-to-end verification/hardening

Defer (if capacity does not permit): 999.94 + 999.85 — both independent, both safely deferrable to a
later pass without blocking any of the above.

## Sources

- `crates/devflow-core/src/agents/mod.rs` (`AgentDriver` trait + `DriverCapabilities`/`DriverHealth`)
- `crates/devflow-core/src/agents/{claude,codex,opencode,pi}.rs`
- Installed CLI `--help` probes (2026-08-18)
