//! Cross-domain projections for the Models workbench.
//!
//! The seam lives here rather than in `skillstar-models` because a DTO is a
//! frontend contract, and per D-034 domain types with their own refactoring
//! rhythm must not be the thing the frontend is pinned to.

pub mod account_book;
pub mod agents;
pub mod board;
mod codex_save;
pub mod dto;
mod gateway_save;
pub mod groups;
pub mod listen;
pub mod names;
pub mod picker;
pub mod profiles;
pub mod recent;
pub mod routing;

pub use account_book::UsageAccountBook;
pub use codex_save::{CodexRoute, release_codex, save_codex};
pub use gateway_save::save_agent;
pub use groups::{SavedGroupDto, SaveGroupControlError, load_saved_groups, save_group_members};
pub use listen::{SaveListenControlError, load_listen_mode, save_listen_mode};
pub use names::{SaveModelNameControlError, save_model_name};
pub use picker::{ModelChoiceDto, load_model_choices};
pub use profiles::{
    ApplyProfileControlError, ProfileAgentDto, ProfileApplyDto, SaveProfileControlError,
    apply_saved_profile, load_profile_names, save_profile_agents,
};
pub use recent::{RecentCallDto, load_recent_calls};
pub use routing::{
    RoutingControl, RoutingGroupControl, RoutingPage, load_routing_page, save_routing_control,
};
