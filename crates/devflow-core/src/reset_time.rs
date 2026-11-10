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
pub fn parse_reset_time(_input: &str) -> Option<i64> {
    None
}

/// The proleptic-Gregorian `(year, month, day)` of the day `days` after
/// 1970-01-01.
pub(crate) fn civil_from_days(_days: i64) -> (i32, u32, u32) {
    (1970, 1, 1)
}

/// The number of days from 1970-01-01 to the given civil date.
pub(crate) fn days_from_civil(_year: i32, _month: u32, _day: u32) -> i64 {
    0
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

    /// The relocated civil-date helpers are inverses of each other.
    #[test]
    fn civil_from_days_round_trips_days_from_civil() {
        for days in [-1, 0, 1, 19_000, 2_932_896] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days, "day {days}");
        }
    }
}
