> invocation: antigravity --model gemini-3.7-flash-high --print-timeout 15m --dangerously-skip-permissions -p "$(cat /tmp/p37.1-research-prompt.txt)" (MCP linear/vercel disabled during run, restored after)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-37.1 (feature/phase-37.1 @ cef4059)
> Primary-source method: inspected the installed 0.84.1 binary + bundled package source; unpacked 16 npm candidate packages + 2 bundled examples to /tmp/pi-subagent-research/packages/; verified DevFlow launch/completion contracts in pi.rs / pipeline_launch.rs / agent_result.rs.

## 1. Enumeration

| Candidate | Source | Install | Class | Status |
|---|---|---|---|---|
| Bundled Reference Example | `examples/extensions/subagent/` in `@earendil-works/pi-coding-agent` | `~/.pi/agent/extensions/subagent/` | Subprocess spawner (`pi --mode json -p --no-session`) | Official example (uncompiled TS) |
| `@bacnh85/pi-subagent` v0.15.1 | npm | `pi install npm:@bacnh85/pi-subagent` | In-process SDK session (`createAgentSession`) | Maintained |
| `@dreki-gg/pi-subagent` v0.19.0 | npm | `pi install npm:@dreki-gg/pi-subagent` | Prompt-native subprocess (pi/claude CLI) | Maintained |
| `@parke.dev/pi-subagent` v0.8.0 | npm | `pi install npm:@parke.dev/pi-subagent` | Subprocess + locks/semaphores | Maintained |
| `@mystilleef/pi-subagent` v0.12.2 | npm | `pi install npm:@mystilleef/pi-subagent` | Subprocess + summary formatter | Maintained |
| `pi-subagents` v0.50.0 (nicobailon) | npm | `pi install npm:pi-subagents` | Hybrid sync/async fleet + watchdog | 50+ releases |
| `pi-subagents-j0k3r` v1.5.2 | npm | `pi install npm:pi-subagents-j0k3r` | Multi-tool SDK (`subagent_run`, `subagent_status`) | Forked/active |
| `@zhushanwen/pi-subagent-workflow` v8.0.0 | npm | `pi install npm:@zhushanwen/pi-subagent-workflow` | Workflow script executor | Maintained |
| `pi-subagent-lite` v0.1.4 | npm | `pi install npm:pi-subagent-lite` | File-output batch spawner | Active |
| Pi core native (0.84.1) | binary | built-in | **None** (no native subagent tool) | N/A |

## 2. Evaluation (vs launch shape)

1. **Sync vs background:** bundled example + `@dreki-gg` spawn children and the `execute()` handler returns an `await`ed promise resolving on child `close`; `@bacnh85` awaits in-process sessions synchronously; `pi-subagents` defaults `asyncByDefault: true` → parent emits `DEVFLOW_RESULT` before children finish (must pin `asyncByDefault: false`).
2. **`pi -p` + process-exit:** synchronous extensions do not resolve the tool call until children complete, so the parent blocks and stays alive until all subagents finish, then emits `DEVFLOW_RESULT` and exits 0. `MonitorLaunch::Legacy` (kill -0) suffices — no background hooks.
3. **stdin / MonitorLaunch:** DevFlow uses Legacy for Pi; Pi takes the positional prompt, not stdin. Child processes open their own stdin pipes. No deadlock with Legacy (only `PipeOwning` would deadlock — must not use it).
4. **`--no-approve` trust boundary:** `--no-approve` sets `projectTrusted = false`, blocks project-local `.pi/extensions/`; user extensions stay loaded. `@bacnh85` fails closed headlessly for project agents (`extensions/index.ts:614` — returns "Project agents require explicit user approval…" when `!ctx.hasUI`); `@dreki-gg` is prompt-native (no repo `.md` loading); bundled example defaults `scope: "user"`.
5. **Completion observability:** `evaluate_layer1` (`agent_result.rs:1824`) scans the trailing 4000 chars for `DEVFLOW_RESULT`. In synchronous execution the parent only emits it after all subagent tool calls returned. Process exit + marker = observed completion.

## 3. Verdict per candidate

| Candidate | Verdict | Changes |
|---|---|---|
| `@bacnh85/pi-subagent` | **YES** | Works out of the box — sync in-process sessions, `background: false`, `agentScope: "user"`, fails closed headless. |
| `@dreki-gg/pi-subagent` | **YES** | Works out of the box — sync subprocess + line-delimited parser, prompt-native, `pi -p --no-approve` compatible. |
| `@parke.dev/pi-subagent` | **YES** | Works out of the box — sync child exec with locks/semaphores. |
| `@mystilleef/pi-subagent` | **YES** | Works out of the box — sync child exec + summary formatter. |
| Bundled example | **YES-WITH-CHANGES** | Copy into `~/.pi/agent/extensions/subagent/` (or compile JS + register); definitions in `~/.pi/agent/agents/*.md`. |
| `pi-subagents` (nicobailon) | **YES-WITH-CHANGES** | Set `{"asyncByDefault": false, "forceTopLevelAsync": false}`. |
| `pi-subagents-j0k3r` | **YES-WITH-CHANGES** | Tool is `subagent_run` (not `subagent`); prompt must call `subagent_run` with `mode: "task"`. |
| `pi-subagent-lite` | **YES-WITH-CHANGES** | Requires explicit `{ agent, prompt, output }` file path. |
| Pi core native | **NO** | No native subagent tool; extension strictly required. |

## 4. Negative controls / what is NOT established

- Does NOT establish Pi subagents run without prior user-level setup (an extension must be installed at `~/.pi/agent/extensions/`).
- Does NOT establish async/backgrounded subagents work (process-exit monitor would see premature exit). Only **synchronous await** preserves the `DEVFLOW_RESULT` contract.
- Verified: `pi -p --no-approve` without extensions → no subagent tools in `pi.getAllTools()`; `--no-approve` → project-local `.pi/extensions/` in the worktree are ignored (only trusted user-level extensions execute).

## 5. Bottom line

BOTTOM LINE: YES — `@bacnh85/pi-subagent` and `@dreki-gg/pi-subagent` are fully functional, maintained extensions that execute subagents synchronously via `await`, preserving process-exit supervision under `pi -p --no-approve` and allowing DevFlow to observe `DEVFLOW_RESULT` reliably without any core Rust driver changes.
