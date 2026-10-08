//! Claude Code 插件市场外部格式的产品无关协议叶子（D-102）。
//!
//! 只拥有 `.claude-plugin/marketplace.json` 与插件 `.claude-plugin/plugin.json`
//! 的 schema、命名与路径校验，以及自包含市场目录写出。不依赖任何 `ss-*` crate；
//! 频道注册表读取、baseline 校验与技能内容物化留在 `ss-skills::channels`，入口
//! 在 `ss-app`。生成只覆盖相对路径源形态；远程源形态仅用于解析外部清单。

pub mod layout;
pub mod marketplace;
pub mod plugin;
pub mod validate;

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod marketplace_tests;
#[cfg(test)]
mod validate_tests;

pub use layout::{PluginPayload, SkillDir, WrittenMarketplace, write_marketplace};
pub use marketplace::{
    MarketplaceManifest, MarketplaceMetadata, PluginEntry, PluginOwner, PluginSource,
    RemoteSource, MARKETPLACE_SCHEMA_URL,
};
pub use plugin::PluginManifest;
pub use validate::{
    validate_entry_name, validate_marketplace, validate_marketplace_name,
    validate_relative_source, MarketplaceError,
};
