//! Env-parsing helpers for the pipeline's tunable timeouts, plus the gate
//! escalation threshold constant.
//!
//! Given its own module rather than folded back into the thin `main.rs`
//! crate root (CONTEXT.md left this to discretion): each parser's tests
//! (`parse_gate_timeout_env_override`, `parse_checkout_lock_timeout_defaults_and_parses`)
//! were carried here verbatim per the plan's test-attribution rule. Keeping
//! this cluster in its own file keeps configuration parsing and its tests out
//! of the thin crate root; `main.rs` retains only the test for its own
//! `project_root` helper.

/// A pending gate becomes visually urgent after thirty minutes without an
/// answer. The banner remains visible before and after this threshold.
pub(crate) const GATE_ESCALATION_THRESHOLD_SECS: u64 = 30 * 60;

/// Parse `DEVFLOW_GATE_TIMEOUT_SECS`'s raw value, falling back to 7 days on
/// an absent or unparsable value. Pure (no env access) so it's unit-testable
/// without mutating process-global env.
fn parse_gate_timeout(raw: Option<String>) -> u64 {
    const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
    raw.and_then(|s| s.parse().ok()).unwrap_or(SEVEN_DAYS)
}

/// How long a background gate poll waits for a human response, configurable
/// via `DEVFLOW_GATE_TIMEOUT_SECS` (defaults to 7 days).
pub(crate) fn gate_timeout_secs() -> u64 {
    parse_gate_timeout(std::env::var("DEVFLOW_GATE_TIMEOUT_SECS").ok())
}

/// Parse `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`, falling back to 60s. Pure
/// (no env access) so it's unit-testable without mutating process-global env.
fn parse_foreground_gate_timeout(raw: Option<String>) -> u64 {
    const DEFAULT_SECS: u64 = 60;
    raw.and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_SECS)
}

/// How long the FOREGROUND `devflow ship --phase` manual override (WR-02,
/// phase 20 review) waits for a re-opened Ship gate to be answered before
/// failing fast, configurable via `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`
/// (defaults to 60s).
///
/// Every other caller of `finish_workflow`/`run_gate` runs inside a detached
/// monitor process, so [`gate_timeout_secs`]'s multi-day production default
/// is invisible to an operator's terminal. `ship_override` calls
/// `finish_workflow` directly from the foreground CLI — if a terminal-hook
/// failure reopens the Ship gate, waiting out the multi-day default would
/// block the operator's shell for however long the gate takes to resolve.
/// This bound only caps how long the FOREGROUND wait can run before erroring
/// out with an actionable message; it does not weaken the fail-closed
/// terminal-Ship invariant — an unanswered gate still fails the operation
/// entirely, exactly as [`gate_timeout_secs`]'s timeout does today, just
/// after seconds instead of days.
pub(crate) fn foreground_gate_timeout_secs() -> u64 {
    parse_foreground_gate_timeout(std::env::var("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS").ok())
}

/// Parse `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, falling back to 120s. Pure
/// (no env access) so it's unit-testable without mutating process-global env.
fn parse_checkout_lock_timeout(raw: Option<String>) -> std::time::Duration {
    const DEFAULT_SECS: u64 = 120;
    std::time::Duration::from_secs(raw.and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_SECS))
}

/// How long a caller waits out a sibling phase's short critical section on
/// the project-wide checkout lock before giving up, configurable via
/// `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` (defaults to 120s) — generous
/// relative to the seconds the lock is held for, tiny relative to a gate
/// wait.
pub(crate) fn checkout_lock_timeout() -> std::time::Duration {
    parse_checkout_lock_timeout(std::env::var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS").ok())
}

/// Parse the raw value of the sweep's max-unattended-age override. Pure (no
/// env access) so it's unit-testable without mutating process-global env.
/// Both an unparsable value and an explicit zero fall back to the six-hour
/// default — the fail-safe direction matters: a threshold of zero would
/// make an invoked sweep reap every open gate on the machine on its next
/// run, so a typo or an empty override must never resolve to "reap
/// everything."
fn parse_gate_max_unattended_age(raw: Option<String>) -> u64 {
    const SIX_HOURS: u64 = 6 * 60 * 60;
    match raw.and_then(|s| s.parse::<u64>().ok()) {
        Some(0) | None => SIX_HOURS,
        Some(secs) => secs,
    }
}

/// How long an open gate may sit unattended before an invoked `devflow gate
/// sweep` is willing to call it abandoned. Deliberately independent of and
/// far shorter than [`gate_timeout_secs`]'s multi-day default: "how long to
/// wait for a human" and "how long before a sweep gives up on one" are
/// different questions, and conflating them is arguably what let the
/// forensics record's oldest orphaned gates reach thirty hours. This value
/// gates nothing on its own — the sweep it feeds is on-demand only
/// (23-RESEARCH.md Open Question 3); nothing schedules it.
pub(crate) fn gate_max_unattended_age_secs() -> u64 {
    parse_gate_max_unattended_age(std::env::var("DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS").ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 23b (Task 2): the four fail-safe cases named in the plan's behavior
    /// block — absent, explicit, unparsable, and explicit-zero. Named with a
    /// `max_unattended_age_` prefix (not `parse_gate_max_unattended_age_`)
    /// so these test names don't collide with the plan's literal `rg -n 'fn
    /// parse_gate_max_unattended_age'` acceptance grep, which must match
    /// only the function definition (23-03 precedent: `dereg_*` renaming).
    #[test]
    fn max_unattended_age_defaults_when_absent() {
        assert_eq!(parse_gate_max_unattended_age(None), 6 * 60 * 60);
    }

    #[test]
    fn max_unattended_age_parses_explicit_value() {
        assert_eq!(parse_gate_max_unattended_age(Some("900".into())), 900);
    }

    #[test]
    fn max_unattended_age_defaults_on_unparsable() {
        assert_eq!(
            parse_gate_max_unattended_age(Some("nonsense".into())),
            6 * 60 * 60
        );
    }

    #[test]
    fn max_unattended_age_defaults_on_explicit_zero() {
        assert_eq!(parse_gate_max_unattended_age(Some("0".into())), 6 * 60 * 60);
    }

    #[test]
    fn parse_checkout_lock_timeout_defaults_and_parses() {
        assert_eq!(
            parse_checkout_lock_timeout(None),
            std::time::Duration::from_secs(120)
        );
        assert_eq!(
            parse_checkout_lock_timeout(Some("5".into())),
            std::time::Duration::from_secs(5)
        );
        assert_eq!(
            parse_checkout_lock_timeout(Some("nope".into())),
            std::time::Duration::from_secs(120)
        );
    }

    /// `parse_gate_timeout` is a pure function — no env mutation needed, so
    /// this test cannot race any other test.
    #[test]
    fn parse_gate_timeout_env_override() {
        const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
        assert_eq!(parse_gate_timeout(Some("42".into())), 42);
        assert_eq!(parse_gate_timeout(Some("bad".into())), SEVEN_DAYS);
        assert_eq!(parse_gate_timeout(None), SEVEN_DAYS);
    }

    /// WR-02: the foreground bound defaults to 60s — orders of magnitude
    /// shorter than `gate_timeout_secs`' multi-day production default — and
    /// is independently configurable/pure like its sibling.
    #[test]
    fn parse_foreground_gate_timeout_env_override() {
        assert_eq!(parse_foreground_gate_timeout(Some("5".into())), 5);
        assert_eq!(parse_foreground_gate_timeout(Some("bad".into())), 60);
        assert_eq!(parse_foreground_gate_timeout(None), 60);
    }
}
