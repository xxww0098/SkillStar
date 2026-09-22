//! Request validation and answer shaping — the `agentjev.decision.v1` contract.
//!
//! This is a faithful port of the reference implementation's `jev_service`
//! `contract.py`: same accepted shapes, same rejection order, and the same
//! message text. Two properties are deliberate there and kept here:
//!
//! * **Transport IDs never reach the model.** Question ids exist so the caller
//!   can match answers; they are not part of any candidate text.
//! * **Nothing is truncated.** An over-length state is an error, never a
//!   silent crop of the question or a candidate.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{DecisionError, Result};

/// Version string echoed in every response.
pub const API_VERSION: &str = "agentjev.decision.v1";

/// Longest accepted token path (`[STATE] … \n[QUESTION] … \n[CANDIDATE] …`).
pub const MAX_PATH_TOKENS: usize = 2048;

/// Candidate ceiling for one `choice` question.
pub const MAX_CHOICE_CANDIDATES: usize = 255;

/// Question ceiling for one batch.
pub const MAX_QUESTIONS: usize = 128;

/// Candidate-path ceiling for one batch.
pub const MAX_PATHS: usize = 1024;

/// State ceiling for one batch.
pub const MAX_REQUESTS: usize = 32;

/// The three answer shapes the model was trained on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "DecisionQuestionKind.ts", rename = "DecisionQuestionKind")]
pub enum QuestionKind {
    /// A proposition: returns the probability of `true`.
    Boolean,
    /// Which of these options, given exactly this set.
    Choice,
    /// Where on this ordered rubric.
    Score,
}

impl QuestionKind {
    /// Lowercase wire name, matching the reference JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Choice => "choice",
            Self::Score => "score",
        }
    }
}

/// One validated question, ready to be tokenized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedQuestion {
    /// Caller-supplied id, echoed back with the answer.
    pub id: String,
    /// Which primitive this question is.
    pub kind: QuestionKind,
    /// The question text shown to the model.
    pub text: String,
    /// Answer keys in candidate order (`true`/`false`, option ids, level ids).
    pub keys: Vec<String>,
    /// Candidate descriptions in the same order as [`Self::keys`].
    pub candidates: Vec<String>,
}

/// One validated state with its questions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRequest {
    /// Caller-supplied request id, or the request index as a string.
    pub id: String,
    /// The unstructured state (text, or a compact JSON serialization).
    pub state: String,
    /// Validated questions, in request order.
    pub questions: Vec<PreparedQuestion>,
}

/// A model distribution over one question's candidates.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionAnswer {
    /// Boolean answer with the full two-mass distribution.
    Boolean {
        /// Question id.
        id: String,
        /// Probability the proposition is true.
        probability: f32,
        /// `probability >= 0.5`.
        value: bool,
        /// `[key, probability]` pairs in candidate order.
        distribution: Vec<(String, f32)>,
    },
    /// Choice answer with ranking metadata.
    Choice {
        /// Question id.
        id: String,
        /// Winning option key.
        value: String,
        /// Winning option description.
        description: String,
        /// Probability of the winning option.
        top_probability: f32,
        /// Gap between the best and the second best option.
        margin: f32,
        /// `[key, probability]` pairs in candidate order.
        distribution: Vec<(String, f32)>,
    },
    /// Score answer over an ordered rubric.
    Score {
        /// Question id.
        id: String,
        /// Expected level, `Σ i · Pᵢ`.
        score: f32,
        /// Argmax level index.
        level: usize,
        /// Level descriptions, lowest first.
        legend: Vec<String>,
        /// `[key, probability]` pairs in candidate order.
        distribution: Vec<(String, f32)>,
    },
}

impl DecisionAnswer {
    /// Question id this answer belongs to.
    pub fn id(&self) -> &str {
        match self {
            Self::Boolean { id, .. } | Self::Choice { id, .. } | Self::Score { id, .. } => id,
        }
    }

    /// Which primitive produced this answer.
    pub fn kind(&self) -> QuestionKind {
        match self {
            Self::Boolean { .. } => QuestionKind::Boolean,
            Self::Choice { .. } => QuestionKind::Choice,
            Self::Score { .. } => QuestionKind::Score,
        }
    }
}

/// Normalize a `state` / `question` / candidate value.
///
/// Strings pass through unchanged (only emptiness is rejected); objects and
/// arrays become compact JSON with sorted keys, which is what the reference
/// feeds the model for structured state.
fn semantic(value: Option<&Value>, name: &str) -> Result<String> {
    match value {
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(text.clone()),
        Some(value @ (Value::Object(_) | Value::Array(_))) => serde_json::to_string(value)
            .map_err(|_| DecisionError::contract(format!("{name} must contain valid JSON"))),
        _ => Err(DecisionError::contract(format!(
            "{name} must be nonempty text, an object or an array"
        ))),
    }
}

fn object_field<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    object.get(key)
}

/// Parse and validate a decision payload.
///
/// Accepts either a single `{state, questions}` request or a batch under
/// `requests`, exactly like the reference `prepare()`.
pub fn prepare(payload: &Value) -> Result<Vec<PreparedRequest>> {
    let Value::Object(root) = payload else {
        return Err(DecisionError::contract("request must be an object"));
    };

    // `payload['requests'] if present else [payload]` — a payload that carries
    // `requests` drops its own `state`/`questions` entirely.
    let requests: Vec<&Value> = match root.get("requests") {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(_) => {
            return Err(DecisionError::contract(
                "requests must contain 1..32 states",
            ));
        }
        None => vec![payload],
    };
    if requests.is_empty() || requests.len() > MAX_REQUESTS {
        return Err(DecisionError::contract(
            "requests must contain 1..32 states",
        ));
    }

    let mut prepared = Vec::with_capacity(requests.len());
    let mut total_paths = 0usize;
    let mut total_questions = 0usize;

    for (request_index, request) in requests.iter().enumerate() {
        let Value::Object(request) = request else {
            return Err(DecisionError::contract("each request must be an object"));
        };
        let state = semantic(object_field(request, "state"), "state")?;

        let Some(Value::Array(questions)) = request.get("questions") else {
            return Err(DecisionError::contract(
                "questions must be a nonempty array",
            ));
        };
        if questions.is_empty() {
            return Err(DecisionError::contract(
                "questions must be a nonempty array",
            ));
        }

        let mut rows = Vec::with_capacity(questions.len());
        let mut seen_ids: Vec<String> = Vec::with_capacity(questions.len());
        for (question_index, question) in questions.iter().enumerate() {
            let Value::Object(question) = question else {
                return Err(DecisionError::contract("each question must be an object"));
            };

            let id = match question.get("id") {
                Some(Value::String(id)) => id.clone(),
                Some(_) => {
                    return Err(DecisionError::contract(
                        "question IDs must be unique nonempty strings within a state",
                    ));
                }
                None => question_index.to_string(),
            };
            if id.is_empty() || seen_ids.iter().any(|seen| seen == &id) {
                return Err(DecisionError::contract(
                    "question IDs must be unique nonempty strings within a state",
                ));
            }
            seen_ids.push(id.clone());

            let kind = match question.get("type") {
                None => QuestionKind::Choice,
                Some(Value::String(name)) if name == "boolean" => QuestionKind::Boolean,
                Some(Value::String(name)) if name == "choice" => QuestionKind::Choice,
                Some(Value::String(name)) if name == "score" => QuestionKind::Score,
                Some(_) => {
                    return Err(DecisionError::contract(
                        "type must be boolean, choice or score",
                    ));
                }
            };

            let text_value = question
                .get("question")
                .or_else(|| question.get("instructions"));
            let text = semantic(text_value, "question")?;

            let (keys, candidates) = match kind {
                QuestionKind::Boolean => {
                    let criteria: Option<&Value> = match question.get("criteria") {
                        None => None,
                        Some(value @ Value::Object(_)) => Some(value),
                        Some(_) => {
                            return Err(DecisionError::contract("criteria must be an object"));
                        }
                    };
                    let mut candidates = Vec::with_capacity(2);
                    for key in ["true", "false"] {
                        let override_value = criteria.and_then(|criteria| criteria.get(key));
                        candidates.push(match override_value {
                            Some(value) => semantic(Some(value), &format!("{key} criterion"))?,
                            None => key.to_uppercase(),
                        });
                    }
                    (
                        vec!["true".to_string(), "false".to_string()],
                        candidates,
                    )
                }
                QuestionKind::Choice => {
                    let Some(options) = question.get("options") else {
                        return Err(DecisionError::contract(
                            "choice.options must be an object or array",
                        ));
                    };
                    let (keys, candidates) = match options {
                        Value::Object(map) => {
                            let mut keys = Vec::with_capacity(map.len());
                            let mut candidates = Vec::with_capacity(map.len());
                            for (key, value) in map {
                                keys.push(key.clone());
                                candidates.push(semantic(Some(value), "option")?);
                            }
                            (keys, candidates)
                        }
                        Value::Array(items) => {
                            let mut keys = Vec::with_capacity(items.len());
                            let mut candidates = Vec::with_capacity(items.len());
                            for (index, value) in items.iter().enumerate() {
                                keys.push(index.to_string());
                                candidates.push(semantic(Some(value), "option")?);
                            }
                            (keys, candidates)
                        }
                        _ => {
                            return Err(DecisionError::contract(
                                "choice.options must be an object or array",
                            ));
                        }
                    };
                    if keys.len() < 2 || keys.len() > MAX_CHOICE_CANDIDATES {
                        return Err(DecisionError::contract(
                            "choice requires 2..255 candidates",
                        ));
                    }
                    if keys.iter().any(String::is_empty) {
                        return Err(DecisionError::contract(
                            "option IDs must be nonempty strings",
                        ));
                    }
                    (keys, candidates)
                }
                QuestionKind::Score => {
                    let Some(Value::Array(levels)) = question.get("levels") else {
                        return Err(DecisionError::contract(
                            "score.levels must contain 2..10 ordered descriptions",
                        ));
                    };
                    if levels.len() < 2 || levels.len() > 10 {
                        return Err(DecisionError::contract(
                            "score.levels must contain 2..10 ordered descriptions",
                        ));
                    }
                    let mut candidates = Vec::with_capacity(levels.len());
                    for level in levels {
                        candidates.push(semantic(Some(level), "level")?);
                    }
                    let keys = (0..levels.len()).map(|index| index.to_string()).collect();
                    (keys, candidates)
                }
            };

            if has_duplicates(&candidates) {
                return Err(DecisionError::contract(
                    "candidate descriptions must be distinct",
                ));
            }

            total_paths += candidates.len();
            total_questions += 1;
            rows.push(PreparedQuestion {
                id,
                kind,
                text,
                keys,
                candidates,
            });
        }

        prepared.push(PreparedRequest {
            id: match request.get("id") {
                Some(Value::String(id)) => id.clone(),
                _ => request_index.to_string(),
            },
            state,
            questions: rows,
        });
    }

    if total_questions > MAX_QUESTIONS || total_paths > MAX_PATHS {
        return Err(DecisionError::contract(
            "batch exceeds 128 questions or 1024 candidate paths",
        ));
    }

    Ok(prepared)
}

fn has_duplicates(values: &[String]) -> bool {
    let mut seen: Vec<&str> = Vec::with_capacity(values.len());
    for value in values {
        if seen.contains(&value.as_str()) {
            return true;
        }
        seen.push(value);
    }
    false
}

/// Turn one question's per-candidate probabilities into an answer.
///
/// Mirrors `contract.answer()`: the distribution must be finite, non-negative
/// and sum to one — a broken distribution is an error, not a rounded answer.
pub fn answer(question: &PreparedQuestion, probabilities: &[f32]) -> Result<DecisionAnswer> {
    if probabilities.len() != question.keys.len()
        || probabilities.iter().any(|p| !p.is_finite() || *p < 0.0)
    {
        return Err(DecisionError::Inference(
            "invalid model distribution".to_string(),
        ));
    }
    let sum: f32 = probabilities.iter().sum();
    if (sum - 1.0).abs() > 1e-4 {
        return Err(DecisionError::Inference(
            "model distribution does not sum to one".to_string(),
        ));
    }

    let distribution: Vec<(String, f32)> = question
        .keys
        .iter()
        .cloned()
        .zip(probabilities.iter().copied())
        .collect();

    let mut index = 0usize;
    for (candidate, probability) in probabilities.iter().enumerate() {
        if *probability > probabilities[index] {
            index = candidate;
        }
    }

    Ok(match question.kind {
        QuestionKind::Boolean => DecisionAnswer::Boolean {
            id: question.id.clone(),
            probability: probabilities[0],
            value: probabilities[0] >= 0.5,
            distribution,
        },
        QuestionKind::Choice => {
            let mut ranked: Vec<f32> = probabilities.to_vec();
            ranked.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            DecisionAnswer::Choice {
                id: question.id.clone(),
                value: question.keys[index].clone(),
                description: question.candidates[index].clone(),
                top_probability: ranked[0],
                margin: ranked[0] - ranked[1],
                distribution,
            }
        }
        QuestionKind::Score => DecisionAnswer::Score {
            id: question.id.clone(),
            score: probabilities
                .iter()
                .enumerate()
                .map(|(level, probability)| level as f32 * probability)
                .sum(),
            level: index,
            legend: question.candidates.clone(),
            distribution,
        },
    })
}
