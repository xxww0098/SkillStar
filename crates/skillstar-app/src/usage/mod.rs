//! Application-facing Usage facade.
//!
//! Owns frontend-safe DTO projection and cross-domain subscription use cases.
//! Framework adapters add only window/event behavior around this interface.

mod dto;
mod service;
mod token_import;

pub use dto::*;
pub use service::*;
pub use token_import::import_subscription_token;
