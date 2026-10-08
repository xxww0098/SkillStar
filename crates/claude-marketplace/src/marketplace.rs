use serde::{Deserialize, Serialize};

/// schemastore 上该格式的官方 schema 地址（官方仓库自带，生成时默认写入）。
pub const MARKETPLACE_SCHEMA_URL: &str =
    "https://json.schemastore.org/claude-code-marketplace.json";

/// `marketplace.json` 根对象。解析同时接受官方顶层 `version` / `description` 形态与
/// 上游社区常用的 `metadata` 嵌套形态；本 crate 生成只写官方顶层形态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketplaceManifest {
    #[serde(
        rename = "$schema",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub schema: Option<String>,
    pub name: String,
    pub owner: PluginOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MarketplaceMetadata>,
    pub plugins: Vec<PluginEntry>,
}

impl MarketplaceManifest {
    pub fn new(name: impl Into<String>, owner: PluginOwner) -> Self {
        Self {
            schema: None,
            name: name.into(),
            owner,
            description: None,
            version: None,
            metadata: None,
            plugins: Vec::new(),
        }
    }
}

/// `metadata` 嵌套形态（社区市场常用）；解析保留，生成不写。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOwner {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

impl PluginOwner {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<PluginOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    pub source: PluginSource,
}

impl PluginEntry {
    /// 构造相对路径源条目——本 crate 生成路径唯一支持的来源形态。
    pub fn relative(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            version: None,
            author: None,
            category: None,
            strict: None,
            source: PluginSource::Relative(source.into()),
        }
    }
}

/// 插件来源。相对路径写成纯字符串（官方形态），远程来源是带 `source` 判别字段的
/// 对象；解析对未知判别值回退到 [`PluginSource::Other`]，不整体失败。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginSource {
    Relative(String),
    Remote(RemoteSource),
    Other(serde_json::Value),
}

impl PluginSource {
    pub fn as_relative(&self) -> Option<&str> {
        match self {
            Self::Relative(value) => Some(value),
            _ => None,
        }
    }

    /// 给错误信息用的稳定渲染。
    pub fn describe(&self) -> String {
        match self {
            Self::Relative(value) => value.clone(),
            Self::Remote(source) => match source {
                RemoteSource::Github { repo, .. } => format!("github:{repo}"),
                RemoteSource::GitSubdir { url, path, .. } => format!("git-subdir:{url}#{path}"),
                RemoteSource::Url { url, .. } => format!("url:{url}"),
                RemoteSource::Archive { url } => format!("archive:{url}"),
                RemoteSource::Npm { package } => format!("npm:{package}"),
                RemoteSource::Command { command } => format!("command:{command}"),
            },
            Self::Other(value) => value.to_string(),
        }
    }
}

/// 远程来源形态；`ref` 字段允许 pin 到分支/tag/commit。仅解析，不生成。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "kebab-case")]
pub enum RemoteSource {
    Github {
        repo: String,
        #[serde(
            rename = "ref",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        git_ref: Option<String>,
    },
    GitSubdir {
        url: String,
        path: String,
        #[serde(
            rename = "ref",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        git_ref: Option<String>,
    },
    Url {
        url: String,
        #[serde(
            rename = "ref",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        git_ref: Option<String>,
    },
    Archive {
        url: String,
    },
    Npm {
        package: String,
    },
    Command {
        command: String,
    },
}
