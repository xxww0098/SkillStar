//! Application-facing Usage facade.
//!
//! Owns frontend-safe DTO projection and cross-domain subscription use cases.
//! Framework adapters add only window/event behavior around this interface.

mod dto;
mod service;

pub use dto::*;
pub use service::*;
