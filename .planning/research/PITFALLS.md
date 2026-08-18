# Domain Pitfalls — Adding Harness Drivers (Milestone v2.8.0)

**Domain:** AI coding-agent harness integration
**Researched:** 2026-08-18 (synthesized from prior phase lessons + CLI probing)

## Critical Pitfalls

### Pitfall 1: Flag placement is binary-specific and unverifiable by reading docs
**What goes wrong:** an approval flag in the wrong position is rejected at spawn time (Codex
`codex exec -a never` is an unknown argument; the flag must be GLOBAL and precede `exec`).
**Why it happens:** flag grammar differs per CLI; docs and memory both lie.
**Consequences:** a driver that passes unit tests but fails at the first real spawn.
**Prevention:** spawn-test the exact argv against the installed CLI before trusting the driver.
**Detection:** `ensure_agent_binary`/preflight passes but the monitor records an immediate CLI usage error.

### Pitfall 2: Fake completion signal lets a marker-less run advance
**What goes wrong:** process-exit is treated as success when the agent produced no `DEVFLOW_RESULT`.
**Why it happens:** the completion contract is carried *in the prompt*, not in argv.
**Consequences:** a silently-failed stage advances (Phase 13/17 lesson).
**Prevention:** `render_prompt` must state the `DEVFLOW_RESULT` contract (enforced by `test_contract`'s
`render_prompt states the completion contract` check), and `parse_completion` must return `None`
(process-exit transport) unless a real parsed stream exists.

### Pitfall 3: Capability detection that substring-matches a package name
**What goes wrong:** `*subagent*` matched unsafe/deferred packages (`@mystilleef`, `@smoose`).
**Why it happens:** cheap name matching is tempting; the registry is not a trust list.
**Consequences:** reporting "subagent dispatch available" for a package that was never vetted.
**Prevention:** exact vetted-package match, fail-closed on any probe error (Pi's `@bacnh85/pi-subagent`).

### Pitfall 4: Health probes a provider the launch never uses
**What goes wrong:** probing "any ready `models.json` provider" false-greens a credential the
`build_command` argv never selects (and false-rejects standard installs that have no `models.json`).
**Why it happens:** the health check is written before the argv it must match.
**Consequences:** preflight green on an uncredentialled run, or a hard refusal of a normal install.
**Prevention:** `health()` probes exactly what `build_command` will invoke (Pi's `defaultProvider` lesson).

## Moderate Pitfalls

### Pitfall 5: The Antigravity binary-name ambiguity
`antigravity` (1.1.13) vs `agycli`→`antigravity-cli` (1.1.14) vs `agy` (GUI alias). The vetted,
stream-json-capable binary is **1.1.14**. Prevention: the driver documents which binary + version it
targets and `ensure_agent_binary`/health checks that shape, not just "some `antigravity` on PATH".

### Pitfall 6: Hermes oneshot prints only final text — no structured stream
`hermes -z` emits bare final-response text to stdout. Do not try to parse JSON out of it. Prevention:
treat Hermes as Family B (positional + process-exit + prompt contract), like Pi.

### Pitfall 7: OpenCode `--auto` is "dangerous" and `--format json` changes the transport
`--auto` auto-approves everything not explicitly denied; `--format json` switches stdout from
formatted text to raw JSON events. Both must land together or the driver is either un-headless-able
or unparseable. Prevention: one atomic build_command change + parse_completion in the same plan.

## Minor Pitfalls

- **`--dangerously-skip-permissions` / `--yolo` are load-bearing, not cosmetic** — like Claude's
  existing `--dangerously-skip-permissions`, dropping them reintroduces interactive permission
  prompts that hang a headless monitor. Keep them, document why.
- **Python-based Hermes startup cost** — first-run can be slower; do not fold it into a timing-sensitive
  test.

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Antigravity driver | using 1.1.13 (no `--input-format`) | target `antigravity-cli` 1.1.14; spawn-test argv |
| Hermes driver | parsing oneshot stdout as JSON | Family B, process-exit + prompt contract |
| OpenCode completion | `--format json` vs default text mismatch | add `--auto` + `--format json` + parser in one change |
| Pi dogfood (Phase 40) | dead monitor / gate wedge mid-run | reuse the v2.5.0/v2.7.0 dogfood playbook (bounded gate timeout, monitor liveness) |
| 999.94 | blind first-option auto-advance is a *policy* change | scope tightly; it gates unattended runs, needs explicit acceptance |

## Sources

- Phase 37/39 code-review findings (flag placement, capability detection, provider probe)
- Phase 13/17 completion-protocol lessons (marker-less advance)
- Phase 31 D-11 (no silent parse downgrade)
- Installed CLI `--help` probes (2026-08-18)
