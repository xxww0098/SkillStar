//! Local model gateway. This crate owns protocol translation. It does not own
//! provider keys, usage accounts, or the decision model.

mod translate;

pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
