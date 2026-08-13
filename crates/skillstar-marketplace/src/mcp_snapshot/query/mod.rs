//! `&Connection` query/write core for the MCP marketplace snapshot.
//!
//! Pure functions over a `rusqlite::Connection` (no process-global runtime),
//! so they're unit-testable without the snapshot runtime. Split by concern:
//!
//! - [`write`] — the post-sync catalog swap.
//! - [`cards`] — the one parameterized card query behind list/search/filter.
//! - [`detail`] — full-row reads (detail drawer, install draft).
//! - [`publishers`] — publisher grid aggregation.
//! - [`sync_state`] — `marketplace_sync_state` bookkeeping.

mod cards;
mod detail;
mod publishers;
mod sync_state;
mod write;

#[cfg(test)]
pub(crate) use cards::build_fts_match;
pub(crate) use cards::{load_cards, load_cards_by_publisher, query_cards, search_cards};
pub(crate) use detail::{load_curated_servers, load_full_server};
pub(crate) use publishers::load_publishers;
#[cfg(test)]
pub(crate) use sync_state::mark_success;
pub(crate) use sync_state::{
    is_fresh, mark_attempt, mark_error, mark_scope_error, mark_scope_success,
    mark_success_with_meta, read_source_states, read_sync_state, source_scope,
};
pub(crate) use write::{count_servers, replace_servers};
