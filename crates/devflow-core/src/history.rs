//! Read-only per-phase attempt history assembled from existing DevFlow stores.

use crate::phase_id::PhaseId;
use crate::{agent_result, events, gates};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// One chronological point in a phase's history, with nearby retained
/// evidence attached to the event that produced it.
#[derive(Debug, Clone)]
pub struct AttemptEntry {
    pub timestamp: u64,
    pub event: Option<serde_json::Value>,
    pub capture_files: Vec<PathBuf>,
    pub review_files: Vec<PathBuf>,
}

/// The complete read-only attempt view for one phase.
#[derive(Debug, Clone)]
pub struct AttemptTimeline {
    pub phase: PhaseId,
    pub entries: Vec<AttemptEntry>,
}

/// Correlate schema-v1 events, retained capture generations, and review
/// artifacts without creating a second history store.
pub fn attempt_timeline(project_root: &Path, phase: PhaseId) -> AttemptTimeline {
    let indexed_events = phase_events(project_root, phase);

    let mut entries = indexed_events
        .into_iter()
        .map(|(timestamp, _, event)| AttemptEntry {
            timestamp,
            event: Some(event),
            capture_files: Vec::new(),
            review_files: Vec::new(),
        })
        .collect::<Vec<_>>();

    for generation in capture_generations(project_root, phase) {
        if let Some(index) = entries.iter().position(|entry| {
            entry
                .event
                .as_ref()
                .and_then(|event| event.get("stamp"))
                .and_then(|stamp| stamp.as_str())
                == Some(generation.stamp.as_str())
        }) {
            entries[index]
                .capture_files
                .extend(generation.capture_files);
            entries[index].review_files.extend(generation.review_files);
        } else {
            attach_artifacts(
                &mut entries,
                generation.timestamp,
                generation.capture_files,
                true,
            );
            attach_artifacts(
                &mut entries,
                generation.timestamp,
                generation.review_files,
                false,
            );
        }
    }
    for review in review_files(project_root, phase) {
        let timestamp = modified_timestamp(&review);
        attach_artifacts(&mut entries, timestamp, vec![review], false);
    }
    entries.sort_by_key(|entry| entry.timestamp);

    AttemptTimeline { phase, entries }
}

/// The phase's schema-v1 events from `.devflow/events.jsonl` with their
/// line index, in `(ts, line index)` order.
fn phase_events(project_root: &Path, phase: PhaseId) -> Vec<(u64, usize, serde_json::Value)> {
    let mut indexed_events = std::fs::read_to_string(events::events_path(project_root))
        .unwrap_or_default()
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let event = serde_json::from_str::<serde_json::Value>(line).ok()?;
            (event.get("v").and_then(|v| v.as_u64()) == Some(1)
                && phase.matches_json(event.get("phase")))
            .then(|| {
                let timestamp = event.get("ts").and_then(|ts| ts.as_u64()).unwrap_or(0);
                (timestamp, index, event)
            })
        })
        .collect::<Vec<_>>();
    indexed_events.sort_by_key(|(timestamp, index, _)| (*timestamp, *index));
    indexed_events
}

/// Human-readable history output; event summaries deliberately reuse the
/// schema-v1 formatter used by `devflow status`.
pub fn render_timeline(timeline: &AttemptTimeline) -> String {
    if timeline.entries.is_empty() {
        return format!("no attempts recorded for phase {}", timeline.phase);
    }

    let resolved = resolved_question_ids(timeline);
    let mut resolved_so_far = HashSet::new();
    let mut rendered = format!("attempt history for phase {}\n", timeline.phase);
    for entry in &timeline.entries {
        let summary = entry
            .event
            .as_ref()
            .map(events::describe)
            .unwrap_or_else(|| "retained artifact".into());
        rendered.push_str(&format!("[{}] {summary}\n", entry.timestamp));
        if let Some(event) = &entry.event {
            rendered.push_str(&question_rows(event, &resolved, &mut resolved_so_far));
        }
        for capture in &entry.capture_files {
            rendered.push_str(&format!("  capture: {}\n", capture.display()));
        }
        for review in &entry.review_files {
            rendered.push_str(&format!("  review: {}\n", review.display()));
        }
    }
    rendered.trim_end().to_string()
}

/// What history prints for a field an event did not record.
const MISSING_FIELD: &str = "<missing field>";

/// The `question_id`s some `question_answered` or `question_closed` event
/// resolves: an asked question outside this set is still open.
fn resolved_question_ids(timeline: &AttemptTimeline) -> HashSet<&str> {
    timeline
        .entries
        .iter()
        .filter_map(|entry| entry.event.as_ref())
        .filter(|event| {
            matches!(
                event.get("event").and_then(|kind| kind.as_str()),
                Some(events::QUESTION_ANSWERED | events::QUESTION_CLOSED)
            )
        })
        .filter_map(|event| event.get("question_id").and_then(|id| id.as_str()))
        .collect()
}

/// The indented sub-lines for a question event (D-16); empty for any other
/// event. Only the first answer or close recorded for a `question_id` is
/// applied; a later one is shown as ignored.
fn question_rows<'a>(
    event: &'a serde_json::Value,
    resolved: &HashSet<&str>,
    resolved_so_far: &mut HashSet<&'a str>,
) -> String {
    let key = event.get("question_id").and_then(|id| id.as_str());
    let id = text_field(event, "question_id");
    match event.get("event").and_then(|kind| kind.as_str()) {
        Some(events::QUESTION_ASKED) => {
            let mut rows = format!("  question {id}: {}\n", text_field(event, "question"));
            rows.push_str(&format!("  options: {}\n", options_line(event)));
            if event.get("recommended").is_none() {
                rows.push_str(&format!("  recommended: {MISSING_FIELD}\n"));
            }
            if !key.is_some_and(|key| resolved.contains(key)) {
                rows.push_str("  answer: not yet answered\n");
            }
            rows
        }
        Some(events::QUESTION_ANSWERED | events::QUESTION_CLOSED)
            if key.is_some_and(|key| !resolved_so_far.insert(key)) =>
        {
            format!("  ignored {id}: already answered or closed by an earlier event\n")
        }
        Some(events::QUESTION_ANSWERED) => format!(
            "  answer {id}: {} by {} via {}\n",
            answer_text(event),
            text_field(event, "answered_by"),
            text_field(event, "policy")
        ),
        Some(events::QUESTION_CLOSED) => {
            format!("  closed {id}: {}\n", text_field(event, "reason"))
        }
        _ => String::new(),
    }
}

/// `<id> "<label>"[ (recommended)]` per option, in payload order.
fn options_line(event: &serde_json::Value) -> String {
    let Some(serde_json::Value::Array(options)) = event.get("options") else {
        return text_field(event, "options");
    };
    if options.is_empty() {
        return "(none)".into();
    }
    let recommended = event.get("recommended").and_then(|id| id.as_str());
    options
        .iter()
        .map(|option| {
            let is_recommended = recommended
                .is_some_and(|rec| option.get("id").and_then(|id| id.as_str()) == Some(rec));
            let mark = if is_recommended { " (recommended)" } else { "" };
            let label = text_field(option, "label");
            format!("{} \"{label}\"{mark}", text_field(option, "id"))
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// `option <id>` or `text "<text>"`, marking an empty text answer so it never
/// reads as an open question; any other shape is shown as recorded.
fn answer_text(event: &serde_json::Value) -> String {
    match event.get("answer") {
        Some(answer) if answer.get("option_id").is_some() => {
            format!("option {}", text_field(answer, "option_id"))
        }
        Some(answer) if answer.get("text").is_some() => {
            let text = text_field(answer, "text");
            let empty = if text.is_empty() { " (empty)" } else { "" };
            format!("text \"{text}\"{empty}")
        }
        _ => text_field(event, "answer"),
    }
}

/// A field as terminal-safe text, never truncated (D-16 keeps the full
/// text): a string as is, any other value as JSON, an absent one as
/// [`MISSING_FIELD`].
fn text_field(value: &serde_json::Value, key: &str) -> String {
    let raw = match value.get(key) {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => return MISSING_FIELD.into(),
    };
    gates::render_gate_context(&raw, usize::MAX)
}

/// The decision ledger of one phase (D-14): a fold over the event log, with
/// no store of its own. Questions triage decided are in `decided`; every
/// other question, answered by the operator or not, is in `escalated`.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct Ledger {
    pub phase: PhaseId,
    pub decided: Vec<LedgerEntry>,
    pub escalated: Vec<LedgerEntry>,
}

/// One question in a [`Ledger`]. An optional field the log did not record
/// is absent from the JSON, never empty: an escalated question the operator
/// has not answered has no `answer` key.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct LedgerEntry {
    pub question_id: String,
    /// `ts` of the `question_asked` event.
    pub asked_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// The options array as recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Absent for a question triage decided: no gate was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_started_at: Option<String>,
    /// The `triage` object of the question: scores and thresholds in force.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage: Option<serde_json::Value>,
    /// Why the question went to the operator; absent for a decided one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<LedgerFallback>,
    /// The first recorded answer; absent until one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// The close reason when the question ended without an answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<String>,
    /// Operator overrides of the answer, in log order.
    pub overrides: Vec<LedgerOverride>,
    /// The last override's answer, else `answer`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_answer: Option<serde_json::Value>,
}

/// An operator's replacement of a triage answer, in log order.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct LedgerOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_id: Option<String>,
    pub answer: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overridden_by: Option<String>,
    /// `ts` of the `question_overridden` event.
    pub at: u64,
}

/// Why triage could not score a question, from its `triage_fallback` event.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct LedgerFallback {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The `reason` of a question that was not triaged at all.
const NOT_TRIAGED: &str = "not_triaged";

/// Fold the phase's question events from `.devflow/events.jsonl` into its
/// ledger. Only the first answer or close of a question applies, as in
/// `devflow history`; overrides apply in log order. The last `question_asked`
/// of a question sets its triage outcome and gate; it keeps the first one's
/// place.
pub fn ledger(project_root: &Path, phase: PhaseId) -> Ledger {
    let events = phase_events(project_root, phase);
    let mut entries: Vec<LedgerEntry> = Vec::new();
    let mut positions: std::collections::HashMap<String, usize> = Default::default();
    for (timestamp, _, event) in &events {
        if event_kind(event) != Some(events::QUESTION_ASKED) {
            continue;
        }
        let Some(id) = string_field(event, "question_id") else {
            continue;
        };
        // A later ask of the same question (an escalation of a decision whose
        // answer could not be recorded) replaces it in the first one's place.
        if let Some(&index) = positions.get(&id) {
            let first_asked_at = entries[index].asked_at;
            entries[index] = asked_entry(id, first_asked_at, event);
            continue;
        }
        positions.insert(id.clone(), entries.len());
        entries.push(asked_entry(id, *timestamp, event));
    }

    let mut resolved = HashSet::new();
    for (timestamp, _, event) in &events {
        let Some(&index) = event
            .get("question_id")
            .and_then(|id| id.as_str())
            .and_then(|id| positions.get(id))
        else {
            continue;
        };
        apply_event(&mut entries[index], *timestamp, event, &mut resolved, index);
    }

    let (mut decided, mut escalated) = (Vec::new(), Vec::new());
    for mut entry in entries {
        entry.effective_answer = entry
            .overrides
            .last()
            .map(|last| last.answer.clone())
            .or_else(|| entry.answer.clone());
        entry.reason = escalation_reason(entry.triage.as_ref());
        if entry.reason.is_none() {
            decided.push(entry);
        } else {
            escalated.push(entry);
        }
    }
    Ledger {
        phase,
        decided,
        escalated,
    }
}

/// A ledger entry from a `question_asked` event, with no answer yet.
fn asked_entry(question_id: String, asked_at: u64, event: &serde_json::Value) -> LedgerEntry {
    LedgerEntry {
        question_id,
        asked_at,
        stage: string_field(event, "stage"),
        kind: string_field(event, "kind"),
        question: string_field(event, "question"),
        options: event.get("options").cloned(),
        recommended: string_field(event, "recommended"),
        context: string_field(event, "context"),
        gate_id: string_field(event, "gate_id"),
        run_started_at: string_field(event, "run_started_at"),
        triage: event.get("triage").filter(|t| t.is_object()).cloned(),
        reason: None,
        fallback: None,
        answer: None,
        answered_by: None,
        policy: None,
        closed: None,
        overrides: Vec::new(),
        effective_answer: None,
    }
}

/// Apply an answer, close, override or triage fallback to its entry.
fn apply_event(
    entry: &mut LedgerEntry,
    timestamp: u64,
    event: &serde_json::Value,
    resolved: &mut HashSet<usize>,
    index: usize,
) {
    match event_kind(event) {
        Some(events::QUESTION_ANSWERED) if resolved.insert(index) => {
            entry.answer = Some(event.get("answer").cloned().unwrap_or_default());
            entry.answered_by = string_field(event, "answered_by");
            entry.policy = string_field(event, "policy");
        }
        Some(events::QUESTION_CLOSED) if resolved.insert(index) => {
            entry.closed = Some(string_field(event, "reason").unwrap_or(MISSING_FIELD.into()));
        }
        Some(events::QUESTION_OVERRIDDEN) => entry.overrides.push(LedgerOverride {
            override_id: string_field(event, "override_id"),
            answer: event.get("answer").cloned().unwrap_or_default(),
            was: event.get("was").cloned(),
            overridden_by: string_field(event, "overridden_by"),
            at: timestamp,
        }),
        Some(events::TRIAGE_FALLBACK) => {
            entry.fallback = Some(LedgerFallback {
                reason: string_field(event, "reason").unwrap_or(MISSING_FIELD.into()),
                detail: string_field(event, "detail"),
            });
        }
        _ => {}
    }
}

/// `None` for a question triage decided; otherwise why it was escalated.
fn escalation_reason(triage: Option<&serde_json::Value>) -> Option<String> {
    let Some(triage) = triage else {
        return Some(NOT_TRIAGED.into());
    };
    let outcome = triage.get("outcome").and_then(|outcome| outcome.as_str());
    if outcome == Some("decided") {
        return None;
    }
    let reason = triage.get("reason").and_then(|reason| reason.as_str());
    Some(reason.or(outcome).unwrap_or(MISSING_FIELD).into())
}

fn event_kind(event: &serde_json::Value) -> Option<&str> {
    event.get("event").and_then(|kind| kind.as_str())
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(String::from)
}

/// The ledger as text: a `Decided (N)` section, then `Escalated (N)`.
pub fn render_ledger(ledger: &Ledger) -> String {
    let mut rendered = format!("decision ledger for phase {}\n", ledger.phase);
    for (heading, entries) in [
        ("Decided", &ledger.decided),
        ("Escalated", &ledger.escalated),
    ] {
        rendered.push_str(&format!("{heading} ({})\n", entries.len()));
        if entries.is_empty() {
            rendered.push_str("  none\n");
        }
        for entry in entries {
            rendered.push_str(&ledger_entry_block(entry));
        }
    }
    rendered.trim_end().to_string()
}

/// Optional text as terminal-safe text, an absent value as [`MISSING_FIELD`].
fn safe_text(text: Option<&str>) -> String {
    text.map_or(MISSING_FIELD.into(), |text| {
        gates::render_gate_context(text, usize::MAX)
    })
}

/// One entry: header, options, triage, answer and overrides.
fn ledger_entry_block(entry: &LedgerEntry) -> String {
    let gate = entry
        .gate_id
        .as_deref()
        .map(|gate| format!(", gate {}", safe_text(Some(gate))))
        .unwrap_or_default();
    let mut block = format!(
        "  {} ({}, {}{gate}): {}\n",
        gates::render_gate_context(&entry.question_id, usize::MAX),
        safe_text(entry.stage.as_deref()),
        safe_text(entry.kind.as_deref()),
        safe_text(entry.question.as_deref())
    );
    let options = serde_json::json!({"options": entry.options, "recommended": entry.recommended});
    block.push_str(&format!("    options: {}\n", options_line(&options)));
    if let Some(reason) = &entry.reason {
        block.push_str(&format!("    reason: {}\n", safe_text(Some(reason))));
    }
    if let Some(triage) = &entry.triage {
        block.push_str(&format!("    {}\n", triage_line(triage)));
    }
    if let Some(fallback) = &entry.fallback {
        block.push_str(&format!(
            "    fallback: {} {}\n",
            safe_text(Some(&fallback.reason)),
            safe_text(fallback.detail.as_deref())
        ));
    }
    block.push_str(&format!("    {}\n", ledger_answer_line(entry)));
    for change in &entry.overrides {
        block.push_str(&format!(
            "    override {}: {} (was {}) by {} at {}\n",
            safe_text(change.override_id.as_deref()),
            answer_value_text(&change.answer),
            change
                .was
                .as_ref()
                .map_or(MISSING_FIELD.into(), answer_value_text),
            safe_text(change.overridden_by.as_deref()),
            change.at
        ));
    }
    if let (false, Some(effective)) = (entry.overrides.is_empty(), &entry.effective_answer) {
        block.push_str(&format!(
            "    effective answer: {}\n",
            answer_value_text(effective)
        ));
    }
    block
}

/// `answer: ...` for an answered, closed or still open question.
fn ledger_answer_line(entry: &LedgerEntry) -> String {
    match (&entry.answer, &entry.closed) {
        (Some(answer), _) => format!(
            "answer: {} by {} via {}",
            answer_value_text(answer),
            safe_text(entry.answered_by.as_deref()),
            safe_text(entry.policy.as_deref())
        ),
        (None, Some(reason)) => format!("answer: none, closed ({})", safe_text(Some(reason))),
        (None, None) => "answer: no answer yet — answer it with devflow gate answer".into(),
    }
}

/// [`answer_text`] for a bare answer value.
fn answer_value_text(answer: &serde_json::Value) -> String {
    answer_text(&serde_json::json!({ "answer": answer }))
}

/// `triage: <outcome>, pre-check .., top probability .., model ..`.
fn triage_line(triage: &serde_json::Value) -> String {
    let outcome = text_field(triage, "outcome");
    let head = if outcome == "decided" {
        format!("decided {}", text_field(triage, "top_option"))
    } else if triage.get("reason").is_some() {
        format!("{outcome} {}", text_field(triage, "reason"))
    } else {
        outcome
    };
    let pre_check = match triage.get("pre_check") {
        Some(_) => format!(
            "pre-check {} (threshold {})",
            text_field(triage, "pre_check"),
            text_field(triage, "pre_check_threshold")
        ),
        None => format!(
            "pre-check threshold {}",
            text_field(triage, "pre_check_threshold")
        ),
    };
    let top = match triage.get("top_probability") {
        Some(_) => format!(
            "top probability {} for {} (threshold {})",
            text_field(triage, "top_probability"),
            text_field(triage, "top_option"),
            text_field(triage, "threshold")
        ),
        None => format!(
            "top probability threshold {}",
            text_field(triage, "threshold")
        ),
    };
    format!(
        "triage: {head}, {pre_check}, {top}, model {}",
        text_field(triage, "model")
    )
}

struct CaptureGeneration {
    stamp: String,
    timestamp: u64,
    sequence: u64,
    capture_files: Vec<PathBuf>,
    review_files: Vec<PathBuf>,
}

fn capture_generations(project_root: &Path, phase: PhaseId) -> Vec<CaptureGeneration> {
    let dir = agent_result::history_dir(project_root, phase);
    let Ok(files) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut generations: BTreeMap<String, CaptureGeneration> = BTreeMap::new();
    for file in files.flatten() {
        let path = file.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let stamp = name
            .strip_suffix("-stdout")
            .or_else(|| name.strip_suffix("-exit"))
            .or_else(|| name.strip_suffix("-REVIEW.md"));
        let Some(stamp) = stamp else { continue };
        let Some(nanos) = stamp
            .split('-')
            .next()
            .and_then(|value| value.parse::<u128>().ok())
        else {
            continue;
        };
        let timestamp = (nanos / 1_000_000_000).min(u64::MAX as u128) as u64;
        let sequence = stamp
            .split('-')
            .nth(1)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let generation =
            generations
                .entry(stamp.to_string())
                .or_insert_with(|| CaptureGeneration {
                    stamp: stamp.to_string(),
                    timestamp,
                    sequence,
                    capture_files: Vec::new(),
                    review_files: Vec::new(),
                });
        if name.ends_with("-REVIEW.md") {
            generation.review_files.push(path);
        } else {
            generation.capture_files.push(path);
        }
    }
    let mut generations = generations.into_values().collect::<Vec<_>>();
    for generation in &mut generations {
        generation.capture_files.sort();
        generation.review_files.sort();
    }
    generations.sort_by_key(|generation| (generation.timestamp, generation.sequence));
    generations
}

fn review_files(project_root: &Path, phase: PhaseId) -> Vec<PathBuf> {
    let phases = project_root.join(".planning").join("phases");
    let prefix = format!("{padded}-", padded = phase.padded());
    let mut reviews = Vec::new();
    let Ok(dirs) = std::fs::read_dir(phases) else {
        return reviews;
    };
    for dir in dirs.flatten() {
        if !dir
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&prefix))
        {
            continue;
        }
        collect_reviews(&dir.path(), &mut reviews);
    }
    reviews.sort_by_key(|path| (modified_timestamp(path), path.clone()));
    reviews
}

fn collect_reviews(dir: &Path, reviews: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect_reviews(&path, reviews);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("REVIEW.md"))
        {
            reviews.push(path);
        }
    }
}

fn modified_timestamp(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn attach_artifacts(
    entries: &mut Vec<AttemptEntry>,
    timestamp: u64,
    files: Vec<PathBuf>,
    captures: bool,
) {
    if entries.is_empty() {
        entries.push(AttemptEntry {
            timestamp,
            event: None,
            capture_files: Vec::new(),
            review_files: Vec::new(),
        });
    }
    let index = entries
        .iter()
        .rposition(|entry| entry.timestamp <= timestamp)
        .unwrap_or(0);
    if captures {
        entries[index].capture_files.extend(files);
    } else {
        entries[index].review_files.extend(files);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn seed_event_log(root: &Path) {
        let path = events::events_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            concat!(
                r#"{"v":1,"ts":30,"phase":16,"event":"hook_run","hook":"Merge"}"#,
                "\n",
                r#"{"v":1,"ts":10,"phase":16,"event":"transition","to":"code"}"#,
                "\n",
                r#"{"v":1,"ts":20,"phase":16,"event":"gate_fired","stage":"validate"}"#,
                "\n",
                r#"{"v":1,"ts":21,"phase":16,"event":"capture_archived","stage":"validate","stamp":"20000000000-0"}"#,
                "\n",
                r#"{"v":1,"ts":15,"phase":99,"event":"workflow_started"}"#,
                "\n",
            ),
        )
        .unwrap();
    }

    /// Render the timeline of phase 53 from these event lines, in order.
    fn render_events(events: &[serde_json::Value]) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = events::events_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: String = events.iter().map(|event| format!("{event}\n")).collect();
        std::fs::write(path, lines).unwrap();
        render_timeline(&attempt_timeline(dir.path(), PhaseId::new(53)))
    }

    fn asked(ts: u64, question_id: &str, question: &str) -> serde_json::Value {
        serde_json::json!({
            "v": 1, "ts": ts, "phase": 53, "event": events::QUESTION_ASKED,
            "stage": "code", "question_id": question_id, "gate_id": "g-1",
            "kind": "decision", "question": question,
            "options": [
                {"id": "keep", "label": "Keep the cache"},
                {"id": "drop", "label": "Drop the cache", "description": "slower"},
            ],
            "context": null, "recommended": "keep",
        })
    }

    fn answered(ts: u64, question_id: &str, answer: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "v": 1, "ts": ts, "phase": 53, "event": events::QUESTION_ANSWERED,
            "stage": "code", "question_id": question_id, "gate_id": "g-1",
            "answer": answer, "policy": events::ANSWERING_POLICY_OPERATOR_VIA_GATE,
            "answered_by": "operator",
        })
    }

    fn closed(ts: u64, question_id: &str, reason: &str) -> serde_json::Value {
        serde_json::json!({
            "v": 1, "ts": ts, "phase": 53, "event": events::QUESTION_CLOSED,
            "stage": "code", "question_id": question_id, "gate_id": "g-1",
            "reason": reason, "closed_by": null,
        })
    }

    /// `asked` with a `triage` object of this outcome; a decided question
    /// has no gate.
    fn triaged(
        ts: u64,
        question_id: &str,
        outcome: &str,
        reason: Option<&str>,
    ) -> serde_json::Value {
        let mut event = asked(ts, question_id, "Keep the cache?");
        let mut triage = serde_json::json!({
            "outcome": outcome, "pre_check": 0.93, "top_option": "keep",
            "top_probability": 0.91, "threshold": 0.85,
            "pre_check_threshold": 0.8, "model": "jev-test",
        });
        if let Some(reason) = reason {
            triage["reason"] = serde_json::json!(reason);
        }
        event["triage"] = triage;
        if outcome == "decided" {
            event.as_object_mut().unwrap().remove("gate_id");
        }
        event
    }

    fn triage_answered(ts: u64, question_id: &str) -> serde_json::Value {
        let mut event = answered(ts, question_id, serde_json::json!({"option_id": "keep"}));
        event["policy"] = serde_json::json!(events::ANSWERING_POLICY_TRIAGE);
        event["answered_by"] = serde_json::json!("triage:jev");
        event.as_object_mut().unwrap().remove("gate_id");
        event
    }

    fn overridden(ts: u64, question_id: &str, answer: &str, was: &str) -> serde_json::Value {
        serde_json::json!({
            "v": 1, "ts": ts, "phase": 53, "event": events::QUESTION_OVERRIDDEN,
            "question_id": question_id, "override_id": format!("o-{ts}"),
            "answer": {"option_id": answer}, "was": {"option_id": was},
            "overridden_by": "operator", "run_started_at": "100",
        })
    }

    /// The ledger of phase 53 from these event lines, in order.
    fn ledger_of(events: &[serde_json::Value]) -> Ledger {
        let dir = tempfile::tempdir().unwrap();
        let path = events::events_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let lines: String = events.iter().map(|event| format!("{event}\n")).collect();
        std::fs::write(path, lines).unwrap();
        ledger(dir.path(), PhaseId::new(53))
    }

    fn ids(entries: &[LedgerEntry]) -> Vec<&str> {
        entries
            .iter()
            .map(|entry| entry.question_id.as_str())
            .collect()
    }

    #[test]
    fn an_escalated_entry_has_no_answer_field() {
        let mut events = vec![
            triaged(10, "q-decided", "decided", None),
            triage_answered(11, "q-decided"),
            triaged(
                12,
                "q-escalated",
                "escalated",
                Some("top_probability_below_threshold"),
            ),
        ];
        let json = serde_json::to_value(ledger_of(&events)).unwrap();
        assert!(json["escalated"][0].get("answer").is_none(), "{json}");
        assert!(json["decided"][0].get("answer").is_some(), "{json}");

        events.push(answered(
            13,
            "q-escalated",
            serde_json::json!({"option_id": "drop"}),
        ));
        let json = serde_json::to_value(ledger_of(&events)).unwrap();
        assert_eq!(
            json["escalated"][0].get("answer"),
            Some(&serde_json::json!({"option_id": "drop"})),
            "{json}"
        );
    }

    #[test]
    fn an_empty_phase_lists_two_empty_sections() {
        let ledger = ledger_of(&[]);
        assert!(ledger.decided.is_empty() && ledger.escalated.is_empty());

        let rendered = render_ledger(&ledger);
        assert!(rendered.contains("Decided (0)\n  none\n"), "{rendered}");
        assert!(rendered.ends_with("Escalated (0)\n  none"), "{rendered}");
        let json = serde_json::to_value(&ledger).unwrap();
        assert_eq!(json["decided"], serde_json::json!([]), "{json}");
        assert_eq!(json["escalated"], serde_json::json!([]), "{json}");
    }

    #[test]
    fn entries_keep_log_order_within_one_second() {
        let ledger = ledger_of(&[
            asked(10, "q-b", "Second id, first line?"),
            asked(10, "q-a", "First id, second line?"),
        ]);
        assert_eq!(ids(&ledger.escalated), ["q-b", "q-a"]);
    }

    #[test]
    fn overrides_are_listed_in_order_and_the_last_wins() {
        let ledger = ledger_of(&[
            triaged(10, "q-1", "decided", None),
            triage_answered(11, "q-1"),
            overridden(12, "q-1", "drop", "keep"),
            overridden(13, "q-1", "keep", "drop"),
        ]);
        let entry = &ledger.decided[0];
        let answers: Vec<_> = entry.overrides.iter().map(|o| o.answer.clone()).collect();
        assert_eq!(
            answers,
            [
                serde_json::json!({"option_id": "drop"}),
                serde_json::json!({"option_id": "keep"})
            ]
        );
        assert_eq!(
            entry.effective_answer,
            Some(serde_json::json!({"option_id": "keep"}))
        );
        assert_eq!(entry.answer, Some(serde_json::json!({"option_id": "keep"})));

        let rendered = render_ledger(&ledger);
        let first = rendered.find("override o-12: option drop (was option keep)");
        let second = rendered.find("override o-13: option keep (was option drop)");
        assert!(first.is_some() && second > first, "{rendered}");
        assert!(
            rendered.contains("answer: option keep by triage:jev via triage"),
            "{rendered}"
        );
        assert!(
            rendered.contains("effective answer: option keep"),
            "{rendered}"
        );
    }

    #[test]
    fn a_question_without_a_triage_object_is_escalated_as_not_triaged() {
        let ledger = ledger_of(&[asked(10, "q-1", "Keep the cache?")]);
        assert!(ledger.decided.is_empty());
        assert_eq!(ids(&ledger.escalated), ["q-1"]);
        assert_eq!(ledger.escalated[0].reason.as_deref(), Some("not_triaged"));
    }

    #[test]
    fn a_later_question_asked_supersedes_a_decided_one() {
        let mut fallback = triaged(12, "q-1", "fallback", Some("ledger_unwritable"));
        fallback["triage"]
            .as_object_mut()
            .unwrap()
            .retain(|key, _| {
                [
                    "outcome",
                    "reason",
                    "threshold",
                    "pre_check_threshold",
                    "model",
                ]
                .contains(&key.as_str())
            });
        let ledger = ledger_of(&[
            asked(9, "q-0", "Earlier question?"),
            triaged(10, "q-1", "decided", None),
            asked(11, "q-2", "Later question?"),
            fallback,
        ]);
        assert!(ledger.decided.is_empty(), "{:?}", ids(&ledger.decided));
        assert_eq!(ids(&ledger.escalated), ["q-0", "q-1", "q-2"]);
        let entry = &ledger.escalated[1];
        assert_eq!(entry.reason.as_deref(), Some("ledger_unwritable"));
        assert_eq!(entry.gate_id.as_deref(), Some("g-1"));
        let json = serde_json::to_value(entry).unwrap();
        assert!(json.get("answer").is_none(), "{json}");
    }

    #[test]
    fn history_shows_an_override_a_fallback_and_a_discard() {
        let rendered = render_events(&[
            serde_json::json!({
                "v": 1, "ts": 9, "phase": 53, "event": events::TRIAGE_FALLBACK,
                "question_id": "q-1", "run_started_at": "100",
                "reason": "timeout", "detail": "no answer in 5s",
            }),
            triaged(10, "q-1", "decided", None),
            triage_answered(11, "q-1"),
            overridden(12, "q-1", "drop", "keep"),
            serde_json::json!({
                "v": 1, "ts": 13, "phase": 53, "event": events::OVERRIDES_DELIVERED,
                "run_started_at": "100", "override_ids": ["o-12", "o-99"],
            }),
            serde_json::json!({
                "v": 1, "ts": 14, "phase": 53, "event": events::RUN_DISCARDED,
                "started_at": "100",
            }),
        ]);

        assert!(
            rendered.contains("  override q-1: option drop (was option keep) by operator"),
            "{rendered}"
        );
        assert!(
            rendered.contains("  overrides delivered to the launch: 2"),
            "{rendered}"
        );
        assert!(
            rendered.contains("  triage fell back for q-1: timeout"),
            "{rendered}"
        );
        assert!(
            rendered.contains("  run discarded: started at 100"),
            "{rendered}"
        );
        assert!(!rendered.contains("ignored"), "{rendered}");
    }

    #[test]
    fn a_triage_decided_question_shows_its_scores() {
        let rendered = render_events(&[
            triaged(10, "q-1", "decided", None),
            triage_answered(11, "q-1"),
        ]);

        let triage_row = rendered
            .lines()
            .find(|line| line.starts_with("  triage: decided keep"))
            .unwrap_or_else(|| panic!("no triage row in {rendered}"));
        assert!(triage_row.contains("pre-check 0.93"), "{triage_row}");
        assert!(triage_row.contains("top probability 0.91"), "{triage_row}");
        assert!(
            rendered.contains("  answer q-1: option keep by triage:jev via triage"),
            "{rendered}"
        );
        assert!(!rendered.contains("ignored"), "{rendered}");
    }

    const QUESTION_ROW_PREFIXES: [&str; 7] = [
        "  question ",
        "  options: ",
        "  recommended: ",
        "  answer: ",
        "  answer ",
        "  closed ",
        "  ignored ",
    ];

    #[test]
    fn an_open_question_renders_not_yet_answered() {
        let rendered = render_events(&[asked(10, "q-1", "Keep the cache?")]);

        assert!(rendered.contains("question_asked (code)"), "{rendered}");
        assert!(
            rendered.contains("  question q-1: Keep the cache?"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                r#"  options: keep "Keep the cache" (recommended) | drop "Drop the cache""#
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("  answer: not yet answered"),
            "{rendered}"
        );
    }

    #[test]
    fn an_empty_text_answer_renders_distinct_from_not_yet_answered() {
        let rendered = render_events(&[
            asked(10, "q-1", "Anything to add?"),
            answered(11, "q-1", serde_json::json!({"text": ""})),
        ]);

        assert!(
            rendered.contains(r#"  answer q-1: text "" (empty) by operator via operator_via_gate"#),
            "{rendered}"
        );
        assert!(!rendered.contains("not yet answered"), "{rendered}");
    }

    #[test]
    fn an_agent_answer_shows_its_agent_in_history() {
        let mut by_agent = answered(11, "q-1", serde_json::json!({"option_id": "keep"}));
        by_agent["answered_by"] = serde_json::json!("agent:claude");
        let rendered = render_events(&[asked(10, "q-1", "Keep the cache?"), by_agent]);

        assert!(
            rendered.contains("by agent:claude via operator_via_gate"),
            "{rendered}"
        );
    }

    #[test]
    fn a_run_with_no_questions_has_no_question_rows() {
        let dir = tempfile::tempdir().unwrap();
        seed_event_log(dir.path());
        let rendered = render_timeline(&attempt_timeline(dir.path(), PhaseId::new(16)));

        assert!(rendered.contains("gate_fired (validate)"), "{rendered}");
        for line in rendered.lines() {
            for prefix in QUESTION_ROW_PREFIXES {
                assert!(
                    !line.starts_with(prefix),
                    "question row {line:?} in {rendered}"
                );
            }
        }
    }

    #[test]
    fn question_rows_pair_by_question_id_not_position() {
        let rendered = render_events(&[
            asked(10, "q-1", "First?"),
            asked(11, "q-2", "Second?"),
            answered(12, "q-2", serde_json::json!({"option_id": "drop"})),
            answered(13, "q-1", serde_json::json!({"option_id": "keep"})),
        ]);

        assert!(
            rendered.contains("  answer q-2: option drop by operator via operator_via_gate"),
            "{rendered}"
        );
        assert!(
            rendered.contains("  answer q-1: option keep by operator via operator_via_gate"),
            "{rendered}"
        );
        assert!(!rendered.contains("not yet answered"), "{rendered}");
    }

    #[test]
    fn question_text_is_rendered_terminal_safe() {
        let rendered = render_events(&[asked(10, "q-1", "Clear\u{1b}[2Jthe screen?")]);

        assert!(
            rendered.contains("  question q-1: Clear [2Jthe screen?"),
            "{rendered}"
        );
        assert!(!rendered.contains('\u{1b}'), "{rendered:?}");
    }

    #[test]
    fn a_second_resolution_for_one_question_is_shown_but_not_applied() {
        let rendered = render_events(&[
            asked(10, "q-1", "Keep the cache?"),
            answered(11, "q-1", serde_json::json!({"option_id": "keep"})),
            closed(12, "q-1", "timed_out"),
        ]);

        assert!(
            rendered.contains("  answer q-1: option keep by operator via operator_via_gate"),
            "{rendered}"
        );
        assert!(rendered.contains("question_closed (code)"), "{rendered}");
        assert!(!rendered.contains("  closed q-1:"), "{rendered}");
        assert!(
            rendered.contains("  ignored q-1: already answered or closed by an earlier event"),
            "{rendered}"
        );
    }

    #[test]
    fn timeline_orders_events_and_correlates_retained_captures() {
        let dir = tempfile::tempdir().unwrap();
        seed_event_log(dir.path());
        let captures = agent_result::history_dir(dir.path(), PhaseId::new(16));
        std::fs::create_dir_all(&captures).unwrap();
        std::fs::write(captures.join("20000000000-0-stdout"), "attempt output").unwrap();
        std::fs::write(captures.join("20000000000-0-exit"), "1").unwrap();

        let timeline = attempt_timeline(dir.path(), PhaseId::new(16));

        assert_eq!(timeline.entries.len(), 4);
        assert_eq!(timeline.entries[0].timestamp, 10);
        assert_eq!(timeline.entries[1].timestamp, 20);
        assert_eq!(timeline.entries[2].timestamp, 21);
        assert_eq!(timeline.entries[3].timestamp, 30);
        assert_eq!(timeline.entries[2].capture_files.len(), 2);
        assert!(
            timeline.entries[2]
                .capture_files
                .iter()
                .all(|path| path.starts_with(&captures))
        );
        let rendered = render_timeline(&timeline);
        assert!(rendered.contains("transition (code)"));
        assert!(rendered.contains("gate_fired (validate)"));
        assert!(rendered.contains("capture:"));
    }

    #[test]
    fn empty_phase_has_clean_no_attempts_result() {
        let dir = tempfile::tempdir().unwrap();
        let timeline = attempt_timeline(dir.path(), PhaseId::new(42));

        assert!(timeline.entries.is_empty());
        assert_eq!(
            render_timeline(&timeline),
            "no attempts recorded for phase 42"
        );
    }

    #[test]
    fn orphaned_capture_and_review_artifacts_remain_visible() {
        let dir = tempfile::tempdir().unwrap();
        let captures = agent_result::history_dir(dir.path(), PhaseId::new(16));
        std::fs::create_dir_all(&captures).unwrap();
        let archived_capture = captures.join("20000000000-2-stdout");
        let archived_review = captures.join("20000000000-2-REVIEW.md");
        std::fs::write(&archived_capture, "attempt output").unwrap();
        std::fs::write(&archived_review, "archived review").unwrap();

        let live_review = dir
            .path()
            .join(".planning/phases/16-example/nested/16-REVIEW.md");
        std::fs::create_dir_all(live_review.parent().unwrap()).unwrap();
        std::fs::write(&live_review, "current review").unwrap();

        let timeline = attempt_timeline(dir.path(), PhaseId::new(16));

        assert_eq!(timeline.entries.len(), 1);
        let entry = &timeline.entries[0];
        assert!(entry.event.is_none());
        assert_eq!(entry.capture_files, vec![archived_capture.clone()]);
        assert!(entry.review_files.contains(&archived_review));
        assert!(entry.review_files.contains(&live_review));

        let rendered = render_timeline(&timeline);
        assert!(rendered.contains("retained artifact"));
        assert!(rendered.contains(&archived_capture.display().to_string()));
        assert!(rendered.contains(&live_review.display().to_string()));
    }
}
