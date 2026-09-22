//! Local AgentJev-0.6B decision model.
//!
//! A "System One" model: you hand it an unstructured state (a diff, a trace, a
//! ticket) plus questions you already phrased, and one forward pass returns a
//! calibrated probability for every option. It decodes **zero** output tokens,
//! so there is no JSON to repair and no prose to parse.
//!
//! This crate owns everything below the app seam:
//!
//! * [`ModelPaths`] / [`download`] — where the 1.2 GB checkpoint lives and how
//!   it is fetched, resumed, and verified.
//! * [`DecisionEngine`] — the candle runtime: a Qwen3-0.6B trunk plus the
//!   permutation-equivariant candidate head, with the `[STATE] [QUESTION]
//!   [CANDIDATE]` prefix encoded once per question and every candidate scored
//!   as a branch off it.
//! * [`prepare`] / [`answer`] — the `agentjev.decision.v1` request/answer
//!   contract, ported from the reference implementation message for message.
//!
//! The runtime is deliberately **not** a chatbot backend: it cannot write.
//! Put it where an agent loop needs a gate, a route, or a score, and leave the
//! prose to a model that generates text.

mod backbone;
mod contract;
mod engine;
mod error;
mod head;
mod model_files;
mod ops;

pub use contract::{
    answer, prepare, DecisionAnswer, PreparedQuestion, PreparedRequest, QuestionKind, API_VERSION,
    MAX_CHOICE_CANDIDATES, MAX_PATH_TOKENS, MAX_PATHS, MAX_QUESTIONS, MAX_REQUESTS,
};
pub use engine::{
    AnswerDto, DecisionEngine, DecisionOutcome, DecisionUsage, DeviceChoice, DistributionEntry,
    DTypeChoice, EncodedPath, EngineInfo, EngineOptions, RequestAnswers, TemperatureReport,
    Temperatures,
};
pub use error::{DecisionError, Result};
pub use model_files::{
    download, download_with_shared_client, endpoint, file_url, sha256_file, total_bytes, DownloadProgress, ModelFileStatus,
    ModelPaths, ModelState, ModelStatus, MODEL_FILES, MODEL_ID, MODEL_REPO, MODEL_REVISION,
};
