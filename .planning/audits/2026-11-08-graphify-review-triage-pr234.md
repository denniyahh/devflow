# Graphify review triage: PR #234 (Phase 51), head 241f22d

Bot verdict: gate PASS, "Worth a look" with 2 advisory findings (review partial: diff larger than one pass). Inline
"Health regression" coupling notes skipped per the close-out procedure (stale baseline).

| Finding | Check | Decision |
|---|---|---|
| "Fallback is not necessarily once per run" (`OPERATIONS.md:291`) | Read lines 289-300: the sentence says the hop happens once per run and, in the same sentence, that `resume --fallback-agent` re-arms it. `resume --agent` handoffs do not consume it. | Wording imprecise, behavior documented. Filed with the README item as 999.211; not merge-blocking. |
| "README overstates what qualifies as a rate limit" (`README.md:110`) | "if an agent's stdout contains rate-limit messages (429)": the stream path classifies only a top-level `rate_limit_event` with `status: "rejected"`; the single-document path matches nested fields (999.196). The wording predates Phase 51 (`origin/develop` has it); Phase 51 extended the sentence. | Valid docs defect, pre-existing. Filed as 999.211; not merge-blocking. |

Decided by the session (close-out step 3a), 2026-11-08.
