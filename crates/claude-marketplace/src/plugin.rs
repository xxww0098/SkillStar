use serde::{Deserialize, Serialize};

/// schemastore 上该格式的官方 schema 地址。
pub const PLUGIN_SCHEMA_URL: &str = "https://json.schemastore.org/claude-code-plugin.json";

/// 插件目录内 `.claude-plugin/plugin.json`。生成只写 `name` / `description` /
/// `version` 最小集；解析保留常见可选字段，未知字段忽略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    #[serde(
        rename = "$schema",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub schema: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<crate::marketplace::PluginOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl PluginManifest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: None,
            name: name.into(),
            description: None,
            version: None,
            author: None,
            strict: None,
        }
    }
}
