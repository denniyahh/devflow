# Memory Recall (MemPalace)

_Wing: devflow · Mode: augment · Transport: mcp · Control: a nonsense query scored similarity ≤0.175; the hits below scored 0.49–0.65._

## Prior decisions
- Gate answers are keyed by `gate_id`; the answer body carries `gate_id`, and the unkeyed per-stage response slot was retired (Phase 50.1-06, D-05). This is the documented operator interface and part of `devflow-core`'s public API. A `GateKind::Question` answer should reuse this binding. — drawer_devflow_planning_762072810583e4a5cdf1c37e (50.1-06-PLAN.md)
- 999.57 part B: a multi-option `checkpoint:decision` cannot reliably receive an operator answer under a one-shot launch. Earlier plans worked around it with `checkpoint:human-verify` or with conditional next steps. Phase 53 is meant to close this defect, not work around it. — drawer_devflow_planning_33a32c5ebb1e58d3c6b6153f (30-02-PLAN.md)

## Patterns
- Phase 28 recorded the stdout shape of a headless `claude -p` `blocking-human` checkpoint as an observed probe fact (28-PROBE.md) rather than an assumption. 53-MCP-SPIKE.md plays the same role for MCP client behaviour. — drawer_devflow_planning_7b20d302da924e5c5a9db17f (28-01-PLAN.md)
- When a one-way authorization depends on evidence, split the decision so that it is asked only after the evidence exists. — drawer_devflow_planning_395e3bec7cb7900d488dd19d (23-10-PLAN.md)

## Surprises / gotchas
- Plans that carry `gate="blocking-human"` were executable only through `/gsd-execute-phase` in a session, not through a DevFlow one-shot launch (Phase 50-19). The Phase 53 stage prompts must route such gates through `devflow_ask`. — drawer_devflow_planning_b0d48f1918560043b4df2066 (50-19-PLAN.md)
