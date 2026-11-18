# API Coverage — Model Context Protocol (server side, stdio)

> Full coverage by default. Opt-outs are explicit, reasoned decisions.

DevFlow does not call an external API in this phase; it serves the MCP server surface to claude and codex over stdio
(`devflow __ask`, plans 53-01, 53-05, 53-07). The matrix is that server surface, per the MCP 2025-11-25 specification
and the clients observed in 53-MCP-SPIKE.md.

| capability | decision | reason |
|---|---|---|
| initialize | INTEGRATE | |
| notifications/initialized | INTEGRATE | |
| ping | INTEGRATE | |
| tools/list | INTEGRATE | |
| tools/call | INTEGRATE | |
| notifications/cancelled | INTEGRATE | |
| server/discover | OPT-OUT | not needed: answered with -32601 so Claude falls back to initialize (spike Item 1; spec 2026-07-28 stdio backward compatibility) |
| tools/list_changed notification | OPT-OUT | not needed: the server's one tool never changes during a launch |
| resources/* | OPT-OUT | explicitly out of scope: the channel exposes one tool and no resources |
| prompts/* | OPT-OUT | explicitly out of scope: the channel exposes one tool and no prompts |
| completion/complete | OPT-OUT | explicitly out of scope: no prompt or resource arguments to complete |
| logging/setLevel and notifications/message | OPT-OUT | not needed: server logs go to stderr, which both clients capture |
| notifications/progress | OPT-OUT | not needed yet: the per-server client timeouts are raised instead (53-08); revisit if a client's idle window cannot be raised |
| tasks (task-augmented tools/call) | OPT-OUT | not needed yet: neither client's support is established (RESEARCH Alternatives); the blocking call is the contract |
| sampling/createMessage, elicitation/create, roots/list | OPT-OUT | explicitly out of scope: client features the server never requests |
