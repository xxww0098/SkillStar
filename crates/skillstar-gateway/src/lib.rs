//! Local model gateway. This crate owns protocol translation and the stream
//! hold. It does not own provider keys, usage accounts, or the decision model.

mod hold;
mod route;
mod translate;

pub use hold::{HOLD_LONGEST, HOLD_MOST, HoldWriter};
pub use route::{AllowanceSnapshot, RouteCandidate, USED_SHARE, route_smart};
pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
