# Phase 28 — Deferred Items (out-of-scope discoveries)

Issues discovered during execution that are unrelated to the current task's
changes and therefore not auto-fixed, per the executor's scope-boundary rule.

## 1. `cargo test -p devflow-core <filter>` fails to compile (pre-existing, unrelated to 28-01)

**Found during:** 28-01 Task 2.

**Issue:** `crates/devflow-core/tests/devflow_dir_gitignore.rs` and
`crates/devflow-core/tests/monitor_e2e.rs` both reference
`devflow_core::test_support::*`, which is gated behind the `test-support`
feature (`#[cfg(any(test, feature = "test-support"))]`,
`crates/devflow-core/src/lib.rs:78-79`). That feature is enabled implicitly
when running `cargo test --workspace` (Cargo unifies features across the
resolve graph because `devflow-cli`'s `[dev-dependencies]` declares
`devflow-core = { workspace = true, features = ["test-support"] }`,
`crates/devflow-cli/Cargo.toml:23`), but a scoped `cargo test -p devflow-core
<filter>` invocation does **not** pull in `devflow-cli`'s dev-dependency
declaration, so the feature is off and both integration test binaries fail
with `E0433: cannot find test_support in devflow_core`.

**Effect:** The plan's own `<verify>` command for Task 2
(`cargo test -p devflow-core verify::tests:: ...`) fails to compile as
literally written. Workaround used locally: `cargo test -p devflow-core
--features test-support verify::tests::`. The phase-level gate,
`scripts/check.sh test` (== `cargo test --workspace --no-fail-fast`), is
unaffected — it enables the feature transitively via `devflow-cli`'s
dev-dependency and was confirmed green (see 28-01-SUMMARY.md).

**Not fixed here:** out of scope for 28-01 (touches `tests/` files this
plan's `<files>` list does not name, and the fix — adding
`--features test-support` to `scripts/check.sh`'s targeted single-package
test convenience commands, or restructuring the feature gate — is a
cross-cutting build-tooling decision, not a Task 2 concern).

**Suggested follow-up:** a small standalone fix (either document
`--features test-support` as the required scoped-test invocation in
`CONTRIBUTING.md`/this skill's `rules/change-acceptance.md`, or move
`test_support` out from behind a default-off feature) — candidate for a
future backlog item.
