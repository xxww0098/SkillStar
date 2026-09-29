//! Routing groups.
//!
//! A model id `group/<id>` expands to that group's members, and a member may
//! name another group. A write that would make a group contain itself, or nest
//! deeper than 8, does not change `model_gateway.json`.
//!
//! A model served under the same name by more than one provider is a group
//! derived from the list the caller passes in. It stays off disk until a save.
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

/// One model a provider serves. Passed in by the caller. Not read from the key file.
#[derive(Clone, Copy, Debug)]
pub struct ServedModel<'a> {
    /// Catalog entry, usually `provider/model`. Empty becomes `provider_id/model`.
    pub id: &'a str,
    /// Vendor model string. Same-name groups compare this, not `id`.
    pub model: &'a str,
    pub provider_id: &'a str,
}

struct Group {
    id: String,
    members: Vec<String>,
}

struct Bucket {
    key: String,
    providers: Vec<String>,
    members: Vec<String>,
}

/// Group ids saved in `model_gateway.json`, without the `group/` prefix.
pub fn stored_group_ids() -> Vec<String> {
    groups_in(&read_doc())
        .into_iter()
        .map(|group| group.id)
        .collect()
}

/// Members of `group/<id>`, outer order preserved, nested groups flattened.
///
/// A repeated member stays where it first appeared. A missing group, a loop,
/// or a nest deeper than [`MAX_NEST`] is skipped. Auto groups are included
/// when `models` lists the same name twice. This does not create or rewrite
/// the gateway file.
pub fn expand_group(model_id: &str, models: &[ServedModel<'_>]) -> Vec<String> {
    let Some(id) = prefixed_group_id(model_id) else {
        return Vec::new();
    };
    let stored = groups_in(&read_doc());
    let autos = auto_groups(models);
    let Some(group) = find_group(&stored, &autos, id) else {
        return Vec::new();
    };
    flatten(group, &stored, &autos)
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

fn read_doc() -> Value {
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

fn flatten(root: &Group, stored: &[Group], autos: &[Group]) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.id.clone()];
    walk(root, stored, autos, 0, &mut stack, &mut out);
    out
}

fn walk(
    group: &Group,
    stored: &[Group],
    autos: &[Group],
    via: usize,
    stack: &mut Vec<String>,
    out: &mut Vec<String>,
) {
    for member in &group.members {
        if let Some(gid) = group_suffix(member) {
            if stack.iter().any(|seen| seen == gid) || via >= MAX_NEST {
                continue;
            }
            let Some(sub) = find_group(stored, autos, gid) else {
                continue;
            };
            stack.push(gid.to_string());
            walk(sub, stored, autos, via + 1, stack, out);
            stack.pop();
            continue;
        }
        if member.is_empty() || out.iter().any(|seen| seen == member) {
            continue;
        }
        out.push(member.clone());
    }
}

fn find_group<'a>(stored: &'a [Group], autos: &'a [Group], id: &str) -> Option<&'a Group> {
    stored
        .iter()
        .find(|group| group.id == id)
        .or_else(|| autos.iter().find(|group| group.id == id))
}

fn groups_in(doc: &Value) -> Vec<Group> {
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

fn auto_groups(models: &[ServedModel<'_>]) -> Vec<Group> {
    let mut buckets: Vec<Bucket> = Vec::new();
    for model in models {
        let provider = model.provider_id.trim();
        if provider.is_empty() {
            continue;
        }
        let key = same_model(model.model);
        let entry = entry_id(model);
        if let Some(bucket) = buckets.iter_mut().find(|bucket| bucket.key == key) {
            if bucket.providers.iter().any(|seen| seen == provider) {
                continue;
            }
            bucket.providers.push(provider.to_string());
            bucket.members.push(entry);
        } else {
            buckets.push(Bucket {
                key,
                providers: vec![provider.to_string()],
                members: vec![entry],
            });
        }
    }
    buckets
        .into_iter()
        .filter(|bucket| bucket.providers.len() >= 2)
        .map(|bucket| Group {
            id: format!("auto-{}", slug(&bucket.key)),
            members: bucket.members,
        })
        .collect()
}

/// Vendors spell one model differently. Lowercase, drop the vendor prefix,
/// turn a dot between digits into a dash, and drop a trailing `-20YYYYMMDD`.
/// A `:variant` stays on the name until the group id is slugged.
fn same_model(id: &str) -> String {
    let mut key = id.to_lowercase();
    if let Some(index) = key.rfind('/') {
        key = key[index + 1..].to_string();
    }
    let chars: Vec<char> = key.chars().collect();
    let mut replaced = String::new();
    for (index, ch) in chars.iter().enumerate() {
        let between_digits = index > 0
            && index + 1 < chars.len()
            && *ch == '.'
            && chars[index - 1].is_ascii_digit()
            && chars[index + 1].is_ascii_digit();
        if between_digits {
            replaced.push('-');
        } else {
            replaced.push(*ch);
        }
    }
    if let Some(index) = replaced.rfind('-') {
        let tail = &replaced[index + 1..];
        if tail.len() == 8
            && tail.starts_with("20")
            && tail.bytes().all(|byte| byte.is_ascii_digit())
        {
            replaced.truncate(index);
        }
    }
    replaced
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dashed = false;
    for ch in name.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dashed = false;
        } else if !dashed && !out.is_empty() {
            out.push('-');
            dashed = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

fn entry_id(model: &ServedModel<'_>) -> String {
    let id = model.id.trim();
    if id.is_empty() {
        format!("{}/{}", model.provider_id.trim(), model.model.trim())
    } else {
        id.to_string()
    }
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

fn prefixed_group_id(model_id: &str) -> Option<&str> {
    let bare = model_id.trim().strip_prefix(GROUP_PREFIX)?.trim();
    if bare.is_empty() || bare.contains('/') {
        None
    } else {
        Some(bare)
    }
}

fn group_suffix(member: &str) -> Option<&str> {
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
