# Phase 48 fix-round review — orchestrator verification

Review base: `f745bd5` (git archive snapshot; the only snapshot delta during the run was a graphify cache
stamp). The diff under review was `3f6669b..9a33c70`: fixes A, E and D from the first round. The prompt
asked for other instances of each defect class, not only the fixes themselves. Lanes: codex
(`gpt-5.6-terra`, high, 9.4 min, exit 0) and agy (`gemini-3.8-flash-high`, high, 14.7 min, exit 0).
Neither dropped. Every finding was re-traced against source before classification; dispositions are the
operator's decisions of 2026-09-22.

| Finding | Lanes | Verdict | Disposition |
|---------|-------|---------|-------------|
| `recover --clean --phase N` on a phase with nothing on disk prints "cleaned up" (class D) | codex C-3, agy C-5 | CONFIRMED — `clean_phase` is idempotent and the CLI printed unconditionally | Fixed `d36936e` (`clean_phase_report.found_anything`; e2e + core tests with a lone-gate control) |
| The implicit sweep clears state but leaves gate files | agy C-2 | CONFIRMED — `clean_report` called only `clear_state` | Fixed `854bbce` (gate cleanup under the lock; kept-phase control) |
| `abort` prints "aborted" and returns `Ok` after discarding cleanup and state-clear errors (class D) | codex C-2, agy C-7 | CONFIRMED — `let _ =` on both, predates Phase 48 (`33f7962`) | Backlog 999.133 |
| `devflow cleanup` prints "no worktrees to clean up" and exits 0 after every removal failed (class D) | agy C-6 | CONFIRMED — `removed == 0` covers both "none present" and "all failed" | Backlog 999.133 |
| `recover --phase N` inspection prints nothing and exits 0 when N has no state but others do | agy S-1 (suspected) | CONFIRMED from source — the phase filter skips every entry | Backlog 999.133 |
| `status` → `list_states` migrates a legacy `state.json` without the lock (class A) | codex C-1, agy C-3 | CONFIRMED mechanically (since Phase 14) | Ignored: operator decision, no pre-Phase-14 binary is in use |
| The sweep's orphan-cron pass deletes cron records without the lock (class A) | agy C-1 | Mechanism real, no demonstrated harm — cron records are written only after `save_state`, and the one live state-less window is the finish path's own `clear_state` → `consume_cron_instructions`, which deletes that record itself. codex reached the same conclusion under CHECKED AND CLEAN. | No action |
| `gate approve|reject` accepts an Unconfirmable holder while `stop` refuses one (class E) | agy C-4 | REFUTED — intended: 48-CONTEXT records that Unconfirmable "claims neither a waiter nor its absence", `stop` is the stricter destructive path, and on non-Linux hosts every holder is Unconfirmable, so refusing would disable `gate approve` there | No action |

Class E (answer writers applying a weaker holder rule than `stop` for Recycled holders): both lanes
enumerated every response writer and found no remaining instance.
