//! Cross-domain projections for the Models workbench.
//!
//! The seam lives here rather than in `skillstar-models` because a DTO is a
//! frontend contract, and per D-034 domain types with their own refactoring
//! rhythm must not be the thing the frontend is pinned to.
//!
//! The gateway lenses live in the `gateway` submodule; `board` is the
//! cross-domain models-v4 board projection and stays at this level next to
//! `agents` and `dto`. The re-export set below is the stable surface the
//! src-tauri command layer binds to: it does not change when files move
//! inside the submodule.

pub mod agents;
pub mod board;
pub mod dto;
mod gateway;

pub use gateway::{
    ApplyProfileControlError, CodexRoute, ModelChoiceDto, ProfileAgentDto, ProfileApplyDto,
    RecentCallDto, RoutingControl, RoutingGroupControl, RoutingPage, SavedGroupDto,
    SaveGroupControlError, SaveListenControlError, SaveModelNameControlError,
    SaveProfileControlError, UsageAccountBook, apply_saved_profile, load_listen_mode,
    load_model_choices, load_profile_names, load_recent_calls, load_routing_page,
    load_saved_groups, loopback_origin, model_efforts, release_codex, save_codex,
    save_codex_model, save_agent, save_group_members, save_listen_mode, save_model_name,
    save_profile_agents, save_routing_control,
};
