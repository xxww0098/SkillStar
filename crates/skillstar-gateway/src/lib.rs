//! Local model gateway. This crate owns protocol translation, the stream hold,
//! one smart ordering, and the loopback listener. It does not own provider
//! keys, usage accounts, or the decision model.

mod hold;
mod route;
mod serve;
mod translate;

pub use hold::{HOLD_LONGEST, HOLD_MOST, HoldWriter};
pub use route::{AllowanceSnapshot, RouteCandidate, USED_SHARE, route_smart};
pub use serve::{
    ADDR_ENV, DEFAULT_ADDR, HEADER_READ_TIMEOUT, IDLE_TIMEOUT, PLACEHOLDER_BEARER, REFUSED_PORT,
    ServeError, ServeOptions, Stop, resolve_addr, serve,
};
pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
