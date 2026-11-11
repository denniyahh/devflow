//! Reset-time parsing for the rate-limit gate (D-27.1).
//!
//! An agent that hits a rate limit reports when the limit lifts, as raw text in
//! its output stream. This module turns that text into an exact UTC instant.
//!
//! **It never guesses a time.** Text it cannot read is `None`, and the caller
//! opens a plain gate instead of a timed one (D-16).
//!
//! It does not own scheduling, waiting, relaunching or the gate itself; it only
//! reads a timestamp and owns the civil-date arithmetic other modules format
//! timestamps with.

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 60 * SECS_PER_MINUTE;
const SECS_PER_DAY: i64 = 24 * SECS_PER_HOUR;

/// How long after a reported reset the one automatic relaunch waits (D-27.2,
/// D-16): a fixed margin, so a relaunch never lands on the reset second
/// itself. Deliberately a constant: there is no environment variable or
/// config key for it.
pub const RATE_LIMIT_RELAUNCH_MARGIN_SECS: i64 = 60;

/// The not-before time of a rate-limit gate whose denial named `reset_at`:
/// the reset plus [`RATE_LIMIT_RELAUNCH_MARGIN_SECS`], saturating rather than
/// overflowing on an absurd reset.
pub fn relaunch_not_before(reset_at: i64) -> i64 {
    reset_at.saturating_add(RATE_LIMIT_RELAUNCH_MARGIN_SECS)
}

/// Whether the one automatic relaunch is due at `now` for a denial that
/// named `reset_at` (D-27.2): true exactly when `now` has reached
/// [`relaunch_not_before`].
pub fn auto_relaunch_due(now: i64, reset_at: i64) -> bool {
    now >= relaunch_not_before(reset_at)
}

/// The exact UTC instant, in Unix epoch seconds, that `input` names.
///
/// Two forms are read, after trimming surrounding whitespace:
///
/// - non-negative epoch seconds, optionally with a decimal fraction;
/// - an RFC 3339-like timestamp (`YYYY-MM-DDTHH:MM[:SS[.frac]]`, `T` or a
///   space as the separator) with no offset, `Z`, or a `±HH:MM`, `±HHMM` or
///   `±HH` offset, normalised to UTC.
///
/// A fractional second is rounded UP to the next whole second, so the result
/// is never earlier than the instant the input names. Anything else is `None`,
/// including a sign on epoch seconds, a value that overflows `i64`, and an
/// offset that is present but malformed: that is refused rather than read as
/// UTC.
pub fn parse_reset_time(input: &str) -> Option<i64> {
    let input = input.trim();
    parse_epoch_seconds(input).or_else(|| parse_rfc3339ish(input))
}

/// `digits` or `digits.digits` as whole epoch seconds, fraction rounded up.
fn parse_epoch_seconds(input: &str) -> Option<i64> {
    let (whole, fraction) = match input.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (input, None),
    };
    if !is_ascii_digits(whole) {
        return None;
    }
    let seconds = whole.parse::<i64>().ok()?;
    match fraction {
        None => Some(seconds),
        Some(fraction) if !is_ascii_digits(fraction) => None,
        Some(fraction) if fraction.bytes().all(|b| b == b'0') => Some(seconds),
        Some(_) => seconds.checked_add(1),
    }
}

/// True when `text` is one or more ASCII digits and nothing else.
fn is_ascii_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// An RFC 3339-like timestamp as UTC epoch seconds, fraction rounded up.
fn parse_rfc3339ish(input: &str) -> Option<i64> {
    let split_at = input.find('T').or_else(|| input.find(' '))?;
    let (date, rest) = input.split_at(split_at);
    let time = rest.get(1..)?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let (time, offset_minutes) = split_time_and_offset(time)?;
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let (second, round_up) = match time_parts.next() {
        None => (0, false),
        Some(seconds) => parse_seconds_field(seconds)?,
    };
    if time_parts.next().is_some() {
        return None;
    }
    if month == 0 || month > 12 || day == 0 || day > 31 || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    days_from_civil(year, month, day)
        .checked_mul(SECS_PER_DAY)?
        .checked_add(i64::from(hour) * SECS_PER_HOUR)?
        .checked_add(i64::from(minute) * SECS_PER_MINUTE)?
        .checked_add(i64::from(second))?
        .checked_sub(i64::from(offset_minutes) * SECS_PER_MINUTE)?
        .checked_add(i64::from(round_up))
}

/// `SS` or `SS.frac` as the whole second and whether a non-zero fraction
/// requires rounding up.
fn parse_seconds_field(field: &str) -> Option<(u32, bool)> {
    let (whole, fraction) = match field.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (field, None),
    };
    if !is_ascii_digits(whole) {
        return None;
    }
    let second = whole.parse::<u32>().ok()?;
    match fraction {
        None => Some((second, false)),
        Some(fraction) if !is_ascii_digits(fraction) => None,
        Some(fraction) => Some((second, fraction.bytes().any(|b| b != b'0'))),
    }
}

/// Split the clock time from its UTC offset, in minutes.
///
/// No offset, or a bare `Z`, is a zero offset. An offset that is present but
/// that [`parse_offset_minutes`] refuses is `None`: reading `+99:00` as UTC
/// would invent a deadline the agent never reported (D-16, D-27.1).
fn split_time_and_offset(time: &str) -> Option<(&str, i32)> {
    let trimmed = time.trim_end_matches('Z');
    if trimmed.len() > 6 {
        if let Some(idx) = trimmed.rfind('+') {
            return Some((&trimmed[..idx], parse_offset_minutes(&trimmed[idx..])?));
        }
        if let Some(idx) = trimmed.rfind('-')
            && idx > 0
        {
            return Some((&trimmed[..idx], parse_offset_minutes(&trimmed[idx..])?));
        }
    }
    Some((trimmed, 0))
}

fn parse_offset_minutes(offset: &str) -> Option<i32> {
    // WR-07 (13-REVIEW.md), revised: accept the three ISO-8601 offset forms
    // — ±HH:MM, ±HHMM, and hour-only ±HH — with bound-checked values.
    // Requiring a colon (the first WR-07 fix) silently rescheduled valid
    // ±HH/±HHMM timestamps to UTC through the callers' `unwrap_or(0)`,
    // firing the resume cron hours off; the original pre-WR-07 code misread
    // ±HHMM as HHMM *hours*. Anything else (wrong digit count, out-of-range
    // values) still fails safe as None. `retry_after` is raw agent output,
    // so no producer guarantees one form.
    const MAX_OFFSET_HOURS: i32 = 23;
    const MAX_OFFSET_MINUTES: i32 = 59;
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let rest = offset.get(1..)?;
    let (hours_part, minutes_part) = match rest.split_once(':') {
        Some((hours, minutes)) => (hours, minutes),
        None => match rest.len() {
            2 => (rest, "0"),              // ±HH
            4 => (&rest[..2], &rest[2..]), // ±HHMM
            _ => return None,
        },
    };
    let hours = hours_part.parse::<i32>().ok()?;
    let minutes = minutes_part.parse::<i32>().ok()?;
    if !(0..=MAX_OFFSET_HOURS).contains(&hours) || !(0..=MAX_OFFSET_MINUTES).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 60 + minutes))
}

/// The number of days from 1970-01-01 to the given proleptic-Gregorian date.
pub(crate) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = i64::from(year).div_euclid(400);
    let yoe = i64::from(year) - era * 400;
    let month = i64::from(month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic-Gregorian `(year, month, day)` of the day `days` after
/// 1970-01-01.
pub(crate) fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D-27.2, CUT-03 boundary: the automatic relaunch is due exactly at the
    /// reset plus the margin. One second before it is not due; at and after it
    /// is.
    #[test]
    fn auto_relaunch_is_due_exactly_at_reset_plus_margin() {
        let reset = august_2_2026_midnight_utc();
        let due_at = reset + RATE_LIMIT_RELAUNCH_MARGIN_SECS;
        assert_eq!(RATE_LIMIT_RELAUNCH_MARGIN_SECS, 60);
        assert_eq!(relaunch_not_before(reset), due_at);
        assert!(!auto_relaunch_due(due_at - 1, reset), "one second early");
        assert!(auto_relaunch_due(due_at, reset), "at the reset plus margin");
        assert!(auto_relaunch_due(due_at + 1, reset), "after it");
        // Control: at the reset itself, before the margin, it is not due.
        assert!(!auto_relaunch_due(reset, reset), "at the bare reset");
        // An absurd reset saturates instead of overflowing.
        assert_eq!(relaunch_not_before(i64::MAX), i64::MAX);
    }

    /// An epoch-seconds reset is read to the exact second, surrounding
    /// whitespace is ignored, and a fraction is rounded UP: the deadline must
    /// never name an instant earlier than the one the agent reported.
    #[test]
    fn an_epoch_seconds_reset_parses_to_the_exact_second() {
        assert_eq!(parse_reset_time("1785645600"), Some(1_785_645_600));
        assert_eq!(parse_reset_time(" 1785645600 "), Some(1_785_645_600));
        assert_eq!(parse_reset_time("1785645600.2"), Some(1_785_645_601));
        // A zero fraction names the whole second itself: no rounding.
        assert_eq!(parse_reset_time("1785645600.000"), Some(1_785_645_600));
    }

    /// Text that names no instant is `None`, never a guessed time.
    #[test]
    fn an_empty_or_invalid_reset_is_none() {
        for input in [
            "",
            "   ",
            "soon",
            "-5",
            "99999999999999999999",
            "1785645600.",
            "1785645600.5x",
            "1785645600.-1",
        ] {
            assert_eq!(parse_reset_time(input), None, "input {input:?}");
        }
    }

    /// 2026-08-02T00:00:00Z in epoch seconds, computed from the calendar
    /// rather than written as a literal so the tests do not trust hand
    /// arithmetic.
    fn august_2_2026_midnight_utc() -> i64 {
        const SECS_PER_DAY: i64 = 86_400;
        days_from_civil(2026, 8, 2) * SECS_PER_DAY
    }

    /// The same instant written as epoch seconds, as RFC 3339 UTC, and with a
    /// positive or negative offset reads to one value.
    #[test]
    fn epoch_and_rfc3339_name_the_same_instant() {
        let instant = august_2_2026_midnight_utc();
        assert_eq!(parse_reset_time(&instant.to_string()), Some(instant));
        for input in [
            "2026-08-02T00:00:00Z",
            "2026-08-02T02:00:00+02:00",
            "2026-08-01T19:00:00-05:00",
        ] {
            assert_eq!(parse_reset_time(input), Some(instant), "input {input:?}");
        }
    }

    /// A fractional RFC 3339 second is rounded UP, never down to an instant
    /// earlier than the one reported.
    #[test]
    fn a_fractional_rfc3339_second_is_never_rounded_earlier() {
        assert_eq!(
            parse_reset_time("2026-08-02T00:00:00.4Z"),
            Some(august_2_2026_midnight_utc() + 1)
        );
    }

    /// A present but malformed offset is `None`, never the UTC reading of the
    /// clock time; a valid offset in the same shape still parses.
    #[test]
    fn a_malformed_offset_is_none() {
        for input in [
            "2026-08-02T00:00:00+99:00",
            "2026-08-02T00:00:00+05:60",
            "2026-08-02T00:00:00+5",
            "2026-08-02T00:00:00+abcd",
        ] {
            assert_eq!(parse_reset_time(input), None, "input {input:?}");
        }
        assert_eq!(
            parse_reset_time("2026-08-02T05:30:00+05:30"),
            Some(august_2_2026_midnight_utc()),
            "a valid offset must not be refused"
        );
    }

    /// Moved from `ship.rs` unchanged: the accepted offset spellings and bounds.
    #[test]
    fn parse_offset_minutes_bounds_and_forms() {
        assert_eq!(parse_offset_minutes("+05:30"), Some(330));
        assert_eq!(parse_offset_minutes("+0530"), Some(330));
        assert_eq!(parse_offset_minutes("-0530"), Some(-330));
        assert_eq!(parse_offset_minutes("+05"), Some(300));
        assert_eq!(parse_offset_minutes("-05"), Some(-300));
        // Out-of-range and wrong digit counts fail safe.
        assert_eq!(parse_offset_minutes("+24"), None);
        assert_eq!(parse_offset_minutes("+05:60"), None);
        assert_eq!(parse_offset_minutes("+5"), None);
        assert_eq!(parse_offset_minutes("+530"), None);
        assert_eq!(parse_offset_minutes("+abcd"), None);
    }

    /// Each RFC 3339 field outside its range is `None`, at both ends; the
    /// largest valid value of every field (a leap second included) still
    /// parses.
    #[test]
    fn an_out_of_range_rfc3339_field_is_none() {
        assert!(parse_reset_time("2026-12-31T23:59:60Z").is_some());
        for input in [
            "2026-00-15T12:00:00Z",
            "2026-13-15T12:00:00Z",
            "2026-08-00T12:00:00Z",
            "2026-08-32T12:00:00Z",
            "2026-08-15T24:00:00Z",
            "2026-08-15T12:60:00Z",
            "2026-08-15T12:00:61Z",
        ] {
            assert_eq!(parse_reset_time(input), None, "input {input:?}");
        }
    }

    /// A seconds field with a non-digit or empty fraction is `None`, not a
    /// rounded-up guess.
    #[test]
    fn a_malformed_seconds_fraction_is_none() {
        for input in ["2026-08-02T00:00:00.4xZ", "2026-08-02T00:00:00.Z"] {
            assert_eq!(parse_reset_time(input), None, "input {input:?}");
        }
    }

    /// `civil_from_days` names the right date at the century and 400-year
    /// boundaries of the Gregorian leap rule (day numbers from Python's
    /// `datetime.date`), where a round trip alone cannot catch a skew.
    #[test]
    fn civil_from_days_names_leap_rule_boundaries() {
        for (days, date) in [
            (-25_509, (1900, 2, 28)),
            (-25_508, (1900, 3, 1)),
            (11_016, (2000, 2, 29)),
            (47_540, (2100, 2, 28)),
            (47_541, (2100, 3, 1)),
            (157_113, (2400, 2, 29)),
            (157_114, (2400, 3, 1)),
        ] {
            assert_eq!(civil_from_days(days), date, "day {days}");
            assert_eq!(
                days_from_civil(date.0, date.1, date.2),
                days,
                "date {date:?}"
            );
        }
    }

    /// The relocated civil-date helpers are inverses of each other.
    #[test]
    fn civil_from_days_round_trips_days_from_civil() {
        // 11_016 is 2000-02-29 and 19_754 is 2024-02-01: the January/February
        // branch of the March-based year.
        for days in [-1, 0, 1, 11_016, 19_000, 19_754, 2_932_896] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days, "day {days}");
        }
    }
}
