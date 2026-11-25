//! A question an agent asks the operator through DevFlow's `devflow_ask` tool
//! (Phase 53, D-07): what it asks, the options it offers, and the answer the
//! operator chose. A question is carried by a gate of kind
//! [`crate::gates::GateKind::Question`]; the spec lives in that gate's file and
//! the choice in that gate's answer, so the gate protocol is the only channel.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The most characters a question may have. Lengths are counted in
/// characters, not bytes, and over-long text is refused, never truncated.
pub const QUESTION_MAX_CHARS: usize = 4096;
/// The most characters a question's context may have.
pub const CONTEXT_MAX_CHARS: usize = 16384;
/// The most characters an option id may have.
pub const OPTION_ID_MAX_CHARS: usize = 64;
/// The most characters an option label may have.
pub const OPTION_LABEL_MAX_CHARS: usize = 512;
/// The most characters an option description may have.
pub const OPTION_DESCRIPTION_MAX_CHARS: usize = 2048;
/// The most options a question may offer.
pub const MAX_OPTIONS: usize = 32;

/// Why a `devflow_ask` payload was refused (D-07, D-08). Each refusal names
/// the field it is about, and each is returned before any gate opens.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QuestionError {
    /// The arguments are not a JSON object.
    #[error("the arguments are not a JSON object")]
    NotAnObject,
    /// A required field is absent.
    #[error("`{field}` is missing")]
    Missing { field: String },
    /// A field has the wrong JSON type.
    #[error("`{field}` is not {expected}")]
    WrongType {
        field: String,
        expected: &'static str,
    },
    /// A field that must say something is empty or only whitespace.
    #[error("`{field}` is empty")]
    Blank { field: String },
    /// A field is longer than its bound, in characters.
    #[error("`{field}` is {chars} characters long; the most allowed is {max}")]
    TooLong {
        field: String,
        chars: usize,
        max: usize,
    },
    /// No option is offered.
    #[error("`options` is empty: offer at least one option")]
    NoOptions,
    /// More options are offered than [`MAX_OPTIONS`].
    #[error("`options` has {count} entries; the most allowed is {max}", max = MAX_OPTIONS)]
    TooManyOptions { count: usize },
    /// An option id contains a control character, so it cannot be typed at a shell.
    #[error("`{field}` contains a control character")]
    ControlCharacter { field: String },
    /// Two options share an id.
    #[error("`{field}` `{id}` repeats an earlier option's id")]
    DuplicateOptionId { field: String, id: String },
    /// The recommended id is not an offered option's id.
    #[error("`recommended` `{0}` is not the id of an offered option")]
    RecommendedNotOffered(String),
    /// The kind is not one of the three accepted kinds.
    #[error("`kind` `{0}` is not one of `decision`, `blocking-human`, `discuss`")]
    UnknownKind(String),
}

/// One option the asking agent offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    /// The id the operator answers with (`devflow gate answer --option <id>`).
    pub id: String,
    /// A short label for the option.
    pub label: String,
    /// What choosing the option means, when the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// What kind of question the agent asks (D-07). Its wire values are
/// `decision`, `blocking-human` and `discuss`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuestionKind {
    /// A choice between options the agent can name.
    Decision,
    /// A step only a human can take or confirm.
    BlockingHuman,
    /// A question raised while discussing a phase.
    Discuss,
}

/// A question as the asking agent sent it, under the id DevFlow gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionSpec {
    /// The question's identity, fresh for every question asked.
    pub question_id: String,
    /// The question itself.
    pub question: String,
    /// The options offered.
    pub options: Vec<QuestionOption>,
    /// Context the agent gives with the question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// The question's kind.
    pub kind: QuestionKind,
    /// The id of the option the agent recommends, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<String>,
}

impl QuestionSpec {
    /// The question a `devflow_ask` call's `args` ask, under a fresh question
    /// id (any id the agent sent is not read). Each field is read by name and
    /// checked; unknown keys are ignored.
    pub fn from_arguments(args: &Value) -> Result<QuestionSpec, QuestionError> {
        let fields = args.as_object().ok_or(QuestionError::NotAnObject)?;
        let question = required_text(fields.get("question"), "question")?;
        bounded(question, "question", QUESTION_MAX_CHARS)?;
        let options = parse_options(fields.get("options"))?;
        let context = optional_text(fields.get("context"), "context")?;
        if let Some(context) = context {
            bounded(context, "context", CONTEXT_MAX_CHARS)?;
        }
        let kind = parse_kind(fields.get("kind"))?;
        let recommended = optional_text(fields.get("recommended"), "recommended")?;
        if let Some(id) = recommended
            && !options.iter().any(|option| option.id == id)
        {
            return Err(QuestionError::RecommendedNotOffered(id.to_string()));
        }
        Ok(QuestionSpec {
            question_id: new_question_id(),
            question: question.to_string(),
            options,
            context: context.map(str::to_string),
            kind,
            recommended: recommended.map(str::to_string),
        })
    }
}

impl QuestionSpec {
    /// The question a `devflow_ask` call's `args` ask, as [`Self::from_arguments`]
    /// reads it, beside the `question_id` a re-call carries to continue a
    /// question it was told is still pending. The id is optional and must have
    /// a question id's form (16 lowercase hex digits); whether it names a
    /// question is for the server that issued it to say.
    pub fn from_call_arguments(
        args: &Value,
    ) -> Result<(QuestionSpec, Option<String>), QuestionError> {
        let spec = Self::from_arguments(args)?;
        let recall_id = optional_text(args.get("question_id"), "question_id")?;
        if let Some(id) = recall_id
            && !crate::gates::is_gate_id(id)
        {
            return Err(QuestionError::WrongType {
                field: "question_id".to_string(),
                expected: "a question id of 16 lowercase hex digits",
            });
        }
        Ok((spec, recall_id.map(str::to_string)))
    }
}

/// `value` as text: `None` when absent or `null`, refused when not a string.
fn optional_text<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> Result<Option<&'a str>, QuestionError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(QuestionError::WrongType {
            field: field.to_string(),
            expected: "a string",
        }),
    }
}

/// `value` as text that must be present and say something.
fn required_text<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a str, QuestionError> {
    let text = optional_text(value, field)?.ok_or_else(|| QuestionError::Missing {
        field: field.to_string(),
    })?;
    if text.trim().is_empty() {
        return Err(QuestionError::Blank {
            field: field.to_string(),
        });
    }
    Ok(text)
}

/// Refuse `text` when it has more than `max` characters.
fn bounded(text: &str, field: &str, max: usize) -> Result<(), QuestionError> {
    let chars = text.chars().count();
    if chars > max {
        return Err(QuestionError::TooLong {
            field: field.to_string(),
            chars,
            max,
        });
    }
    Ok(())
}

/// The `options` array: at least one, at most [`MAX_OPTIONS`], ids unique.
fn parse_options(value: Option<&Value>) -> Result<Vec<QuestionOption>, QuestionError> {
    let entries = match value {
        None | Some(Value::Null) => {
            return Err(QuestionError::Missing {
                field: "options".to_string(),
            });
        }
        Some(Value::Array(entries)) => entries,
        Some(_) => {
            return Err(QuestionError::WrongType {
                field: "options".to_string(),
                expected: "an array",
            });
        }
    };
    if entries.is_empty() {
        return Err(QuestionError::NoOptions);
    }
    if entries.len() > MAX_OPTIONS {
        return Err(QuestionError::TooManyOptions {
            count: entries.len(),
        });
    }
    let mut options: Vec<QuestionOption> = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let option = parse_option(entry, index)?;
        if options.iter().any(|earlier| earlier.id == option.id) {
            return Err(QuestionError::DuplicateOptionId {
                field: format!("options[{index}].id"),
                id: option.id,
            });
        }
        options.push(option);
    }
    Ok(options)
}

/// `options[index]`: an id an operator can type, a label, and an optional description.
fn parse_option(entry: &Value, index: usize) -> Result<QuestionOption, QuestionError> {
    let fields: &Map<String, Value> =
        entry.as_object().ok_or_else(|| QuestionError::WrongType {
            field: format!("options[{index}]"),
            expected: "an object",
        })?;
    let id_field = format!("options[{index}].id");
    let id = required_text(fields.get("id"), &id_field)?;
    bounded(id, &id_field, OPTION_ID_MAX_CHARS)?;
    if id.chars().any(char::is_control) {
        return Err(QuestionError::ControlCharacter { field: id_field });
    }
    let label_field = format!("options[{index}].label");
    let label = optional_text(fields.get("label"), &label_field)?.ok_or_else(|| {
        QuestionError::Missing {
            field: label_field.clone(),
        }
    })?;
    bounded(label, &label_field, OPTION_LABEL_MAX_CHARS)?;
    let description_field = format!("options[{index}].description");
    let description = optional_text(fields.get("description"), &description_field)?;
    if let Some(description) = description {
        bounded(
            description,
            &description_field,
            OPTION_DESCRIPTION_MAX_CHARS,
        )?;
    }
    Ok(QuestionOption {
        id: id.to_string(),
        label: label.to_string(),
        description: description.map(str::to_string),
    })
}

/// The `kind` field, read through [`QuestionKind`]'s own wire names.
fn parse_kind(value: Option<&Value>) -> Result<QuestionKind, QuestionError> {
    let text = optional_text(value, "kind")?.ok_or_else(|| QuestionError::Missing {
        field: "kind".to_string(),
    })?;
    serde_json::from_value(Value::String(text.to_string()))
        .map_err(|_| QuestionError::UnknownKind(text.to_string()))
}

/// The operator's answer to a question: an offered option, or free text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QuestionChoice {
    /// One of the offered options, by id.
    Option {
        /// The chosen option's id.
        option_id: String,
    },
    /// A free-text answer.
    Text {
        /// The answer's text.
        text: String,
    },
}

/// What `devflow_ask` returns to the asking agent (D-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskAnswer {
    /// The question answered.
    pub question_id: String,
    /// The operator's choice.
    pub answer: QuestionChoice,
    /// Who answered.
    pub answered_by: String,
    /// When the answer was consumed, as Unix epoch seconds.
    pub answered_at: String,
}

/// The kind of answerer an `answered_by` / `responded_by` string names (#343,
/// D-13). Only [`AnswererKind::Operator`] answers count as triage reference
/// labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnswererKind {
    /// The operator: any name not carrying a reserved prefix.
    Operator,
    /// An agent answering for the operator, `agent:<name>`.
    Agent,
    /// DevFlow's own triage, `triage:<model>`.
    Triage,
    /// Nobody recorded: `unknown`, which `devflow_ask` writes when a response
    /// names no answerer, or a blank string.
    Unknown,
}

/// Classify `answered_by` by its prefix: `agent:` is an agent, `triage:` is
/// triage, `unknown` or blank is unknown, and every other name is the operator.
#[must_use]
pub fn answerer_kind(answered_by: &str) -> AnswererKind {
    if answered_by.starts_with("agent:") {
        AnswererKind::Agent
    } else if answered_by.starts_with("triage:") {
        AnswererKind::Triage
    } else if answered_by.trim().is_empty() || answered_by == "unknown" {
        AnswererKind::Unknown
    } else {
        AnswererKind::Operator
    }
}

/// A fresh question id: 16 lowercase hex digits, built as a launch id is.
#[must_use]
pub fn new_question_id() -> String {
    crate::liveness::new_launch_id()
}

/// The gate `context` line for `spec`, which `gate list` shows: the question,
/// then each option as `id: label`, the recommended one marked.
#[must_use]
pub fn render_question_context(spec: &QuestionSpec) -> String {
    let options: Vec<String> = spec
        .options
        .iter()
        .map(|option| {
            let marker = if spec.recommended.as_deref() == Some(option.id.as_str()) {
                " (recommended)"
            } else {
                ""
            };
            format!("{}: {}{marker}", option.id, option.label)
        })
        .collect();
    format!("{} Options — {}.", spec.question, options.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answerers_are_classified_by_prefix() {
        for (answered_by, kind) in [
            ("denniyahh", AnswererKind::Operator),
            ("devflow-cli", AnswererKind::Operator),
            ("agent:claude", AnswererKind::Agent),
            ("triage:jev", AnswererKind::Triage),
            ("unknown", AnswererKind::Unknown),
            ("", AnswererKind::Unknown),
        ] {
            assert_eq!(answerer_kind(answered_by), kind, "{answered_by:?}");
        }
    }

    #[test]
    fn the_blocking_human_kind_serializes_as_kebab_case() {
        let wire = serde_json::to_value(QuestionKind::BlockingHuman).expect("serializes");
        assert_eq!(
            wire,
            serde_json::json!("blocking-human"),
            "the blocking-human kind's wire value is kebab-case (D-07)"
        );
    }

    /// An option object with `id` and a label.
    fn option(id: &str) -> Value {
        serde_json::json!({ "id": id, "label": format!("label of {id}") })
    }

    /// A well-formed payload with options `a` and `b`, for one field to break.
    fn well_formed() -> Value {
        serde_json::json!({
            "question": "Pick one",
            "options": [option("a"), option("b")],
            "kind": "decision",
        })
    }

    /// [`well_formed`] with `key` set to `value`, or removed when `value` is `None`.
    fn with(key: &str, value: Option<Value>) -> Value {
        let mut args = well_formed();
        let fields = args.as_object_mut().expect("an object");
        match value {
            Some(value) => fields.insert(key.to_string(), value),
            None => fields.remove(key),
        };
        args
    }

    fn field(name: &str) -> String {
        name.to_string()
    }

    #[test]
    fn each_malformed_payload_is_refused_naming_its_field() {
        use serde_json::json;
        let cases: Vec<(&str, Value, QuestionError, &str)> = vec![
            (
                "empty question",
                with("question", Some(json!(""))),
                QuestionError::Blank {
                    field: field("question"),
                },
                "question",
            ),
            (
                "whitespace-only question",
                with("question", Some(json!("  \t\n "))),
                QuestionError::Blank {
                    field: field("question"),
                },
                "question",
            ),
            (
                "missing options",
                with("options", None),
                QuestionError::Missing {
                    field: field("options"),
                },
                "options",
            ),
            (
                "empty options",
                with("options", Some(json!([]))),
                QuestionError::NoOptions,
                "options",
            ),
            (
                "duplicate option id",
                with("options", Some(json!([option("a"), option("a")]))),
                QuestionError::DuplicateOptionId {
                    field: field("options[1].id"),
                    id: "a".to_string(),
                },
                "options[1].id",
            ),
            (
                "empty option id",
                with("options", Some(json!([option("a"), option("")]))),
                QuestionError::Blank {
                    field: field("options[1].id"),
                },
                "options[1].id",
            ),
            (
                "recommended id not offered",
                with("recommended", Some(json!("c"))),
                QuestionError::RecommendedNotOffered("c".to_string()),
                "recommended",
            ),
            (
                "missing kind",
                with("kind", None),
                QuestionError::Missing {
                    field: field("kind"),
                },
                "kind",
            ),
            (
                "unknown kind",
                with("kind", Some(json!("survey"))),
                QuestionError::UnknownKind("survey".to_string()),
                "kind",
            ),
        ];
        for (case, args, expected, field_name) in cases {
            let refused = match QuestionSpec::from_arguments(&args) {
                Ok(spec) => panic!("case `{case}` parsed when it should be refused: {spec:?}"),
                Err(error) => error,
            };
            assert_eq!(
                refused, expected,
                "case `{case}` refused with another error"
            );
            assert!(
                refused.to_string().contains(field_name),
                "case `{case}`: the refusal `{refused}` names `{field_name}`"
            );
        }
    }

    #[test]
    fn a_recall_question_id_is_returned_beside_the_spec_and_its_form_checked() {
        let (_, absent) = QuestionSpec::from_call_arguments(&well_formed()).expect("parses");
        assert_eq!(absent, None, "a first call carries no question id");
        let id = "0123456789abcdef";
        let (spec, recall) =
            QuestionSpec::from_call_arguments(&with("question_id", Some(id.into())))
                .expect("a well-formed question id parses");
        assert_eq!(recall.as_deref(), Some(id));
        assert_ne!(spec.question_id, id, "the spec still gets a fresh id");
        let refused = QuestionSpec::from_call_arguments(&with("question_id", Some("Q-1".into())))
            .expect_err("a malformed question id is refused");
        assert!(
            refused.to_string().contains("`question_id`"),
            "the refusal names the field: {refused}"
        );
    }

    #[test]
    fn a_well_formed_payload_with_one_option_is_accepted() {
        let args = serde_json::json!({
            "question": "Proceed?",
            "options": [option("go")],
            "kind": "discuss",
            "unknown-extra-key": { "ignored": true },
        });
        let spec = QuestionSpec::from_arguments(&args).expect("a well-formed payload parses");
        assert_eq!(spec.question, "Proceed?");
        assert_eq!(spec.options.len(), 1, "one option: {spec:?}");
        assert_eq!(spec.options[0].id, "go");
        assert_eq!(spec.kind, QuestionKind::Discuss);
        assert_eq!(spec.context, None);
        assert_eq!(spec.recommended, None);
        assert!(
            crate::gates::is_gate_id(&spec.question_id),
            "a fresh question id is assigned: {spec:?}"
        );
    }

    #[test]
    fn option_ids_differing_only_by_case_are_distinct() {
        let args = with(
            "options",
            Some(serde_json::json!([option("a"), option("A")])),
        );
        let spec = QuestionSpec::from_arguments(&args).expect("`a` and `A` are distinct ids");
        let ids: Vec<&str> = spec.options.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["a", "A"]);
    }

    #[test]
    fn lengths_are_counted_in_characters() {
        let at_bound = "é".repeat(QUESTION_MAX_CHARS);
        assert!(
            at_bound.len() > QUESTION_MAX_CHARS,
            "the bound case is longer in bytes than in characters"
        );
        let accepted = QuestionSpec::from_arguments(&with("question", Some(at_bound.into())))
            .expect("a question at the bound, in characters, is accepted");
        assert_eq!(accepted.question.chars().count(), QUESTION_MAX_CHARS);

        let past_bound = "é".repeat(QUESTION_MAX_CHARS + 1);
        let refused = QuestionSpec::from_arguments(&with("question", Some(past_bound.into())));
        assert_eq!(
            refused,
            Err(QuestionError::TooLong {
                field: field("question"),
                chars: QUESTION_MAX_CHARS + 1,
                max: QUESTION_MAX_CHARS,
            }),
            "one character past the bound is refused, not truncated"
        );
    }
}
