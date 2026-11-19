//! A question an agent asks the operator through DevFlow's `devflow_ask` tool
//! (Phase 53, D-07): what it asks, the options it offers, and the answer the
//! operator chose. A question is carried by a gate of kind
//! [`crate::gates::GateKind::Question`]; the spec lives in that gate's file and
//! the choice in that gate's answer, so the gate protocol is the only channel.

use serde::{Deserialize, Serialize};

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
    fn the_blocking_human_kind_serializes_as_kebab_case() {
        let wire = serde_json::to_value(QuestionKind::BlockingHuman).expect("serializes");
        assert_eq!(
            wire,
            serde_json::json!("blocking-human"),
            "the blocking-human kind's wire value is kebab-case (D-07)"
        );
    }
}
