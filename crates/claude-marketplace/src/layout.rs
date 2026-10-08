use std::fs;
use std::path::{Path, PathBuf};

use crate::marketplace::{MarketplaceManifest, PluginSource};
use crate::plugin::PluginManifest;
use crate::validate::{self, MarketplaceError};

/// 复制边界与技能域内容快照的默认上限对齐：单技能 2048 个文件、单文件 8 MiB、
/// 总量 32 MiB、目录深度 64。写出侧独立执行同一套界限，不信任来源目录。
pub const COPY_MAX_FILES: usize = 2_048;
pub const COPY_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const COPY_MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
pub const COPY_MAX_DEPTH: usize = 64;

/// 一个待物化的技能：内容根目录被复制到 `<插件目录>/skills/<name>/`。
#[derive(Debug, Clone)]
pub struct SkillDir<'a> {
    pub name: &'a str,
    pub root: &'a Path,
}

impl<'a> SkillDir<'a> {
    pub fn new(name: &'a str, root: &'a Path) -> Self {
        Self { name, root }
    }
}

/// 一个待物化的插件：条目（source 必须是相对路径）、生成的 `plugin.json`（name
/// 必须与条目名一致）和至少零个技能目录。
#[derive(Debug, Clone)]
pub struct PluginPayload<'a> {
    pub entry: crate::marketplace::PluginEntry,
    pub manifest: PluginManifest,
    pub skills: Vec<SkillDir<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenMarketplace {
    pub root: PathBuf,
    pub plugins: Vec<String>,
    pub files: u64,
}

/// 写出自包含市场目录：根 `.claude-plugin/marketplace.json`，每个插件在自身目录内
/// 生成 `.claude-plugin/plugin.json` 并复制技能内容。`out_dir` 必须不存在或为空；
/// 全部内容先写进同级暂存目录，成功后一次 rename 落位，失败清理暂存残留。
pub fn write_marketplace(
    out_dir: &Path,
    manifest: &MarketplaceManifest,
    plugins: &[PluginPayload<'_>],
) -> Result<WrittenMarketplace, MarketplaceError> {
    validate::validate_marketplace(manifest)?;
    validate_payloads(plugins)?;

    let parent = out_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| MarketplaceError::InvalidSourcePath {
            value: out_dir.display().to_string(),
            reason: "output directory must have a parent directory",
        })?;
    fs::create_dir_all(parent)?;
    if out_dir.is_dir() && !dir_is_empty(out_dir)? {
        return Err(MarketplaceError::OutputNotEmpty(
            out_dir.display().to_string(),
        ));
    }
    if out_dir.exists() && !out_dir.is_dir() {
        return Err(MarketplaceError::OutputNotEmpty(
            out_dir.display().to_string(),
        ));
    }

    let file_name = out_dir
        .file_name()
        .ok_or_else(|| MarketplaceError::InvalidSourcePath {
            value: out_dir.display().to_string(),
            reason: "output directory must have a file name",
        })?
        .to_string_lossy()
        .to_string();
    let staging = parent.join(format!(".{file_name}.claude-marketplace-staging"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    match build_tree(&staging, manifest, plugins) {
        Ok(mut written) => {
            if out_dir.is_dir() {
                fs::remove_dir(out_dir)?;
            }
            fs::rename(&staging, out_dir)?;
            written.root = out_dir.to_path_buf();
            Ok(written)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn validate_payloads(plugins: &[PluginPayload<'_>]) -> Result<(), MarketplaceError> {
    let mut source_dirs: Vec<Vec<String>> = Vec::new();
    for payload in plugins {
        if payload.manifest.name != payload.entry.name {
            return Err(MarketplaceError::EntryNameMismatch {
                entry: payload.entry.name.clone(),
                manifest: payload.manifest.name.clone(),
            });
        }
        let PluginSource::Relative(source) = &payload.entry.source else {
            return Err(MarketplaceError::NotRelativeSource(
                payload.entry.source.describe(),
            ));
        };
        let components = validate::relative_components(source)?;
        if components.first().is_some_and(|first| first == ".claude-plugin") {
            return Err(MarketplaceError::InvalidSourcePath {
                value: source.clone(),
                reason: ".claude-plugin at the marketplace root is reserved",
            });
        }
        let mut skills = std::collections::BTreeSet::new();
        for skill in &payload.skills {
            validate::validate_entry_name(skill.name)?;
            if !skills.insert(skill.name.to_string()) {
                return Err(MarketplaceError::DuplicateSkillDir(skill.name.to_string()));
            }
        }
        for previous in &source_dirs {
            if is_prefix(previous, &components) || is_prefix(&components, previous) {
                return Err(MarketplaceError::OverlappingSourcePaths(
                    source.clone(),
                    previous.join("/"),
                ));
            }
        }
        source_dirs.push(components);
    }
    Ok(())
}

fn is_prefix(candidate: &[String], of: &[String]) -> bool {
    candidate.len() <= of.len() && of[..candidate.len()] == *candidate
}

fn dir_is_empty(dir: &Path) -> Result<bool, MarketplaceError> {
    Ok(fs::read_dir(dir)?.next().is_none())
}

fn build_tree(
    staging: &Path,
    manifest: &MarketplaceManifest,
    plugins: &[PluginPayload<'_>],
) -> Result<WrittenMarketplace, MarketplaceError> {
    let mut files = 0_u64;
    let manifest_dir = staging.join(".claude-plugin");
    fs::create_dir_all(&manifest_dir)?;
    fs::write(
        manifest_dir.join("marketplace.json"),
        serde_json::to_string_pretty(manifest)? + "\n",
    )?;
    files += 1;

    let mut written = Vec::new();
    for payload in plugins {
        let source = payload
            .entry
            .source
            .as_relative()
            .ok_or_else(|| MarketplaceError::NotRelativeSource(payload.entry.source.describe()))?;
        let plugin_root = staging.join(source.strip_prefix("./").unwrap_or(source));
        let plugin_manifest_dir = plugin_root.join(".claude-plugin");
        fs::create_dir_all(&plugin_manifest_dir)?;
        fs::write(
            plugin_manifest_dir.join("plugin.json"),
            serde_json::to_string_pretty(&payload.manifest)? + "\n",
        )?;
        files += 1;

        for skill in &payload.skills {
            let dest = plugin_root.join("skills").join(skill.name);
            files += copy_tree(skill.root, &dest)?;
        }
        written.push(payload.entry.name.clone());
    }
    Ok(WrittenMarketplace {
        root: staging.to_path_buf(),
        plugins: written,
        files,
    })
}

struct CopyBudget {
    files: u64,
    bytes: u64,
}

fn copy_tree(src: &Path, dest: &Path) -> Result<u64, MarketplaceError> {
    let mut budget = CopyBudget { files: 0, bytes: 0 };
    copy_dir(src, dest, 0, &mut budget)?;
    Ok(budget.files)
}

fn copy_dir(
    src: &Path,
    dest: &Path,
    depth: usize,
    budget: &mut CopyBudget,
) -> Result<(), MarketplaceError> {
    let limit = |reason: &'static str| MarketplaceError::CopyLimit {
        path: src.display().to_string(),
        reason,
    };
    if depth > COPY_MAX_DEPTH {
        return Err(limit("directory depth exceeds 64"));
    }
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(MarketplaceError::UnsupportedSymlink(
                path.display().to_string(),
            ));
        }
        let child_dest = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&path, &child_dest, depth + 1, budget)?;
        } else if file_type.is_file() {
            let size = entry.metadata()?.len();
            if size > COPY_MAX_FILE_BYTES {
                return Err(limit("single file exceeds 8 MiB"));
            }
            budget.files += 1;
            if budget.files as usize > COPY_MAX_FILES {
                return Err(limit("more than 2048 files in one skill"));
            }
            budget.bytes += size;
            if budget.bytes > COPY_MAX_TOTAL_BYTES {
                return Err(limit("skill content exceeds 32 MiB in total"));
            }
            fs::copy(&path, &child_dest)?;
        } else {
            return Err(limit("not a regular file or directory"));
        }
    }
    Ok(())
}
