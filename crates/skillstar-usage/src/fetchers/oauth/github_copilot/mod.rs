//! GitHub Copilot quota, browser login, and pasted-token import.
//!
//! The stored credential is the long-lived GitHub token. The Copilot session
//! token from `copilot_internal/v2/token` is fetched on each refresh and dropped.

mod import;
mod login;
mod quota;

pub(crate) use import::import_from_token;
pub(crate) use login::{normalize_callback_input, start_login};
pub(crate) use quota::fetch;
