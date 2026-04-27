//! Cross-cutting infrastructure: paths, filesystem operations, error types,
//! database connection pool, and shared utilities.
//!
//! All domain modules depend on `infra` — never the reverse.

pub mod paths { pub use skillstar_infra::paths::*; }
pub mod error { pub use skillstar_infra::error::*; }
pub mod util { pub use skillstar_infra::util::*; }
pub mod db_pool { pub use skillstar_infra::db_pool::*; }
pub mod fs_ops { pub use skillstar_infra::fs_ops::*; }
pub mod migration { pub use skillstar_infra::migration::*; }
pub mod daily_log { pub use skillstar_infra::daily_log::*; }

pub use daily_log::*;
pub use db_pool::*;
pub use error::*;
pub use fs_ops::*;
pub use migration::*;
pub use paths::*;
pub use util::*;
