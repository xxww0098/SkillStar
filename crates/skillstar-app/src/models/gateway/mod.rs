//! Gateway lens projections for the Models workbench.
//!
//! One lens or write entry per file, each a thin adapter over the
//! `skillstar-gateway` pub face: `routing`, `groups`, `profiles`, `names`,
//! `listen`, `effort`, `picker`, `recent` (the row shape), `ledger` (the
//! merged ledger + ring view behind it), `gateway_save`, `codex_save`, and
//! `account_book` (the gateway's `AccountBook` trait implemented over the
//! usage storage, so it lives with the lenses it serves).
//!
//! ## Projection rules
//!
//! - Projection files must not import each other. The only module paths a
//!   projection may use are this module's re-exports (`use super::…`) and
//!   `crate::test_support`; sibling paths such as `super::codex_save::…` are
//!   forbidden.
//! - `models::board` consumes this module's pub face; the reverse direction
//!   is forbidden — nothing here may reach `board`, `agents`, or `dto`.

pub mod account_book;
mod codex_save;
pub mod effort;
mod gateway_save;
pub mod groups;
pub mod ledger;
pub mod listen;
pub mod names;
pub mod picker;
pub mod profiles;
pub mod recent;
pub mod routing;

pub use account_book::UsageAccountBook;
pub use codex_save::{CodexRoute, release_codex, save_codex, save_codex_model};
pub use effort::model_efforts;
pub use gateway_save::save_agent;
pub use groups::{SavedGroupDto, SaveGroupControlError, load_saved_groups, save_group_members};
pub use ledger::{LedgerQuery, PAGE_KEEP, load_ledger_page};
pub use listen::{SaveListenControlError, load_listen_mode, loopback_origin, save_listen_mode};
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
