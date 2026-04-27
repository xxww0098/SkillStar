//! Launch Deck & Terminal compatibility shim.
//!
//! Pure config/types have moved to `skillstar-terminal` crate.
//! This module re-exports them for backward compatibility while
//! platform-specific launch logic remains here.

pub use skillstar_terminal::config::{LaunchConfig, LaunchMode, LayoutNode, SplitDirection};
pub use skillstar_terminal::{collect_leaf_panes, count_panes, deployable_layout};

/// Terminal backend: deploy, script generation, CLI detection.
#[allow(unused_imports)]
pub use super::terminal_backend;
