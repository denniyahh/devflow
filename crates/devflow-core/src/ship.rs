//! Ship bookkeeping.
//!
//! Holds the Hermes cron-instructions manifest (used to resume a rate-limited
//! DevFlow run later) plus the pure document-finalization transform
//! (CHANGELOG) used on ship completion.

use crate::phase_id::PhaseId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Manifest consumed by Hermes to resume a rate-limited DevFlow run later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronInstructions {
    /// Absolute project root.
    pub project: String,
    /// Phase that should resume.
    pub phase: PhaseId,
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
    /// Hermes schedule string. DevFlow writes an ISO-8601 UTC instant with an
    /// explicit `Z`, which Hermes will not reinterpret in its configured zone.
    pub schedule: String,
    /// Stable job name.
    pub name: String,
    /// Shell command to execute.
    pub command: String,
    /// Whether Hermes should remove the job after it runs.
    pub once: bool,
}

/// The phase-owned cron-instruction record or records consumed by a lifecycle
/// transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronInstructionPathKind {
    /// The current per-phase record was consumed.
    PerPhase,
    /// A legacy single-slot record naming the phase was consumed.
    Legacy,
    /// Both the current per-phase and matching legacy records were consumed.
    Both,
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
pub fn cron_instructions_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    project_root.join(".devflow").join(format!(
        "cron-instructions-{padded}.json",
        padded = phase.padded()
    ))
}

/// Path of the legacy single-slot record written by pre-14a binaries. Still
/// read/deleted for compatibility; never written.
pub(crate) fn legacy_cron_instructions_path(project_root: &Path) -> PathBuf {
    project_root.join(".devflow").join("cron-instructions.json")
}

/// Persist Hermes cron instructions for the phase recorded inside them.
pub fn write_cron_instructions(
    project_root: &Path,
    instructions: &CronInstructions,
) -> Result<(), ShipError> {
    let path = cron_instructions_path(project_root, instructions.phase);
    if let Some(parent) = path.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(instructions)?)?;
    Ok(())
}

/// Load a phase's Hermes cron instructions, or [`ShipError::Missing`] if
/// absent. Falls back to a legacy single-slot record when it names this phase.
pub fn load_cron_instructions(
    project_root: &Path,
    phase: PhaseId,
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
pub fn delete_cron_instructions(project_root: &Path, phase: PhaseId) -> Result<(), ShipError> {
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

/// Remove `path` if still present, reporting whether THIS call was the one
/// that removed it (`Ok(true)`) versus finding it already gone (`Ok(false)`).
///
/// [`consume_cron_instructions`] checks `.exists()` and only later calls this
/// — a TOCTOU window a concurrent consumer of the SAME record (or a manual
/// cleanup) can win, leaving nothing here to remove by the time this runs.
/// That is not a failure of THIS consumption (whoever removed it already
/// removed it, on our behalf as far as observable state goes), but it also
/// must not be reported as OUR consumption — the caller uses the `bool` to
/// decide that, not the earlier `.exists()` snapshot, or two racing
/// consumers both report having consumed the one record that existed
/// (44-CORE-REVIEW-FINDINGS.md finding 3). Any other I/O error propagates.
fn remove_file_if_still_present(path: &Path) -> Result<bool, ShipError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

/// Consume phase-owned cron-instruction records and report the record kind
/// only after deletion succeeds.
///
/// Unlike [`delete_cron_instructions`], this path leaves unreadable legacy
/// records alone because their phase ownership cannot be established for an
/// audit event. Recovery cleanup retains its existing behavior.
pub fn consume_cron_instructions(
    project_root: &Path,
    phase: PhaseId,
) -> Result<Option<CronInstructionPathKind>, ShipError> {
    let per_phase = cron_instructions_path(project_root, phase);
    let has_per_phase = per_phase.exists();

    let legacy = legacy_cron_instructions_path(project_root);
    let has_matching_legacy = if legacy.exists() {
        match std::fs::read_to_string(&legacy)
            .ok()
            .and_then(|contents| serde_json::from_str::<CronInstructions>(&contents).ok())
        {
            Some(instructions) => instructions.phase == phase,
            None => false,
        }
    } else {
        false
    };

    if !has_per_phase && !has_matching_legacy {
        return Ok(None);
    }

    // Each removal reports whether THIS call actually removed the file —
    // never the pre-removal `.exists()` snapshot above, which two racing
    // callers can both observe as `true` for the same, single file.
    let removed_per_phase = if has_per_phase {
        remove_file_if_still_present(&per_phase)?
    } else {
        false
    };
    let removed_matching_legacy = if has_matching_legacy {
        remove_file_if_still_present(&legacy)?
    } else {
        false
    };

    if !removed_per_phase && !removed_matching_legacy {
        // Every candidate this call found was already consumed by a racing
        // caller between our `.exists()` checks and our removal attempts —
        // nothing left for THIS call to report.
        return Ok(None);
    }

    Ok(Some(match (removed_per_phase, removed_matching_legacy) {
        (true, true) => CronInstructionPathKind::Both,
        (true, false) => CronInstructionPathKind::PerPhase,
        (false, true) => CronInstructionPathKind::Legacy,
        (false, false) => unreachable!("empty consumption case returned above"),
    }))
}

/// Build a Hermes cron-instructions manifest for resuming the PRIMARY
/// single-agent `advance()` monitor loop (D-09, review consensus #5) via
/// `devflow resume --phase N`. `agent` is intentionally omitted from the
/// resume command: `devflow resume` loads it (along with mode and stage)
/// from the phase's saved state.
pub fn build_single_agent_cron_instructions(
    project_root: &Path,
    phase: PhaseId,
    retry_after: &str,
) -> CronInstructions {
    let project = project_root.display().to_string();
    let args = vec![
        "resume".to_string(),
        "--phase".to_string(),
        phase.to_string(),
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
            schedule: hermes_schedule_from_retry_after(retry_after).unwrap_or_default(),
            name: format!("devflow-phase-{padded}-resume", padded = phase.padded()),
            // Deliberately NOT shell_quote()'d here: this is one shell word
            // embedded inside a larger command line by its caller (e.g.
            // `cron_hint_line`), which is the single place responsible for
            // quoting the whole thing. Quoting the path here too would nest
            // quotes and produce invalid/exploitable shell (see 44-CORE-
            // REVIEW-FINDINGS.md finding 1).
            command: format!("cd {project} && devflow resume --phase {phase}"),
            once: true,
        },
    }
}

/// Convert a retry timestamp to an ISO-8601 UTC instant, rounding up to the
/// nearest minute. Supports RFC3339-like timestamps and Unix epoch seconds.
pub fn hermes_schedule_from_retry_after(retry_after: &str) -> Option<String> {
    // WR-06: never turn unparseable agent output into an every-minute cron.
    parse_retry_timestamp(retry_after).map(|ts| ts.round_up_minute().to_iso_utc())
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

    fn to_iso_utc(self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:00Z",
            self.year, self.month, self.day, self.hour, self.minute
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

/// Quote `value` as a single POSIX shell word if it contains any character
/// that isn't unambiguously safe unquoted. Callers embedding an
/// already-composite command string (e.g. `HermesCronJob.command`) into a
/// larger command line must call this exactly once, on the whole string —
/// never quote a sub-part and then quote the containing string again, since
/// POSIX single quotes don't nest.
pub fn shell_quote(value: &str) -> String {
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
///
/// `body` is the Keep-a-Changelog-grouped content
/// [`crate::version::render_changelog_body`] produces (D-12) — trimmed of
/// trailing newlines and re-terminated with a single `\n`. When `body.trim()`
/// is empty (no version-affecting content, or the caller couldn't compute
/// one), the fallback line `- No changes recorded since the previous
/// release.` is substituted instead, so an entry is never silently blank.
pub fn prepend_changelog(existing: &str, version: &str, date: &str, body: &str) -> String {
    const HEADER: &str = "# Changelog\n\n\
        All notable changes to this project are documented here.\n";
    const FALLBACK: &str = "- No changes recorded since the previous release.";
    let trimmed_body = body.trim_end_matches('\n');
    let body_content = if trimmed_body.trim().is_empty() {
        FALLBACK
    } else {
        trimmed_body
    };
    let entry = format!("## {version} — {date}\n\n{body_content}\n");

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
        let record = build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(7),
            "2026-06-18T15:45:30Z",
        );

        write_cron_instructions(dir.path(), &record).unwrap();

        assert_eq!(
            load_cron_instructions(dir.path(), PhaseId::new(7)).unwrap(),
            record
        );
    }

    #[test]
    fn delete_cron_instructions_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(7),
            "2026-06-18T15:45:30Z",
        );
        write_cron_instructions(dir.path(), &record).unwrap();

        delete_cron_instructions(dir.path(), PhaseId::new(7)).unwrap();
        assert!(!cron_instructions_path(dir.path(), PhaseId::new(7)).exists());
        delete_cron_instructions(dir.path(), PhaseId::new(7)).unwrap();
    }

    /// 13-DEFERRED-CR-03 re-check: two phases' rate-limit records must
    /// coexist — the old single-slot file let one clobber the other.
    #[test]
    fn cron_instructions_are_per_phase() {
        let dir = tempfile::tempdir().unwrap();
        let a = build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(7),
            "2026-06-18T15:45:30Z",
        );
        let b = build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(8),
            "2026-06-18T16:45:30Z",
        );
        write_cron_instructions(dir.path(), &a).unwrap();
        write_cron_instructions(dir.path(), &b).unwrap();

        assert_eq!(
            load_cron_instructions(dir.path(), PhaseId::new(7)).unwrap(),
            a
        );
        assert_eq!(
            load_cron_instructions(dir.path(), PhaseId::new(8)).unwrap(),
            b
        );
        let listed = list_cron_instructions(dir.path());
        assert_eq!(
            listed.iter().map(|i| i.phase).collect::<Vec<_>>(),
            [PhaseId::new(7), PhaseId::new(8)]
        );

        delete_cron_instructions(dir.path(), PhaseId::new(7)).unwrap();
        assert!(load_cron_instructions(dir.path(), PhaseId::new(7)).is_err());
        assert_eq!(
            load_cron_instructions(dir.path(), PhaseId::new(8)).unwrap(),
            b
        );
    }

    /// Upgrade path: a legacy single-slot `cron-instructions.json` written by
    /// an older binary is still loadable/listable/deletable for its phase.
    #[test]
    fn legacy_cron_instructions_are_read_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(5),
            "2026-06-18T15:45:30Z",
        );
        let legacy = legacy_cron_instructions_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, serde_json::to_string_pretty(&record).unwrap()).unwrap();

        assert_eq!(
            load_cron_instructions(dir.path(), PhaseId::new(5)).unwrap(),
            record
        );
        assert!(load_cron_instructions(dir.path(), PhaseId::new(6)).is_err());
        assert_eq!(list_cron_instructions(dir.path()).len(), 1);

        delete_cron_instructions(dir.path(), PhaseId::new(5)).unwrap();
        assert!(!legacy.exists());
    }

    #[test]
    fn consume_cron_instructions_deletes_per_phase_record() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(7);
        let record =
            build_single_agent_cron_instructions(dir.path(), phase, "2026-06-18T15:45:30Z");
        write_cron_instructions(dir.path(), &record).unwrap();
        let path = cron_instructions_path(dir.path(), phase);
        assert!(path.exists(), "fixture must exist before consumption");

        assert_eq!(
            consume_cron_instructions(dir.path(), phase).unwrap(),
            Some(CronInstructionPathKind::PerPhase)
        );
        assert!(!path.exists());
    }

    #[test]
    fn consume_cron_instructions_deletes_matching_legacy_record() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(7);
        let record =
            build_single_agent_cron_instructions(dir.path(), phase, "2026-06-18T15:45:30Z");
        let legacy = legacy_cron_instructions_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, serde_json::to_string_pretty(&record).unwrap()).unwrap();
        assert!(legacy.exists(), "fixture must exist before consumption");

        assert_eq!(
            consume_cron_instructions(dir.path(), phase).unwrap(),
            Some(CronInstructionPathKind::Legacy)
        );
        assert!(!legacy.exists());
    }

    #[test]
    fn consume_cron_instructions_reports_both_deleted_records() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(7);
        let record =
            build_single_agent_cron_instructions(dir.path(), phase, "2026-06-18T15:45:30Z");
        write_cron_instructions(dir.path(), &record).unwrap();
        let per_phase = cron_instructions_path(dir.path(), phase);
        let legacy = legacy_cron_instructions_path(dir.path());
        std::fs::write(&legacy, serde_json::to_string_pretty(&record).unwrap()).unwrap();
        assert!(
            per_phase.exists(),
            "per-phase fixture must exist before consumption"
        );
        assert!(
            legacy.exists(),
            "legacy fixture must exist before consumption"
        );

        assert_eq!(
            consume_cron_instructions(dir.path(), phase).unwrap(),
            Some(CronInstructionPathKind::Both)
        );
        assert!(!per_phase.exists());
        assert!(!legacy.exists());
    }

    #[test]
    fn consume_cron_instructions_preserves_foreign_legacy_record() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(7);
        let foreign = build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(8),
            "2026-06-18T15:45:30Z",
        );
        let legacy = legacy_cron_instructions_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, serde_json::to_string_pretty(&foreign).unwrap()).unwrap();
        assert!(
            legacy.exists(),
            "foreign fixture must exist before consumption"
        );

        assert_eq!(consume_cron_instructions(dir.path(), phase).unwrap(), None);
        assert!(legacy.exists(), "foreign legacy record must remain");
    }

    #[cfg(unix)]
    #[test]
    fn consume_cron_instructions_preserves_unreadable_legacy_record() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(7);
        let record =
            build_single_agent_cron_instructions(dir.path(), phase, "2026-06-18T15:45:30Z");
        let legacy = legacy_cron_instructions_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, serde_json::to_string_pretty(&record).unwrap()).unwrap();
        std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            legacy.exists(),
            "unreadable fixture must exist before consumption"
        );

        assert_eq!(consume_cron_instructions(dir.path(), phase).unwrap(), None);
        assert!(legacy.exists(), "unreadable legacy record must remain");
        std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    /// Regression for 44-CORE-REVIEW-FINDINGS.md finding 3 (TOCTOU): two
    /// concurrent consumers of the SAME per-phase record both pass the
    /// `.exists()` check before either has removed the file, so whichever
    /// runs `remove_file` second used to see `NotFound` and return `Err` —
    /// even though the record WAS consumed, just by the other caller.
    /// Neither call may error; at most one may report a `Some` kind (the
    /// second observed nothing left to report), and the file must be gone.
    #[test]
    fn consume_cron_instructions_tolerates_a_racing_concurrent_consumer() {
        use std::sync::Arc;
        use std::sync::Barrier;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(7);
        let record = build_single_agent_cron_instructions(&root, phase, "2026-06-18T15:45:30Z");
        write_cron_instructions(&root, &record).unwrap();
        let path = cron_instructions_path(&root, phase);
        assert!(path.exists(), "fixture must exist before consumption");

        let barrier = Arc::new(Barrier::new(2));
        let run = move |root: PathBuf, barrier: Arc<Barrier>| {
            barrier.wait();
            consume_cron_instructions(&root, phase)
        };
        let (root_a, barrier_a) = (root.clone(), Arc::clone(&barrier));
        let (root_b, barrier_b) = (root.clone(), Arc::clone(&barrier));
        let a = std::thread::spawn(move || run(root_a, barrier_a));
        let b = std::thread::spawn(move || run(root_b, barrier_b));

        let result_a = a.join().unwrap();
        let result_b = b.join().unwrap();

        assert!(
            result_a.is_ok(),
            "racing consumer A must not error: {result_a:?}"
        );
        assert!(
            result_b.is_ok(),
            "racing consumer B must not error: {result_b:?}"
        );
        let reported = [result_a.unwrap(), result_b.unwrap()]
            .into_iter()
            .filter(|kind| kind.is_some())
            .count();
        assert_eq!(
            reported, 1,
            "exactly one racing consumer should observe and report the record"
        );
        assert!(!path.exists());
    }

    #[test]
    fn hermes_schedule_rounds_up_to_nearest_minute() {
        assert_eq!(
            hermes_schedule_from_retry_after("2026-06-18T15:45:30Z"),
            Some("2026-06-18T15:46:00Z".to_string())
        );
        assert_eq!(
            hermes_schedule_from_retry_after("2026-06-18T15:45:00Z"),
            Some("2026-06-18T15:45:00Z".to_string())
        );
    }

    #[test]
    fn hermes_schedule_normalizes_negative_offset() {
        // 15:45:30 local at UTC-5 → 20:45:30 UTC → round up to 20:46.
        assert_eq!(
            hermes_schedule_from_retry_after("2026-06-18T15:45:30-05:00"),
            Some("2026-06-18T20:46:00Z".to_string())
        );
        // 15:45:00 local at UTC-5:30 → 21:15:00 UTC, no rounding needed.
        assert_eq!(
            hermes_schedule_from_retry_after("2026-06-18T15:45:00-05:30"),
            Some("2026-06-18T21:15:00Z".to_string())
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
            hermes_schedule_from_retry_after("2026-06-18T15:45:30+0530"),
            hermes_schedule_from_retry_after("2026-06-18T15:45:30+05:30"),
        );
        // Hour-only ±HH: 15:45:30 at -05 → 20:45:30 UTC → 20:46.
        assert_eq!(
            hermes_schedule_from_retry_after("2026-06-18T15:45:30-05"),
            Some("2026-06-18T20:46:00Z".to_string())
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
    fn hermes_schedule_formats_unix_seconds() {
        assert_eq!(
            hermes_schedule_from_retry_after("1766678401"),
            Some("2025-12-25T16:01:00Z".to_string())
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

    /// Review consensus #5: the single-agent resume record must invoke
    /// `devflow resume --phase N` (which relaunches saved state), never the
    /// unsafe `devflow start` (resets to Define) or the two-agent
    /// `sequentagent` command.
    #[test]
    fn single_agent_cron_instructions_resume_command_is_devflow_resume() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(9),
            "2026-06-18T15:45:30Z",
        );

        assert_eq!(record.resume.command, "devflow");
        assert_eq!(record.resume.args, ["resume", "--phase", "9"]);
        assert!(
            record
                .hermes_cron
                .command
                .contains("devflow resume --phase 9")
        );
        assert!(!record.hermes_cron.command.contains("sequentagent"));
        assert!(!record.hermes_cron.command.contains(" start"));
        assert!(record.hermes_cron.once);
    }

    #[test]
    fn cron_instructions_reject_unparseable_retry_time() {
        let dir = tempfile::tempdir().unwrap();
        let record = build_single_agent_cron_instructions(dir.path(), PhaseId::new(7), "unknown");

        assert_ne!(record.hermes_cron.schedule, "* * * * *");
        assert!(record.hermes_cron.schedule.is_empty());
    }

    #[test]
    fn prepend_changelog_creates_header_when_empty() {
        let out = prepend_changelog("", "0.5.2", "2026-06-18", "- some change\n");
        assert!(out.starts_with("# Changelog"));
        assert!(out.contains("## 0.5.2 — 2026-06-18"));
        assert!(out.contains("- some change"));
    }

    #[test]
    fn prepend_changelog_inserts_after_header() {
        let existing = "# Changelog\n\n## 0.5.1 — 2026-06-17\n\n- old\n";
        let out = prepend_changelog(existing, "0.5.2", "2026-06-18", "- new change\n");
        let new_idx = out.find("0.5.2").unwrap();
        let old_idx = out.find("0.5.1").unwrap();
        assert!(new_idx < old_idx, "new entry should come before old");
        assert!(out.starts_with("# Changelog"));
        assert!(out.contains("- new change"));
    }

    #[test]
    fn prepend_changelog_uses_the_generated_body() {
        let body = "### Added\n\n- add the widget endpoint\n";
        let out = prepend_changelog("", "1.2.0", "2026-07-29", body);
        assert!(out.contains("## 1.2.0 — 2026-07-29"));
        assert!(out.contains("### Added"));
        assert!(out.contains("- add the widget endpoint"));
        // Empty-body fallback (never a silently blank entry).
        let fallback = prepend_changelog("", "1.2.1", "2026-07-30", "");
        assert!(fallback.contains("- No changes recorded since the previous release."));
    }
}
