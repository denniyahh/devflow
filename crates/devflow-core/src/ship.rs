//! Ship bookkeeping.
//!
//! Holds the Hermes cron-instructions manifest (used to resume a rate-limited
//! DevFlow run later) plus the pure document-finalization transform
//! (CHANGELOG) used on ship completion.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Manifest consumed by Hermes to resume a rate-limited DevFlow run later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronInstructions {
    /// Absolute project root.
    pub project: String,
    /// Phase that should resume.
    pub phase: u32,
    /// Current handoff status, e.g. "rate_limited".
    pub status: String,
    /// Upstream retry timestamp or description.
    pub retry_after: String,
    /// DevFlow resume command.
    pub resume: ResumeCommand,
    /// Hermes cron job definition derived from the retry timestamp.
    pub hermes_cron: HermesCronJob,
}

/// Command + args that resume the DevFlow workflow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResumeCommand {
    /// Executable name.
    pub command: String,
    /// Command arguments.
    pub args: Vec<String>,
}

/// Hermes one-shot cron job payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesCronJob {
    /// Cron schedule in `M H D M W` format.
    pub schedule: String,
    /// Stable job name.
    pub name: String,
    /// Shell command to execute.
    pub command: String,
    /// Whether Hermes should remove the job after it runs.
    pub once: bool,
}

/// Errors produced by ship bookkeeping.
#[derive(Debug, thiserror::Error)]
pub enum ShipError {
    /// Filesystem operation failed.
    #[error("ship I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse or serialization failed.
    #[error("ship JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// No last-ship record exists.
    #[error("no last-ship record found — nothing to confirm or reject")]
    Missing,
}

/// Path to a phase's cron-instructions record. Per-phase since 14a
/// (13-DEFERRED-CR-03): the old single-slot `cron-instructions.json` let one
/// phase's rate-limit record clobber another's under `devflow parallel`.
pub fn cron_instructions_path(project_root: &Path, phase: u32) -> PathBuf {
    project_root
        .join(".devflow")
        .join(format!("cron-instructions-{phase:02}.json"))
}

/// Path of the legacy single-slot record written by pre-14a binaries. Still
/// read/deleted for compatibility; never written.
fn legacy_cron_instructions_path(project_root: &Path) -> PathBuf {
    project_root.join(".devflow").join("cron-instructions.json")
}

/// Persist Hermes cron instructions for the phase recorded inside them.
pub fn write_cron_instructions(
    project_root: &Path,
    instructions: &CronInstructions,
) -> Result<(), ShipError> {
    let path = cron_instructions_path(project_root, instructions.phase);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(instructions)?)?;
    Ok(())
}

/// Load a phase's Hermes cron instructions, or [`ShipError::Missing`] if
/// absent. Falls back to a legacy single-slot record when it names this phase.
pub fn load_cron_instructions(
    project_root: &Path,
    phase: u32,
) -> Result<CronInstructions, ShipError> {
    let path = cron_instructions_path(project_root, phase);
    if path.exists() {
        return Ok(serde_json::from_str(&std::fs::read_to_string(&path)?)?);
    }
    let legacy = legacy_cron_instructions_path(project_root);
    if legacy.exists() {
        let instructions: CronInstructions =
            serde_json::from_str(&std::fs::read_to_string(&legacy)?)?;
        if instructions.phase == phase {
            return Ok(instructions);
        }
    }
    Err(ShipError::Missing)
}

/// Every pending cron-instructions record (per-phase files plus a legacy
/// single-slot one), sorted by phase. Unparsable files are skipped.
pub fn list_cron_instructions(project_root: &Path) -> Vec<CronInstructions> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(project_root.join(".devflow")) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with("cron-instructions") || !name.ends_with(".json") {
                continue;
            }
            if let Ok(contents) = std::fs::read_to_string(entry.path())
                && let Ok(instructions) = serde_json::from_str::<CronInstructions>(&contents)
            {
                found.push(instructions);
            }
        }
    }
    found.sort_by_key(|i| i.phase);
    found.dedup_by_key(|i| i.phase);
    found
}

/// Remove a phase's cron-instructions record (and a legacy single-slot record
/// naming the same phase). Idempotent.
pub fn delete_cron_instructions(project_root: &Path, phase: u32) -> Result<(), ShipError> {
    let path = cron_instructions_path(project_root, phase);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let legacy = legacy_cron_instructions_path(project_root);
    if legacy.exists()
        && let Ok(contents) = std::fs::read_to_string(&legacy)
        && serde_json::from_str::<CronInstructions>(&contents)
            .map(|i| i.phase == phase)
            .unwrap_or(true)
    {
        std::fs::remove_file(&legacy)?;
    }
    Ok(())
}

/// Build a Hermes cron-instructions manifest for resuming `sequentagent`.
pub fn build_cron_instructions(
    project_root: &Path,
    phase: u32,
    retry_after: &str,
    next_agents: &str,
) -> CronInstructions {
    let project = project_root.display().to_string();
    let args = vec![
        "sequentagent".to_string(),
        "--phase".to_string(),
        phase.to_string(),
        "--agents".to_string(),
        next_agents.to_string(),
    ];
    CronInstructions {
        project: project.clone(),
        phase,
        status: "rate_limited".to_string(),
        retry_after: retry_after.to_string(),
        resume: ResumeCommand {
            command: "devflow".to_string(),
            args,
        },
        hermes_cron: HermesCronJob {
            schedule: cron_schedule_from_retry_after(retry_after).unwrap_or_default(),
            name: format!("devflow-phase-{phase:02}-resume"),
            command: format!(
                "cd {} && devflow sequentagent --phase {phase} --agents {next_agents}",
                shell_quote(&project)
            ),
            once: true,
        },
    }
}

/// Convert a retry timestamp to `M H D M W` cron syntax, rounding up to the
/// nearest minute. Supports RFC3339-like timestamps and Unix epoch seconds.
pub fn cron_schedule_from_retry_after(retry_after: &str) -> Option<String> {
    // WR-06: never turn unparseable agent output into an every-minute cron.
    parse_retry_timestamp(retry_after).map(|ts| ts.round_up_minute().to_cron())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RetryTimestamp {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

impl RetryTimestamp {
    fn round_up_minute(self) -> Self {
        if self.second == 0 {
            return self;
        }
        Self::from_epoch_minutes(self.to_epoch_minutes() + 1)
    }

    fn to_cron(self) -> String {
        format!(
            "{} {} {} {} *",
            self.minute, self.hour, self.day, self.month
        )
    }

    fn to_epoch_minutes(self) -> i64 {
        let days = days_from_civil(self.year, self.month, self.day);
        days * 24 * 60 + i64::from(self.hour) * 60 + i64::from(self.minute)
    }

    fn from_epoch_minutes(minutes: i64) -> Self {
        let days = minutes.div_euclid(24 * 60);
        let minute_of_day = minutes.rem_euclid(24 * 60);
        let (year, month, day) = civil_from_days(days);
        Self {
            year,
            month,
            day,
            hour: (minute_of_day / 60) as u32,
            minute: (minute_of_day % 60) as u32,
            second: 0,
        }
    }
}

fn parse_retry_timestamp(input: &str) -> Option<RetryTimestamp> {
    parse_unix_seconds(input).or_else(|| parse_rfc3339ish(input))
}

fn parse_unix_seconds(input: &str) -> Option<RetryTimestamp> {
    let seconds = input.trim().parse::<i64>().ok()?;
    let minutes = seconds.div_euclid(60) + i64::from(seconds.rem_euclid(60) > 0);
    Some(RetryTimestamp::from_epoch_minutes(minutes))
}

fn parse_rfc3339ish(input: &str) -> Option<RetryTimestamp> {
    let input = input.trim();
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

    let (time, offset_minutes) = split_time_and_offset(time);
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second = time_parts
        .next()
        .map(|s| s.split('.').next().unwrap_or_default().parse::<u32>().ok())
        .unwrap_or(Some(0))?;
    if month == 0 || month > 12 || day == 0 || day > 31 || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let ts = RetryTimestamp {
        year,
        month,
        day,
        hour,
        minute,
        second,
    };
    let utc_minutes = ts.to_epoch_minutes() - i64::from(offset_minutes);
    let mut normalized = RetryTimestamp::from_epoch_minutes(utc_minutes);
    // `to_epoch_minutes`/`from_epoch_minutes` normalize at whole-minute
    // granularity (the offset subtraction above only ever shifts whole
    // minutes, since `offset_minutes` is itself an integer minute count),
    // so `from_epoch_minutes` always zeroes `second`. A timezone offset never
    // carries a sub-minute component, so the original `second` is
    // timezone-invariant and safe to restore verbatim here.
    normalized.second = second;
    Some(normalized)
}

fn split_time_and_offset(time: &str) -> (&str, i32) {
    let trimmed = time.trim_end_matches('Z');
    if trimmed.len() > 6 {
        if let Some(idx) = trimmed.rfind('+') {
            return (
                &trimmed[..idx],
                parse_offset_minutes(&trimmed[idx..]).unwrap_or(0),
            );
        }
        if let Some(idx) = trimmed.rfind('-')
            && idx > 0
        {
            return (
                &trimmed[..idx],
                parse_offset_minutes(&trimmed[idx..]).unwrap_or(0),
            );
        }
    }
    (trimmed, 0)
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

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = i64::from(year).div_euclid(400);
    let yoe = i64::from(year) - era * 400;
    let month = i64::from(month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
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

fn shell_quote(value: &str) -> String {
    // Characters that never need quoting in a POSIX shell word: alphanumerics
    // plus the common punctuation used in paths, versions, and identifiers
    // (`/ . _ -`) and additional unambiguously-safe characters (`~ : @ + = %`)
    // that have no special meaning to the shell when unquoted. Anything not
    // in this set falls through to single-quote wrapping below, so widening
    // this list only reduces over-quoting — it can never under-quote.
    if value.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '/' | '.' | '_' | '-' | '~' | ':' | '@' | '+' | '=' | '%')
    }) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

/// Prepend a CHANGELOG entry for `version`, creating a standard header if the
/// file did not exist. Pure transform over the existing CHANGELOG contents.
pub fn prepend_changelog(existing: &str, version: &str, date: &str) -> String {
    const HEADER: &str = "# Changelog\n\n\
        All notable changes to this project are documented here.\n";
    let entry = format!("## {version} — {date}\n\n- Released phase via DevFlow.\n");

    if existing.trim().is_empty() {
        return format!("{HEADER}\n{entry}");
    }
    // Insert the new entry after the header block (first blank line after the
    // top-level title), or at the top if no header is recognized.
    if let Some(idx) = existing.find("\n\n") {
        let (head, tail) = existing.split_at(idx + 2);
        format!("{head}{entry}\n{tail}")
    } else {
        format!("{entry}\n{existing}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_instructions_save_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_cron_instructions(dir.path(), 7, "2026-06-18T15:45:30Z", "codex,claude");

        write_cron_instructions(dir.path(), &record).unwrap();

        assert_eq!(load_cron_instructions(dir.path(), 7).unwrap(), record);
    }

    #[test]
    fn delete_cron_instructions_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_cron_instructions(dir.path(), 7, "2026-06-18T15:45:30Z", "codex,claude");
        write_cron_instructions(dir.path(), &record).unwrap();

        delete_cron_instructions(dir.path(), 7).unwrap();
        assert!(!cron_instructions_path(dir.path(), 7).exists());
        delete_cron_instructions(dir.path(), 7).unwrap();
    }

    /// 13-DEFERRED-CR-03 re-check: two phases' rate-limit records must
    /// coexist — the old single-slot file let one clobber the other.
    #[test]
    fn cron_instructions_are_per_phase() {
        let dir = tempfile::tempdir().unwrap();
        let a = build_cron_instructions(dir.path(), 7, "2026-06-18T15:45:30Z", "claude,codex");
        let b = build_cron_instructions(dir.path(), 8, "2026-06-18T16:45:30Z", "codex,claude");
        write_cron_instructions(dir.path(), &a).unwrap();
        write_cron_instructions(dir.path(), &b).unwrap();

        assert_eq!(load_cron_instructions(dir.path(), 7).unwrap(), a);
        assert_eq!(load_cron_instructions(dir.path(), 8).unwrap(), b);
        let listed = list_cron_instructions(dir.path());
        assert_eq!(listed.iter().map(|i| i.phase).collect::<Vec<_>>(), [7, 8]);

        delete_cron_instructions(dir.path(), 7).unwrap();
        assert!(load_cron_instructions(dir.path(), 7).is_err());
        assert_eq!(load_cron_instructions(dir.path(), 8).unwrap(), b);
    }

    /// Upgrade path: a legacy single-slot `cron-instructions.json` written by
    /// an older binary is still loadable/listable/deletable for its phase.
    #[test]
    fn legacy_cron_instructions_are_read_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_cron_instructions(dir.path(), 5, "2026-06-18T15:45:30Z", "claude,codex");
        let legacy = legacy_cron_instructions_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, serde_json::to_string_pretty(&record).unwrap()).unwrap();

        assert_eq!(load_cron_instructions(dir.path(), 5).unwrap(), record);
        assert!(load_cron_instructions(dir.path(), 6).is_err());
        assert_eq!(list_cron_instructions(dir.path()).len(), 1);

        delete_cron_instructions(dir.path(), 5).unwrap();
        assert!(!legacy.exists());
    }

    #[test]
    fn cron_schedule_rounds_up_to_nearest_minute() {
        assert_eq!(
            cron_schedule_from_retry_after("2026-06-18T15:45:30Z"),
            Some("46 15 18 6 *".to_string())
        );
        assert_eq!(
            cron_schedule_from_retry_after("2026-06-18T15:45:00Z"),
            Some("45 15 18 6 *".to_string())
        );
    }

    #[test]
    fn cron_schedule_normalizes_negative_offset() {
        // 15:45:30 local at UTC-5 → 20:45:30 UTC → round up to 20:46.
        assert_eq!(
            cron_schedule_from_retry_after("2026-06-18T15:45:30-05:00"),
            Some("46 20 18 6 *".to_string())
        );
        // 15:45:00 local at UTC-5:30 → 21:15:00 UTC, no rounding needed.
        assert_eq!(
            cron_schedule_from_retry_after("2026-06-18T15:45:00-05:30"),
            Some("15 21 18 6 *".to_string())
        );
    }

    /// WR-07 (13-REVIEW.md), revised: all three ISO-8601 offset forms must
    /// parse to their real value. The pre-WR-07 code misread "+0530" as 530
    /// *hours*; the first WR-07 fix rejected everything without a colon, so
    /// valid ±HHMM and hour-only ±HH offsets silently fell back to UTC via
    /// `split_time_and_offset`'s `unwrap_or(0)` — scheduling the resume cron
    /// hours away from when the rate limit actually lifts.
    #[test]
    fn cron_schedule_parses_all_iso8601_offset_forms() {
        // ±HHMM: 15:45:30 at +05:30 → 10:15:30 UTC → 10:16 (seconds round up).
        assert_eq!(
            cron_schedule_from_retry_after("2026-06-18T15:45:30+0530"),
            cron_schedule_from_retry_after("2026-06-18T15:45:30+05:30"),
        );
        // Hour-only ±HH: 15:45:30 at -05 → 20:45:30 UTC → 20:46.
        assert_eq!(
            cron_schedule_from_retry_after("2026-06-18T15:45:30-05"),
            Some("46 20 18 6 *".to_string())
        );
    }

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

    #[test]
    fn cron_schedule_formats_unix_seconds() {
        assert_eq!(
            cron_schedule_from_retry_after("1766678401"),
            Some("1 16 25 12 *".to_string())
        );
    }

    #[test]
    fn shell_quote_leaves_common_safe_chars_unquoted() {
        assert_eq!(
            shell_quote("user@host:1.2.3+build"),
            "user@host:1.2.3+build"
        );
        assert_eq!(shell_quote("~/proj/build=1_2%3"), "~/proj/build=1_2%3");
    }

    #[test]
    fn shell_quote_quotes_unsafe_input() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn cron_instructions_include_resume_command() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_cron_instructions(dir.path(), 7, "2026-06-18T15:45:30Z", "codex,claude");

        assert_eq!(record.resume.command, "devflow");
        assert_eq!(
            record.resume.args,
            ["sequentagent", "--phase", "7", "--agents", "codex,claude"]
        );
        assert!(
            record
                .hermes_cron
                .command
                .contains("devflow sequentagent --phase 7 --agents codex,claude")
        );
        assert!(record.hermes_cron.once);
    }

    #[test]
    fn cron_instructions_reject_unparseable_retry_time() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_cron_instructions(dir.path(), 7, "unknown", "codex,claude");

        assert_ne!(record.hermes_cron.schedule, "* * * * *");
        assert!(record.hermes_cron.schedule.is_empty());
    }

    #[test]
    fn prepend_changelog_creates_header_when_empty() {
        let out = prepend_changelog("", "0.5.2", "2026-06-18");
        assert!(out.starts_with("# Changelog"));
        assert!(out.contains("## 0.5.2 — 2026-06-18"));
    }

    #[test]
    fn prepend_changelog_inserts_after_header() {
        let existing = "# Changelog\n\n## 0.5.1 — 2026-06-17\n\n- old\n";
        let out = prepend_changelog(existing, "0.5.2", "2026-06-18");
        let new_idx = out.find("0.5.2").unwrap();
        let old_idx = out.find("0.5.1").unwrap();
        assert!(new_idx < old_idx, "new entry should come before old");
        assert!(out.starts_with("# Changelog"));
    }
}
