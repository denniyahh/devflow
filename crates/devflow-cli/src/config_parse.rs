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

/// Parse `DEVFLOW_GATE_TIMEOUT_SECS`'s raw value, falling back to 3 days on
/// an absent or unparsable value. Pure (no env access) so it's unit-testable
/// without mutating process-global env.
fn parse_gate_timeout(raw: Option<String>) -> u64 {
    const THREE_DAYS: u64 = 3 * 24 * 60 * 60;
    raw.and_then(|s| s.parse().ok()).unwrap_or(THREE_DAYS)
}

/// How long a background gate poll waits for a human response, configurable
/// via `DEVFLOW_GATE_TIMEOUT_SECS` (defaults to 3 days).
///
/// This bounds how long a monitor stays alive HOLDING THE PHASE LOCK while
/// parked at a gate — it does not bound how long the operator has to answer.
/// The gate request and the phase state are files; nothing expires them, and
/// timing out here is a clean resumable stop (`run_gate_with_timeout` emits
/// `gate_timeout` and returns `Err` — no abort, no cleanup, no state clear),
/// so an answer given a week later is still an answer. What the operator
/// loses past the timeout is only the automatic pickup: with no monitor left
/// polling, the written response sits unconsumed until a `devflow resume`
/// drives it.
///
/// Three days rather than the original seven: it still covers a gate that
/// fires on a Friday evening and is answered on Monday morning (~60h), which
/// is the longest absence this project treats as routine, while capping the
/// "immortal parked monitor" case that wedged phase 35.1 for nine hours in
/// 2026-08. Raise it via the env var for a genuinely unattended long run.
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
/// Both an unparsable value and an explicit zero fall back to the three-day
/// default — the fail-safe direction matters: a threshold of zero would
/// make an invoked sweep reap every open gate on the machine on its next
/// run, so a typo or an empty override must never resolve to "reap
/// everything."
fn parse_gate_max_unattended_age(raw: Option<String>) -> u64 {
    const THREE_DAYS: u64 = 3 * 24 * 60 * 60;
    match raw.and_then(|s| s.parse::<u64>().ok()) {
        Some(0) | None => THREE_DAYS,
        Some(secs) => secs,
    }
}

/// How long an open gate may sit unattended before an invoked `devflow gate
/// sweep` is willing to call it abandoned. This value gates nothing on its
/// own — the sweep it feeds is on-demand only (23-RESEARCH.md Open Question
/// 3); nothing schedules it.
///
/// Held EQUAL to [`gate_timeout_secs`], where it was previously six hours
/// against that function's seven days. The old split treated "how long to
/// wait for a human" and "how long before a sweep gives up on one" as
/// independent questions. They are not, because the sweep does not merely
/// report: `Gates::reap` writes an `abort:` response, and any monitor still
/// polling that gate consumes it on its next read and tears the phase down
/// via `abort()` — which clears state and deregisters, unlike the clean
/// resumable stop a poll timeout produces. A threshold SHORTER than the poll
/// timeout therefore means an operator who runs a sweep to tidy up abandoned
/// work destroys the run they were deliberately holding open over a weekend,
/// machine-wide across every registered root. Equality is the smallest value
/// that cannot do that: past the poll timeout, nothing is waiting any more.
///
/// Known edge at equality: a gate reaches this age at roughly the moment its
/// own poll gives up, so a sweep invoked inside that window can still catch a
/// live poller mid-backoff (`poll_response` caps its sleep at 60s) and turn
/// what would have been a timeout into an abort. Set this above
/// `DEVFLOW_GATE_TIMEOUT_SECS` if that distinction matters for a given run.
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
        assert_eq!(parse_gate_max_unattended_age(None), 3 * 24 * 60 * 60);
    }

    #[test]
    fn max_unattended_age_parses_explicit_value() {
        assert_eq!(parse_gate_max_unattended_age(Some("900".into())), 900);
    }

    #[test]
    fn max_unattended_age_defaults_on_unparsable() {
        assert_eq!(
            parse_gate_max_unattended_age(Some("nonsense".into())),
            3 * 24 * 60 * 60
        );
    }

    #[test]
    fn max_unattended_age_defaults_on_explicit_zero() {
        assert_eq!(
            parse_gate_max_unattended_age(Some("0".into())),
            3 * 24 * 60 * 60
        );
    }

    /// The sweep threshold must never be SHORTER than the poll timeout — a
    /// sweep that outruns a live poller converts a clean, resumable timeout
    /// into an `abort()` that clears state, destroying a run the operator was
    /// deliberately holding open. Pinned as an invariant rather than as two
    /// independent literals so a future change to either default has to
    /// confront the relationship instead of silently re-opening the gap.
    #[test]
    fn sweep_threshold_is_not_shorter_than_the_gate_poll_timeout() {
        assert!(
            parse_gate_max_unattended_age(None) >= parse_gate_timeout(None),
            "sweep max-age ({}) must be >= the gate poll timeout ({}), or an \
             invoked sweep reaps gates a live monitor is still polling",
            parse_gate_max_unattended_age(None),
            parse_gate_timeout(None),
        );
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
        const THREE_DAYS: u64 = 3 * 24 * 60 * 60;
        assert_eq!(parse_gate_timeout(Some("42".into())), 42);
        assert_eq!(parse_gate_timeout(Some("bad".into())), THREE_DAYS);
        assert_eq!(parse_gate_timeout(None), THREE_DAYS);
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
