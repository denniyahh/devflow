# CI failure ledger

Operator rule (2026-11-20): every CI or pre-push check failure is logged here, flaky or not, so a pattern can be
seen across runs. One row per failure. If a failure is a real defect or recurs, also file it as a 999.x entry and
name that entry in the row.

| Date | Where | Ref / head | Failing check or test | Reproduced? | Outcome / 999.x |
|---|---|---|---|---|---|
| 2026-11-20 | local pre-push (`check-in-container.sh all`, then `check.sh service`) | feature/phase-53-pr f2c8349 | Unknown. The tool kept only the first 30 KB of 210 KB of output, so the failing test's name was lost. | No. Re-runs passed: container `all`, host `service`, then the full push. | Unidentified, likely a flake. From now on, every push writes its output to a file. |
| 2026-11-20 | local pre-push (`check-in-container.sh all`, cargo fmt step) | workspace/denniyahh after the Phase 53 merge and develop sync | No test failed: rustup in the container failed the DNS lookup for static.rust-lang.org while syncing toolchain 1.97.1 | Retried at once (see the next row) | Environment, not code |
