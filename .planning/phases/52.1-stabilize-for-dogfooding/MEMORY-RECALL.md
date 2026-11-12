# Memory Recall (MemPalace)

_Wing: devflow · Mode: augment · Transport: mcp_

Run 2026-11-12, after the plans were committed (`48582cd`), so this is a cross-check of the committed plans, not
planner input. The plan-time `plan:pre` hook was not dispatched during `/gsd-plan-phase 52.1` (manual mode).
Score floor used: `max_distance` 0.55; a nonsense query scores about 0.62–0.63 distance on this palace.

## Prior decisions
- **R-6 (Phase 50, 50-14): a probe error is refused and never reaches the backstop.** D-21/D-25 (operator,
  2026-11-11) deliberately reverse this for `stop`. They hold up because the backstop's own identity rule still
  stands: "the backstop never signals a process group it cannot prove is the original: a reused pgid and a
  leader-gone group are never signalled (R-4)". Plan 52.1-08 must keep R-4's controls green. —
  drawer_devflow_planning_d9368d7c6a16fb32d2e19213 (50-14-PLAN), drawer_devflow_planning_08a7702e556e23ed496531f9
  (50-09-PLAN), drawer_devflow_milestones_8be23547dc5ef37e3da5877f (50-14-SUMMARY)
- **Never discard unmerged work at cleanup (Phase 12, 12-04): non-force branch delete is intentional.** This matches
  D-10 ("no path deletes an unmerged branch implicitly"). — drawer_devflow_planning_346cf64e4e07a88aada72e90
- **The >50-commits check began as a "pre-start divergence check: warn if develop has advanced" (Phase 11r).** That was
  before later work (F3, CR-45-03) made both fork paths cut the phase branch from the resolved base. This is
  consistent with D-24's finding that the check now guards nothing. — drawer_devflow_planning_ce08f6349c490b0fb2ad7373
- **The write lock's descriptor is close-on-exec (Phase 50.1, D-08)**, which does not cover the window between fork and
  exec. This is consistent with D-05's "a close-on-exec audit proves nothing" and the fork-held test helper. —
  drawer_devflow_planning_fb6118af4abde9b9540c598c, drawer_devflow_decisions_29756e1e4e64fb3ef47d7359

## Patterns
- **Honest disclosure over a racy test (Phase 25, 25-07):** a branch reachable only through a real external race was
  left untested and documented, not covered with a flaky test. This applies to 999.223's "not reproduced live" cause. —
  drawer_devflow_milestones_8f3cf5296adce00d7106fe76
- **The `stop` / `stop_in(cache_dir)` split** keeps in-process tests from sweeping the operator's real cache (50-14).
  Checked 2026-11-12: 52.1-08's new `stop` tests are end-to-end tests in `stop_e2e.rs` under `HermeticHome`, so they
  cannot sweep the real cache and this pattern does not apply. 52.1-08 gates R-4's
  `supervisor::backstop::tests::classify_covers_every_rule_in_order` by name. —
  drawer_devflow_milestones_8be23547dc5ef37e3da5877f

## Surprises / gotchas
- **The palace already holds a 2026-11-11 copy of 52.1-CONTEXT.md** (decisions room, staged from a session
  scratchpad). It predates D-20..D-27, so it is stale on `stop` over ProbeFailed (it lacks D-21/D-25), the stale-base
  check (it lacks D-24) and the start guard design (it lacks D-26/D-27). A later capture must supersede those drawers,
  not add beside them. — e.g. drawer_devflow_decisions_32e3f00f41b6c0a6398514e9
- **The KG facts for Phase 52.1 are stale:** `proposed_scope` and `proposed_exit_criterion` both read "auto,
  unconfirmed", valid_from 2026-11-11. The operator accepted the scope that day (commit `a05ebf4`). Capture should
  supersede them (`mempalace_kg_supersede`). — KG entity "Phase 52.1"
- **"Phase 52" has no KG facts** although Phase 52 is complete, so the KG does not record its completion.
</content>
</invoke>
