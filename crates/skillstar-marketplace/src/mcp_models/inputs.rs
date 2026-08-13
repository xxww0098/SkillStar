//! `Input` semantics from the `server.json` `2025-12-11` schema.
//!
//! This is the data an install wizard needs: for every environment variable,
//! header and CLI argument, whether it is required, whether it is a secret,
//! what it means, what it defaults to, and which values are legal. The old
//! parser kept only the *names* of required/secret variables, so the form had
//! no way to tell "required" from "secret" or to render a select box — see
//! `docs/others/mcp-current-state-audit.md` §A.2-a/A.2-b.
//!
//! Everything is `#[serde(default)]` so rows serialized by the previous
//! (name-only) model still deserialize out of the snapshot's JSON columns.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::raw::{arr_field, bool_field, obj_field, scalar_field, scalar_to_string, str_field};

/// How a value should be collected from the user (`Input.format`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInputFormat.ts")]
pub enum McpInputFormat {
    #[default]
    String,
    Number,
    Boolean,
    /// A path on the user's filesystem — the UI should offer a file picker.
    Filepath,
}

impl McpInputFormat {
    pub fn from_wire(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "number" => McpInputFormat::Number,
            "boolean" => McpInputFormat::Boolean,
            "filepath" => McpInputFormat::Filepath,
            _ => McpInputFormat::String,
        }
    }
}

/// A `{curly_braces}` substitution inside an `Input.value` template.
///
/// The schema types these as full `Input`s (so they nest arbitrarily), but a
/// second level of nesting has never appeared in practice and a recursive
/// type would export as a self-referential TypeScript alias. One level, with
/// the same form-relevant fields, is the deliberate cut.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInputVariable.ts")]
pub struct McpInputVariable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default)]
    pub format: McpInputFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

impl McpInputVariable {
    fn from_json(value: &Value) -> Self {
        Self {
            description: str_field(value, &["description"]),
            is_required: bool_field(value, &["isRequired", "is_required"]).unwrap_or(false),
            is_secret: bool_field(value, &["isSecret", "is_secret"]).unwrap_or(false),
            format: str_field(value, &["format"])
                .map(|f| McpInputFormat::from_wire(&f))
                .unwrap_or_default(),
            default: scalar_field(value, &["default"]),
            choices: parse_choices(value),
        }
    }
}

/// The shared `Input` body: description, requiredness, secrecy, format,
/// choices, default and the `value` template with its variables.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInput.ts")]
pub struct McpInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default)]
    pub format: McpInputFormat,
    /// Pre-set value (possibly a `{curly_braces}` template). Per the schema the
    /// end user should not be able to edit this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Hint text only — explicitly *not* a default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// When non-empty the user MUST pick one of these (render a select).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// Substitutions for `{curly_braces}` inside `value`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, McpInputVariable>,
}

impl McpInput {
    pub(crate) fn from_json(value: &Value) -> Self {
        Self {
            description: str_field(value, &["description"]),
            is_required: bool_field(value, &["isRequired", "is_required"]).unwrap_or(false),
            is_secret: bool_field(value, &["isSecret", "is_secret"]).unwrap_or(false),
            format: str_field(value, &["format"])
                .map(|f| McpInputFormat::from_wire(&f))
                .unwrap_or_default(),
            value: scalar_field(value, &["value"]),
            default: scalar_field(value, &["default"]),
            placeholder: str_field(value, &["placeholder"]),
            choices: parse_choices(value),
            variables: parse_variables(value),
        }
    }

    /// Does the user have to type something for this input? Secrets count:
    /// they are never shipped in the payload, so the form must ask.
    pub fn needs_user_value(&self) -> bool {
        (self.is_required || self.is_secret) && self.value.is_none()
    }
}

/// An `Input` that carries a name — `environmentVariables[]` and `headers[]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpKeyValueInput.ts")]
pub struct McpKeyValueInput {
    pub name: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub input: McpInput,
}

impl McpKeyValueInput {
    pub(crate) fn parse_list(items: Option<&Vec<Value>>) -> Vec<Self> {
        let Some(items) = items else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let name = str_field(item, &["name"])?;
                Some(Self {
                    name,
                    input: McpInput::from_json(item),
                })
            })
            .collect()
    }
}

/// Positional vs named CLI argument.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpArgumentKind.ts")]
pub enum McpArgumentKind {
    #[default]
    Positional,
    Named,
}

/// One entry of `runtimeArguments[]` / `packageArguments[]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpArgument.ts")]
pub struct McpArgument {
    #[serde(default)]
    pub kind: McpArgumentKind,
    /// Flag name for named arguments, e.g. `--port`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Display hint for positional arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_hint: Option<String>,
    #[serde(default)]
    pub is_repeated: bool,
    #[serde(flatten)]
    #[ts(flatten)]
    pub input: McpInput,
}

impl McpArgument {
    pub(crate) fn parse_list(items: Option<&Vec<Value>>) -> Vec<Self> {
        let Some(items) = items else {
            return Vec::new();
        };
        items
            .iter()
            .filter(|item| item.is_object())
            .map(|item| {
                let kind = match str_field(item, &["type"])
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "named" => McpArgumentKind::Named,
                    _ => McpArgumentKind::Positional,
                };
                Self {
                    kind,
                    name: str_field(item, &["name"]),
                    value_hint: str_field(item, &["valueHint", "value_hint"]),
                    is_repeated: bool_field(item, &["isRepeated", "is_repeated"]).unwrap_or(false),
                    input: McpInput::from_json(item),
                }
            })
            .collect()
    }
}

fn parse_choices(value: &Value) -> Vec<String> {
    arr_field(value, &["choices"])
        .map(|items| items.iter().filter_map(scalar_to_string).collect())
        .unwrap_or_default()
}

fn parse_variables(value: &Value) -> BTreeMap<String, McpInputVariable> {
    let Some(map) = obj_field(value, &["variables"]).and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    map.iter()
        .map(|(name, raw)| (name.clone(), McpInputVariable::from_json(raw)))
        .collect()
}
