//! Canonical provider identity and balance-endpoint metadata.
//!
//! Historically this information was duplicated across three places that drifted
//! apart: the usage-side `catalog` (subscription accounts), the models-side
//! `get_all_presets_flat` (API endpoints), and the per-fetcher `const ENDPOINT`
//! strings. This module centralizes the shared, behaviour-bearing facts so the
//! other layers can derive from one table instead of re-declaring them.
//!
//! It is intentionally free of third-party deps and of any product-domain crate
//! — usage and models both sit above `skillstar-core`, and must not depend on
//! each other (D-004 / D-049).

pub mod balance;
pub mod identity;
