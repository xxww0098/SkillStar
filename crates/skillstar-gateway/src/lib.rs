//! Local model gateway. This crate owns protocol translation and the stream
//! hold. It does not own provider keys, usage accounts, or the decision model.

mod hold;
mod translate;

pub use hold::{HOLD_LONGEST, HOLD_MOST, HoldWriter};
pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
