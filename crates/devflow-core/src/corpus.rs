//! The operator's past answers as labelled corpus entries, and the precedents
//! a question is sent to triage with (D-02, D-03, D-04, D-06, D-18, D-23,
//! D-24).
//!
//! Labels come from two sources in one project: the event log's answered
//! questions and the project's DISCUSSION-LOG files. Only answers the
//! operator gave are labels (D-13, [`crate::question::answerer_kind`]); every
//! answer left out is counted in [`ExclusionCounts`], never dropped silently.
//!
//! This module is pure apart from reading the project's files: the run-time
//! call ([`precedents_for`]) and the scoring tool both use it.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events;
use crate::phase_id::PhaseId;
use crate::question::{QuestionKind, QuestionOption, QuestionSpec};
use crate::triage::{Precedent, PrecedentAnswer, TriageConfig};

/// The most precedents one question is sent with (Pitfall 9).
pub const MAX_PRECEDENTS: usize = 8;

/// The most characters the serialized precedents of one question may take
/// (Pitfall 9). The question's own context is not counted and never dropped.
pub const PRECEDENT_CHAR_BUDGET: usize = 24_000;

/// Picks that hand the choice back to the agent (D-24), matched
/// case-insensitively on the chosen label or the choice text. Such a pick is
/// not a label.
pub const DELEGATION_PHRASES: &[&str] = &[
    "you decide",
    "your call",
    "your discretion",
    "claude decides",
    "claude's discretion",
    "agent's discretion",
    "let claude decide",
];

/// Where a corpus entry was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusSource {
    /// The project's `.devflow/events.jsonl`.
    Events,
    /// A `*-DISCUSSION-LOG.md` file under `.planning/`.
    DiscussionLog,
}

/// One answered question whose answer is the operator's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusEntry {
    /// `events:<question_id>`, or `log:<path relative to the project
    /// root>:<line of the option table>`.
    pub id: String,
    /// The project the entry was mined from, as the caller named it.
    pub project: String,
    /// The event's phase, or a log file's prefix as written (`11r`).
    pub phase: String,
    /// Where the entry was read from.
    pub source: CorpusSource,
    /// The question's kind; every log entry is `discuss`.
    pub kind: QuestionKind,
    /// The question.
    pub question: String,
    /// The options it offered, recommendation markers stripped from log
    /// labels.
    pub options: Vec<QuestionOption>,
    /// Context given with the question, if any.
    pub context: Option<String>,
    /// The operator's answer.
    pub answer: PrecedentAnswer,
    /// The recommended option's id, for the recommendation baseline (D-10).
    pub recommended: Option<String>,
    /// The event's `ts`, or the log line number: later is larger.
    pub order: u64,
}

/// What was left out of the corpus, by reason. Each count is of questions,
/// except [`ExclusionCounts::unreadable_event_lines`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ExclusionCounts {
    /// Answered by an agent, triage or nobody recorded, with no operator
    /// override (D-02, D-13).
    pub non_operator: usize,
    /// Asked and never answered.
    pub unanswered: usize,
    /// An operator label in a phase the caller excluded (D-23).
    pub excluded_phase: usize,
    /// A log pick made by a `--auto` discuss run (D-02).
    pub auto: usize,
    /// A pick that hands the choice back (D-24, [`DELEGATION_PHRASES`]).
    pub delegation: usize,
    /// A log question with more than one option selected.
    pub multi_select: usize,
    /// A log question whose phase's discuss ran through `devflow_ask`, so the
    /// event log already holds it.
    pub duplicate_log: usize,
    /// A block that does not fit the grammar.
    pub unparsed: usize,
    /// An event-log line that is not a JSON object (a torn last line, or
    /// garbage).
    pub unreadable_event_lines: usize,
}

/// A project's corpus entries and what was left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MiningReport {
    /// The labelled entries, event-log entries first, each source in file
    /// order.
    pub entries: Vec<CorpusEntry>,
    /// The counts of what was left out.
    pub excluded: ExclusionCounts,
}

/// Mine `project_root`'s event log and DISCUSSION-LOG files into labelled
/// entries named `project`. Event-log labels of `exclude_phases` are left
/// out and counted (D-23). A missing `.devflow/` or `.planning/` is an empty
/// corpus.
pub fn mine_project(
    project_root: &Path,
    project: &str,
    exclude_phases: &[PhaseId],
) -> MiningReport {
    let _ = exclude_phases;
    let mut report = MiningReport::default();
    mine_events(project_root, project, &mut report);
    report
}

/// The event log's v1 lines, in `(ts, line index)` order.
fn read_events(project_root: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(events::events_path(project_root)).unwrap_or_default();
    let mut indexed: Vec<(u64, usize, Value)> = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let event = serde_json::from_str::<Value>(line).ok()?;
            (event.get("v").and_then(Value::as_u64) == Some(1)).then(|| {
                let ts = event.get("ts").and_then(Value::as_u64).unwrap_or(0);
                (ts, index, event)
            })
        })
        .collect();
    indexed.sort_by_key(|(ts, index, _)| (*ts, *index));
    indexed.into_iter().map(|(_, _, event)| event).collect()
}

/// Pair each `question_asked` with its `question_answered` by `question_id`.
fn mine_events(project_root: &Path, project: &str, report: &mut MiningReport) {
    let mut asked: Vec<Value> = Vec::new();
    let mut answers: HashMap<String, Value> = HashMap::new();
    for event in read_events(project_root) {
        let Some(qid) = event.get("question_id").and_then(Value::as_str) else {
            continue;
        };
        match event.get("event").and_then(Value::as_str) {
            Some(events::QUESTION_ASKED) => asked.push(event.clone()),
            Some(events::QUESTION_ANSWERED) => {
                answers.insert(qid.to_string(), event.clone());
            }
            _ => {}
        }
    }
    for question in asked {
        let qid = question["question_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let Some(answer) = answers
            .get(&qid)
            .and_then(|a| precedent_answer(a.get("answer")))
        else {
            report.excluded.unanswered += 1;
            continue;
        };
        match event_entry(&question, &qid, project, answer) {
            Some(entry) => report.entries.push(entry),
            None => report.excluded.unparsed += 1,
        }
    }
}

/// An event's `answer` (`{option_id}` or `{text}`) as a precedent answer.
fn precedent_answer(answer: Option<&Value>) -> Option<PrecedentAnswer> {
    let answer = answer?;
    if let Some(option_id) = answer.get("option_id").and_then(Value::as_str) {
        return Some(PrecedentAnswer::Option {
            option_id: option_id.to_string(),
        });
    }
    let text = answer.get("text").and_then(Value::as_str)?;
    Some(PrecedentAnswer::OwnWords {
        text: text.to_string(),
    })
}

/// The entry a `question_asked` event and its label make, or `None` when
/// the event lacks a readable kind, question or options.
fn event_entry(
    question: &Value,
    qid: &str,
    project: &str,
    answer: PrecedentAnswer,
) -> Option<CorpusEntry> {
    let kind = serde_json::from_value::<QuestionKind>(question.get("kind")?.clone()).ok()?;
    let options =
        serde_json::from_value::<Vec<QuestionOption>>(question.get("options")?.clone()).ok()?;
    Some(CorpusEntry {
        id: format!("events:{qid}"),
        project: project.to_string(),
        phase: event_phase(question.get("phase")),
        source: CorpusSource::Events,
        kind,
        question: question.get("question")?.as_str()?.to_string(),
        options,
        context: question
            .get("context")
            .and_then(Value::as_str)
            .map(str::to_string),
        answer,
        recommended: question
            .get("recommended")
            .and_then(Value::as_str)
            .map(str::to_string),
        order: question.get("ts").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// An event's `phase` as written: the canonical phase id, or the raw value.
fn event_phase(phase: Option<&Value>) -> String {
    match (PhaseId::from_json(phase), phase) {
        (Some(id), _) => id.to_string(),
        (None, Some(Value::String(text))) => text.clone(),
        (None, Some(other)) => other.to_string(),
        (None, None) => String::new(),
    }
}

/// `text` lowercased, punctuation removed and whitespace collapsed.
pub fn normalize_question(text: &str) -> String {
    let kept: String = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The precedents `spec` is sent with, from `entries`: the most similar
/// first, at most [`MAX_PRECEDENTS`] within [`PRECEDENT_CHAR_BUDGET`]. The
/// entry whose id is `exclude_entry_id` is never among them (D-06
/// leave-one-out; run time passes `None`, since its question is new).
pub fn select_precedents(
    entries: &[CorpusEntry],
    spec: &QuestionSpec,
    exclude_entry_id: Option<&str>,
) -> Vec<Precedent> {
    let _ = spec;
    entries
        .iter()
        .filter(|entry| Some(entry.id.as_str()) != exclude_entry_id)
        .map(|entry| Precedent {
            question: entry.question.clone(),
            options: entry.options.clone(),
            answer: entry.answer.clone(),
        })
        .collect()
}

/// The precedents for `spec` at run time: this project's labels only (D-04),
/// with `config`'s excluded phases left out.
pub fn precedents_for(
    project_root: &Path,
    config: &TriageConfig,
    spec: &QuestionSpec,
) -> Vec<Precedent> {
    let report = mine_project(project_root, "", &config.exclude_label_phases);
    select_precedents(&report.entries, spec, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn options(labels: &[(&str, &str)]) -> Vec<QuestionOption> {
        labels
            .iter()
            .map(|(id, label)| QuestionOption {
                id: (*id).to_string(),
                label: (*label).to_string(),
                description: None,
            })
            .collect()
    }

    fn spec(question: &str, labels: &[(&str, &str)]) -> QuestionSpec {
        QuestionSpec {
            question_id: "fresh0000000000".to_string(),
            question: question.to_string(),
            options: options(labels),
            context: None,
            kind: QuestionKind::Decision,
            recommended: None,
        }
    }

    fn asked(qid: &str, phase: Value, kind: &str, question: &str, ts: u64) -> Value {
        json!({
            "v": 1, "ts": ts, "phase": phase, "event": "question_asked",
            "stage": "plan", "question_id": qid, "gate_id": format!("g-{qid}"),
            "kind": kind, "question": question,
            "options": [{"id": "a", "label": "Thirty seconds"}, {"id": "b", "label": "Sixty seconds"}],
            "context": null, "recommended": "a", "run_started_at": 1,
        })
    }

    fn answered(qid: &str, phase: Value, answer: Value, by: &str, ts: u64) -> Value {
        json!({
            "v": 1, "ts": ts, "phase": phase, "event": "question_answered",
            "stage": "plan", "question_id": qid, "gate_id": format!("g-{qid}"),
            "answer": answer, "policy": "operator_via_gate", "answered_by": by,
        })
    }

    fn write_events(root: &Path, lines: &[String]) {
        let dir = root.join(".devflow");
        std::fs::create_dir_all(&dir).unwrap();
        let mut text = lines.join("\n");
        text.push('\n');
        std::fs::write(dir.join("events.jsonl"), text).unwrap();
    }

    fn lines(events: &[Value]) -> Vec<String> {
        events.iter().map(Value::to_string).collect()
    }

    fn project() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        (dir, root)
    }

    #[test]
    fn a_projects_operator_answers_become_precedents() {
        let (_dir, root) = project();
        let budget = "Which retry budget should the build step use";
        let policy = "Which retry policy should the build step use";
        write_events(
            &root,
            &lines(&[
                asked("q1", json!(54), "decision", budget, 10),
                answered("q1", json!(54), json!({"option_id": "b"}), "op", 11),
                asked("q2", json!(54), "decision", policy, 20),
                answered(
                    "q2",
                    json!(54),
                    json!({"text": "back off twice then stop"}),
                    "op",
                    21,
                ),
            ]),
        );
        let config = TriageConfig::default();
        let new_question = spec(
            "Which retry budget should the deploy step use",
            &[("a", "Thirty seconds"), ("b", "Sixty seconds")],
        );

        let precedents = precedents_for(&root, &config, &new_question);
        let answers: Vec<&PrecedentAnswer> = precedents.iter().map(|p| &p.answer).collect();
        assert_eq!(precedents.len(), 2, "{precedents:?}");
        assert!(answers.contains(&&PrecedentAnswer::Option {
            option_id: "b".to_string()
        }));
        assert!(answers.contains(&&PrecedentAnswer::OwnWords {
            text: "back off twice then stop".to_string()
        }));

        let report = mine_project(&root, "devflow", &[]);
        assert_eq!(report.entries[0].id, "events:q1");
        assert_eq!(report.entries[0].project, "devflow");
        let left_out = select_precedents(&report.entries, &new_question, Some("events:q1"));
        assert_eq!(left_out.len(), 1);
        assert_eq!(left_out[0].question, policy);

        let same_text = spec(budget, &[("a", "Thirty seconds"), ("b", "Sixty seconds")]);
        assert_eq!(
            select_precedents(&report.entries, &same_text, None).len(),
            2
        );
    }

    #[test]
    fn normalize_question_folds_case_punctuation_and_spacing() {
        assert_eq!(
            normalize_question("  Keep IT,   simple?! "),
            "keep it simple"
        );
    }
}
