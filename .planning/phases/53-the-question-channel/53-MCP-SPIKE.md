# Phase 53 — MCP-consumption spike (D-01)

Run 2026-11-18 by an agent session, before `devflow start --until plan` (operator answer to D-01: "1 spike now").
Clients: Claude Code 2.1.327 (`--model haiku`), codex-cli 0.170.0 (config default model, `-c model_reasoning_effort="low"`).
10 agent launches in total. Server: a stdlib-only Python stdio MCP server exposing one tool `devflow_ask`
(`question`, `options[]`), logging every request to a file, holding each call `HOLD` seconds, then returning
`options[0]` (`MODE=ok`) or `{"isError": true}` (`MODE=error`). It answers `initialize` (echoing the client's
`protocolVersion`), `tools/list`, `tools/call`, `ping`, and `-32601` for any other request. Scratch paths below are
shortened to `$S` (the session scratch dir, deleted after the spike).

Prompt for every call run: `Call the devflow_ask tool with question "Spike question X?" and options ["a","b"]. Then print
exactly the answer text the tool returned, nothing else. If the tool is not available or fails, print exactly NO_TOOL
or TOOL_ERROR: followed by the error text.`

## Commands

claude (registration file `$S/<tag>.mcp.json` = `{"mcpServers":{"devflow":{"type":"stdio","command":"python3",
"args":["-I","$S/server.py"],"env":{"HOLD":"…","MODE":"…","LOG":"…"}}}}`):

```
cd $S/work && timeout <N> claude -p "<prompt>" --model haiku --output-format stream-json --verbose \
  --strict-mcp-config --allowedTools mcp__devflow__devflow_ask --mcp-config $S/<tag>.mcp.json < /dev/null
```

codex:

```
timeout <N> codex exec --json --skip-git-repo-check -s read-only -C $S/work \
  -c mcp_servers.node_repl.enabled=false -c mcp_servers.cua_repl.enabled=false \
  -c mcp_servers.github.enabled=false -c mcp_servers.mempalace.enabled=false -c model_reasoning_effort="low" \
  -c mcp_servers.devflow.command="python3" -c 'mcp_servers.devflow.args=["-I","$S/server.py"]' \
  -c 'mcp_servers.devflow.env={HOLD="…",MODE="…",LOG="…"}' \
  -c mcp_servers.devflow.default_tools_approval_mode="approve" [-c mcp_servers.devflow.tool_timeout_sec=1200] \
  "<prompt>" < /dev/null
```

Codex has no `--strict-mcp-config` equivalent: the operator's four global MCP servers would also start, so each launch
turned them off with `-c …enabled=false` (scoped to the launch).

Launches: cl-ctrl, cl-call, cl-err, cl-knob30 (`MCP_TOOL_TIMEOUT=30000`, HOLD 60), cl-900 (HOLD 900);
cx-ctrl, cx-call, cx-err, cx-900 (HOLD 900, default timeout), cx-900raised (HOLD 900, `tool_timeout_sec=1200`).

## Item 1 — registration scoped to one launch

| Agent | Observed | Evidence |
|---|---|---|
| claude | `--mcp-config <file> --strict-mcp-config` spawned the server; the control launch (no `--mcp-config`, still `--strict-mcp-config`) answered `NO_TOOL`. | cl-call server log: `server/discover` → `initialize` → `notifications/initialized` → `tools/list`. cl-ctrl result: `"result":"NO_TOOL"`. |
| codex | `-c mcp_servers.devflow.*` overrides spawned the server; control launch (no devflow overrides) answered `NO_TOOL`. | cx-call server log: `initialize` (protocolVersion `2025-06-18`, clientInfo `codex-mcp-client`) → `notifications/initialized` → `tools/list`. cx-ctrl: `agent_message "NO_TOOL"`. |

Operator config untouched by the spike: `~/.codex/config.toml` sha256 `02a70101…a0e8302` before and after (identical).
`~/.claude.json` changed during the spike window (`dbc59459…` → `525bca3d…`); it holds no project entry for the scratch
directory, and it did not change over a 60 s window with no launch, so the change is not attributed to the spike's
launches but is also not proven to come from elsewhere (other live Claude sessions write this file). The launches did
create the agents' own session transcripts (`~/.codex/sessions/2026/11/18/rollout-*`, `~/.claude/projects/…`); kept.

Protocol detail for D-04: Claude 2.1.327 first sends `server/discover` (`io.modelcontextprotocol/protocolVersion
"2026-07-28"`); the server's `-32601` reply made it fall back to `initialize` with `protocolVersion "2025-11-25"`. Codex
sends `initialize` with `"2025-06-18"`. Echoing the client's version worked for both.

## Item 2 — discovery and call

| Agent | Observed | Evidence |
|---|---|---|
| claude | Called `mcp__devflow__devflow_ask` with the given args, printed `a`. 12 s, 3 turns. | log: `call_start … "args": {"question": "Spike question X?", "options": ["a", "b"]}`; result `"result":"a"`; permission `"source":"config","reason_type":"rule"` (from `--allowedTools`). |
| codex | Called `devflow/devflow_ask`, printed `a`. 11.5 s. | `item.completed … "type":"mcp_tool_call","server":"devflow","tool":"devflow_ask" … "status":"completed"`, then `agent_message "a"`. |

Not established: whether claude needs `--allowedTools` and codex needs `default_tools_approval_mode="approve"` (no launch
without them; codex source says a non-interactive approval request aborts the call).

## Item 3 — blocking calls

| Agent | Case | Observed | Evidence |
|---|---|---|---|
| claude | HOLD 900 s, defaults | Waited the full 900 s and printed `a`; no cancel, no retry; emitted `tool_progress` heartbeats every 30 s in stream-json. Run 912.9 s. | log: `tools/call` at …856.929, `call_end` at …756.931 (900.0 s); `"duration_ms":906085`, `"result":"a"`. |
| claude | HOLD 60 s, `MCP_TOOL_TIMEOUT=30000` (knob control) | Aborted at 30.0 s, sent `notifications/cancelled`, no retry; the agent continued and printed the error. | log: `notifications/cancelled … "requestId": 2, "reason": "SdkError: Request timed out"` 30.05 s after `call_start`; tool_result: `MCP server "devflow" tool "devflow_ask" sent no response or progress for 30s; aborting. … set a per-server "timeout" (ms) … otherwise set CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT (ms) globally (0 disables).` |
| codex | HOLD 900 s, default | Timed out at 300 s, sent `notifications/cancelled`, no retry; agent continued and printed the error. | log: `notifications/cancelled … "reason": "MCP request cancelled by caller"` 300.0 s after `tools/call`; item error `timed out awaiting tools/call after 300s`. |
| codex | HOLD 900 s, `tool_timeout_sec=1200` | Waited the full 900 s, printed `a`. Run 922.5 s. | log: `tools/call` …855.176, `call_end` …755.177 (900.0 s); `status":"completed"`. |

Cost finding (codex): while a call is pending, the model polls it with a `wait` function (`yield_time_ms: 20000`) about
every 21 s, each a model round-trip: the 900 s run made 39 `wait` calls and used 1,346,920 input tokens (1,306,880
cached) against 63,587 for the instant call (cx-900, cut at 300 s: 449,905). Input grows with wait time, so an hours-long
hold on codex costs model turns the whole time. Claude made no model request while waiting (3 turns, as for the instant call).

Documented knobs (current docs via ctx7; only the claude `MCP_TOOL_TIMEOUT` and codex `tool_timeout_sec` knobs were run):
- claude: `MCP_TOOL_TIMEOUT` (ms; default 100000000 ≈ 28 h, the wall-clock cap); per-server `"timeout"` (ms) in the
  `--mcp-config` entry overrides it for that server and also floors the idle window; `CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT`
  (ms; stdio default 1800000 = 30 min with no response and no progress notification; `0` disables; capped at the
  effective `MCP_TOOL_TIMEOUT`, which is why the 30000 control fired as an idle abort); `MCP_TIMEOUT` (startup, 30 s).
  Docs also say a main-conversation call past two minutes moves to a background task; in `-p` the 900 s call stayed in
  the foreground and returned normally.
- codex: per-server `tool_timeout_sec` (default 300 s, `DEFAULT_TOOL_TIMEOUT` in `codex-mcp/src/rmcp_client.rs`),
  `startup_timeout_sec` (default 30 s). No documented maximum found; the field is a seconds `Duration`.

Hours case (not run): claude can hold up to ≈28 h by default only if the idle window is raised past the hold (per-server
`timeout` in the registration file, or server progress notifications — not established whether claude sends a
`progressToken` on `tools/call`). Codex holds as long as `tool_timeout_sec` allows, with the polling cost above.
Not established: behaviour of either client past 15 min, any limit above those values (session, API, or auth expiry).

## Item 4 — tool error

| Agent | Observed | Evidence |
|---|---|---|
| claude | `isError: true` arrived as an error tool_result; agent was not blocked, made no retry, printed the error and ended. 16.5 s. | tool_result `"is_error":true`, `"result":"TOOL_ERROR: devflow_ask failed: run stopped (spike error mode)"`; one `tools/call` in log. |
| codex | Item marked `"status":"failed"` with the error text; no retry; printed it and ended. 15.2 s. | `agent_message "TOOL_ERROR:devflow_ask failed: run stopped (spike error mode)"`; one `tools/call` in log. |

Not established: how either agent behaves on an error with a richer prompt (a GSD stage may retry by its own choice).

## Verdicts

- **D-01.1 registration:** PASS for claude and codex (one launch each, plus a no-registration control each answering `NO_TOOL`). Codex additionally needs per-launch `enabled=false` for each global server to get an isolated set.
- **D-01.2 discover and call:** PASS for claude and codex (one launch each).
- **D-01.3 blocking:** claude held 15 min with defaults; codex held 15 min only with `tool_timeout_sec` raised (default cuts at 300 s). Both clients cancel with `notifications/cancelled` on timeout and do not retry. Hours not run: claude needs the idle window raised past 30 min; codex keeps the model polling throughout.
- **D-01.4 tool error:** PASS for claude and codex: error seen, agent not blocked, no retry (one launch each).

## What this means for D-03..D-06

- **D-03** holds: both clients take a per-launch stdio registration on the command line (claude: a JSON file; codex:
  `-c` overrides — no file needed, or a file layered with `-p <profile>`, not tested).
- **D-04** holds with two notes: answer unknown requests (Claude's `server/discover`) with `-32601`, and negotiate
  `protocolVersion` (clients sent `2025-11-25` and `2025-06-18`).
- **D-05** holds, with a gap: on a client timeout both clients send `notifications/cancelled` and move on, so the
  server must treat it as the agent abandoning the question (CONTEXT does not say what happens to the gate then).
- **D-06** holds in shape: typed tool errors leave neither agent blocked; "raise the client's tool timeout where the agent
  allows it" is available on both (claude per-server `timeout` in the registration file; codex `tool_timeout_sec`).
  Two inputs it does not cover: claude's 30-min stdio idle abort applies unless the registration raises it, and codex's
  model polls a pending call about every 21 s, so a long block on codex costs tokens for its whole duration — this bears
  on whether codex should use the `pending`/re-call fallback rather than a long block (a choice for planning or the operator).

## Addendum — pi 1.5.2 (Phase 53, operator answer OD-2)

Run 2026-11-19 by the 53-14 executor in the phase worktree session. Client: pi 1.5.2 on its own configured model
(`settings.json` `defaultProvider` litellm, `defaultModel` deepseek-flash). Server: the same stdlib-only Python stdio
server as above (one tool `devflow_ask`; `HOLD`, `MODE`, `LOG` env; echoes `protocolVersion`; `-32601` for any other
request; `python3 -I`), with each `tools/call` answered from a thread so a `notifications/cancelled` is logged when it
arrives. Same prompt as above. Scratch (`$S`) was a session scratch directory outside the repository, deleted after
the spike. 9 pi launches in total.

pi's docs (`docs/mcp.md`, `docs/security.md`, `docs/extensions.md` in the installed package) say: servers come from
`~/.pi/agent/mcp.json` and, only after project trust, `.pi/mcp.json`; there is no per-launch MCP config flag or
environment variable; `--approve` trusts *all* project-local resources for one run (settings, `.pi/extensions`, skills,
prompts, `.pi/mcp.json`); an extension can call `pi.registerMcpServer(name, config)` with an `mcpServers`-entry
config for the current session; a CLI `-e <path>` extension loads without project trust; each server has a
per-request `timeout` in seconds (default 60) that progress notifications reset; `exposure: "direct"` declares a
server's tools to the model (the default `codemode` does not).

### Commands

Registration file `$S/<tag>.ext.ts` (route 1: a per-launch flag pointing at a file):

```
export default function (pi: any) {
  pi.registerMcpServer("devflow", {"command":"python3","args":["-I","$S/server.py"],
    "env":{"HOLD":"…","MODE":"…","LOG":"…"},"exposure":"direct"[,"timeout":<secs>]});
}
```

```
cd $S/work && timeout <N> pi -p --no-approve --no-session --mode json [-e $S/<tag>.ext.ts] "<prompt>" < /dev/null
```

Launches: pi-ctrl (no `-e`), pi-call (HOLD 0), pi-err (`MODE=error`), pi-knob30 (HOLD 60, `"timeout":30`), pi-900
(HOLD 900, pi's default timeout), pi-knob (HOLD 900, `"timeout":960`), pi-shape (HOLD 0, `"timeout":960`, the exact
one-line text `PiDriver::ask_wiring` writes, at `$S/shape/.devflow/ask-07.mcp.ts`), and one load check of the same
extension named `*.mcp.json`. pi-900 and pi-knob ran in the background and were polled; their PIDs were recorded and
both had exited on their own before cleanup. `--no-session` kept the launches from writing session files under
`~/.pi/agent/sessions/`. Route 2 (`.pi/mcp.json` under `--approve`) was not tried, because route 1 worked first; it
would also have needed `--approve`, which DevFlow's pi launch never passes (pi.rs: it loads project extensions that run
unsandboxed).

### Item 1 — registration without global config

| Case | Observed | Evidence |
|---|---|---|
| pi-ctrl | No `devflow` server, answered `NO_TOOL`. 4 s. | no server log; final assistant text `NO_TOOL`, no `tool_execution_start` event |
| pi-call | `-e` extension under `--no-approve` spawned the server; pi listed and called the tool, printed `a`. 4 s. | log: `initialize` (`protocolVersion "2025-11-25"`, clientInfo `pi 1.5.2`, capabilities `roots`) → `notifications/initialized` → `tools/list` → `tools/call` with `_meta.progressToken 3`; event `tool_execution_end … "toolName":"mcp__devflow__devflow_ask"`, result `a`; final text `a` |
| pi-shape | The one-line text DevFlow writes, named `ask-07.mcp.ts` under a `.devflow/`, loads and calls the tool. | final text `a` |
| `.json` name | pi refuses an extension named `*.mcp.json` and exits 1 (it parses it as JSON). | stderr `Failed to load extension … is not valid JSON`; exit 1 |

No `--tools`, approval flag or setting was needed: pi has no per-call approval, and the tool was declared directly
(`exposure: "direct"`). `~/.pi/agent/`: sha256 of all 1,361 files before and after; no file changed or was removed, and
one file was added, `sessions/--tmp-claude-1000--var-home-denniyahh-Github-wsx-…-scratchpad-review-snapshot--/2026-11-19T18-12-02-823Z_….jsonl`,
a session of a pi run whose working directory was another project's review snapshot (pi names a session directory
after its working directory). Every spike launch ran in `$S/work` or `$S/shape` with `--no-session`, so the added file
is not from the spike.

### Item 2 — held call

| Case | Observed | Evidence |
|---|---|---|
| pi-900 (default timeout) | Cancelled at 60.0 s, no retry; the agent continued and printed the error. 63 s. | log: `tools/call` …111.681, `notifications/cancelled {"requestId":3,"reason":"Request timed out"}` …171.693 (60.01 s); tool result `MCP request timed out after 60000ms`, `isError` |
| pi-knob30 (`timeout` 30, HOLD 60) | Cancelled at 30.0 s, no retry (knob control). | log: `tools/call` …118.704, `notifications/cancelled` …148.703 (30.00 s); `MCP request timed out after 30000ms` |
| pi-knob (`timeout` 960, HOLD 900) | Waited the full 900 s and printed `a`. Run 901.9 s. | log: `call_start` 1795111111.686, `call_end` 1795112011.687 (900.0 s); one `tools/call`; final text `a` |

Cost: pi made no model request while the call was held. pi-knob had 2 assistant turns with the same usage as the
instant pi-call (395 + 223 input, 26,624 + 26,880 cache read, 72 output tokens).

### Item 3 — tool error

| Case | Observed | Evidence |
|---|---|---|
| pi-err | `isError: true` reached the model as an error tool result; no retry; printed it and ended. 3 s. | event `tool_execution_end … "isError":true`, text `devflow_ask failed: run stopped (spike error mode)`; one `tools/call` in the log; final `TOOL_ERROR: devflow_ask failed: run stopped (spike error mode)` |

**pi registration without global config:** PASS — a one-line extension `.devflow/ask-NN.mcp.ts` calling `pi.registerMcpServer("devflow", {command, args, exposure: "direct", timeout})`, passed per launch with `-e <path>` before the prompt; no trust is granted: `--no-approve` stays, and an explicit `-e` extension loads without project trust
**pi hold:** held 900 s — per-server `timeout` (seconds) in the registration, set to 960; pi's default 60 s cancels at 60.0 s
**pi tool name:** mcp__devflow__devflow_ask
**pi tool error:** `isError: true` arrives as an error tool result; the agent is not blocked, makes no retry, prints the error and ends
**pi verdict:** BUILD — per-launch `-e .devflow/ask-NN.mcp.ts` extension calling `pi.registerMcpServer`, under `--no-approve`; held 900 s

Not established: how pi's model treats the server's `pending` reply after a 900 s hold (53-13's re-call by
`question_id`); no pi launch received one. No live DevFlow run on pi was made. A server named `devflow` in the
operator's `~/.pi/agent/mcp.json` would take precedence over the extension's registration (pi docs; not run). pi's
behaviour past 15 min, and with an exposure other than `direct`, was not run.
