> invocation: codex exec -m gpt-5.6-terra -c model_reasoning_effort=high --cd /var/home/denniyahh/Github/devflow/.worktrees/phase-37.1 "$(cat /tmp/p37.1-research-prompt.txt)"
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-37.1 (feature/phase-37.1 @ cef4059)
> DEDUPLICATED — codex emitted the report twice; one copy kept.
> Primary-source method: unpacked 24 published npm tarballs to /tmp/pi-subagent-registry/ and inspected each package.json + source; ran `pi --version/--help/--list-models/install --help`, `npm search` (3 queries), an isolated `pi install` + offline child-run probe, and a negative control on the real profile (`pi list` → "No packages installed").

## Candidates (registry inventory)

| Candidate | Execution | DevFlow verdict |
|---|---|---|
| Pi bundled `examples/extensions/subagent` | Foreground; awaits every child | **Yes-with-changes** |
| `@smoose/pi-subagent@0.1.0` | Foreground; awaits child/parallel workers | **Yes-with-changes** |
| `@mystilleef/pi-subagent@0.12.2` | Foreground; awaits child | **No**: child forcibly uses `--approve` |
| `avtc-pi-subagent@1.0.6` | Foreground JSON or RPC children | **No**: can load agent-provided extensions; unsafe under DevFlow boundary |
| `@bacnh85/pi-subagent@0.15.1`, `@dreki-gg/pi-subagent@0.19.0`, `@parke.dev/pi-subagent@0.8.0`, `pi-simple-agents`, `pi-dynamic-workflow`, `pi-ate-workflow`, `pi-subagent-workflow@0.5.1`, `pi-subagents@0.50.0`, `pi-cohort`, `pi-subagents-j0k3r`, `@tintinweb/pi-subagents`, `@maplezzk/pi-dynamic-workflows`, `@vigolium/piolium` | Foreground and/or mixed workflow variants | **No verified secure integration** — none installed in this Pi profile; child trust/extension policy is not a DevFlow contract |
| `@agwab/pi-subagent`, `@heyhuynhgiabuu/pi-task`, `@zhushanwen/pi-subagent-workflow@8.0.0` | Foreground **and** durable/background modes | **No as background**; foreground unverified against trust boundary |
| `@4fu/pi-subagent`, `@wkronmiller/pi-subagent-extension`, `pi-subagent-lite`, `@ghoulm370/pi-subagent-ui`, `@ryan_nookpi/pi-extension-subagent`, `@yusukeshib/pi-babysit`, `pi-vault-mind` | Detached/background, durable state, long-lived RPC | **No**: parent can finish before children; DevFlow has no Pi drain predicate |
| `pi-adaptive-orchestrator` | Human-approved orchestration | **No** in headless `-p` |
| `@howaboua/pi-subagent-review` | Fixed isolated review child | **No**: review-only |
| `@firstpick/pi-extension-subagent-minimum-fanout` | Policy layer | **No**: does not dispatch children |
| `avtc-pi` | Suite manifest | **No** standalone dispatcher |
| `@jmcombs/pi-relay` | Dispatches external Claude/Codex/Grok | **No**: not Pi-child dispatch |
| `pi-subagent-model-selection`, `@fyeeme/pi-subagent-core`, `pi-subagent-bridge` | Shared library / Codex bridge | **No**: not Pi extensions |
| First-party Pi feature | — | **No native subagent command/tool** |

## Per-candidate evaluation (key)

- **Bundled example** is a real `pi.registerTool({ name: "subagent" })`, not a built-in feature. Spawns `pi --mode json -p --no-session`, child `stdio: ["ignore","pipe","pipe"]`, parses JSONL, awaits `close`; parallel mode awaits its worker pool (`examples/extensions/subagent/index.ts:294-414`, `:472-698`). **Children finish before the parent emits `DEVFLOW_RESULT` → Legacy process-exit supervision is sufficient; PipeOwning is neither needed nor appropriate.** It defaults to user-level agents, but `agentScope: project|both` reads `.pi/agents`; its interactive confirmation is skipped headless because the condition is `ctx.hasUI` (`index.ts:505-528`) — that reopens the trust boundary.
  **Exact changes:** (1) add `--no-approve` to the child argv at `index.ts:294`; (2) reject `agentScope !== "user"` when `!ctx.hasUI`; (3) install as user/global extension; (4) no DevFlow source change needed for completion (the prompt already demands the final exact `DEVFLOW_RESULT` line, `prompt.rs:42-55`).

- **`@smoose/pi-subagent`** — strongest third-party foreground candidate. Tool "wait[s] for their results", awaits single + concurrent calls (`src/index.ts:45-172`); child launcher awaits `close` (`src/runner.ts:302-317`); uses `--mode json -p --no-session --no-extensions`, `stdio: ["ignore","pipe","pipe"]` (`src/runner.ts:135-160,223-230`). Needs no stream, no PipeOwning.
  **Exact change:** add `--no-approve` to `buildPiArgs` (`src/runner.ts:135-160`); `--no-extensions` alone does not preserve the project-resource boundary. Could not complete an authenticated e2e run (isolated package loaded; `PI_OFFLINE=1 pi -p --no-approve --tools subagent …` → `Connection error`) — that proves neither success nor failure.

- **`@mystilleef/pi-subagent`** — foreground, returns child result, ignored stdin, no PipeOwning needed. **But** explicitly adds `--approve` at `src/child/process.ts:401`; default agent scope `"both"` (`src/orchestration/subagent-orchestrator.ts:58-62`); headless project-agent confirmation bypassed (`:370-374`). **Not usable as shipped.**

- **Background/detached packages** are structurally incompatible: `@4fu` returns immediately unless `wait` supplied; `@agwab` has detached paths; `@heyhuynhgiabuu/pi-task` backgrounds with `void input.run()`; `@zhushanwen` `action:"start"` is background-only; `pi-babysit` uses a long-lived RPC child and detects `agent_settled`, not process exit. DevFlow parses none of their state/event files. Async would require: a Pi launch/result protocol in `PiDriver::build_command`, a Pi JSON/RPC parser in `evaluate_layer1`, and a Pi child-drain predicate — PipeOwning cannot be reused (Pi consumes stdin until EOF; PipeOwning holds it open).

## DevFlow facts confirmed

- Pi launches as `pi -p --no-approve <positional>` (`pi.rs:43-52`); routes to Legacy (`pipeline_launch.rs:210-212`), Legacy stdin is `/dev/null`.
- `AgentDriver::parse_completion` defaults `None` (`agents/mod.rs:144-148`); Pi does not override.
- This does NOT prevent a synchronous extension from completing: generic Layer 1 scans stdout for `DEVFLOW_RESULT` (`parse_devflow_result` at `agent_result.rs:166`, `evaluate_layer1` at `:1824`). It only prevents interpreting Pi-native JSON/RPC background events.

## Commands run (excerpt)

`pi --version` · `pi --help` · `pi list` (real profile → "No packages installed") · `pi --list-models` · `pi install --help` · `npm search --json pi-subagent` (×3 variants) · `npm pack` of published candidates to /tmp · isolated `pi install npm:@smoose/pi-subagent@0.1.0` + `pi list` (negative control: list/install works) · offline child-run probe (→ Connection error, inconclusive).

BOTTOM LINE: NO — DevFlow currently loads no Pi package (`pi list`: No packages installed), and every discovered background dispatcher can outlive the Legacy parent while the only verified foreground candidates require trust-boundary changes before use.
