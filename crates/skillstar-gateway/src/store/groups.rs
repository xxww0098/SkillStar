//! Routing groups: the `groups` rows of `model_gateway.json`.
//!
//! A write that would make a group contain itself, or nest deeper than 8,
//! does not change `model_gateway.json`. Expanding a saved group, and the
//! same-name groups derived from a caller's list, live in `route::groups`.
//! This module does not read the provider key file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use skillstar_core::infra::fs_ops::atomic_write;

/// Prefix on a model id that names a group.
pub const GROUP_PREFIX: &str = "group/";

/// Deepest legal nesting. A group this deep can be saved. One deeper cannot.
pub const MAX_NEST: usize = 8;

/// Why a group was not saved. The gateway file is left as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveGroupError {
    /// The group would contain itself, directly or through another group.
    Cycle,
    /// The group would nest more than [`MAX_NEST`] groups deep.
    TooDeep,
    /// The id is empty, or the file could not be read or replaced.
    Store,
}

impl std::fmt::Display for SaveGroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cycle => "group_cycle",
            Self::TooDeep => "group_too_deep",
            Self::Store => "group_store",
        })
    }
}

/// One `groups` row, in memory.
pub(crate) struct Group {
    pub(crate) id: String,
    pub(crate) members: Vec<String>,
}

/// Group ids saved in `model_gateway.json`, without the `group/` prefix.
pub fn stored_group_ids() -> Vec<String> {
    groups_in(&read_doc())
        .into_iter()
        .map(|group| group.id)
        .collect()
}

/// One group saved in `model_gateway.json`. Auto groups are absent until saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedGroup {
    pub id: String,
    pub members: Vec<String>,
}

/// Saved groups and their members, in file order. This read does not create the file.
pub fn stored_groups() -> Vec<SavedGroup> {
    groups_in(&read_doc())
        .into_iter()
        .map(|group| SavedGroup {
            id: group.id,
            members: group.members,
        })
        .collect()
}

/// Save `id` with these members. `group/` in front of `id` is optional.
///
/// On [`SaveGroupError`] the file is not created and an existing file is not
/// modified. A successful save replaces only this group's `members`, and keeps
/// every other field in the file.
pub fn save_group(id: &str, members: &[impl AsRef<str>]) -> Result<(), SaveGroupError> {
    let Some(id) = bare_group_id(id) else {
        return Err(SaveGroupError::Store);
    };
    let members = clean_members(members);
    let path = gateway_path();
    let mut doc = load_object(&path)?;
    check_write(id, &members, &groups_in(&doc))?;
    insert_group(&mut doc, id, &members)?;
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|_| SaveGroupError::Store)?;
    atomic_write(&path, &bytes).map_err(|_| SaveGroupError::Store)
}

fn gateway_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("model_gateway.json")
}

/// The stored document as one `Value`, or an empty object.
pub(crate) fn read_doc() -> Value {
    let Ok(bytes) = fs::read(gateway_path()) else {
        return json!({});
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => json!({}),
    }
}

fn load_object(path: &Path) -> Result<Value, SaveGroupError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(json!({})),
        Err(_) => return Err(SaveGroupError::Store),
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value @ Value::Object(_)) => match value.get("groups") {
            None | Some(Value::Array(_)) => Ok(value),
            Some(_) => Err(SaveGroupError::Store),
        },
        _ => Err(SaveGroupError::Store),
    }
}

fn check_write(id: &str, members: &[String], stored: &[Group]) -> Result<(), SaveGroupError> {
    for member in members {
        let Some(gid) = group_suffix(member) else {
            continue;
        };
        if gid == id {
            return Err(SaveGroupError::Cycle);
        }
        let Some(sub) = stored.iter().find(|group| group.id == gid) else {
            continue;
        };
        if reaches(stored, sub, id, &[]) {
            return Err(SaveGroupError::Cycle);
        }
        if depth_of(stored, sub, &[]) + 1 > MAX_NEST {
            return Err(SaveGroupError::TooDeep);
        }
    }
    Ok(())
}

fn reaches(all: &[Group], group: &Group, target: &str, stack: &[String]) -> bool {
    for member in &group.members {
        let Some(gid) = group_suffix(member) else {
            continue;
        };
        if stack.iter().any(|seen| seen == gid) {
            continue;
        }
        if gid == target {
            return true;
        }
        let Some(sub) = all.iter().find(|item| item.id == gid) else {
            continue;
        };
        let mut next = stack.to_vec();
        next.push(group.id.clone());
        if reaches(all, sub, target, &next) {
            return true;
        }
    }
    false
}

fn depth_of(all: &[Group], group: &Group, stack: &[String]) -> usize {
    let mut depth = 0;
    for member in &group.members {
        let Some(gid) = group_suffix(member) else {
            continue;
        };
        if stack.iter().any(|seen| seen == gid) {
            continue;
        }
        let Some(sub) = all.iter().find(|item| item.id == gid) else {
            continue;
        };
        let mut next = stack.to_vec();
        next.push(group.id.clone());
        depth = depth.max(1 + depth_of(all, sub, &next));
    }
    depth
}

fn insert_group(doc: &mut Value, id: &str, members: &[String]) -> Result<(), SaveGroupError> {
    let members_value = Value::Array(members.iter().cloned().map(Value::String).collect());
    let list = doc
        .as_object_mut()
        .ok_or(SaveGroupError::Store)?
        .entry("groups")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(list) = list.as_array_mut() else {
        return Err(SaveGroupError::Store);
    };
    if let Some(existing) = list.iter_mut().find(|group| group_id(group) == Some(id)) {
        let Some(object) = existing.as_object_mut() else {
            return Err(SaveGroupError::Store);
        };
        object.insert("id".to_string(), Value::String(id.to_string()));
        object.insert("members".to_string(), members_value);
        object.remove("auto");
        return Ok(());
    }
    list.push(json!({
        "id": id,
        "members": members,
    }));
    Ok(())
}

pub(crate) fn groups_in(doc: &Value) -> Vec<Group> {
    let Some(list) = doc.get("groups").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut groups: Vec<Group> = Vec::new();
    for value in list {
        let Some(id) = group_id(value) else {
            continue;
        };
        if groups.iter().any(|group| group.id == id) {
            continue;
        }
        groups.push(Group {
            id: id.to_string(),
            members: member_list(value),
        });
    }
    groups
}

fn member_list(group: &Value) -> Vec<String> {
    group
        .get("members")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|member| !member.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn clean_members(members: &[impl AsRef<str>]) -> Vec<String> {
    members
        .iter()
        .map(|member| member.as_ref().trim().to_string())
        .filter(|member| !member.is_empty())
        .collect()
}

fn bare_group_id(id: &str) -> Option<&str> {
    let trimmed = id.trim();
    let bare = trimmed.strip_prefix(GROUP_PREFIX).unwrap_or(trimmed).trim();
    if bare.is_empty() || bare.contains('/') {
        None
    } else {
        Some(bare)
    }
}

pub(crate) fn group_suffix(member: &str) -> Option<&str> {
    member
        .trim()
        .strip_prefix(GROUP_PREFIX)
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

fn group_id(group: &Value) -> Option<&str> {
    group
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
}
