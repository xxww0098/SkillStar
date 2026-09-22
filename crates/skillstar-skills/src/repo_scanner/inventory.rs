//! Tree-SHA inventory: one representative copy per skill identity.
//!
//! Distribution repos ("pack" layout, e.g. `pbakaus/impeccable`) mirror the
//! same skill into a dozen harness directories (`.claude/skills/x`,
//! `.cursor/skills/x`, …). Materializing every copy meant lazily fetching
//! each duplicate's blobs over the git smart protocol — the dominant cost of
//! installing such repos.
//!
//! A treeless partial clone already carries every path plus each directory's
//! tree SHA, so the duplicates are visible *before* any blob is downloaded:
//! identical tree SHAs are byte-identical copies. This module plans the sparse
//! checkout to materialize exactly one representative per identity and
//! records the rest as `deferred_dirs` for on-demand materialization
//! (`add_sparse_checkout_dirs_in_session`) when a harness-specific copy is
//! later requested.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::git::ops as git_ops;

/// Where the plan is persisted inside the cache entry. Living under `.git`
/// keeps it out of the worktree, so scans never see it as skill content and
/// `git status` never reports it.
const SIDECAR: &str = ".git/skillstar-inventory.json";

/// Manifest blobs are tiny; refuse anything pathological before loading it.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SparsePlan {
    /// The HEAD revision the plan was computed from; mismatches force a
    /// rebuild after a fetch/reset moves the checkout.
    pub revision: String,
    /// Directories the sparse checkout should materialize now.
    pub sparse_dirs: Vec<String>,
    /// Tracked skill directories deliberately left unmaterialized because a
    /// representative of the same identity is already in `sparse_dirs`.
    pub deferred_dirs: Vec<String>,
}

impl SparsePlan {
    /// Deferred directories whose basename matches `name` (case-insensitive)
    /// or that live under `harness_prefix`.
    #[allow(dead_code)]
    pub(crate) fn deferred_matches(&self, name: Option<&str>, harness_prefix: Option<&str>) -> Vec<String> {
        let name_lower = name.map(str::to_lowercase);
        let prefix = harness_prefix
            .filter(|prefix| !prefix.is_empty())
            .map(|prefix| format!("{prefix}/"));
        self.deferred_dirs
            .iter()
            .filter(|dir| {
                let base = dir.rsplit('/').next().unwrap_or(dir).to_lowercase();
                let name_hit = name_lower
                    .as_deref()
                    .is_some_and(|wanted| base == wanted);
                let prefix_hit = prefix
                    .as_deref()
                    .is_some_and(|prefix| dir.starts_with(prefix));
                name_hit || prefix_hit
            })
            .cloned()
            .collect()
    }
}

/// Compute (or reload from the sidecar) the sparse plan for a cache entry.
///
/// `installed_folders` are the lockfile `source_folder` values of skills that
/// already link into this checkout: they must be materialized even when a
/// different representative would otherwise win, or a fetch/reset would leave
/// their hub symlinks dangling.
pub(crate) fn load_or_plan(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    installed_folders: &[String],
) -> Result<SparsePlan> {
    let revision = git_ops::head_revision(repo_dir)
        .context("Failed to read cache revision while planning sparse checkout")?;
    if let Some(plan) = load_sidecar(repo_dir)
        && plan.revision == revision
    {
        return Ok(plan);
    }
    let plan = plan_sparse_dirs(repo_dir, session, &revision, installed_folders)?;
    save_sidecar(repo_dir, &plan);
    Ok(plan)
}

/// Materialize deferred copies this request can select.
///
/// Filesystem discovery cannot see a deferred directory, so a harness-specific
/// install (or an explicit skill name that only exists as a duplicate copy)
/// must first surface the matching directories. Every surfaced directory is
/// folded back into the plan so a later fetch/reset re-applies the sparse set
/// with it included and never dangles an installed skill's link.
///
/// Returns `true` when at least one directory was newly materialized.
pub(crate) fn materialize_deferred_matching(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    names: &[&str],
    harness_prefix: Option<&str>,
) -> bool {
    let Ok(mut plan) = load_or_plan(repo_dir, session, &[]) else {
        return false;
    };
    let lowered: Vec<String> = names
        .iter()
        .map(|name| name.to_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    let candidates: Vec<String> = plan
        .deferred_dirs
        .iter()
        .filter(|dir| {
            let base = dir.rsplit('/').next().unwrap_or(dir).to_lowercase();
            let name_hit = lowered.iter().any(|wanted| &base == wanted);
            let prefix_hit = harness_prefix
                .filter(|prefix| !prefix.is_empty())
                .is_some_and(|prefix| dir.starts_with(&format!("{prefix}/")));
            name_hit || prefix_hit
        })
        .cloned()
        .collect();
    if candidates.is_empty() {
        return false;
    }

    let fresh: Vec<String> = candidates
        .iter()
        .filter(|dir| !repo_dir.join(dir).exists())
        .cloned()
        .collect();

    // Fold intent into the plan even when the files are already on disk: the
    // sparse pattern must keep the directory, or the next re-apply drops it.
    for dir in &candidates {
        plan.sparse_dirs.push(dir.clone());
        plan.deferred_dirs.retain(|deferred| deferred != dir);
    }
    plan.sparse_dirs.sort();
    plan.sparse_dirs.dedup();
    save_sidecar(repo_dir, &plan);

    if fresh.is_empty() {
        return false;
    }
    if crate::tarball_fetch::is_tarball_cache(repo_dir) {
        // A tarball cache holds no git objects for deferred copies, so the
        // sparse pattern cannot materialize them; re-download the archive
        // and commit the new directories instead. The extension commit moves
        // HEAD, so the sidecar must follow it or the next call would rebuild
        // the plan from the synthetic tree and forget the deferred set.
        if crate::tarball_fetch::add_dirs_via_tarball(repo_dir, &fresh).is_ok() {
            if let Ok(revision) = git_ops::head_revision(repo_dir) {
                plan.revision = revision;
            }
            save_sidecar(repo_dir, &plan);
            return true;
        }
        return false;
    }
    if git_ops::add_sparse_checkout_dirs_in_session(repo_dir, &fresh, session).is_err() {
        // The sidecar already records the intent; the next sparse re-apply
        // (any fetch or install of this repo) heals the checkout.
        return false;
    }
    true
}

/// Carry a plan computed from the real repository tree over to a tarball-built
/// synthetic cache entry.
///
/// The synthetic tree only contains the extracted directories, so a plan
/// rebuilt from it would see no deferred copies at all. Re-stamping the
/// revision keeps the real plan valid for on-demand materialization.
pub(crate) fn adopt_plan_for_synthetic_repo(repo_dir: &Path, plan: &SparsePlan) {
    let mut plan = plan.clone();
    if let Ok(revision) = git_ops::head_revision(repo_dir) {
        plan.revision = revision;
    }
    save_sidecar(repo_dir, &plan);
}

fn sidecar_path(repo_dir: &Path) -> PathBuf {
    repo_dir.join(SIDECAR)
}

fn load_sidecar(repo_dir: &Path) -> Option<SparsePlan> {
    let content = std::fs::read_to_string(sidecar_path(repo_dir)).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_sidecar(repo_dir: &Path, plan: &SparsePlan) {
    let path = sidecar_path(repo_dir);
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_ok()
        && let Ok(content) = serde_json::to_string(plan)
    {
        // Best effort: a failed sidecar only costs a rebuild on the next call.
        let _ = std::fs::write(path, content);
    }
}

fn plan_sparse_dirs(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    revision: &str,
    installed_folders: &[String],
) -> Result<SparsePlan> {
    let entries = git_ops::list_tree_entries_with_trees(repo_dir, revision)
        .context("Failed to read the repository tree while planning sparse checkout")?;

    let mut dir_shas: HashMap<String, String> = HashMap::new();
    let mut tracked_files: Vec<&str> = Vec::new();
    let mut skill_dirs: Vec<String> = Vec::new();
    for entry in &entries {
        if entry.kind == "tree" {
            dir_shas.insert(entry.path.clone(), entry.sha.clone());
        } else {
            tracked_files.push(&entry.path);
            // Repo-root `SKILL.md` is always present in cone mode; whether it
            // is a shim or a genuine skill stays with discovery.
            if entry.path.ends_with("/SKILL.md")
                && let Some(parent) = parent_dir(&entry.path)
            {
                skill_dirs.push(parent);
            }
        }
    }
    skill_dirs.sort();
    skill_dirs.dedup();

    if skill_dirs.is_empty() {
        // Nothing nested to filter: the caller disables sparse and checks out
        // everything, exactly like the pre-inventory behavior.
        return Ok(SparsePlan {
            revision: revision.to_string(),
            sparse_dirs: Vec::new(),
            deferred_dirs: Vec::new(),
        });
    }

    let manifest_dirs = manifest_declared_dirs(repo_dir, session, &tracked_files);

    // Group by identity (directory basename); identical basenames across
    // containers are the pack-layout duplicate shape.
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for dir in &skill_dirs {
        let base = dir.rsplit('/').next().unwrap_or(dir).to_lowercase();
        groups.entry(base).or_default().push(dir.clone());
    }

    let mut sparse = Vec::new();
    let mut deferred = Vec::new();
    let mut group_list: Vec<(&String, &Vec<String>)> = groups.iter().collect();
    group_list.sort_by(|a, b| a.0.cmp(b.0));
    for (_identity, dirs) in group_list {
        let representative = choose_representative(dirs, &manifest_dirs, installed_folders);
        for dir in dirs {
            if dir == &representative || installed_folders.contains(dir) {
                sparse.push(dir.clone());
            } else if is_duplicate_of(dir, &representative, &dir_shas) {
                deferred.push(dir.clone());
            } else {
                // Unknown relationship (different content, not a harness
                // container copy): keep today's behavior and materialize it —
                // its frontmatter may declare a different identity.
                sparse.push(dir.clone());
            }
        }
    }

    sparse.sort();
    sparse.dedup();
    deferred.sort();

    Ok(SparsePlan {
        revision: revision.to_string(),
        sparse_dirs: compact_to_common_parents(&sparse),
        deferred_dirs: deferred,
    })
}

/// Preference order for the copy that gets materialized:
/// manifest-declared containers, then the canonical `skills/<name>` layout,
/// then the shared `.agents/skills/<name>`, then anything already installed,
/// then lexicographic order.
fn choose_representative(
    dirs: &[String],
    manifest_dirs: &[String],
    installed_folders: &[String],
) -> String {
    dirs.iter()
        .min_by_key(|dir| {
            let dir = dir.as_str();
            let parent = parent_dir(dir).unwrap_or_default();
            let base = dir.rsplit('/').next().unwrap_or(dir);
            let rank_manifest = usize::from(!manifest_dirs.contains(&parent));
            let rank_canonical = usize::from(dir != format!("skills/{base}"));
            let rank_agents = usize::from(dir != format!(".agents/skills/{base}"));
            let rank_installed = usize::from(!installed_folders.iter().any(|installed| installed == dir));
            (
                rank_manifest,
                rank_canonical,
                rank_agents,
                rank_installed,
                dir.to_string(),
            )
        })
        .cloned()
        .expect("groups are never empty")
}

/// True when `dir` is safe to leave unmaterialized because `representative`
/// already provides byte-identical content: same tree SHA. Content-divergent
/// copies always materialize — their frontmatter may declare a different
/// identity, and losing that would hide a skill discovery could find today.
fn is_duplicate_of(dir: &str, representative: &str, dir_shas: &HashMap<String, String>) -> bool {
    if dir == representative {
        return true;
    }
    match (dir_shas.get(dir), dir_shas.get(representative)) {
        (Some(dir_sha), Some(rep_sha)) => dir_sha == rep_sha,
        _ => false,
    }
}

/// Read `.claude-plugin` manifests as blobs so the plan can prefer
/// declared directories before anything is materialized. Any failure simply
/// drops the manifest preference.
fn manifest_declared_dirs(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    tracked_files: &[&str],
) -> Vec<String> {
    let read = |path: &str| -> Option<String> {
        tracked_files
            .contains(&path)
            .then(|| {
                git_ops::read_blob_in_session(repo_dir, "HEAD", path, MAX_MANIFEST_BYTES, session)
                    .ok()
            })
            .flatten()
    };
    let marketplace = read(".claude-plugin/marketplace.json");
    let plugin = read(".claude-plugin/plugin.json");
    crate::plugin_manifest::declared_skill_dir_strings(marketplace.as_deref(), plugin.as_deref())
}

fn parent_dir(path: &str) -> Option<String> {
    let parent = Path::new(path).parent()?.to_string_lossy().to_string();
    (!parent.is_empty()).then_some(parent)
}

/// Batch sibling directories into their shared parent.
///
/// Cone-mode sparse checkout pulls whole directories, so once two chosen
/// directories share a parent there is no extra cost in taking everything
/// under it — and one pattern beats two.
pub(crate) fn compact_to_common_parents(dirs: &[String]) -> Vec<String> {
    if dirs.is_empty() {
        return Vec::new();
    }

    let mut parent_counts: HashMap<String, usize> = HashMap::new();
    let mut parent_to_dirs: HashMap<String, Vec<String>> = HashMap::new();

    for dir in dirs {
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            *parent_counts.entry(parent_str.clone()).or_insert(0) += 1;
            parent_to_dirs
                .entry(parent_str)
                .or_default()
                .push(dir.clone());
        }
    }

    let mut result = Vec::new();
    let mut handled: std::collections::HashSet<String> = std::collections::HashSet::new();

    for dir in dirs {
        if handled.contains(dir) {
            continue;
        }
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if parent_counts.get(&parent_str).copied().unwrap_or(0) >= 2 {
                if !handled.contains(&parent_str) {
                    result.push(parent_str.clone());
                    if let Some(children) = parent_to_dirs.get(&parent_str) {
                        for child in children {
                            handled.insert(child.clone());
                        }
                    }
                    handled.insert(parent_str);
                }
            } else {
                result.push(dir.clone());
                handled.insert(dir.clone());
            }
        } else {
            result.push(dir.clone());
            handled.insert(dir.clone());
        }
    }

    result.sort();
    result
}

#[cfg(test)]
mod tests;
