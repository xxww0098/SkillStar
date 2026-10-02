//! Named profiles in `model_gateway.json`.
//!
//! A profile is a name plus agent id and `model_ref` pairs. Applying it calls
//! `apply_gateway` for each pair. An agent that writer does not implement is
//! reported and no file is opened for it. This module does not sync a skill
//! library, MCP, or WebDAV, and it does not write agent files itself.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use skillstar_core::infra::fs_ops::atomic_write;

use crate::agents::apply_gateway;
use crate::codex::ApplyError;

const MAX_NAME: usize = 64;

/// One agent's selected model inside a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileAgent {
    pub id: String,
    pub model_ref: String,
}

/// Agents the existing writer accepted, and ids it does not implement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileApply {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
}

/// The name cannot be stored. The gateway file is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveProfileError {
    Name,
    Store,
}

impl std::fmt::Display for SaveProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Name => "profile_name",
            Self::Store => "profile_store",
        })
    }
}

/// Apply did not run the writers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyProfileError {
    Missing,
    Store,
    Write,
}

impl std::fmt::Display for ApplyProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Missing => "profile_missing",
            Self::Store => "profile_store",
            Self::Write => "profile_write",
        })
    }
}

struct StoredProfile {
    name: String,
    agents: Vec<ProfileAgent>,
}

/// Profile names in file order. A missing file is empty and is not created.
pub fn profile_names() -> Vec<String> {
    stored_profiles(&read_doc())
        .into_iter()
        .map(|profile| profile.name)
        .collect()
}

/// Replace the agent list stored under `name`.
///
/// A name that is empty or longer than 64 Unicode scalars is refused. A
/// secret-shaped name, id, or model ref is refused. Either way the file is
/// left as it was.
pub fn save_profile(name: &str, agents: &[ProfileAgent]) -> Result<(), SaveProfileError> {
    let name = clean_name(name)?;
    let agents = clean_agents(agents)?;
    let path = gateway_path();
    let mut doc = load_object(&path)?;
    let mut profiles = stored_profiles(&doc);
    if let Some(existing) = profiles.iter_mut().find(|profile| profile.name == name) {
        existing.agents = agents;
    } else {
        profiles.push(StoredProfile { name, agents });
    }
    write_profiles(&mut doc, &profiles)?;
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|_| SaveProfileError::Store)?;
    atomic_write(&path, &bytes).map_err(|_| SaveProfileError::Store)
}

/// Call the existing file writer for each saved agent.
///
/// The gateway file is not rewritten. An unimplemented id is skipped.
pub fn apply_profile(name: &str) -> Result<ProfileApply, ApplyProfileError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApplyProfileError::Missing);
    }
    let profiles = stored_profiles(&read_doc());
    let Some(profile) = profiles.into_iter().find(|profile| profile.name == name) else {
        return Err(ApplyProfileError::Missing);
    };
    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    for agent in profile.agents {
        match apply_gateway(&agent.id, &agent.model_ref) {
            Ok(()) => applied.push(agent.id),
            Err(ApplyError::NotManaged) => skipped.push(agent.id),
            Err(ApplyError::Io(_)) => return Err(ApplyProfileError::Write),
        }
    }
    Ok(ProfileApply { applied, skipped })
}

fn clean_name(name: &str) -> Result<String, SaveProfileError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(SaveProfileError::Name);
    }
    if secret_shaped(name) {
        return Err(SaveProfileError::Store);
    }
    Ok(name.to_string())
}

fn clean_agents(agents: &[ProfileAgent]) -> Result<Vec<ProfileAgent>, SaveProfileError> {
    let mut out: Vec<ProfileAgent> = Vec::new();
    for agent in agents {
        let id = agent.id.trim();
        let model_ref = agent.model_ref.trim();
        if id.is_empty() || model_ref.is_empty() || secret_shaped(id) || secret_shaped(model_ref) {
            return Err(SaveProfileError::Store);
        }
        if out.iter().any(|seen| seen.id == id) {
            continue;
        }
        out.push(ProfileAgent {
            id: id.to_string(),
            model_ref: model_ref.to_string(),
        });
    }
    Ok(out)
}

fn secret_shaped(value: &str) -> bool {
    value.contains("://") || value.contains("sk-") || value.contains("api.openai.com")
}

fn gateway_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("model_gateway.json")
}

fn read_doc() -> Value {
    let Ok(bytes) = fs::read(gateway_path()) else {
        return json!({});
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value @ Value::Object(_)) => value,
        _ => json!({}),
    }
}

fn load_object(path: &Path) -> Result<Value, SaveProfileError> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value @ Value::Object(_)) => Ok(value),
            _ => Err(SaveProfileError::Store),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err(SaveProfileError::Store),
    }
}

fn stored_profiles(doc: &Value) -> Vec<StoredProfile> {
    let Some(list) = doc.get("profiles").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut profiles: Vec<StoredProfile> = Vec::new();
    for value in list {
        let Some(name) = value.get("name").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if name.is_empty() || profiles.iter().any(|profile| profile.name == name) {
            continue;
        }
        let agents = value
            .get("agents")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let id = item.get("id")?.as_str()?.trim();
                        let model_ref = item.get("model_ref")?.as_str()?.trim();
                        if id.is_empty() || model_ref.is_empty() {
                            return None;
                        }
                        Some(ProfileAgent {
                            id: id.to_string(),
                            model_ref: model_ref.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        profiles.push(StoredProfile {
            name: name.to_string(),
            agents,
        });
    }
    profiles
}

fn write_profiles(doc: &mut Value, profiles: &[StoredProfile]) -> Result<(), SaveProfileError> {
    let list = profiles
        .iter()
        .map(|profile| {
            json!({
                "name": profile.name,
                "agents": profile.agents.iter().map(|agent| json!({
                    "id": agent.id,
                    "model_ref": agent.model_ref,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    doc.as_object_mut()
        .ok_or(SaveProfileError::Store)?
        .insert("profiles".to_string(), Value::Array(list));
    Ok(())
}
