//! Cross-domain projections for the Models workbench.
//!
//! The seam lives here rather than in `skillstar-models` because a DTO is a
//! frontend contract, and per D-034 domain types with their own refactoring
//! rhythm must not be the thing the frontend is pinned to.

pub mod account_book;
pub mod agents;
pub mod board;
mod codex_save;
pub mod dto;
mod gateway_save;
pub mod picker;

pub use account_book::UsageAccountBook;
pub use codex_save::{CodexRoute, release_codex, save_codex};
pub use gateway_save::save_agent;
pub use picker::{ModelChoiceDto, load_model_choices};
