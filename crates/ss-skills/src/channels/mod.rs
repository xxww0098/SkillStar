//! Shared publication, subscription lifecycle and patrol within the skills domain.
//!
//! Uses skill lifecycle primitives through operation-level scanner/installer
//! seams; generic mutations are protected by the domain-default channel policy.

pub mod patrol;
pub(crate) mod policy;
pub mod shared_channels;
