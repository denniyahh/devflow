# Memory Recall (MemPalace)

_Wing: devflow · Mode: augment · Transport: mcp_

Signal check: topic hits sat at cosine distance 0.37–0.45 (closet-boosted to 0.04–0.28); a nonsense-query control returned 0.70–0.75. The hits are real, not noise.

## Prior decisions
- **Phase 53.1, recorded 2026-11-22:** the remaining Plan→Validate resume gap was filed as 999.260 and not fixed. It is the same family as #281; check whether #281 and 999.260 are one defect. Source: KG fact, Phase 53.1.
- **Phase 53.1:** a resume past a fired `--until` cap advances through the shared `enter_stage`. A #281 fix should extend this path, not add a second one. Source: KG fact, valid_from 2026-11-22.
- **Phase 53.1:** gate guidance names `devflow gate reject N --note` and never a bare `--note`. #247's question-gate banner should keep this guidance consistent. Source: KG fact.
- **Phase 53:** DevFlow has no discuss stage. The open question at 53 discuss was how a closing run would produce real GSD discuss questions, and #246 is the answer to it. Source: drawer_devflow_decisions_8e775c86568bc7820706d044 (53-CONTEXT.md).
- **Phase 53:** a reviews replan was hand-run because "DevFlow's Plan stage has no reviews mode", which is exactly what #250 fixes. Source: drawer_devflow_planning_430a6521a2c7e1b53038ee7d (53-REVIEWS.md).
- **Phase 52.2-04/06:** `start` and `resume` hold the phase holder across guards, reservation and launch, and read runner_liveness under it. A Stopping unit is waited on with `wait_unit_inactive` (`TIMEOUT_STOP_SEC` + 5 s). This is the contract #297's stop-vs-resume fix must respect. Source: drawer_devflow_planning_a87562b8481c2fa1ac47995e.
- **Phase 52.2-20/06:** the test fakes for `systemctl`, `systemd-run` and `loginctl` live in `crates/devflow-cli/tests/common/mod.rs`. The fake systemd-run takes readiness from the runner lock's identity record, and a fake answers `unhandled: <args>` to any unplanned call. #296's change goes here. Source: drawer_devflow_planning_ac966aea6d7cebd5e101ea14, drawer_devflow_planning_40983325f97679a90fbbab1c.

## Patterns
- **Revert-once negative control before accepting a GREEN.** Source: KG Phase 53.1 uses_pattern.
- **Gate text as a pure function with the clock passed in.** This fits #249's ISO time and #247's banner. Source: KG Phase 53.1 uses_pattern.
- **Scripted GSD stub replies with `!exit <code>`.** Source: KG Phase 53.1 uses_pattern.
- **Publish by rename for files a poller reads.** Source: KG Phase 53.1 uses_pattern.
- **Run a hook byte copy in a scratch repo.** Source: KG Phase 53.1 uses_pattern.

## Surprises / gotchas
- **A negative control must catch the partial fix**, not only the full revert. Source: KG Phase 53.1 learned.
- **Plan text can name the wrong CLI shape or stale line numbers.** Check every path, test name and CLI shape against HEAD. Source: KG Phase 53.1 learned.
- **Moving a write site breaks a source-scan test.** This matters for #276's and #324's new source scans. Source: KG Phase 53.1 learned.
- **A planned gate can fail on pre-existing debt.** Source: KG Phase 53.1 learned.
- **Several backlog entries were already fixed.** Confirm #273 and #178 before planning work for them. Source: KG Phase 53.1 surprised_by.
- **Plan-time assumptions were wrong in the test rig.** Source: KG Phase 53.1 surprised_by.
- **The plan's mandated commit subjects exceeded 72 characters.** Keep the Conventional Commit subjects the plan specifies at 72 characters or fewer. Source: KG Phase 53.1 learned.
