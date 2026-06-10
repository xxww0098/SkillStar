//! OAuth infrastructure primitives shared by all OAuth fetchers.
//!
//! Each submodule is a building block; the per-provider `fetchers/oauth/*.rs`
//! compose them.

pub mod device_flow;
pub mod local_server;
pub mod manual_callback;
pub mod pending_state;
pub mod pkce;
pub mod poll_flow;
pub mod token_refresh;
