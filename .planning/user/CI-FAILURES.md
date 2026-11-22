# CI failure ledger

Operator rule (2026-11-20): every CI or pre-push check failure is logged here, flaky or not, so a pattern can be
seen across runs. One row per failure. If a failure is a real defect or recurs, also file it as a 999.x entry and
name that entry in the row.

| Date | Where | Ref / head | Failing check or test | Reproduced? | Outcome / 999.x |
|---|---|---|---|---|---|
| 2026-11-20 | local pre-push (`check-in-container.sh all`, then `check.sh service`) | feature/phase-53-pr f2c8349 | Unknown. The tool kept only the first 30 KB of 210 KB of output, so the failing test's name was lost. | No. Re-runs passed: container `all`, host `service`, then the full push. | Unidentified, likely a flake. From now on, every push writes its output to a file. |
| 2026-11-20 | local pre-push (`check-in-container.sh all`, cargo fmt step) | workspace/denniyahh after the Phase 53 merge and develop sync | No test failed: rustup in the container failed the DNS lookup for static.rust-lang.org while syncing toolchain 1.97.1 | Retried at once; the retry passed the full pre-push check and pushed | Environment, not code |
| 2026-11-21 | GitHub CI, PR #251: Test and Sequential 2-CPU check (push and pull_request runs) | feature/phase-53.1-pr 4c5fb89 | `the_live_payload_commits_a_context_file_on_integration`: git exit 128, "detected dubious ownership in repository at '/__w/devflow/devflow'". The test set `GIT_CONFIG_GLOBAL=/dev/null`, which hid the CI image's global `safe.directory` from a checkout owned by another user. The local container check runs as the same user, so it passed there. | Yes, locally, with `GIT_TEST_ASSUME_DIFFERENT_OWNER=1`: same error text | Real defect in the 999.255 test, fixed in this PR: the test now uses a scratch global git config that trusts every directory. It passes under the same condition. |
