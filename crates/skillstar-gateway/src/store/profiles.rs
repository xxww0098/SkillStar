//! Named profiles in `model_gateway.json`.
//!
//! A profile is a name plus agent id and `model_ref` pairs. Applying it calls
//! `apply_gateway` for each pair. An agent that writer does not implement is
//! reported and no file is opened for it. This module does not sync a skill
//! library, MCP, or WebDAV, and it does not write agent files itself. The
//! gateway file is opened only through [`ModelGatewayDoc`] (see
//! `store::doc`).

use super::doc::{ModelGatewayDoc, ProfileAgentRow, ProfileRow};
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

pub(crate) struct StoredProfile {
    pub(crate) name: String,
    pub(crate) agents: Vec<ProfileAgent>,
}

/// Profile names in file order. A missing file is empty and is not created.
pub fn profile_names() -> Vec<String> {
    stored_profiles(&ModelGatewayDoc::open_lenient())
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
    let mut doc = ModelGatewayDoc::open().map_err(|_| SaveProfileError::Store)?;
    let mut profiles = stored_profiles(&doc);
    if let Some(existing) = profiles.iter_mut().find(|profile| profile.name == name) {
        existing.agents = agents;
    } else {
        profiles.push(StoredProfile { name, agents });
    }
    write_profiles(&mut doc, &profiles);
    doc.save().map_err(|_| SaveProfileError::Store)
}

/// Call the existing file writer for each saved agent.
///
/// The gateway file is not rewritten. An unimplemented id is skipped.
pub fn apply_profile(name: &str) -> Result<ProfileApply, ApplyProfileError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApplyProfileError::Missing);
    }
    let profiles = stored_profiles(&ModelGatewayDoc::open_lenient());
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

/// Profiles held by an already-open document, in file order.
///
/// A row without a usable name, and repeats of a name already taken, are
/// not profiles; the container still keeps those rows exactly as they are.
fn stored_profiles(doc: &ModelGatewayDoc) -> Vec<StoredProfile> {
    let mut profiles: Vec<StoredProfile> = Vec::new();
    for row in doc.profiles() {
        let name = row.name.trim();
        if name.is_empty() || profiles.iter().any(|profile| profile.name == name) {
            continue;
        }
        let agents = row
            .agents
            .iter()
            .filter_map(|agent| {
                let id = agent.id.trim();
                let model_ref = agent.model_ref.trim();
                if id.is_empty() || model_ref.is_empty() {
                    return None;
                }
                Some(ProfileAgent {
                    id: id.to_string(),
                    model_ref: model_ref.to_string(),
                })
            })
            .collect();
        profiles.push(StoredProfile {
            name: name.to_string(),
            agents,
        });
    }
    profiles
}

/// The profile write lens: rebuild the whole `profiles` array from name and
/// agents only. Every other row field — including on bystander rows the
/// save never touched — dies here; that drop is today's documented
/// behavior, pinned by the behavior-lock test, not an accident.
pub(crate) fn write_profiles(doc: &mut ModelGatewayDoc, profiles: &[StoredProfile]) {
    doc.set_profiles(
        profiles
            .iter()
            .map(|profile| {
                ProfileRow::new(
                    profile.name.clone(),
                    profile
                        .agents
                        .iter()
                        .map(|agent| {
                            ProfileAgentRow::new(agent.id.clone(), agent.model_ref.clone())
                        })
                        .collect(),
                )
            })
            .collect(),
    );
}
