//! Package / remote / icon / status models from `server.json` `2025-12-11`.
//!
//! The pre-existing summary fields (`runtime`, `identifier`, `version`,
//! `required_env`, `transport`, `url`, `required_headers`) are kept verbatim so
//! cached snapshot rows and the install mapping in the app layer keep working;
//! every new field is `#[serde(default)]`, which is what lets a row written by
//! the old model deserialize into the new one without a data migration.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::inputs::{McpArgument, McpKeyValueInput};
use super::raw::{arr_field, obj_field, str_field};

/// Lifecycle status published by the registry (`_meta` → `status`).
///
/// `deprecated` servers stay listed but must be flagged; `deleted` ones are
/// hidden from default listings. Both were previously unparsed, so deprecated
/// servers were offered for install exactly like healthy ones (audit A.2-c).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpServerStatus.ts")]
pub enum McpServerStatus {
    #[default]
    Active,
    Deprecated,
    Deleted,
}

impl McpServerStatus {
    /// Stable string used for DB storage (matches the camelCase serde tag).
    pub fn as_db_str(&self) -> &'static str {
        match self {
            McpServerStatus::Active => "active",
            McpServerStatus::Deprecated => "deprecated",
            McpServerStatus::Deleted => "deleted",
        }
    }

    pub fn from_db_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "deprecated" => McpServerStatus::Deprecated,
            "deleted" => McpServerStatus::Deleted,
            _ => McpServerStatus::Active,
        }
    }
}

/// One `icons[]` entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpIcon.ts")]
pub struct McpIcon {
    pub src: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sizes: Vec<String>,
    /// `light` | `dark` when the publisher ships a theme-specific icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

impl McpIcon {
    pub(crate) fn parse_list(server: &Value) -> Vec<Self> {
        arr_field(server, &["icons"])
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let src = str_field(item, &["src"])?;
                        Some(Self {
                            src,
                            mime_type: str_field(item, &["mimeType", "mime_type"]),
                            sizes: arr_field(item, &["sizes"])
                                .map(|s| {
                                    s.iter()
                                        .filter_map(|v| v.as_str().map(str::to_string))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            theme: str_field(item, &["theme"]),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A package-level transport block (`stdio` | `streamable-http` | `sse`).
/// Local packages may legitimately speak HTTP, so this is not redundant with
/// [`super::McpServerKind`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpTransportSpec.ts")]
pub struct McpTransportSpec {
    /// Raw transport tag as published: `stdio`, `streamable-http`, `sse`.
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub transport_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<McpKeyValueInput>,
}

impl McpTransportSpec {
    fn from_json(value: &Value) -> Option<Self> {
        let transport = obj_field(value, &["transport"])?;
        let transport_type = str_field(transport, &["type", "transport_type"])?;
        Some(Self {
            transport_type,
            url: str_field(transport, &["url"]),
            headers: McpKeyValueInput::parse_list(arr_field(transport, &["headers"])),
        })
    }
}

/// A runnable package. Name kept as `…Summary` because the app layer and the
/// Tauri command layer already depend on it; it is no longer a summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpRegistryPackageSummary.ts")]
pub struct McpRegistryPackageSummary {
    /// Runner command we'd use: `npx` / `uvx` / `docker` / `dnx` / …
    pub runtime: String,
    /// Package identifier (npm/pypi/oci name).
    pub identifier: String,
    pub version: Option<String>,
    /// Names of env vars the user must supply (required or secret). Derived
    /// from [`Self::environment_variables`]; kept for card-level display.
    #[serde(default)]
    pub required_env: Vec<String>,
    /// `npm` | `pypi` | `nuget` | `cargo` | `oci` | `mcpb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_base_url: Option<String>,
    /// Publisher-declared artifact hash (`^[a-f0-9]{64}$`). Mandatory for MCPB;
    /// the registry does not verify it, the client is expected to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<McpTransportSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_arguments: Vec<McpArgument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_arguments: Vec<McpArgument>,
    /// Full `Input` semantics for every environment variable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_variables: Vec<McpKeyValueInput>,
}

/// A remote endpoint. Same naming note as [`McpRegistryPackageSummary`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpRegistryRemoteSummary.ts")]
pub struct McpRegistryRemoteSummary {
    /// Normalized transport: `http` or `sse`.
    pub transport: String,
    pub url: String,
    /// Names of headers the user must supply (required or secret). Derived
    /// from [`Self::headers`]; kept for card-level display.
    #[serde(default)]
    pub required_headers: Vec<String>,
    /// Raw transport tag as published (`streamable-http` / `sse`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    /// Full `Input` semantics for every header (value template, secrecy, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<McpKeyValueInput>,
    /// `{curly_braces}` substitutions inside [`Self::url`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<McpKeyValueInput>,
}

/// Map a package's `registryType`/`registry_name`/`runtimeHint` into the runner
/// command we'd invoke for a stdio server.
pub fn runtime_command_for(registry_type: &str, runtime_hint: &str) -> String {
    if !runtime_hint.trim().is_empty() {
        return runtime_hint.trim().to_string();
    }
    match registry_type.trim().to_ascii_lowercase().as_str() {
        "npm" => "npx".to_string(),
        "pypi" | "uv" => "uvx".to_string(),
        "oci" | "docker" => "docker".to_string(),
        "nuget" => "dnx".to_string(),
        "cargo" => "cargo".to_string(),
        // MCPB bundles are downloaded, not launched by a runner.
        "mcpb" => "mcpb".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => String::new(),
    }
}

/// Normalize a remote's transport tag into `http` | `sse`.
pub(crate) fn normalize_remote_transport(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "sse" => "sse".to_string(),
        // "streamable-http", "streamable_http", "http", "" → http
        _ => "http".to_string(),
    }
}

pub(crate) fn parse_packages(server: &Value) -> Vec<McpRegistryPackageSummary> {
    let Some(arr) = arr_field(server, &["packages"]) else {
        return Vec::new();
    };
    arr.iter()
        .map(|pkg| {
            // The registry has shipped `registryType`, `registry_type` and the
            // even older `registry_name`; and both `identifier` and `name`.
            let registry_type = str_field(pkg, &["registryType", "registry_type", "registry_name"]);
            let runtime_hint = str_field(pkg, &["runtimeHint", "runtime_hint"]);
            let identifier = str_field(pkg, &["identifier", "name"]).unwrap_or_default();
            let environment_variables = McpKeyValueInput::parse_list(arr_field(
                pkg,
                &["environmentVariables", "environment_variables"],
            ));
            McpRegistryPackageSummary {
                runtime: runtime_command_for(
                    registry_type.as_deref().unwrap_or_default(),
                    runtime_hint.as_deref().unwrap_or_default(),
                ),
                identifier,
                version: str_field(pkg, &["version"]),
                required_env: environment_variables
                    .iter()
                    .filter(|env| env.input.is_required || env.input.is_secret)
                    .map(|env| env.name.clone())
                    .collect(),
                registry_type,
                registry_base_url: str_field(pkg, &["registryBaseUrl", "registry_base_url"]),
                file_sha256: str_field(pkg, &["fileSha256", "file_sha256"]),
                runtime_hint,
                transport: McpTransportSpec::from_json(pkg),
                runtime_arguments: McpArgument::parse_list(arr_field(
                    pkg,
                    &["runtimeArguments", "runtime_arguments"],
                )),
                package_arguments: McpArgument::parse_list(arr_field(
                    pkg,
                    &["packageArguments", "package_arguments"],
                )),
                environment_variables,
            }
        })
        .collect()
}

pub(crate) fn parse_remotes(server: &Value) -> Vec<McpRegistryRemoteSummary> {
    let Some(arr) = arr_field(server, &["remotes"]) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|remote| {
            let url = str_field(remote, &["url"])?;
            // `2025-12-11` uses `type`; `/v0` used `transport_type`.
            let raw_type = str_field(remote, &["type", "transport_type"]);
            let headers = McpKeyValueInput::parse_list(arr_field(remote, &["headers"]));
            let variables = obj_field(remote, &["variables"])
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .map(|(name, raw)| McpKeyValueInput {
                            name: name.clone(),
                            input: super::inputs::McpInput::from_json(raw),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(McpRegistryRemoteSummary {
                transport: normalize_remote_transport(raw_type.as_deref().unwrap_or_default()),
                url,
                required_headers: headers
                    .iter()
                    .filter(|h| h.input.is_required || h.input.is_secret)
                    .map(|h| h.name.clone())
                    .collect(),
                transport_type: raw_type,
                headers,
                variables,
            })
        })
        .collect()
}
