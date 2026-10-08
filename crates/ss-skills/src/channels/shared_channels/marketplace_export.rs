//! 频道 marketplace 导出编排（纯本地只读，D-102 的产品侧）。
//!
//! 从本地频道 registry 与订阅 registry 读取当前选择，逐 Skill 重新校验 canonical
//! 副本的内容 hash 与安装 baseline 相等（fail-closed，一次性报告全部问题），再交给
//! 协议叶子 `claude-marketplace` 写出自包含的市场目录。不访问网络、不持有技能事务
//! 锁、不修改订阅、锁或任何远端状态。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use claude_marketplace::{
    MARKETPLACE_SCHEMA_URL, MarketplaceError as LeafError, MarketplaceManifest, PluginEntry,
    PluginManifest as MarketplacePluginManifest, PluginOwner, PluginPayload, SkillDir,
};

use super::{
    ChannelSubscription, ChannelSubscriptionRegistry, DiskChannelSubscriptionRegistry,
    DiskSharedChannelRegistry, SharedChannelDescriptor, SharedChannelError, SharedChannelErrorCode,
    SharedChannelRegistry, SharedChannelStatus,
};

/// 单个待物化插件：条目、生成的 `plugin.json` 与校验通过的 canonical 快照。
#[derive(Debug)]
pub struct PreparedChannelPlugin {
    pub entry: PluginEntry,
    pub manifest: MarketplacePluginManifest,
    pub snapshot: crate::content::SkillSnapshot,
}

/// 组装完成、尚未写盘的市场：manifest 加上每个插件的完整内容。
#[derive(Debug)]
pub struct PreparedChannelMarketplace {
    pub manifest: MarketplaceManifest,
    pub plugins: Vec<PreparedChannelPlugin>,
}

/// 导出结果摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelMarketplaceExport {
    pub root: PathBuf,
    pub repository_id: u64,
    pub revision: u64,
    pub plugin_names: Vec<String>,
    pub files: u64,
}

/// 导出一个已订阅频道为 Claude Code 插件市场目录。纯本地：不登录、不联网。
pub fn export_channel_marketplace(
    repository_id: u64,
    out_dir: &Path,
) -> Result<ChannelMarketplaceExport, SharedChannelError> {
    let descriptor = DiskSharedChannelRegistry
        .list_read_only()?
        .into_iter()
        .find(|descriptor| descriptor.repository_id == repository_id)
        .ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::RepositoryNotFound,
                format!("No shared channel is bound to repository {repository_id}"),
            )
        })?;
    if descriptor.status != SharedChannelStatus::Active {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            format!(
                "Shared channel for repository {repository_id} is not active yet ({:?})",
                descriptor.status
            ),
        ));
    }

    let store = DiskChannelSubscriptionRegistry.load_mutable()?;
    let subscription = store
        .subscriptions
        .iter()
        .find(|subscription| subscription.repository_id == repository_id)
        .ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionNotFound,
                format!(
                    "Repository {repository_id} has no local subscription; subscribe and install its skills first"
                ),
            )
        })?;

    let revision = subscription.target.revision;
    let prepared = build_channel_marketplace(&descriptor, subscription)?;
    let written = write_prepared_marketplace(out_dir, &prepared)?;
    Ok(ChannelMarketplaceExport {
        root: out_dir.to_path_buf(),
        repository_id,
        revision,
        plugin_names: written.plugins,
        files: written.files,
    })
}

/// 组装市场：逐 Skill 校验 baseline、净化名字、读取 frontmatter 描述。
pub fn build_channel_marketplace(
    descriptor: &SharedChannelDescriptor,
    subscription: &ChannelSubscription,
) -> Result<PreparedChannelMarketplace, SharedChannelError> {
    if subscription.skills.is_empty() {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "The channel subscription has no installed skills to export",
        ));
    }
    let marketplace_name = sanitize_marketplace_name(&descriptor.name)?;

    let mut problems: Vec<String> = Vec::new();
    let mut plugins: Vec<PreparedChannelPlugin> = Vec::new();
    let mut used_names: BTreeMap<String, String> = BTreeMap::new();
    let version = subscription.target.revision.to_string();

    for skill in &subscription.skills {
        let snapshot = match crate::content::snapshot(&skill.id) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                problems.push(format!(
                    "'{}' is missing its canonical copy: {error}",
                    skill.id
                ));
                continue;
            }
        };
        if snapshot.content_hash != skill.baseline_hash {
            problems.push(format!(
                "'{}' was modified locally after the channel install; update or convert it before exporting",
                skill.id
            ));
            continue;
        }
        let Some(entry_name) = sanitize_plugin_name(&skill.id) else {
            problems.push(format!(
                "'{}' cannot be turned into a marketplace plugin name",
                skill.id
            ));
            continue;
        };
        if let Some(previous) = used_names.get(&entry_name) {
            problems.push(format!(
                "'{previous}' and '{}' both map to plugin name '{entry_name}'",
                skill.id
            ));
            continue;
        }
        used_names.insert(entry_name.clone(), skill.id.clone());

        let description = crate::validation::inspect_skill_frontmatter(&snapshot.root).description;
        let mut entry =
            PluginEntry::relative(entry_name.clone(), format!("./plugins/{entry_name}"));
        entry.description.clone_from(&description);
        entry.version = Some(version.clone());
        let mut manifest = MarketplacePluginManifest::new(entry_name);
        manifest.description = description;
        manifest.version = Some(version.clone());
        plugins.push(PreparedChannelPlugin {
            entry,
            manifest,
            snapshot,
        });
    }

    if !problems.is_empty() {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Integrity,
            format!(
                "Cannot export the channel marketplace: {}",
                problems.join("; ")
            ),
        ));
    }

    let mut manifest =
        MarketplaceManifest::new(marketplace_name, PluginOwner::new(descriptor.owner.clone()));
    manifest.schema = Some(MARKETPLACE_SCHEMA_URL.to_string());
    manifest.description = Some(format!(
        "Skills exported from SkillStar shared channel {}",
        descriptor.name
    ));
    manifest.version = Some(version);
    manifest.plugins = plugins.iter().map(|plugin| plugin.entry.clone()).collect();
    Ok(PreparedChannelMarketplace { manifest, plugins })
}

/// 把组装结果写盘（临时目录 → rename，目录必须不存在或为空）。
pub fn write_prepared_marketplace(
    out_dir: &Path,
    prepared: &PreparedChannelMarketplace,
) -> Result<claude_marketplace::WrittenMarketplace, SharedChannelError> {
    let payloads: Vec<PluginPayload<'_>> = prepared
        .plugins
        .iter()
        .map(|plugin| PluginPayload {
            entry: plugin.entry.clone(),
            manifest: plugin.manifest.clone(),
            skills: vec![SkillDir::new(&plugin.entry.name, &plugin.snapshot.root)],
        })
        .collect();
    claude_marketplace::write_marketplace(out_dir, &prepared.manifest, &payloads)
        .map_err(map_leaf_error)
}

fn map_leaf_error(error: LeafError) -> SharedChannelError {
    let code = match &error {
        LeafError::UnsupportedSymlink(_) | LeafError::CopyLimit { .. } => {
            SharedChannelErrorCode::Integrity
        }
        LeafError::Io(_) | LeafError::Json(_) | LeafError::OutputNotEmpty(_) => {
            SharedChannelErrorCode::Storage
        }
        _ => SharedChannelErrorCode::Protocol,
    };
    SharedChannelError::new(
        code,
        format!("Writing the marketplace directory failed: {error}"),
    )
}

fn sanitize_marketplace_name(raw: &str) -> Result<String, SharedChannelError> {
    sanitize_kebab(raw).ok_or_else(|| {
        SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            format!("Channel repository name '{raw}' cannot be turned into a marketplace name"),
        )
    })
}

fn sanitize_plugin_name(raw: &str) -> Option<String> {
    sanitize_kebab(raw)
}

/// 任意 SkillStar 技能名 → Claude Code 插件名（小写、数字、连字符，首尾字母数字，
/// 1..=64 字符）。分隔符折叠成单个连字符，首尾分隔符丢弃；无法产出合法名字时
/// 返回 `None`。
fn sanitize_kebab(raw: &str) -> Option<String> {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    (!out.is_empty() && out.chars().count() <= 64).then_some(out)
}
