//! Routing groups: the `groups` rows of `model_gateway.json`.
//!
//! A write that would make a group contain itself, or nest deeper than 8,
//! does not change `model_gateway.json`. This module's save path also
//! relies on the strict open refusing a `groups` value that is not an
//! array (the container's parse, not a check of its own), which is why a
//! file like `{"groups": 3}` is left untouched instead of being rewritten.
//! Expanding a saved group, and the same-name groups derived from a
//! caller's list, live in `route::groups`. This module does not read the
//! provider key file and opens the gateway file only through
//! [`ModelGatewayDoc`] (see `store::doc`).

use super::doc::{ModelGatewayDoc, OwnerRow};

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
    groups_in(&ModelGatewayDoc::open_lenient())
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
    groups_in(&ModelGatewayDoc::open_lenient())
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
    let mut doc = ModelGatewayDoc::open().map_err(|_| SaveGroupError::Store)?;
    check_write(id, &members, &groups_in(&doc))?;
    write_group(&mut doc, id, &members);
    doc.save().map_err(|_| SaveGroupError::Store)
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

/// The group write lens: first-match-by-id over the `groups` rows, a
/// missing row is appended at the end. Replacing a row rewrites `id` and
/// `members` and drops a stale `auto` marker; every other row field stays.
pub(crate) fn write_group(doc: &mut ModelGatewayDoc, id: &str, members: &[String]) {
    let rows = doc.groups_mut();
    let row = match rows.iter_mut().find(|row| row.id.trim() == id) {
        Some(row) => row,
        None => {
            rows.push(OwnerRow::from_id(id));
            rows.last_mut().expect("just pushed")
        }
    };
    row.id = id.to_string();
    row.members = members.to_vec();
    row.remove_extra("auto");
}

/// Groups held by an already-open document, in file order.
///
/// A row without a usable id, and repeats of an id already taken, are not
/// groups; the container still keeps those rows exactly as they are.
pub(crate) fn groups_in(doc: &ModelGatewayDoc) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for row in doc.groups() {
        let id = row.id.trim();
        if id.is_empty() || groups.iter().any(|group| group.id == id) {
            continue;
        }
        groups.push(Group {
            id: id.to_string(),
            members: member_list(row),
        });
    }
    groups
}

fn member_list(group: &OwnerRow) -> Vec<String> {
    group
        .members
        .iter()
        .map(|member| member.trim())
        .filter(|member| !member.is_empty())
        .map(str::to_string)
        .collect()
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
