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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events;
use crate::phase_id::PhaseId;
use crate::question::{AnswererKind, QuestionKind, QuestionOption, QuestionSpec, answerer_kind};
use crate::triage::{
    Precedent, PrecedentAnswer, TriageConfig, precedent_state, strip_recommended_marker,
};

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
    let mut report = MiningReport::default();
    let channel_phases = mine_events(project_root, project, exclude_phases, &mut report);
    mine_logs(project_root, project, &channel_phases, &mut report);
    report
}

/// The event log's v1 lines in `(ts, line index)` order, and how many lines
/// were not a JSON object (a torn last line while another process appends,
/// or garbage). Blank lines are neither.
fn read_events(project_root: &Path) -> (Vec<Value>, usize) {
    let text = std::fs::read_to_string(events::events_path(project_root)).unwrap_or_default();
    let mut unreadable = 0;
    let mut indexed: Vec<(u64, usize, Value)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(event) if event.is_object() => {
                if event.get("v").and_then(Value::as_u64) == Some(1) {
                    let ts = event.get("ts").and_then(Value::as_u64).unwrap_or(0);
                    indexed.push((ts, index, event));
                }
            }
            _ => unreadable += 1,
        }
    }
    indexed.sort_by_key(|(ts, index, _)| (*ts, *index));
    let events = indexed.into_iter().map(|(_, _, event)| event).collect();
    (events, unreadable)
}

/// Whether the answerer an event field names is the operator (D-13). An
/// absent field is not.
fn is_operator(answerer: Option<&Value>) -> bool {
    let name = answerer.and_then(Value::as_str).unwrap_or_default();
    answerer_kind(name) == AnswererKind::Operator
}

/// Who gave a question's current answer.
enum EventLabel<'a> {
    /// The operator; the `answer` field it gave.
    Operator(Option<&'a Value>),
    /// An agent, triage or nobody recorded, with no operator override.
    NotOperator,
    /// No answer at all.
    Unanswered,
}

/// The label a question's answer and its last operator override give: the
/// override wins (D-18), else the answer when the operator gave it.
fn event_label<'a>(
    answered: Option<&'a Value>,
    operator_override: Option<&'a Value>,
) -> EventLabel<'a> {
    if let Some(overridden) = operator_override {
        return EventLabel::Operator(overridden.get("answer"));
    }
    match answered {
        Some(answer) if is_operator(answer.get("answered_by")) => {
            EventLabel::Operator(answer.get("answer"))
        }
        Some(_) => EventLabel::NotOperator,
        None => EventLabel::Unanswered,
    }
}

/// Pair each `question_asked` with its answer and its last operator override
/// by `question_id`, keeping operator labels outside `exclude_phases`.
/// Returns the phases holding an operator-answered `discuss` question: their
/// discuss ran through `devflow_ask`, so their logs restate the events.
fn mine_events(
    project_root: &Path,
    project: &str,
    exclude_phases: &[PhaseId],
    report: &mut MiningReport,
) -> HashSet<PhaseId> {
    let (log, unreadable) = read_events(project_root);
    report.excluded.unreadable_event_lines += unreadable;
    let mut asked: Vec<&Value> = Vec::new();
    let mut answers: HashMap<&str, &Value> = HashMap::new();
    let mut overrides: HashMap<&str, &Value> = HashMap::new();
    for event in &log {
        let Some(qid) = event.get("question_id").and_then(Value::as_str) else {
            continue;
        };
        match event.get("event").and_then(Value::as_str) {
            Some(events::QUESTION_ASKED) => asked.push(event),
            Some(events::QUESTION_ANSWERED) => {
                answers.insert(qid, event);
            }
            Some(events::QUESTION_OVERRIDDEN) if is_operator(event.get("overridden_by")) => {
                overrides.insert(qid, event);
            }
            _ => {}
        }
    }
    let mut channel_phases = HashSet::new();
    for question in asked {
        let qid = question["question_id"].as_str().unwrap_or_default();
        let answer = match event_label(answers.get(qid).copied(), overrides.get(qid).copied()) {
            EventLabel::Operator(answer) => answer,
            EventLabel::NotOperator => {
                report.excluded.non_operator += 1;
                continue;
            }
            EventLabel::Unanswered => {
                report.excluded.unanswered += 1;
                continue;
            }
        };
        let Some(entry) =
            precedent_answer(answer).and_then(|answer| event_entry(question, qid, project, answer))
        else {
            report.excluded.unparsed += 1;
            continue;
        };
        let phase = PhaseId::from_json(question.get("phase"));
        if let Some(phase) = phase.filter(|_| entry.kind == QuestionKind::Discuss) {
            channel_phases.insert(phase);
        }
        if phase.is_some_and(|phase| exclude_phases.contains(&phase)) {
            report.excluded.excluded_phase += 1;
            continue;
        }
        report.entries.push(entry);
    }
    channel_phases
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

/// The end of a discuss log's file name; the part before it is the phase.
const LOG_SUFFIX: &str = "-DISCUSSION-LOG.md";

/// The paths inside `dir` (none when it cannot be read), sorted.
fn dir_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

/// `<root>/.planning/phases/*/` and `<root>/.planning/milestones/*-phases/*/`
/// discuss logs, read at fixed depth so no symlink can loop the walk.
fn discussion_logs(project_root: &Path) -> Vec<PathBuf> {
    let planning = project_root.join(".planning");
    let mut phase_dirs = dir_paths(&planning.join("phases"));
    for milestone in dir_paths(&planning.join("milestones")) {
        let name = milestone.file_name().and_then(|name| name.to_str());
        if name.is_some_and(|name| name.ends_with("-phases")) {
            phase_dirs.extend(dir_paths(&milestone));
        }
    }
    phase_dirs
        .iter()
        .filter(|dir| dir.is_dir())
        .flat_map(|dir| dir_paths(dir))
        .filter(|path| path.is_file() && path.to_string_lossy().ends_with(LOG_SUFFIX))
        .collect()
}

/// One `| Option | Description | Selected |` table and the lines after it.
struct LogBlock {
    /// The 1-based line of the table header.
    line: usize,
    /// The question heading, else the area heading.
    question: Option<String>,
    /// The area heading, when the question came from a `### ` heading.
    context: Option<String>,
    /// Each row: label, description, whether it carries `✓`.
    rows: Vec<(String, String, bool)>,
    /// The `**User's choice…:**` line: its parenthetical and its text.
    choice: Option<(String, String)>,
    /// Whether the pick is marked `[auto]`.
    auto_marker: bool,
}

/// What a table header line is.
enum TableHeader {
    /// `| Option | Description | Selected |`: a question.
    Question,
    /// Another table whose first column is `Option`: a block that does not fit.
    Other,
    /// Not an option table.
    None,
}

/// A table line's trimmed cells.
fn row_cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn table_header(line: &str) -> TableHeader {
    if !line.trim_start().starts_with('|') {
        return TableHeader::None;
    }
    let cells: Vec<String> = row_cells(line)
        .iter()
        .map(|cell| cell.to_lowercase())
        .collect();
    match cells.as_slice() {
        [option, description, selected]
            if option == "option" && description == "description" && selected == "selected" =>
        {
            TableHeader::Question
        }
        [option, ..] if option == "option" => TableHeader::Other,
        _ => TableHeader::None,
    }
}

/// Whether a table line is a `|---|---|` separator.
fn is_separator(line: &str) -> bool {
    row_cells(line)
        .iter()
        .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'))
}

/// `title` without a `Q<n>:` / `Q<n> —` / `Q<n> -` / `Q<n>.` prefix.
fn strip_question_number(title: &str) -> String {
    let title = title.trim();
    if let Some(rest) = title.strip_prefix('Q') {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        let tail = &rest[digits..];
        let stripped = tail
            .trim_start_matches(|c: char| matches!(c, ':' | '—' | '-' | '.') || c.is_whitespace());
        if digits > 0 && stripped.len() < tail.len() {
            return stripped.to_string();
        }
    }
    title.to_string()
}

/// The parenthetical and text of a `**User's choice…:**` line at `start`,
/// the text running over following lines until a blank line, a heading, a
/// table row or a `**` line, cut at any `→`.
fn choice_at(lines: &[&str], start: usize, end: usize) -> Option<(String, String)> {
    let line = lines[start].trim();
    let rest = line
        .strip_prefix("**User's choice")
        .or_else(|| line.strip_prefix("**User\u{2019}s choice"))?;
    let close = rest.find(":**")?;
    let mut text = vec![rest[close + 3..].trim().to_string()];
    for line in &lines[start + 1..end] {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('|')
            || line.starts_with("**")
        {
            break;
        }
        text.push(line.to_string());
    }
    let joined = text.join(" ");
    let before_arrow = joined
        .split('→')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    Some((rest[..close].trim().to_string(), before_arrow))
}

/// The question table whose header is `lines[start]`, and the index after
/// its answer block (the next table or heading).
fn parse_block(
    lines: &[&str],
    start: usize,
    question: Option<String>,
    context: Option<String>,
) -> (LogBlock, usize) {
    let mut next = start + 1;
    let mut rows = Vec::new();
    while next < lines.len() && lines[next].trim_start().starts_with('|') {
        if !is_separator(lines[next]) {
            let cells = row_cells(lines[next]);
            let selected = cells.len() >= 2 && cells[cells.len() - 1].contains('✓');
            let description = if cells.len() >= 3 {
                cells[1..cells.len() - 1].join("|")
            } else {
                String::new()
            };
            rows.push((cells[0].clone(), description, selected));
        }
        next += 1;
    }
    let mut end = next;
    while end < lines.len() && !lines[end].trim_start().starts_with(['#', '|']) {
        end += 1;
    }
    let choice = (next..end).find_map(|index| choice_at(lines, index, end));
    let region_auto = lines[next..end]
        .iter()
        .any(|line| line.to_lowercase().contains("[auto]"));
    let row_auto = rows
        .iter()
        .any(|(label, _, selected)| *selected && label.to_lowercase().contains("[auto]"));
    let block = LogBlock {
        line: start + 1,
        question,
        context,
        rows,
        choice,
        auto_marker: region_auto || row_auto,
    };
    (block, end)
}

/// Every question table in a log, and how many other option tables it has.
fn parse_log(text: &str) -> (Vec<LogBlock>, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut other_tables = 0;
    let mut area: Option<String> = None;
    let mut heading: Option<String> = None;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if let Some(title) = line.strip_prefix("### ") {
            heading = Some(strip_question_number(title));
        } else if let Some(title) = line.strip_prefix("## ") {
            area = Some(title.trim().to_string());
            heading = None;
        } else {
            match table_header(line) {
                TableHeader::Question => {
                    let (question, context) = match heading.take() {
                        Some(heading) => (Some(heading), area.clone()),
                        None => (area.clone(), None),
                    };
                    let (block, end) = parse_block(&lines, index, question, context);
                    blocks.push(block);
                    index = end;
                    continue;
                }
                TableHeader::Other => {
                    other_tables += 1;
                    index += 1;
                    while index < lines.len() && lines[index].trim_start().starts_with('|') {
                        index += 1;
                    }
                    continue;
                }
                TableHeader::None => {}
            }
        }
        index += 1;
    }
    (blocks, other_tables)
}

/// How a log question's pick counts.
enum LogOutcome {
    Label(PrecedentAnswer),
    Auto,
    Delegation,
    MultiSelect,
    Unparsed,
}

/// `text` lowercased with whitespace collapsed and recommendation markers
/// removed, for matching a choice to a label.
fn fold_label(text: &str) -> String {
    let (text, _) = strip_recommended_marker(text);
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `text` hands the choice back (D-24).
fn is_delegation(text: &str) -> bool {
    let text = text.to_lowercase().replace('\u{2019}', "'");
    DELEGATION_PHRASES
        .iter()
        .any(|phrase| text.contains(phrase))
}

/// The pick a block records: a free-text choice, else the one `✓` row, else
/// the choice text matched to a label or kept as own words.
fn classify_block(block: &LogBlock, options: &[QuestionOption]) -> LogOutcome {
    if block.auto_marker {
        return LogOutcome::Auto;
    }
    let selected: Vec<&QuestionOption> = block
        .rows
        .iter()
        .zip(options)
        .filter(|((_, _, selected), _)| *selected)
        .map(|(_, option)| option)
        .collect();
    let free_text = block.choice.as_ref().filter(|(parenthetical, _)| {
        let parenthetical = parenthetical.to_lowercase();
        parenthetical.contains("text") || parenthetical.contains("verbatim")
    });
    let (answer, said) = match (free_text, selected.as_slice(), &block.choice) {
        (Some((_, text)), _, _) => (
            PrecedentAnswer::OwnWords { text: text.clone() },
            text.clone(),
        ),
        (None, [option], _) => (
            PrecedentAnswer::Option {
                option_id: option.id.clone(),
            },
            option.label.clone(),
        ),
        (None, [_, _, ..], _) => return LogOutcome::MultiSelect,
        (None, [], Some((_, text))) => match options
            .iter()
            .find(|option| fold_label(&option.label) == fold_label(text))
        {
            Some(option) => (
                PrecedentAnswer::Option {
                    option_id: option.id.clone(),
                },
                option.label.clone(),
            ),
            None => (
                PrecedentAnswer::OwnWords { text: text.clone() },
                text.clone(),
            ),
        },
        (None, [], None) => return LogOutcome::Unparsed,
    };
    if said.is_empty() {
        return LogOutcome::Unparsed;
    }
    if is_delegation(&said) {
        return LogOutcome::Delegation;
    }
    LogOutcome::Label(answer)
}

/// The options a block's rows offer, ids `a`, `b`, `c`… in order, markers
/// stripped, and the id of the first option marked recommended (D-10).
fn block_options(block: &LogBlock) -> (Vec<QuestionOption>, Option<String>) {
    let mut recommended = None;
    let options = block
        .rows
        .iter()
        .enumerate()
        .map(|(index, (label, description, _))| {
            let id = match u8::try_from(index) {
                Ok(offset) if offset < 26 => char::from(b'a' + offset).to_string(),
                _ => format!("option-{}", index + 1),
            };
            let (label, label_marked) = strip_recommended_marker(label);
            let (description, description_marked) = strip_recommended_marker(description);
            if (label_marked || description_marked) && recommended.is_none() {
                recommended = Some(id.clone());
            }
            let description = description.trim().to_string();
            QuestionOption {
                id,
                label: label.trim().to_string(),
                description: (!description.is_empty()).then_some(description),
            }
        })
        .collect();
    (options, recommended)
}

/// Mine every discuss log into `report`, skipping as duplicates the logs of
/// `channel_phases`.
fn mine_logs(
    project_root: &Path,
    project: &str,
    channel_phases: &HashSet<PhaseId>,
    report: &mut MiningReport,
) {
    for path in discussion_logs(project_root) {
        let relative = path
            .strip_prefix(project_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let prefix = file_name
            .strip_suffix(LOG_SUFFIX)
            .unwrap_or_default()
            .to_string();
        let Ok(text) = std::fs::read_to_string(&path) else {
            report.excluded.unparsed += 1;
            continue;
        };
        let (blocks, other_tables) = parse_log(&text);
        report.excluded.unparsed += other_tables;
        let file_auto = text
            .lines()
            .any(|line| line.trim_start().starts_with("**Mode:**") && line.contains("--auto"));
        let duplicate = prefix
            .parse::<PhaseId>()
            .is_ok_and(|phase| channel_phases.contains(&phase));
        for block in blocks {
            if file_auto {
                report.excluded.auto += 1;
                continue;
            }
            if duplicate {
                report.excluded.duplicate_log += 1;
                continue;
            }
            let (options, recommended) = block_options(&block);
            let Some(question) = block.question.clone().filter(|_| !options.is_empty()) else {
                report.excluded.unparsed += 1;
                continue;
            };
            let answer = match classify_block(&block, &options) {
                LogOutcome::Label(answer) => answer,
                LogOutcome::Auto => {
                    report.excluded.auto += 1;
                    continue;
                }
                LogOutcome::Delegation => {
                    report.excluded.delegation += 1;
                    continue;
                }
                LogOutcome::MultiSelect => {
                    report.excluded.multi_select += 1;
                    continue;
                }
                LogOutcome::Unparsed => {
                    report.excluded.unparsed += 1;
                    continue;
                }
            };
            report.entries.push(CorpusEntry {
                id: format!("log:{relative}:{}", block.line),
                project: project.to_string(),
                phase: prefix.clone(),
                source: CorpusSource::DiscussionLog,
                kind: QuestionKind::Discuss,
                question,
                options,
                context: block.context.clone(),
                answer,
                recommended,
                order: block.line as u64,
            });
        }
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
/// first, at most [`MAX_PRECEDENTS`] whose serialized forms together stay
/// within [`PRECEDENT_CHAR_BUDGET`], the lowest-ranked dropped first. The
/// entry whose id is `exclude_entry_id` is never among them (D-06
/// leave-one-out; run time passes `None`, since its question is new).
///
/// Similarity is the Jaccard overlap of the lowercase alphanumeric word sets
/// of question plus option labels. Ties go to event-log entries before log
/// entries, then the later `order`, then the smaller id.
pub fn select_precedents(
    entries: &[CorpusEntry],
    spec: &QuestionSpec,
    exclude_entry_id: Option<&str>,
) -> Vec<Precedent> {
    let target = question_words(&spec.question, &spec.options);
    let mut ranked: Vec<(f64, &CorpusEntry)> = entries
        .iter()
        .filter(|entry| Some(entry.id.as_str()) != exclude_entry_id)
        .map(|entry| {
            (
                jaccard(&target, &question_words(&entry.question, &entry.options)),
                entry,
            )
        })
        .collect();
    ranked.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .total_cmp(left_score)
            .then(source_rank(left.source).cmp(&source_rank(right.source)))
            .then(right.order.cmp(&left.order))
            .then(left.id.cmp(&right.id))
    });
    let mut kept = Vec::new();
    let mut used = 0;
    for (_, entry) in ranked.into_iter().take(MAX_PRECEDENTS) {
        let precedent = Precedent {
            question: entry.question.clone(),
            options: entry.options.clone(),
            answer: entry.answer.clone(),
        };
        let chars = precedent_state(&precedent).to_string().chars().count();
        if used + chars > PRECEDENT_CHAR_BUDGET {
            break;
        }
        used += chars;
        kept.push(precedent);
    }
    kept
}

/// Event-log entries rank before log entries on equal similarity.
fn source_rank(source: CorpusSource) -> u8 {
    match source {
        CorpusSource::Events => 0,
        CorpusSource::DiscussionLog => 1,
    }
}

/// The lowercase alphanumeric words of a question and its option labels.
fn question_words(question: &str, options: &[QuestionOption]) -> HashSet<String> {
    std::iter::once(question)
        .chain(options.iter().map(|option| option.label.as_str()))
        .flat_map(|text| text.split(|c: char| !c.is_alphanumeric()))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// `|a ∩ b| / |a ∪ b|`, or 0 when both are empty.
fn jaccard(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    let union = left.union(right).count();
    if union == 0 {
        return 0.0;
    }
    left.intersection(right).count() as f64 / union as f64
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

    fn overridden(qid: &str, phase: Value, answer: Value, by: &str, ts: u64) -> Value {
        json!({
            "v": 1, "ts": ts, "phase": phase, "event": "question_overridden",
            "question_id": qid, "override_id": format!("o-{qid}-{ts}"),
            "answer": answer, "was": {"option_id": "a"}, "overridden_by": by,
            "run_started_at": 1,
        })
    }

    fn option(id: &str) -> PrecedentAnswer {
        PrecedentAnswer::Option {
            option_id: id.to_string(),
        }
    }

    fn own_words(text: &str) -> PrecedentAnswer {
        PrecedentAnswer::OwnWords {
            text: text.to_string(),
        }
    }

    /// An entry whose own-words answer is its id, so a precedent names the
    /// entry it came from.
    fn entry(id: &str, source: CorpusSource, order: u64, question: &str) -> CorpusEntry {
        CorpusEntry {
            id: id.to_string(),
            project: String::new(),
            phase: "54".to_string(),
            source,
            kind: QuestionKind::Decision,
            question: question.to_string(),
            options: options(&[("a", "Thirty seconds")]),
            context: None,
            answer: own_words(id),
            recommended: None,
            order,
        }
    }

    fn picked(precedents: &[Precedent]) -> Vec<String> {
        precedents
            .iter()
            .map(|p| match &p.answer {
                PrecedentAnswer::OwnWords { text } => text.clone(),
                other => panic!("unexpected answer {other:?}"),
            })
            .collect()
    }

    /// The characters `precedents` take in the Jev request, measured from
    /// the request itself.
    fn wire_chars(spec: &QuestionSpec, precedents: &[Precedent]) -> usize {
        let request = crate::triage::jev_request(spec, precedents, "m");
        request["state"]["precedents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.to_string().chars().count())
            .sum()
    }

    #[test]
    fn only_operator_answers_are_labels() {
        let (_dir, root) = project();
        let mut events = Vec::new();
        for (index, by) in ["op", "agent:claude", "triage:jev", "unknown"]
            .iter()
            .enumerate()
        {
            let qid = format!("q{index}");
            let ts = 10 * index as u64;
            events.push(asked(&qid, json!(54), "decision", "Pick a budget", ts));
            events.push(answered(
                &qid,
                json!(54),
                json!({"option_id": "a"}),
                by,
                ts + 1,
            ));
        }
        write_events(&root, &lines(&events));

        let report = mine_project(&root, "p", &[]);
        assert_eq!(report.entries.len(), 1, "{:?}", report.entries);
        assert_eq!(report.entries[0].id, "events:q0");
        assert_eq!(report.excluded.non_operator, 3);
        assert_eq!(report.excluded.unanswered, 0);
    }

    #[test]
    fn an_excluded_phase_gives_no_labels() {
        let (_dir, root) = project();
        write_events(
            &root,
            &lines(&[
                asked("q1", json!("53.2"), "decision", "Pick a budget", 1),
                answered("q1", json!("53.2"), json!({"option_id": "a"}), "op", 2),
                asked("q2", json!("53.2"), "decision", "Pick a budget", 3),
                answered("q2", json!("53.2"), json!({"option_id": "b"}), "op", 4),
                asked("q3", json!(54), "decision", "Pick a budget", 5),
                answered("q3", json!(54), json!({"option_id": "b"}), "op", 6),
            ]),
        );
        let excluded: PhaseId = "53.2".parse().unwrap();

        let report = mine_project(&root, "p", &[excluded]);
        assert_eq!(report.entries.len(), 1, "{:?}", report.entries);
        assert_eq!(report.entries[0].phase, "54");
        assert_eq!(report.excluded.excluded_phase, 2);
    }

    #[test]
    fn an_operator_override_replaces_the_label() {
        let (_dir, root) = project();
        write_events(
            &root,
            &lines(&[
                asked("q1", json!(54), "decision", "Pick a budget", 1),
                answered("q1", json!(54), json!({"option_id": "a"}), "op", 2),
                overridden("q1", json!(54), json!({"option_id": "b"}), "op", 3),
            ]),
        );

        let report = mine_project(&root, "p", &[]);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].answer, option("b"));
    }

    #[test]
    fn a_triage_answer_overridden_by_the_operator_is_a_label() {
        let (_dir, root) = project();
        write_events(
            &root,
            &lines(&[
                asked("q1", json!(54), "decision", "Pick a budget", 1),
                answered("q1", json!(54), json!({"option_id": "a"}), "triage:jev", 2),
                overridden("q1", json!(54), json!({"option_id": "b"}), "op", 3),
                asked("q2", json!(54), "decision", "Pick a budget", 4),
                answered("q2", json!(54), json!({"option_id": "a"}), "triage:jev", 5),
                overridden("q2", json!(54), json!({"option_id": "b"}), "agent:pi", 6),
            ]),
        );

        let report = mine_project(&root, "p", &[]);
        assert_eq!(report.entries.len(), 1, "{:?}", report.entries);
        assert_eq!(report.entries[0].id, "events:q1");
        assert_eq!(report.entries[0].answer, option("b"));
        assert_eq!(report.excluded.non_operator, 1);
    }

    #[test]
    fn a_text_answer_is_kept_as_own_words() {
        let (_dir, root) = project();
        write_events(
            &root,
            &lines(&[
                asked("q1", json!(54), "discuss", "Pick a budget", 1),
                answered(
                    "q1",
                    json!(54),
                    json!({"text": "neither, use ninety"}),
                    "op",
                    2,
                ),
            ]),
        );

        let report = mine_project(&root, "p", &[]);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].answer, own_words("neither, use ninety"));
        assert_eq!(report.entries[0].kind, QuestionKind::Discuss);
    }

    #[test]
    fn precedent_ties_break_by_source_then_recency_then_id() {
        let question = "Pick a retry budget";
        let entries = vec![
            entry("log:x.md:1", CorpusSource::DiscussionLog, 100, question),
            entry("events:b", CorpusSource::Events, 5, question),
            entry("events:a", CorpusSource::Events, 5, question),
            entry("events:c", CorpusSource::Events, 9, question),
        ];
        let target = spec(question, &[("a", "Thirty seconds")]);

        let first = picked(&select_precedents(&entries, &target, None));
        assert_eq!(first, ["events:c", "events:a", "events:b", "log:x.md:1"]);
        assert_eq!(picked(&select_precedents(&entries, &target, None)), first);
    }

    #[test]
    fn precedents_stay_within_the_budget() {
        let long_question = format!("Pick a retry budget {}", "x".repeat(4_000));
        let entries: Vec<CorpusEntry> = (0..10)
            .map(|index| {
                entry(
                    &format!("events:{index}"),
                    CorpusSource::Events,
                    index,
                    &long_question,
                )
            })
            .collect();
        let target = spec(&long_question, &[("a", "Thirty seconds")]);

        let kept = select_precedents(&entries, &target, None);
        assert!(kept.len() <= MAX_PRECEDENTS, "{}", kept.len());
        assert!(wire_chars(&target, &kept) <= PRECEDENT_CHAR_BUDGET);
        let expected: Vec<String> = (0..10).rev().map(|i| format!("events:{i}")).collect();
        assert_eq!(picked(&kept), expected[..kept.len()]);
        let mut one_more = kept.clone();
        one_more.push(Precedent {
            question: long_question.clone(),
            options: options(&[("a", "Thirty seconds")]),
            answer: own_words(&expected[kept.len()]),
        });
        assert!(wire_chars(&target, &one_more) > PRECEDENT_CHAR_BUDGET);

        let short: Vec<CorpusEntry> = (0..10)
            .map(|index| {
                entry(
                    &format!("events:{index}"),
                    CorpusSource::Events,
                    index,
                    "Pick a retry budget",
                )
            })
            .collect();
        let kept = select_precedents(&short, &target, None);
        assert_eq!(picked(&kept), expected[..MAX_PRECEDENTS]);
    }

    #[test]
    fn run_time_precedents_come_from_this_project_only() {
        let parent = TempDir::new().unwrap();
        let this = parent.path().join("this");
        let sibling = parent.path().join("sibling");
        std::fs::create_dir_all(&this).unwrap();
        write_events(
            &sibling,
            &lines(&[
                asked("q1", json!(54), "decision", "Pick a budget", 1),
                answered("q1", json!(54), json!({"option_id": "a"}), "op", 2),
            ]),
        );
        let target = spec("Pick a budget", &[("a", "Thirty seconds")]);
        let config = TriageConfig::default();

        assert_eq!(precedents_for(&sibling, &config, &target).len(), 1);
        assert!(precedents_for(&this, &config, &target).is_empty());
    }

    #[test]
    fn an_empty_project_gives_no_precedents() {
        let (_dir, root) = project();
        let target = spec("Pick a budget", &[("a", "Thirty seconds")]);

        assert!(precedents_for(&root, &TriageConfig::default(), &target).is_empty());
        assert_eq!(mine_project(&root, "p", &[]), MiningReport::default());
    }

    #[test]
    fn a_torn_event_line_is_skipped_and_counted() {
        let (_dir, root) = project();
        let mut text = lines(&[
            asked("q1", json!(54), "decision", "Pick a budget", 1),
            answered("q1", json!(54), json!({"option_id": "a"}), "op", 2),
        ]);
        text.push("not json".to_string());
        text.extend(lines(&[
            asked("q2", json!(54), "decision", "Pick a budget", 3),
            answered("q2", json!(54), json!({"option_id": "b"}), "op", 4),
        ]));
        text.push(r#"{"v":1,"ts":5,"phase":54,"event":"question_ask"#.to_string());
        write_events(&root, &text);

        let report = mine_project(&root, "p", &[]);
        let ids: Vec<&str> = report.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["events:q1", "events:q2"]);
        assert_eq!(report.excluded.unreadable_event_lines, 2);
    }

    const HEADER: &str =
        "| Option | Description | Selected |\n|--------|-------------|----------|\n";

    fn write_log(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn log_entries(report: &MiningReport) -> Vec<&CorpusEntry> {
        report
            .entries
            .iter()
            .filter(|entry| entry.source == CorpusSource::DiscussionLog)
            .collect()
    }

    /// One `## <area>` question whose table has `rows`, then `after`.
    fn area(name: &str, rows: &[(&str, &str)], after: &str) -> String {
        let mut text = format!("## {name}\n\n{HEADER}");
        for (label, selected) in rows {
            text.push_str(&format!("| {label} | what it means | {selected} |\n"));
        }
        text.push('\n');
        text.push_str(after);
        text.push('\n');
        text
    }

    fn log_file(body: &[String]) -> String {
        format!(
            "# Phase 60: X - Discussion Log\n\n**Date:** 2026-01-01\n\n---\n\n{}",
            body.concat()
        )
    }

    #[test]
    fn auto_picks_are_not_labels() {
        let (_dir, root) = project();
        let auto = format!(
            "# Phase 60: X - Discussion Log\n\n**Mode:** `--auto`\n\n{}{}",
            area("First", &[("Fast", "✓"), ("Slow", "")], ""),
            area("Second", &[("Fast", ""), ("Slow", "✓")], ""),
        );
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &auto);
        let mixed = log_file(&[
            area(
                "First",
                &[("Fast", "✓"), ("Slow", "")],
                "**Choice:** [auto] Fast",
            ),
            area("Second", &[("Fast", ""), ("Slow", "✓")], ""),
            area("Third", &[("Fast", "✓"), ("Slow", "")], ""),
        ]);
        write_log(&root, ".planning/phases/61-y/61-DISCUSSION-LOG.md", &mixed);

        let report = mine_project(&root, "p", &[]);
        let questions: Vec<&str> = log_entries(&report)
            .iter()
            .map(|e| e.question.as_str())
            .collect();
        assert_eq!(questions, ["Second", "Third"]);
        assert!(log_entries(&report).iter().all(|e| e.phase == "61"));
        assert_eq!(report.excluded.auto, 3);
    }

    #[test]
    fn a_delegation_pick_is_not_a_label() {
        let (_dir, root) = project();
        let text = log_file(&[
            area("Delegated", &[("Keep it", ""), ("You decide", "✓")], ""),
            area("Chosen", &[("Keep it", "✓"), ("You decide", "")], ""),
        ]);
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        let questions: Vec<&str> = log_entries(&report)
            .iter()
            .map(|e| e.question.as_str())
            .collect();
        assert_eq!(questions, ["Chosen"]);
        assert_eq!(report.excluded.delegation, 1);
    }

    #[test]
    fn a_multi_select_question_is_excluded_and_counted() {
        let (_dir, root) = project();
        let text = log_file(&[area(
            "Both",
            &[("Red", "✓"), ("Blue", "✓"), ("Green", "")],
            "",
        )]);
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        assert!(log_entries(&report).is_empty());
        assert_eq!(report.excluded.multi_select, 1);
    }

    #[test]
    fn a_phase_whose_discuss_ran_through_the_channel_skips_its_log() {
        let (_dir, root) = project();
        write_events(
            &root,
            &lines(&[
                asked("q54", json!(54), "discuss", "Pick a budget", 1),
                answered("q54", json!(54), json!({"option_id": "a"}), "op", 2),
                asked("q53", json!(53), "discuss", "Pick a budget", 3),
                asked("q52", json!(52), "decision", "Pick a budget", 4),
                answered("q52", json!(52), json!({"option_id": "a"}), "op", 5),
            ]),
        );
        for phase in ["52", "53", "54"] {
            let text = log_file(&[area("Budget", &[("Thirty", "✓"), ("Sixty", "")], "")]);
            write_log(
                &root,
                &format!(".planning/phases/{phase}-x/{phase}-DISCUSSION-LOG.md"),
                &text,
            );
        }

        let report = mine_project(&root, "p", &[]);
        let phases: Vec<&str> = log_entries(&report)
            .iter()
            .map(|e| e.phase.as_str())
            .collect();
        assert_eq!(phases, ["52", "53"]);
        assert_eq!(report.excluded.duplicate_log, 1);
    }

    #[test]
    fn tables_under_an_area_heading_are_separate_questions() {
        let (_dir, root) = project();
        let text = format!(
            "## Area\n\n{HEADER}| One | first | ✓ |\n| Two | second | |\n\n**User's choice:** One\n\n\
             {HEADER}| Red | | |\n| Blue | | ✓ |\n\n**User's choice:** Blue\n"
        );
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        let entries = log_entries(&report);
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert!(
            entries
                .iter()
                .all(|e| e.question == "Area" && e.context.is_none())
        );
        assert_ne!(entries[0].id, entries[1].id);
        assert_eq!(entries[0].answer, option("a"));
        assert_eq!(entries[1].answer, option("b"));
        assert_eq!(entries[1].options[1].label, "Blue");
    }

    #[test]
    fn a_question_heading_names_the_question_and_its_area_is_context() {
        let (_dir, root) = project();
        let text =
            format!("## Area\n\n### Q1 — Which colour?\n\n{HEADER}| Red | | ✓ |\n| Blue | | |\n");
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        let entries = log_entries(&report);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].question, "Which colour?");
        assert_eq!(entries[0].context.as_deref(), Some("Area"));
        assert_eq!(entries[0].kind, QuestionKind::Discuss);
    }

    #[test]
    fn a_multiline_free_text_choice_is_own_words() {
        let (_dir, root) = project();
        let text = log_file(&[
            area(
                "Free",
                &[("A", ""), ("B", "")],
                "**User's choice (free text):** \"first line\nsecond line\"\n\n**Notes:** asked a follow-up",
            ),
            area(
                "Verbatim",
                &[("A", ""), ("B", "")],
                "**User's choice (verbatim intent):** keep\nboth",
            ),
            area(
                "Text",
                &[("A", ""), ("B", "")],
                "**User's choice (text):** neither\nof them",
            ),
        ]);
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        let answers: Vec<&PrecedentAnswer> =
            log_entries(&report).iter().map(|e| &e.answer).collect();
        assert_eq!(
            answers,
            [
                &own_words("\"first line second line\""),
                &own_words("keep both"),
                &own_words("neither of them")
            ]
        );
    }

    #[test]
    fn archived_milestone_logs_are_mined() {
        let (_dir, root) = project();
        let relative = ".planning/milestones/v2.0.0-phases/16-x/16-DISCUSSION-LOG.md";
        write_log(
            &root,
            relative,
            &log_file(&[area("Budget", &[("Thirty", "✓")], "")]),
        );

        let report = mine_project(&root, "p", &[]);
        let entries = log_entries(&report);
        assert_eq!(entries.len(), 1, "{:?}", report.excluded);
        assert_eq!(entries[0].phase, "16");
        assert!(
            entries[0].id.starts_with(&format!("log:{relative}:")),
            "{}",
            entries[0].id
        );
    }

    #[test]
    fn a_log_prefix_that_is_not_a_phase_id_is_still_mined() {
        let (_dir, root) = project();
        let relative = ".planning/milestones/v1.0-phases/11-r/11r-DISCUSSION-LOG.md";
        write_log(
            &root,
            relative,
            &log_file(&[area("Budget", &[("Thirty", "✓")], "")]),
        );

        let report = mine_project(&root, "p", &[]);
        let phases: Vec<&str> = log_entries(&report)
            .iter()
            .map(|e| e.phase.as_str())
            .collect();
        assert_eq!(phases, ["11r"]);
    }

    #[test]
    fn a_users_choice_that_matches_no_option_is_own_words() {
        let (_dir, root) = project();
        let text = log_file(&[
            area(
                "Other",
                &[("Keep it", ""), ("Drop it", "")],
                "**User's choice:** something else entirely → **D-03**",
            ),
            area(
                "Same",
                &[("Drop it", ""), ("Keep it", "")],
                "**User's choice:**   keep   IT ",
            ),
        ]);
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        let answers: Vec<&PrecedentAnswer> =
            log_entries(&report).iter().map(|e| &e.answer).collect();
        assert_eq!(
            answers,
            [&own_words("something else entirely"), &option("b")]
        );
    }

    #[test]
    fn a_log_recommended_marker_is_stripped_and_kept_for_the_baseline() {
        let (_dir, root) = project();
        let text = log_file(&[area(
            "Keep",
            &[("Drop it", ""), ("Keep it (Recommended)", "✓")],
            "",
        )]);
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        let entries = log_entries(&report);
        assert_eq!(entries.len(), 1, "{:?}", report.excluded);
        assert_eq!(entries[0].options[1].label, "Keep it");
        assert_eq!(entries[0].recommended.as_deref(), Some("b"));
        assert_eq!(entries[0].answer, option("b"));
    }

    #[test]
    fn an_unrecognised_log_block_is_counted_not_dropped() {
        let (_dir, root) = project();
        let text = log_file(&[
            area(
                "Unpicked",
                &[("Keep it", ""), ("Drop it", "")],
                "**Notes:** left open",
            ),
            "### A heading with no table\n\nProse only.\n".to_string(),
        ]);
        write_log(&root, ".planning/phases/60-x/60-DISCUSSION-LOG.md", &text);

        let report = mine_project(&root, "p", &[]);
        assert!(report.entries.is_empty(), "{:?}", report.entries);
        let expected = ExclusionCounts {
            unparsed: 1,
            ..ExclusionCounts::default()
        };
        assert_eq!(report.excluded, expected);
    }
}
