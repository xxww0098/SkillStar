//! Load the checkpoint and answer decision requests.
//!
//! One state+question prefix is encoded **once** and every candidate of that
//! question is scored as a branch off it — the shared-prefix runtime the
//! reference implementation measures its ~2× wide-candidate speedup with.
//! Candidates of one question never attend one another, and a branch never
//! writes into the prefix it was read from.

use std::path::Path;
use std::time::Instant;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use serde::Serialize;
use serde_json::Value;
use tokenizers::Tokenizer;
use ts_rs::TS;

use crate::backbone::{Backbone, Qwen3Config};
use crate::contract::{
    self, DecisionAnswer, PreparedQuestion, PreparedRequest, API_VERSION, MAX_PATH_TOKENS,
};
use crate::error::{DecisionError, Result};
use crate::head::CandidateHead;
use crate::model_files::{ModelPaths, MODEL_ID};

/// Candidate-set head width; part of the published checkpoint, not a knob.
const SET_DIM: usize = 256;
/// Candidate-set head depth.
const SET_LAYERS: usize = 2;
/// Candidate-set head attention heads.
const SET_HEADS: usize = 4;

/// Which device to run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceChoice {
    /// Metal on macOS when it is available, otherwise CPU.
    #[default]
    Auto,
    /// Always CPU.
    Cpu,
    /// Metal (macOS only; an error elsewhere).
    Metal,
}

/// Which dtype to run in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DTypeChoice {
    /// f16 on Metal, f32 on CPU.
    #[default]
    Auto,
    /// Full precision; the reference implementation's CPU behavior.
    F32,
    /// Half precision.
    F16,
    /// Bfloat16.
    Bf16,
}

/// Engine construction options.
#[derive(Debug, Clone, Copy, Default)]
pub struct EngineOptions {
    /// Device selection.
    pub device: DeviceChoice,
    /// Dtype selection.
    pub dtype: DTypeChoice,
}

impl EngineOptions {
    /// CPU + f32: the exact configuration the golden vectors were produced in.
    pub fn reference() -> Self {
        Self {
            device: DeviceChoice::Cpu,
            dtype: DTypeChoice::F32,
        }
    }
}

/// Calibration temperatures, one positive scalar per primitive.
#[derive(Debug, Clone, Copy)]
pub struct Temperatures {
    boolean: f32,
    choice: f32,
    score: f32,
}

impl Temperatures {
    fn for_kind(&self, kind: contract::QuestionKind) -> f32 {
        match kind {
            contract::QuestionKind::Boolean => self.boolean,
            contract::QuestionKind::Choice => self.choice,
            contract::QuestionKind::Score => self.score,
        }
    }

    /// Boolean-primitive temperature.
    pub fn boolean(&self) -> f32 {
        self.boolean
    }

    /// Choice-primitive temperature.
    pub fn choice(&self) -> f32 {
        self.choice
    }

    /// Score-primitive temperature.
    pub fn score(&self) -> f32 {
        self.score
    }

    /// Read `temperatures.json`. Out-of-range values are refused rather than
    /// clamped: a bad calibration file silently accepted would skew every
    /// probability the app reports.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|error| DecisionError::io(path, error))?;
        let raw: std::collections::BTreeMap<String, Value> = serde_json::from_str(&text)
            .map_err(|error| DecisionError::ModelFiles(format!("temperatures.json: {error}")))?;
        let mut parsed = std::collections::BTreeMap::new();
        for (kind, value) in raw {
            if !matches!(kind.as_str(), "boolean" | "choice" | "score") {
                continue;
            }
            let temperature = value
                .get("temperature")
                .and_then(Value::as_f64)
                .ok_or_else(|| {
                    DecisionError::ModelFiles("Invalid calibration temperature".to_string())
                })? as f32;
            if !temperature.is_finite() || !(0.05..=20.0).contains(&temperature) {
                return Err(DecisionError::ModelFiles(
                    "Invalid calibration temperature".to_string(),
                ));
            }
            parsed.insert(kind, temperature);
        }
        Ok(Self {
            boolean: *parsed.get("boolean").unwrap_or(&1.0),
            choice: *parsed.get("choice").unwrap_or(&1.0),
            score: *parsed.get("score").unwrap_or(&1.0),
        })
    }
}

/// Per-question distribution, before answer shaping.
///
/// Only the calibrated distribution is kept: the pre-temperature logits are an
/// implementation detail, and the golden test compares the probabilities a
/// caller actually sees.
struct ScoredQuestion {
    probabilities: Vec<f32>,
}

/// The shared prefix plus one suffix per candidate of a single question.
struct QuestionPaths {
    prefix: Vec<u32>,
    suffixes: Vec<Vec<u32>>,
}

/// One fully tokenized candidate path, as the model sees it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionEncodedPath.ts", rename = "DecisionEncodedPath")]
pub struct EncodedPath {
    /// Request id from the payload.
    pub request_id: String,
    /// Question id from the payload.
    pub question_id: String,
    /// Answer key this path belongs to.
    pub key: String,
    /// `[STATE] … \n[QUESTION] … \n[CANDIDATE] …` token ids.
    pub tokens: Vec<u32>,
}

struct ScoredBatch {
    prepared: Vec<PreparedRequest>,
    questions: Vec<ScoredQuestion>,
    usage: DecisionUsage,
}

/// One request's answers.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionRequestAnswers.ts", rename = "DecisionRequestAnswers")]
pub struct RequestAnswers {
    /// Request id from the payload.
    pub id: String,
    /// Answers in question order.
    pub answers: Vec<AnswerDto>,
}

/// One `[key, probability]` pair.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionDistributionEntry.ts", rename = "DecisionDistributionEntry")]
pub struct DistributionEntry {
    /// Answer key (`true`/`false`, option id, level id).
    pub key: String,
    /// Calibrated probability for this key.
    pub probability: f32,
}

/// Flat, UI-shaped answer for one question.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionAnswer.ts", rename = "DecisionAnswer")]
pub struct AnswerDto {
    /// Question id from the payload.
    pub id: String,
    /// Which primitive produced this answer.
    pub kind: contract::QuestionKind,
    /// Full distribution in candidate order.
    pub distribution: Vec<DistributionEntry>,
    /// Argmax key.
    pub selected_key: String,
    /// Argmax description (option text, level text, or `TRUE`/`FALSE`).
    pub selected_description: String,
    /// Probability of the argmax.
    pub top_probability: f32,
    /// Gap between the best and second best candidate.
    pub margin: f32,
    /// Probability of `true`, boolean questions only.
    pub probability_true: Option<f32>,
    /// Expected rubric level, score questions only.
    pub score: Option<f32>,
    /// Argmax rubric level, score questions only.
    pub level: Option<usize>,
    /// Rubric descriptions, score questions only.
    pub level_descriptions: Vec<String>,
}

/// Token + compute accounting for one evaluation.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionUsage.ts", rename = "DecisionUsage")]
pub struct DecisionUsage {
    /// Questions answered.
    pub questions: usize,
    /// Candidate paths scored.
    pub candidate_paths: usize,
    /// Tokens across all `[STATE][QUESTION][CANDIDATE]` paths.
    pub input_path_tokens: usize,
    /// Tokens the trunk actually encoded (shared prefixes counted once).
    pub backbone_input_tokens: usize,
    /// Questions that used a shared prefix.
    pub shared_prefix_questions: usize,
    /// Always zero: this model decodes nothing.
    pub generated_tokens: usize,
    /// Always zero: over-length input is refused, never cropped.
    pub truncated_inputs: usize,
    /// Wall-clock time of the scoring pass.
    // ts-rs maps u64 to bigint; the value crosses the wire as JSON and a
    // millisecond count never approaches 2^53.
    #[ts(type = "number")]
    pub wall_ms: u64,
}

/// Full evaluation result.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionOutcome.ts", rename = "DecisionOutcome")]
pub struct DecisionOutcome {
    /// Contract version.
    pub api_version: String,
    /// Model id.
    pub model: String,
    /// Per-request answers.
    pub results: Vec<RequestAnswers>,
    /// Accounting.
    pub usage: DecisionUsage,
}

/// What the engine is running as, for the UI's status line.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionEngineInfo.ts", rename = "DecisionEngineInfo")]
pub struct EngineInfo {
    /// Contract version.
    pub api_version: String,
    /// Model id.
    pub model: String,
    /// Device label (`metal` / `cpu`).
    pub device: String,
    /// Dtype label (`f16` / `f32` / `bf16`).
    pub dtype: String,
    /// Longest token path accepted.
    pub max_path_tokens: usize,
    /// Candidate ceiling for one choice question.
    pub max_choice_candidates: usize,
    /// Whether output tokens are ever decoded (never).
    pub output_token_decoding: bool,
    /// Whether candidate branches reuse a shared prefix (always).
    pub shared_prefix_compute: bool,
    /// Calibration temperatures in use.
    pub temperatures: TemperatureReport,
}

/// Temperatures as reported to the UI.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionTemperatures.ts", rename = "DecisionTemperatures")]
pub struct TemperatureReport {
    /// Boolean primitive.
    pub boolean: f32,
    /// Choice primitive.
    pub choice: f32,
    /// Score primitive.
    pub score: f32,
}

/// The loaded model. Construction is expensive; keep one per process.
pub struct DecisionEngine {
    tokenizer: Tokenizer,
    backbone: Backbone,
    head: CandidateHead,
    temperatures: Temperatures,
    device_label: &'static str,
    dtype: DType,
}

impl DecisionEngine {
    /// Load the checkpoint from `paths`.
    ///
    /// Sizes are checked here; digests are checked by [`crate::model_files::download`]
    /// after transfer and by the explicit verify command, so a normal startup
    /// does not re-hash 1.2 GB.
    pub fn load(paths: &ModelPaths, options: EngineOptions) -> Result<Self> {
        for spec in crate::model_files::MODEL_FILES {
            let path = paths.file(spec.name);
            let actual = std::fs::metadata(&path)
                .map_err(|_| {
                    DecisionError::ModelFiles(format!(
                        "{} is missing; download the decision model first",
                        path.display()
                    ))
                })?
                .len();
            if actual != spec.bytes {
                return Err(DecisionError::ModelFiles(format!(
                    "{} is {actual} bytes, expected {}; download the decision model again",
                    path.display(),
                    spec.bytes
                )));
            }
        }

        let config_text = std::fs::read_to_string(paths.config())
            .map_err(|error| DecisionError::io(paths.config(), error))?;
        let config: Qwen3Config = serde_json::from_str(&config_text)
            .map_err(|error| DecisionError::ModelFiles(format!("config.json: {error}")))?;
        let hidden_size = config.hidden_size;

        let temperatures = Temperatures::load(&paths.temperatures())?;
        let tokenizer = Tokenizer::from_file(paths.tokenizer()).map_err(|error| {
            DecisionError::ModelFiles(format!("tokenizer.json: {error}"))
        })?;

        let (device, dtype, device_label) = resolve_device_dtype(options)?;
        // SAFETY: the checkpoint is memory-mapped read-only for the lifetime of
        // the VarBuilder, and nothing writes to the file while an engine is
        // alive. Loading is serialized by the caller, which holds the only
        // engine instance.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[paths.weights()], dtype, &device)?
        };
        let backbone = Backbone::load(config, vb.clone())?;
        let head = CandidateHead::load(vb, hidden_size, SET_DIM, SET_LAYERS, SET_HEADS)?;

        Ok(Self {
            tokenizer,
            backbone,
            head,
            temperatures,
            device_label,
            dtype,
        })
    }

    /// What this engine is running as.
    pub fn info(&self) -> EngineInfo {
        EngineInfo {
            api_version: API_VERSION.to_string(),
            model: MODEL_ID.to_string(),
            device: self.device_label.to_string(),
            dtype: dtype_label(self.dtype),
            max_path_tokens: MAX_PATH_TOKENS,
            max_choice_candidates: contract::MAX_CHOICE_CANDIDATES,
            output_token_decoding: false,
            shared_prefix_compute: true,
            temperatures: TemperatureReport {
                boolean: self.temperatures.boolean,
                choice: self.temperatures.choice,
                score: self.temperatures.score,
            },
        }
    }

    /// Validate, tokenize, score, and shape the answers.
    pub fn evaluate(&self, payload: &Value) -> Result<DecisionOutcome> {
        let batch = self.score_internal(payload)?;
        let mut index = 0usize;
        let mut results = Vec::with_capacity(batch.prepared.len());
        for request in &batch.prepared {
            let mut answers = Vec::with_capacity(request.questions.len());
            for question in &request.questions {
                let scored = &batch.questions[index];
                answers.push(to_dto(contract::answer(question, &scored.probabilities)?));
                index += 1;
            }
            results.push(RequestAnswers {
                id: request.id.clone(),
                answers,
            });
        }
        Ok(DecisionOutcome {
            api_version: API_VERSION.to_string(),
            model: MODEL_ID.to_string(),
            results,
            usage: batch.usage,
        })
    }

    fn score_internal(&self, payload: &Value) -> Result<ScoredBatch> {
        let prepared = contract::prepare(payload)?;
        let started = Instant::now();
        let paths = self.tokenize(&prepared)?;

        let mut questions = Vec::new();
        let mut candidate_paths = 0usize;
        let mut input_path_tokens = 0usize;
        let mut backbone_input_tokens = 0usize;

        for (request, question_paths) in prepared.iter().zip(paths.iter()) {
            for (question, paths) in request.questions.iter().zip(question_paths.iter()) {
                let prefix = &paths.prefix;
                let suffixes = &paths.suffixes;
                candidate_paths += suffixes.len();
                for suffix in suffixes {
                    input_path_tokens += prefix.len() + suffix.len();
                }

                // One prefix encode, then one branch per candidate.
                let (_, prefix_kv) = self.backbone.forward(prefix, 0, None)?;
                backbone_input_tokens += prefix.len();
                let mut vectors = Vec::with_capacity(suffixes.len());
                for suffix in suffixes {
                    let (hidden, _) =
                        self.backbone
                            .forward(suffix, prefix.len(), Some(&prefix_kv))?;
                    backbone_input_tokens += suffix.len();
                    vectors.push(hidden);
                }

                let candidates = Tensor::stack(&vectors, 0)?;
                let logits = self.head.score(&candidates)?;
                let temperature = self.temperatures.for_kind(question.kind);
                let scaled = logits.affine(1.0 / temperature as f64, 0.0)?;
                let probabilities = crate::ops::softmax_last_dim(&scaled)?;
                questions.push(ScoredQuestion {
                    probabilities: probabilities.to_dtype(DType::F32)?.to_vec1::<f32>()?,
                });
            }
        }

        let usage = DecisionUsage {
            questions: questions.len(),
            candidate_paths,
            input_path_tokens,
            backbone_input_tokens,
            shared_prefix_questions: questions.len(),
            generated_tokens: 0,
            truncated_inputs: 0,
            wall_ms: started.elapsed().as_millis() as u64,
        };

        Ok(ScoredBatch {
            prepared,
            questions,
            usage,
        })
    }

    /// Tokenize every candidate path exactly as the model sees it.
    ///
    /// The shape mirrors the reference `encode_paths`: `[STATE] …`,
    /// `\n[QUESTION] …`, `\n[CANDIDATE] …`, no special tokens, and over-length
    /// paths rejected rather than truncated.
    pub fn encode_paths(&self, payload: &Value) -> Result<Vec<EncodedPath>> {
        let prepared = contract::prepare(payload)?;
        let paths = self.tokenize(&prepared)?;
        let mut out = Vec::new();
        for (request, question_paths) in prepared.iter().zip(paths.iter()) {
            for (question, paths) in request.questions.iter().zip(question_paths.iter()) {
                for (key, suffix) in question.keys.iter().zip(paths.suffixes.iter()) {
                    let mut tokens = paths.prefix.clone();
                    tokens.extend(suffix.iter().copied());
                    out.push(EncodedPath {
                        request_id: request.id.clone(),
                        question_id: question.id.clone(),
                        key: key.clone(),
                        tokens,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Build the shared prefix and per-candidate suffixes for every question,
    /// rejecting any path that would exceed [`MAX_PATH_TOKENS`].
    fn tokenize(&self, prepared: &[PreparedRequest]) -> Result<Vec<Vec<QuestionPaths>>> {
        let mut requests = Vec::with_capacity(prepared.len());
        for request in prepared {
            let state_ids = self.encode_state(&request.state)?;
            let mut questions = Vec::with_capacity(request.questions.len());
            for question in &request.questions {
                let prefix = self.encode_question_prefix(&state_ids, question)?;
                let suffixes = self.encode_suffixes(question, &prefix)?;
                questions.push(QuestionPaths { prefix, suffixes });
            }
            requests.push(questions);
        }
        Ok(requests)
    }

    fn encode_state(&self, state: &str) -> Result<Vec<u32>> {
        let text = if state.starts_with("[STATE]") {
            state.to_string()
        } else {
            format!("[STATE] {state}")
        };
        self.encode(&text)
    }

    fn encode_question_prefix(&self, state_ids: &[u32], question: &PreparedQuestion) -> Result<Vec<u32>> {
        let mut ids = state_ids.to_vec();
        ids.extend(self.encode(&format!("\n[QUESTION] {}", question.text))?);
        Ok(ids)
    }

    fn encode_suffixes(
        &self,
        question: &PreparedQuestion,
        prefix: &[u32],
    ) -> Result<Vec<Vec<u32>>> {
        let mut suffixes = Vec::with_capacity(question.candidates.len());
        for candidate in &question.candidates {
            let suffix = self.encode(&format!("\n[CANDIDATE] {candidate}"))?;
            let total = prefix.len() + suffix.len();
            if total > MAX_PATH_TOKENS {
                return Err(DecisionError::contract(format!(
                    "question {} needs {total} tokens; limit {MAX_PATH_TOKENS}. Shorten the input; nothing was truncated.",
                    python_repr(&question.id)
                )));
            }
            suffixes.push(suffix);
        }
        Ok(suffixes)
    }

    fn encode(&self, text: &str) -> Result<Vec<u32>> {
        self.tokenizer
            .encode(text, false)
            .map(|encoding| encoding.get_ids().to_vec())
            .map_err(|error| DecisionError::Inference(format!("tokenizer: {error}")))
    }
}

fn to_dto(answer: DecisionAnswer) -> AnswerDto {
    match answer {
        DecisionAnswer::Boolean {
            id,
            probability,
            value,
            distribution,
        } => AnswerDto {
            id,
            kind: contract::QuestionKind::Boolean,
            distribution: entries(distribution),
            selected_key: if value { "true" } else { "false" }.to_string(),
            selected_description: if value { "TRUE" } else { "FALSE" }.to_string(),
            top_probability: probability.max(1.0 - probability),
            margin: (2.0 * probability - 1.0).abs(),
            probability_true: Some(probability),
            score: None,
            level: None,
            level_descriptions: Vec::new(),
        },
        DecisionAnswer::Choice {
            id,
            value,
            description,
            top_probability,
            margin,
            distribution,
        } => AnswerDto {
            id,
            kind: contract::QuestionKind::Choice,
            distribution: entries(distribution),
            selected_key: value,
            selected_description: description,
            top_probability,
            margin,
            probability_true: None,
            score: None,
            level: None,
            level_descriptions: Vec::new(),
        },
        DecisionAnswer::Score {
            id,
            score,
            level,
            legend,
            distribution,
        } => {
            let entries = entries(distribution);
            let top_probability = distribution_top(&entries);
            AnswerDto {
                id,
                kind: contract::QuestionKind::Score,
                distribution: entries,
                selected_key: level.to_string(),
                selected_description: legend.get(level).cloned().unwrap_or_default(),
                top_probability,
                margin: 0.0,
                probability_true: None,
                score: Some(score),
                level: Some(level),
                level_descriptions: legend,
            }
        }
    }
}

fn entries(distribution: Vec<(String, f32)>) -> Vec<DistributionEntry> {
    distribution
        .into_iter()
        .map(|(key, probability)| DistributionEntry { key, probability })
        .collect()
}

fn distribution_top(entries: &[DistributionEntry]) -> f32 {
    entries
        .iter()
        .map(|entry| entry.probability)
        .fold(f32::MIN, f32::max)
}

/// Resolve the configured device/dtype pair, falling back to CPU + f32.
fn resolve_device_dtype(options: EngineOptions) -> Result<(Device, DType, &'static str)> {
    let device = match options.device {
        DeviceChoice::Cpu => Device::Cpu,
        DeviceChoice::Metal => metal_device()?,
        DeviceChoice::Auto => metal_device().unwrap_or(Device::Cpu),
    };
    let is_metal = matches!(device, Device::Metal(_));
    let dtype = match options.dtype {
        // f32 rather than f16 on Metal: candle 0.11's Metal backend has no
        // half-precision kernel for every op this model needs
        // (`softmax-last-dim` among them), and a fallback would silently move
        // work back to the CPU per op. f32 is the accurate, predictable choice;
        // f16 stays available explicitly for callers who measure it.
        DTypeChoice::Auto => DType::F32,
        DTypeChoice::F32 => DType::F32,
        DTypeChoice::F16 => DType::F16,
        DTypeChoice::Bf16 => DType::BF16,
    };
    if !is_metal && dtype != DType::F32 {
        // f16/bf16 on the CPU backend is emulated and slower than f32; the
        // caller asked for it explicitly, so honor it, but the label says cpu.
        return Ok((device, dtype, "cpu"));
    }
    Ok((device, dtype, if is_metal { "metal" } else { "cpu" }))
}

#[cfg(target_os = "macos")]
fn metal_device() -> Result<Device> {
    Device::new_metal(0).map_err(|error| DecisionError::Inference(error.to_string()))
}

#[cfg(not(target_os = "macos"))]
fn metal_device() -> Result<Device> {
    Err(DecisionError::Inference(
        "Metal is only available on macOS".to_string(),
    ))
}

fn dtype_label(dtype: DType) -> String {
    match dtype {
        DType::F32 => "f32".to_string(),
        DType::F16 => "f16".to_string(),
        DType::BF16 => "bf16".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// Python's `repr()` quote choice for a string: single quotes, unless the text
/// contains a single quote and no double quote. The reference implementation
/// builds its over-length message with `{id!r}`, and that message is shown to
/// users verbatim.
fn python_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    format!("{quote}{value}{quote}")
}
