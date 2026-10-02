//! Group expansion.
//!
//! A model id `group/<id>` expands to that group's members, outer order
//! preserved, nested groups flattened. A model served under the same name by
//! more than one provider is a group derived from the list the caller passes
//! in. It stays off disk until a save. The stored rows come from
//! `crate::store::groups`; this module does not open the file itself.

use crate::store::groups::{GROUP_PREFIX, Group, MAX_NEST, group_suffix, groups_in, read_doc};

/// One model a provider serves. Passed in by the caller. Not read from the key file.
#[derive(Clone, Copy, Debug)]
pub struct ServedModel<'a> {
    /// Catalog entry, usually `provider/model`. Empty becomes `provider_id/model`.
    pub id: &'a str,
    /// Vendor model string. Same-name groups compare this, not `id`.
    pub model: &'a str,
    pub provider_id: &'a str,
}

struct Bucket {
    key: String,
    providers: Vec<String>,
    members: Vec<String>,
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

fn prefixed_group_id(model_id: &str) -> Option<&str> {
    let bare = model_id.trim().strip_prefix(GROUP_PREFIX)?.trim();
    if bare.is_empty() || bare.contains('/') {
        None
    } else {
        Some(bare)
    }
}
