# Graphify review triage: PR #229 (Phase 50.1), head 2c80135

Bot gate: PASS (no health regressions, nothing blocking). Review partial: the diff was larger than one pass covers. Formal verification proved 0 of 279 changed functions (49 unsupported, 230 over the cap), so it gives no assurance either way.

| Finding (bot's wording) | Bot's status | Verdict | Basis |
|---|---|---|---|
| Manual stop guidance can kill an unrelated process group after PID reuse | unverified hypothesis | Dismissed, one wording nit | OPERATIONS.md already gates the manual `kill -TERM -- -<pid>` on a `ps -o pid,pgid,lstart,args -p <pid>` check when the pid is unconfirmed, and says a pid read from `phase-NN-agent-pid` can have been reused. A confirmed pid comes from a record whose start time is checked. Nit: it offers "end that process" beside "remove the pid file" for a reused pid; removing the pid file is the safe one. Fixed in the v3.0.0 release PR. |
| Doctor's GONE repair omits Ship-gate recovery | unverified hypothesis | Real, docs only | `commands.rs` (`PhaseLiveness::GoneWhileActive`) adds `devflow ship --phase N` beside `resume` when a Ship gate is pending with no stored answer; the doctor bullet in OPERATIONS.md lists only `resume`. Code is right, doc is incomplete. Fixed in the v3.0.0 release PR. |
| Doc staleness advisory (CHANGELOG sections "reference changed symbols `git`, `end`, `start`...") | advisory | Dismissed | Matches on common words, not on the symbols; the cited sections are old release notes that must not change. |
| Coupling "worse" list (`hermetic_devflow`, `handle_validate_outcome`, ...) | advisory | Not triaged | Bot baseline is the default branch and its "new" labels were provably false in earlier close-outs; none is a blocking finding. |

Hit rate on this PR: 1 real (docs) of 2 findings.
