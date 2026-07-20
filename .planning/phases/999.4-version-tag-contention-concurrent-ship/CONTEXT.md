---
status: backlog
source: Phase 17 dogfood run (2026-07-18/19), moved from ROADMAP Phase 19 (19h) on 2026-07-20
---

# Backlog: Version-Tag Contention on Concurrent Ship

## Goal

Two phases compute the same next version and race to create one tag;
17-09 bounded the *test* (a 2-second gate-timeout poll under `ENV_MUTEX`,
production default untouched) but deliberately left the product race open.
Proven real — instrumentation caught both phases inside `version_bump`
with identical computed versions ~1.8ms apart. See `17-09-SUMMARY.md` and
`STATE.md`'s 2026-07-19 blocker-resolution entry for the full mechanism
(the checkout lock occasionally fails to fully serialize the two threads'
terminal hooks).

## Notes

Low frequency (requires two phases shipping concurrently and landing on
the identical computed version), but a real correctness gap in
`version_bump`, not just test flakiness.

Promote with `/gsd-review-backlog` when ready.
