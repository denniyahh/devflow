//! Decision triage (TRIAGE-01): whether a question an agent asks can be
//! answered from how the operator answered earlier ones.
//!
//! One Jev request per question carries two questions (D-24): `accept`, a
//! noul on whether the operator would take an offered option as offered, and
//! `pick`, a choice over the offered option ids. A question is decided only
//! when `accept` reaches the pre-check threshold AND the probability of the
//! named choice reaches the option threshold; both comparisons are `>=`.
//! Everything else escalates to the operator.
//!
//! This module is pure: it builds the request, parses the response and
//! applies the rule. The HTTP call and the event writes live with the caller.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::phase_id::PhaseId;
use crate::question::{QuestionKind, QuestionOption, QuestionSpec};

/// The `answered_by` value of a question triage decided (D-12).
pub const TRIAGE_ANSWERER: &str = "triage:jev";

/// The Jev model asked when `[triage] model` is absent. A versioned id, not
/// the moving `jev-latest` alias, so a threshold measured against one model
/// is applied to that model.
pub const DEFAULT_MODEL: &str = "jev-1.14.0";

/// The request timeout when `[triage] timeout_secs` is absent (the SDK's
/// per-operation default).
pub const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// The option threshold when `[triage] threshold` is absent: the value the
/// Phase 54 measurement chose (54-BAKEOFF.md, D-07).
pub const DEFAULT_THRESHOLD: Option<f64> = Some(0.92);

/// The pre-check threshold when `[triage] pre_check_threshold` is absent:
/// the value the Phase 54 measurement chose (54-BAKEOFF.md, D-07).
pub const DEFAULT_PRE_CHECK_THRESHOLD: Option<f64> = Some(0.37);

/// The instructions of the `accept` question.
const ACCEPT_INSTRUCTIONS: &str = "Would the operator accept one of the offered `options` as \
     offered, rather than answering `question` in their own words? Use `precedents` as evidence \
     of how this operator answers.";

/// The instructions of the `pick` question.
const PICK_INSTRUCTIONS: &str = "Which offered option would this operator choose for \
     `question`, given `context` and how they answered `precedents`?";

/// How far the offered options' probabilities may sum from 1 and still be a
/// complete distribution: per-value rounding over up to
/// [`crate::question::MAX_OPTIONS`] values.
pub const PROBABILITY_SUM_TOLERANCE: f64 = 0.01;

/// The longest response-supplied value a fallback detail repeats.
const DETAIL_VALUE_MAX_CHARS: usize = 64;

/// How triage is configured for a project (`devflow.toml` `[triage]`).
///
/// A threshold of `None` means escalate every question.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TriageConfig {
    /// Whether triage runs at all.
    pub enabled: bool,
    /// The least `probabilities[choice]` that decides.
    pub threshold: Option<f64>,
    /// The least `accept` score that lets the choice be considered.
    pub pre_check_threshold: Option<f64>,
    /// The Jev model id sent and expected back.
    pub model: String,
    /// How long one Jev request may take.
    pub timeout: Duration,
    /// Phases whose recorded answers are not used as precedents (D-23).
    pub exclude_label_phases: Vec<PhaseId>,
}

impl Default for TriageConfig {
    /// Triage off, every value at its default (D-09).
    fn default() -> Self {
        TriageConfig {
            enabled: false,
            ..TriageConfig::with_thresholds(DEFAULT_THRESHOLD, DEFAULT_PRE_CHECK_THRESHOLD)
        }
    }
}

impl TriageConfig {
    /// An enabled configuration with the given thresholds and every other
    /// value at its default.
    pub fn with_thresholds(threshold: Option<f64>, pre_check_threshold: Option<f64>) -> Self {
        TriageConfig {
            enabled: true,
            threshold,
            pre_check_threshold,
            model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            exclude_label_phases: Vec::new(),
        }
    }
}

/// An earlier question and how the operator answered it, sent to Jev as
/// evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct Precedent {
    /// The earlier question.
    pub question: String,
    /// The options it offered.
    pub options: Vec<QuestionOption>,
    /// How the operator answered it.
    pub answer: PrecedentAnswer,
}

/// How the operator answered a precedent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PrecedentAnswer {
    /// The operator chose an offered option.
    Option { option_id: String },
    /// The operator answered in their own words.
    OwnWords { text: String },
}

/// The scores one Jev response gave a question.
#[derive(Debug, Clone, PartialEq)]
pub struct JevScores {
    /// The `accept` noul: the probability the operator takes an offered option.
    pub accept: f64,
    /// The option Jev named in `pick.choice`.
    pub top_option: String,
    /// `pick.probabilities[choice]`.
    pub top_probability: f64,
    /// The model that answered.
    pub model: String,
}

/// Why a question went to the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EscalateReason {
    /// A `blocking-human` question is never scored (D-08).
    BlockingHuman,
    /// A threshold is unset, so every question escalates (D-07).
    NoThreshold,
    /// `accept` is below the pre-check threshold.
    PreCheckBelowThreshold,
    /// `probabilities[choice]` is below the option threshold.
    TopProbabilityBelowThreshold,
}

impl EscalateReason {
    /// The stable name recorded in events.
    pub fn as_str(self) -> &'static str {
        match self {
            EscalateReason::BlockingHuman => "blocking_human",
            EscalateReason::NoThreshold => "no_threshold",
            EscalateReason::PreCheckBelowThreshold => "pre_check_below_threshold",
            EscalateReason::TopProbabilityBelowThreshold => "top_probability_below_threshold",
        }
    }
}

/// Why triage could not score a question; the question goes to the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FallbackReason {
    /// No API key is configured.
    NoApiKey,
    /// The configured endpoint is not an allowed host.
    EndpointRefused,
    /// The endpoint could not be reached.
    Unreachable,
    /// The request took longer than the configured timeout.
    Timeout,
    /// The endpoint answered with a non-success status.
    HttpStatus,
    /// The response is not a complete, well-formed answer.
    Unparsable,
    /// The response names a model other than the configured one.
    ModelMismatch,
    /// The ask's time budget was spent before triage could answer.
    AskBudgetSpent,
    /// The decision could not be recorded in the event log.
    LedgerUnwritable,
}

impl FallbackReason {
    /// The stable name recorded in events.
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackReason::NoApiKey => "no_api_key",
            FallbackReason::EndpointRefused => "endpoint_refused",
            FallbackReason::Unreachable => "unreachable",
            FallbackReason::Timeout => "timeout",
            FallbackReason::HttpStatus => "http_status",
            FallbackReason::Unparsable => "unparsable",
            FallbackReason::ModelMismatch => "model_mismatch",
            FallbackReason::AskBudgetSpent => "ask_budget_spent",
            FallbackReason::LedgerUnwritable => "ledger_unwritable",
        }
    }
}

/// What triage did with a question.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TriageOutcome {
    /// Triage answered the question with `option_id`.
    Decided {
        option_id: String,
        scores: JevScores,
    },
    /// The question goes to the operator; `scores` is `None` when it was
    /// never scored.
    Escalated {
        reason: EscalateReason,
        scores: Option<JevScores>,
    },
    /// Triage could not score the question; it goes to the operator.
    Fallback {
        reason: FallbackReason,
        detail: String,
    },
}

impl TriageOutcome {
    /// The `triage` object recorded on `question_asked`.
    pub fn event_fields(&self, config: &TriageConfig) -> Value {
        let (outcome, reason, scores) = match self {
            TriageOutcome::Decided { scores, .. } => ("decided", None, Some(scores)),
            TriageOutcome::Escalated { reason, scores } => {
                ("escalated", Some(reason.as_str()), scores.as_ref())
            }
            TriageOutcome::Fallback { reason, .. } => ("fallback", Some(reason.as_str()), None),
        };
        let mut fields = Map::new();
        fields.insert("outcome".into(), json!(outcome));
        if let Some(reason) = reason {
            fields.insert("reason".into(), json!(reason));
        }
        if let Some(scores) = scores {
            fields.insert("pre_check".into(), json!(scores.accept));
            fields.insert("top_option".into(), json!(scores.top_option));
            fields.insert("top_probability".into(), json!(scores.top_probability));
        }
        fields.insert("threshold".into(), json!(config.threshold));
        fields.insert(
            "pre_check_threshold".into(),
            json!(config.pre_check_threshold),
        );
        let model = scores.map_or(config.model.as_str(), |scores| scores.model.as_str());
        fields.insert("model".into(), json!(model));
        Value::Object(fields)
    }
}

/// The Jev request that scores `spec` against `precedents` (D-24). The
/// recommended option is never sent (D-10).
pub fn jev_request(spec: &QuestionSpec, precedents: &[Precedent], model: &str) -> Value {
    let mut state = Map::new();
    state.insert("question".into(), json!(spec.question));
    if let Some(context) = &spec.context {
        state.insert("context".into(), json!(context));
    }
    state.insert("options".into(), options_state(&spec.options));
    let precedents: Vec<Value> = precedents.iter().map(precedent_state).collect();
    state.insert("precedents".into(), Value::Array(precedents));
    let criteria: Map<String, Value> = spec
        .options
        .iter()
        .map(|option| (option.id.clone(), json!(criterion(option))))
        .collect();
    json!({
        "model": model,
        "state": Value::Object(state),
        "questions": {
            "accept": {"type": "noul", "instructions": ACCEPT_INSTRUCTIONS},
            "pick": {"type": "choice", "instructions": PICK_INSTRUCTIONS, "criteria": criteria},
        },
    })
}

/// The options as Jev's `state` carries them.
fn options_state(options: &[QuestionOption]) -> Value {
    let options = options
        .iter()
        .map(|option| {
            let mut entry = Map::new();
            entry.insert("id".into(), json!(option.id));
            entry.insert(
                "label".into(),
                json!(strip_recommended_marker(&option.label).0),
            );
            if let Some(description) = &option.description {
                let description = strip_recommended_marker(description).0;
                entry.insert("description".into(), json!(description));
            }
            Value::Object(entry)
        })
        .collect();
    Value::Array(options)
}

/// One precedent as Jev's `state` carries it.
pub(crate) fn precedent_state(precedent: &Precedent) -> Value {
    let answer = match &precedent.answer {
        PrecedentAnswer::Option { option_id } => json!({"option": option_id}),
        PrecedentAnswer::OwnWords { text } => json!({"own_words": text}),
    };
    json!({
        "question": precedent.question,
        "options": options_state(&precedent.options),
        "operator_answer": answer,
    })
}

/// `text` without a recommendation marker, and whether one was removed (D-10).
///
/// Removed, case-insensitively: every parenthesized or bracketed group whose
/// first word is `recommended`, whatever follows inside it, and a trailing
/// `— recommended…` or `- recommended…`. The word inside ordinary prose, and
/// a group whose first word is another word, are kept.
pub fn strip_recommended_marker(text: &str) -> (String, bool) {
    let mut kept = String::with_capacity(text.len());
    let mut removed = false;
    let mut rest = text;
    while let Some(open) = rest.find(['(', '[']) {
        let closer = if rest[open..].starts_with('(') {
            ')'
        } else {
            ']'
        };
        let inside = &rest[open + 1..];
        match inside.find(closer) {
            Some(close) if starts_with_recommended(&inside[..close]) => {
                kept.push_str(&rest[..open]);
                rest = &inside[close + 1..];
                removed = true;
            }
            _ => {
                kept.push_str(&rest[..=open]);
                rest = inside;
            }
        }
    }
    kept.push_str(rest);
    if let Some(dash) = trailing_recommended_dash(&kept) {
        kept.truncate(dash);
        removed = true;
    }
    if !removed {
        return (text.to_string(), false);
    }
    (kept.split_whitespace().collect::<Vec<_>>().join(" "), true)
}

/// Whether the first word of `text` is `recommended`, in any case.
fn starts_with_recommended(text: &str) -> bool {
    text.trim_start()
        .split(|ch: char| !ch.is_alphanumeric())
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("recommended"))
}

/// Where a trailing `— recommended…` (em dash, en dash or hyphen, after
/// whitespace) starts in `text`.
fn trailing_recommended_dash(text: &str) -> Option<usize> {
    text.char_indices().find_map(|(index, ch)| {
        let dash = matches!(ch, '—' | '–' | '-');
        let after_space = text[..index].ends_with(char::is_whitespace);
        (dash && after_space && starts_with_recommended(&text[index + ch.len_utf8()..]))
            .then_some(index)
    })
}

/// An option's `pick` criterion: `label — description`, or the label alone.
fn criterion(option: &QuestionOption) -> String {
    let (label, _) = strip_recommended_marker(&option.label);
    match &option.description {
        Some(description) => format!("{label} — {}", strip_recommended_marker(description).0),
        None => label,
    }
}

/// The scores a Jev response gives `spec`. A missing or malformed field is a
/// fallback, never a default score.
pub fn parse_jev_response(
    body: &Value,
    spec: &QuestionSpec,
    expected_model: &str,
) -> Result<JevScores, (FallbackReason, String)> {
    let model = text_at(body, &["model"])?;
    if model != expected_model {
        return Err((
            FallbackReason::ModelMismatch,
            format!("the response's `model` is not the configured {expected_model}"),
        ));
    }
    expect_type(body, "accept", "noul")?;
    expect_type(body, "pick", "choice")?;
    let accept = probability_at(body, &["answers", "accept", "noul"])?;
    let choice = text_at(body, &["answers", "pick", "choice"])?;
    if !spec.options.iter().any(|option| option.id == choice) {
        return Err(unparsable(
            &["answers", "pick", "choice"],
            &format!("`{}` is not an offered option", bounded(choice)),
        ));
    }
    let distribution = complete_distribution(body, spec)?;
    let top_probability = distribution
        .iter()
        .find_map(|(id, probability)| (*id == choice).then_some(*probability))
        .ok_or_else(|| unparsable(&["answers", "pick", "probabilities"], "lacks the choice"))?;
    Ok(JevScores {
        accept,
        top_option: choice.to_string(),
        top_probability,
        model: model.to_string(),
    })
}

/// Check that `answers.<question>.type` is `expected`.
fn expect_type(
    body: &Value,
    question: &str,
    expected: &str,
) -> Result<(), (FallbackReason, String)> {
    let path = ["answers", question, "type"];
    let found = text_at(body, &path)?;
    if found == expected {
        return Ok(());
    }
    Err(unparsable(
        &path,
        &format!("is `{}`, expected `{expected}`", bounded(found)),
    ))
}

/// `answers.pick.probabilities` as (option id, probability) pairs, checked to
/// hold exactly the offered ids, each in [0, 1], summing to 1 within
/// [`PROBABILITY_SUM_TOLERANCE`].
fn complete_distribution<'a>(
    body: &Value,
    spec: &'a QuestionSpec,
) -> Result<Vec<(&'a str, f64)>, (FallbackReason, String)> {
    let path = ["answers", "pick", "probabilities"];
    let probabilities = value_at(body, &path)?
        .as_object()
        .ok_or_else(|| unparsable(&path, "is not an object"))?;
    if let Some(extra) = probabilities
        .keys()
        .find(|key| !spec.options.iter().any(|option| &option.id == *key))
    {
        let problem = format!("has `{}`, which was not offered", bounded(extra));
        return Err(unparsable(&path, &problem));
    }
    let mut distribution = Vec::with_capacity(spec.options.len());
    for option in &spec.options {
        if !probabilities.contains_key(&option.id) {
            let problem = format!("has no `{}`", bounded(&option.id));
            return Err(unparsable(&path, &problem));
        }
        let probability = probability_at(body, &["answers", "pick", "probabilities", &option.id])?;
        distribution.push((option.id.as_str(), probability));
    }
    let sum: f64 = distribution
        .iter()
        .map(|(_, probability)| probability)
        .sum();
    if (sum - 1.0).abs() > PROBABILITY_SUM_TOLERANCE {
        return Err(unparsable(&path, &format!("sums to {sum}, not 1")));
    }
    Ok(distribution)
}

/// The number at `path`, checked to be a probability in [0, 1].
fn probability_at(body: &Value, path: &[&str]) -> Result<f64, (FallbackReason, String)> {
    let number = number_at(body, path)?;
    if number.is_finite() && (0.0..=1.0).contains(&number) {
        return Ok(number);
    }
    Err(unparsable(path, &format!("is {number}, not in [0, 1]")))
}

/// The value at `path`, or an `Unparsable` naming the missing field.
fn value_at<'a>(body: &'a Value, path: &[&str]) -> Result<&'a Value, (FallbackReason, String)> {
    path.iter().try_fold(body, |value, key| {
        value.get(key).ok_or_else(|| unparsable(path, "is missing"))
    })
}

/// The string at `path`.
fn text_at<'a>(body: &'a Value, path: &[&str]) -> Result<&'a str, (FallbackReason, String)> {
    value_at(body, path)?
        .as_str()
        .ok_or_else(|| unparsable(path, "is not a string"))
}

/// The number at `path`.
fn number_at(body: &Value, path: &[&str]) -> Result<f64, (FallbackReason, String)> {
    value_at(body, path)?
        .as_f64()
        .ok_or_else(|| unparsable(path, "is not a number"))
}

/// An `Unparsable` fallback naming the field at `path`.
fn unparsable(path: &[&str], problem: &str) -> (FallbackReason, String) {
    let path: Vec<String> = path.iter().map(|key| bounded(key)).collect();
    (
        FallbackReason::Unparsable,
        format!("`{}` {problem}", path.join(".")),
    )
}

/// A response-supplied value cut to [`DETAIL_VALUE_MAX_CHARS`].
fn bounded(value: &str) -> String {
    value.chars().take(DETAIL_VALUE_MAX_CHARS).collect()
}

/// Decide `scores` by D-24: decided only when `accept >= pre_check_threshold`
/// and `top_probability >= threshold`.
pub fn decide(scores: &JevScores, config: &TriageConfig) -> TriageOutcome {
    let escalate = |reason| TriageOutcome::Escalated {
        reason,
        scores: Some(scores.clone()),
    };
    let (Some(threshold), Some(pre_check_threshold)) =
        (config.threshold, config.pre_check_threshold)
    else {
        return escalate(EscalateReason::NoThreshold);
    };
    if !reaches(scores.accept, pre_check_threshold) {
        return escalate(EscalateReason::PreCheckBelowThreshold);
    }
    if !reaches(scores.top_probability, threshold) {
        return escalate(EscalateReason::TopProbabilityBelowThreshold);
    }
    TriageOutcome::Decided {
        option_id: scores.top_option.clone(),
        scores: scores.clone(),
    }
}

/// The outcome of a question that is not sent to Jev at all: a
/// `blocking-human` question (D-08), then any question while a threshold is
/// unset (D-07). `None` means the question is to be scored.
pub fn pre_decide(kind: QuestionKind, config: &TriageConfig) -> Option<TriageOutcome> {
    let skip = |reason| {
        Some(TriageOutcome::Escalated {
            reason,
            scores: None,
        })
    };
    if kind == QuestionKind::BlockingHuman {
        return skip(EscalateReason::BlockingHuman);
    }
    if config.threshold.is_none() || config.pre_check_threshold.is_none() {
        return skip(EscalateReason::NoThreshold);
    }
    None
}

/// Whether `score` reaches `threshold` (D-24's `>=`). A NaN never reaches.
pub(crate) fn reaches(score: f64, threshold: f64) -> bool {
    score >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "jev-1.14.0";

    fn option(id: &str, label: &str, description: Option<&str>) -> QuestionOption {
        QuestionOption {
            id: id.to_string(),
            label: label.to_string(),
            description: description.map(str::to_string),
        }
    }

    fn spec(options: Vec<QuestionOption>, recommended: Option<&str>) -> QuestionSpec {
        QuestionSpec {
            question_id: "q-1".to_string(),
            question: "Which layout?".to_string(),
            options,
            context: Some("planning".to_string()),
            kind: QuestionKind::Decision,
            recommended: recommended.map(str::to_string),
        }
    }

    fn two_options() -> QuestionSpec {
        spec(
            vec![
                option("a", "Flat", Some("one module")),
                option("b", "Nested", Some("one module per stage")),
            ],
            Some("b"),
        )
    }

    fn response(model: &str, accept: f64, choice: &str, probabilities: Value) -> Value {
        json!({
            "model": model,
            "answers": {
                "accept": {"type": "noul", "noul": accept},
                "pick": {
                    "type": "choice",
                    "choice": choice,
                    "confidence": 0.5,
                    "probabilities": probabilities,
                },
            },
            "usage": {"input_tokens": 10, "output_tokens": 0},
        })
    }

    #[test]
    fn a_scored_question_above_both_thresholds_is_decided() {
        let spec = two_options();
        let request = jev_request(&spec, &[], MODEL);
        assert_eq!(request["model"], MODEL);
        assert_eq!(request["questions"]["accept"]["type"], "noul");
        assert_eq!(request["questions"]["pick"]["type"], "choice");
        let criteria = request["questions"]["pick"]["criteria"]
            .as_object()
            .expect("criteria is an object");
        let keys: Vec<&str> = criteria.keys().map(String::as_str).collect();
        assert_eq!(keys, ["a", "b"]);

        let body = response(MODEL, 0.93, "a", json!({"a": 0.9, "b": 0.1}));
        let scores = parse_jev_response(&body, &spec, MODEL).expect("the fixture parses");
        assert!(matches!(
            &scores,
            JevScores { accept, top_option, top_probability, .. }
                if *accept == 0.93 && top_option == "a" && *top_probability == 0.9
        ));
        let config = TriageConfig::with_thresholds(Some(0.5), Some(0.5));
        match decide(&scores, &config) {
            TriageOutcome::Decided { option_id, .. } => assert_eq!(option_id, "a"),
            other => panic!("expected a decision, got {other:?}"),
        }
    }

    fn scores(accept: f64, top_probability: f64) -> JevScores {
        JevScores {
            accept,
            top_option: "a".to_string(),
            top_probability,
            model: MODEL.to_string(),
        }
    }

    fn escalation(outcome: TriageOutcome) -> EscalateReason {
        match outcome {
            TriageOutcome::Escalated { reason, .. } => reason,
            other => panic!("expected an escalation, got {other:?}"),
        }
    }

    #[test]
    fn a_score_equal_to_the_threshold_decides() {
        let config = TriageConfig::with_thresholds(Some(0.8), Some(0.9));
        let outcome = decide(&scores(0.9, 0.8), &config);
        assert!(
            matches!(outcome, TriageOutcome::Decided { ref option_id, .. } if option_id == "a"),
            "a score equal to its threshold decides (D-24): {outcome:?}"
        );
    }

    #[test]
    fn a_score_one_step_below_the_threshold_escalates() {
        let config = TriageConfig::with_thresholds(Some(0.8), Some(0.9));
        let below_pre_check = f64::from_bits(0.9f64.to_bits() - 1);
        assert_eq!(
            escalation(decide(&scores(below_pre_check, 0.8), &config)),
            EscalateReason::PreCheckBelowThreshold
        );
        let below_threshold = f64::from_bits(0.8f64.to_bits() - 1);
        assert_eq!(
            escalation(decide(&scores(0.9, below_threshold), &config)),
            EscalateReason::TopProbabilityBelowThreshold
        );
    }

    #[test]
    fn a_threshold_above_any_probability_never_decides() {
        let above_one = f64::from_bits(1.0f64.to_bits() + 1);
        let config = TriageConfig::with_thresholds(Some(above_one), Some(above_one));
        assert_eq!(
            escalation(decide(&scores(1.0, 1.0), &config)),
            EscalateReason::PreCheckBelowThreshold
        );
    }

    #[test]
    fn a_threshold_of_zero_decides_any_score() {
        let config = TriageConfig::with_thresholds(Some(0.0), Some(0.0));
        let outcome = decide(&scores(0.0, 0.5), &config);
        assert!(
            matches!(outcome, TriageOutcome::Decided { .. }),
            "threshold 0.0 decides every score (D-22): {outcome:?}"
        );
    }

    #[test]
    fn a_blocking_human_question_is_never_scored() {
        let config = TriageConfig::with_thresholds(Some(0.0), Some(0.0));
        assert_eq!(
            pre_decide(QuestionKind::BlockingHuman, &config),
            Some(TriageOutcome::Escalated {
                reason: EscalateReason::BlockingHuman,
                scores: None,
            })
        );
        assert_eq!(pre_decide(QuestionKind::Discuss, &config), None);
        assert_eq!(pre_decide(QuestionKind::Decision, &config), None);
    }

    #[test]
    fn no_threshold_escalates_without_a_request() {
        let no_threshold = Some(TriageOutcome::Escalated {
            reason: EscalateReason::NoThreshold,
            scores: None,
        });
        let without_option_threshold = TriageConfig::with_thresholds(None, Some(0.5));
        assert_eq!(
            pre_decide(QuestionKind::Decision, &without_option_threshold),
            no_threshold
        );
        let without_pre_check = TriageConfig::with_thresholds(Some(0.5), None);
        assert_eq!(
            pre_decide(QuestionKind::Decision, &without_pre_check),
            no_threshold
        );
    }

    /// Every object key anywhere in `value`.
    fn keys_at_any_depth(value: &Value, keys: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    keys.push(key.clone());
                    keys_at_any_depth(child, keys);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| keys_at_any_depth(item, keys)),
            _ => {}
        }
    }

    #[test]
    fn jev_never_sees_the_recommended_option() {
        let spec = spec(
            vec![
                option("a", "Flat", Some("one module")),
                option("b", "Nested (Recommended)", Some("one module per stage")),
            ],
            Some("b"),
        );
        let request = jev_request(&spec, &[], MODEL);
        let mut keys = Vec::new();
        keys_at_any_depth(&request, &mut keys);
        assert!(
            !keys.iter().any(|key| key == "recommended"),
            "the request carries no `recommended` key: {request}"
        );
        let criteria = &request["questions"]["pick"]["criteria"];
        assert_eq!(criteria["a"], "Flat — one module");
        assert_eq!(criteria["b"], "Nested — one module per stage");
        assert_eq!(request["state"]["options"][1]["label"], "Nested");
    }

    #[test]
    fn a_recommended_marker_in_a_label_is_stripped() {
        for (label, stripped) in [
            ("Use X (Recommended)", "Use X"),
            ("Use Y [recommended]", "Use Y"),
            ("Use Z — recommended", "Use Z"),
            ("Use W (recommended option)", "Use W"),
            ("Use V (recommended default)", "Use V"),
            ("Use U (Recommended — keeps the old path)", "Use U"),
            ("Use T - recommended", "Use T"),
        ] {
            assert_eq!(
                strip_recommended_marker(label),
                (stripped.to_string(), true),
                "{label}"
            );
            let spec = spec(vec![option("a", label, None)], None);
            let request = jev_request(&spec, &[], MODEL);
            assert_eq!(request["questions"]["pick"]["criteria"]["a"], stripped);
        }
        for kept in [
            "The recommended Rust layout",
            "Keep it (not recommended for CI)",
        ] {
            assert_eq!(strip_recommended_marker(kept), (kept.to_string(), false));
        }
    }

    fn unparsable(body: &Value) -> String {
        match parse_jev_response(body, &two_options(), MODEL) {
            Err((FallbackReason::Unparsable, detail)) => detail,
            other => panic!("expected an unparsable response, got {other:?}"),
        }
    }

    #[test]
    fn a_response_missing_probabilities_is_unparsable() {
        let mut body = response(MODEL, 0.9, "a", json!({"a": 0.9, "b": 0.1}));
        body["answers"]["pick"]
            .as_object_mut()
            .unwrap()
            .remove("probabilities");
        unparsable(&body);
    }

    #[test]
    fn a_missing_sibling_probability_is_unparsable() {
        let detail = unparsable(&response(MODEL, 0.9, "a", json!({"a": 0.99})));
        assert!(
            detail.contains("`b`"),
            "the detail names the missing id: {detail}"
        );
    }

    #[test]
    fn a_wrong_answer_type_is_unparsable() {
        let mut pick_is_noul = response(MODEL, 0.9, "a", json!({"a": 0.9, "b": 0.1}));
        pick_is_noul["answers"]["pick"]["type"] = json!("noul");
        unparsable(&pick_is_noul);
        let mut accept_is_choice = response(MODEL, 0.9, "a", json!({"a": 0.9, "b": 0.1}));
        accept_is_choice["answers"]["accept"]["type"] = json!("choice");
        unparsable(&accept_is_choice);
    }

    #[test]
    fn a_distribution_that_does_not_sum_to_one_is_unparsable() {
        unparsable(&response(MODEL, 0.9, "a", json!({"a": 0.9, "b": 0.5})));
        let inside_tolerance = response(MODEL, 0.9, "a", json!({"a": 0.6, "b": 0.395}));
        parse_jev_response(&inside_tolerance, &two_options(), MODEL)
            .expect("a sum of 0.995 is inside the tolerance");
    }

    /// A value an endpoint might echo into a response field: a token and a
    /// terminal clear-screen sequence.
    const PLANTED: &str = "echoed\u{1b}[2J";

    /// Assert `detail` names `field` and repeats neither [`PLANTED`] nor an
    /// escape character.
    fn assert_names_only(detail: &str, field: &str) {
        assert!(
            detail.contains(field),
            "the detail names {field}: {detail:?}"
        );
        assert!(
            !detail.contains("echoed"),
            "the detail repeats the planted value: {detail:?}"
        );
        assert!(
            !detail.contains('\u{1b}'),
            "the detail carries the escape: {detail:?}"
        );
    }

    #[test]
    fn a_choice_that_was_not_offered_is_unparsable() {
        let detail = unparsable(&response(MODEL, 0.9, PLANTED, json!({"a": 0.5, "b": 0.5})));
        assert_names_only(&detail, "answers.pick.choice");
    }

    #[test]
    fn a_wrong_answer_type_names_the_field_not_the_value() {
        let mut body = response(MODEL, 0.9, "a", json!({"a": 0.9, "b": 0.1}));
        body["answers"]["pick"]["type"] = json!(PLANTED);
        let detail = unparsable(&body);
        assert_names_only(&detail, "answers.pick.type");
        assert!(
            detail.contains("`choice`"),
            "the detail names the expected type: {detail}"
        );
    }

    #[test]
    fn an_unoffered_probability_key_names_the_field_not_the_key() {
        let mut probabilities = json!({"a": 0.5, "b": 0.5});
        probabilities[PLANTED] = json!(0.0);
        let detail = unparsable(&response(MODEL, 0.9, "a", probabilities));
        assert_names_only(&detail, "answers.pick.probabilities");
    }

    #[test]
    fn an_out_of_range_probability_names_the_field_not_the_value() {
        let detail = unparsable(&response(MODEL, 1.5, "a", json!({"a": 0.9, "b": 0.1})));
        assert!(
            detail.contains("answers.accept.noul"),
            "the detail names the field: {detail}"
        );
        assert!(
            !detail.contains("1.5"),
            "the detail repeats the value: {detail}"
        );
    }

    #[test]
    fn a_distribution_off_one_names_the_field_not_the_sum() {
        let detail = unparsable(&response(MODEL, 0.9, "a", json!({"a": 0.9, "b": 0.5})));
        assert!(
            detail.contains("answers.pick.probabilities"),
            "the detail names the field: {detail}"
        );
        assert!(
            !detail.contains("1.4"),
            "the detail repeats the sum: {detail}"
        );
    }

    #[test]
    fn an_out_of_range_probability_is_unparsable() {
        unparsable(&response(MODEL, 1.5, "a", json!({"a": 0.9, "b": 0.1})));
        unparsable(&response(MODEL, 0.9, "a", json!({"a": -0.1, "b": 1.1})));
    }

    #[test]
    fn a_different_model_is_a_mismatch() {
        let body = response("jev-1.13.0", 0.9, "a", json!({"a": 0.9, "b": 0.1}));
        match parse_jev_response(&body, &two_options(), MODEL) {
            Err((FallbackReason::ModelMismatch, detail)) => {
                assert!(
                    detail.contains("model") && detail.contains("jev-1.14.0"),
                    "the detail names the field and the configured id: {detail}"
                );
                assert!(
                    !detail.contains("jev-1.13.0"),
                    "the detail repeats the response's model: {detail}"
                );
            }
            other => panic!("expected a model mismatch, got {other:?}"),
        }
    }

    #[test]
    fn two_equal_probabilities_use_the_named_choice() {
        let body = response(MODEL, 0.9, "b", json!({"a": 0.5, "b": 0.5}));
        let scores = parse_jev_response(&body, &two_options(), MODEL).expect("parses");
        let config = TriageConfig::with_thresholds(Some(0.4), Some(0.4));
        match decide(&scores, &config) {
            TriageOutcome::Decided { option_id, .. } => assert_eq!(option_id, "b"),
            other => panic!("expected b to be decided, got {other:?}"),
        }
    }

    #[test]
    fn the_threshold_reads_the_chosen_probability_not_confidence() {
        let mut body = response(MODEL, 0.9, "a", json!({"a": 0.9, "b": 0.1}));
        body["answers"]["pick"]["confidence"] = json!(0.2);
        let scores = parse_jev_response(&body, &two_options(), MODEL).expect("parses");
        let config = TriageConfig::with_thresholds(Some(0.5), Some(0.5));
        assert!(matches!(
            decide(&scores, &config),
            TriageOutcome::Decided { ref option_id, .. } if option_id == "a"
        ));
    }

    /// The literals are copied from the Phase 54 measurement report
    /// (54-BAKEOFF.md): model jev-1.14.0, pre-check 0.37, option 0.92.
    #[test]
    fn the_shipped_defaults_are_the_measured_ones() {
        assert_eq!(DEFAULT_MODEL, "jev-1.14.0");
        assert_eq!(DEFAULT_PRE_CHECK_THRESHOLD, Some(0.37));
        assert_eq!(DEFAULT_THRESHOLD, Some(0.92));
    }
}
