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

/// The exact UTC instant, in Unix epoch seconds, that `input` names.
///
/// Accepts non-negative epoch seconds, optionally with a decimal fraction.
/// A fraction is rounded UP to the next whole second, so the result is never
/// earlier than the instant the input names. Anything else, including
/// surrounding text, a sign, or a value that overflows `i64`, is `None`.
pub fn parse_reset_time(input: &str) -> Option<i64> {
    parse_epoch_seconds(input.trim())
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

    /// An epoch-seconds reset is read to the exact second, surrounding
    /// whitespace is ignored, and a fraction is rounded UP: the deadline must
    /// never name an instant earlier than the one the agent reported.
    #[test]
    fn an_epoch_seconds_reset_parses_to_the_exact_second() {
        assert_eq!(parse_reset_time("1785645600"), Some(1_785_645_600));
        assert_eq!(parse_reset_time(" 1785645600 "), Some(1_785_645_600));
        assert_eq!(parse_reset_time("1785645600.2"), Some(1_785_645_601));
    }

    /// Text that names no instant is `None`, never a guessed time.
    #[test]
    fn an_empty_or_invalid_reset_is_none() {
        for input in ["", "   ", "soon", "-5", "99999999999999999999"] {
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

    /// The relocated civil-date helpers are inverses of each other.
    #[test]
    fn civil_from_days_round_trips_days_from_civil() {
        for days in [-1, 0, 1, 19_000, 2_932_896] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days, "day {days}");
        }
    }
}
