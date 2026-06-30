pub mod agents;
pub mod ai;
pub mod fingerprints;
pub mod github;
pub mod mcp_commands;
pub mod models_commands;
pub mod patrol;
pub mod projects;
pub mod updater;
pub mod usage_commands;
pub mod usage_dto;
pub mod usage_windows;

pub use skillstar_app::commands::acp;
pub use skillstar_app::commands::network::*;
pub use skillstar_app::commands::shell_rc::*;

pub mod marketplace {
    pub use skillstar_app::commands::marketplace::*;
}

pub mod mcp_marketplace {
    pub use skillstar_app::commands::mcp_marketplace::*;
}

mod adopt_folder;
mod bundles;
mod deploy_mode;
mod share_install;
mod skill_content;
mod skill_groups;
mod skill_paths;
mod skills;
mod s3_sync;
mod ssh_hosts;

pub use adopt_folder::*;
pub use bundles::*;
pub use deploy_mode::*;
pub use share_install::*;
pub use skill_content::*;
pub use skill_groups::*;
pub use skills::*;
pub use s3_sync::*;
pub use ssh_hosts::*;
pub use skillstar_app::commands::shell::*;
