//! Remote fetchers for the skills.sh marketplace (search, leaderboard,
//! publishers, publisher repos, skill details, AI keyword search).
//!
//! Split from the original single `remote.rs` into cohesive submodules; the
//! shared HTTP client + build constants live here and are reached by the
//! submodules via `use super::*`. Public items are re-exported so external
//! callers keep using `remote::NAME`.

use std::time::Duration;

use anyhow::{Context, Result};

/// Build-time User-Agent string derived from Cargo.toml version.
const USER_AGENT: &str = concat!("SkillStar/", env!("CARGO_PKG_VERSION"));
const MARKETPLACE_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared HTTP client for marketplace requests, rebuilt when the app proxy changes.
fn marketplace_client() -> Result<reqwest::Client> {
    skillstar_core::infra::http_client::probe_http_client(MARKETPLACE_TIMEOUT)
        .context("Failed to build marketplace HTTP client")
}

mod leaderboard;
mod publisher_repos;
mod publishers;
mod search;
mod skill_details;

pub use leaderboard::*;
pub use publisher_repos::*;
pub use publishers::*;
pub use search::*;
pub use skill_details::*;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod ai_search_tests;
