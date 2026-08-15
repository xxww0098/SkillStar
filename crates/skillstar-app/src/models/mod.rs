//! Cross-domain projections for the Models workbench.
//!
//! The seam lives here rather than in `skillstar-models` because a DTO is a
//! frontend contract, and per D-034 domain types with their own refactoring
//! rhythm must not be the thing the frontend is pinned to.

pub mod dto;
