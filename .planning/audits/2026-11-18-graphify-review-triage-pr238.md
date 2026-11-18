# Graphify review triage — PR #238 (2026-11-18)

PR #238 is `workspace/denniyahh -> develop` (opened 2026-11-17 by an agent session). The bot's investigation review on
head 1028d92 lists three bugs. Each was checked against the source; none is in code that runs.

| Sev | Bot finding | Verified | Disposition |
|-----|-------------|----------|-------------|
| P3 | `.planning/spikes/socket-supervisor/main.rs:31`: `status` and `parse` index args unchecked, so a missing value panics | yes, by reading the code | dismissed: throwaway spike its README marks "not built, not tested, not part of the workspace"; the socket supervisor it models was deleted in Phase 52.2 |
| P2 | same spike, `main.rs:110`: shutdown is not synchronized with the watcher's exit handling, so a stop can still write the advance marker | yes, by reading the code | dismissed: same spike; the race class is what 52.2 removed with the supervisor |
| P2 | `.planning/milestones/v3.0.0-phases/49-.../tools/check-evidence.sh:301`: the attempt gate counts `attempt-*/verdict.json` files without checking their `attempt` field or distinctness, so copied records can reach `cap_reached` | yes (lines 296-301 count files only) | dismissed: archived Phase 49 tool from the closed v3.0.0 milestone; nothing outside the archive references it (`rg -F check-evidence.sh`, 0 hits) |
