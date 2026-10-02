//! The persistent usage ledger: one JSONL line per finished turn under
//! `data_root()/gateway/`. The record shape is fixed once (see `record`);
//! appending never breaks a turn, and reading keeps complete lines only.

mod append;
mod record;

pub use append::{append, load};
pub(crate) use record::account_of;
pub use record::{ErrorKind, Record, TokenCounts, key_fingerprint};
