//! Cross-cutting infrastructure: paths, filesystem operations, error types,
//! database connection pool, and shared utilities.
//!
//! All domain modules depend on `infra` — never the reverse.

pub mod db_pool;
pub mod error;
pub mod fs_ops;
pub mod migration;
pub mod paths;
pub mod util;
