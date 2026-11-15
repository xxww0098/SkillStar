//! Claude Code 插件目录格式（D-104）。
//!
//! 只拥有 `.claude-plugin/marketplace.json` 与插件 `.claude-plugin/plugin.json`
//! 的 schema、命名与路径校验，以及自包含目录写出。频道注册表读取与 baseline
//! 校验在同级的 `marketplace_export`。生成只覆盖相对路径源；远程源只解析。

mod layout;
mod marketplace;
mod plugin;
mod validate;

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod marketplace_tests;
#[cfg(test)]
mod validate_tests;

pub use layout::WrittenMarketplace;
pub(super) use layout::{PluginPayload, SkillDir, write_marketplace};
pub(super) use marketplace::MARKETPLACE_SCHEMA_URL;
pub use marketplace::{
    MarketplaceManifest, MarketplaceMetadata, PluginEntry, PluginOwner, PluginSource, RemoteSource,
};
pub use plugin::PluginManifest;
pub(super) use validate::MarketplaceError;
