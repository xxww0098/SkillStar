//! Name-identity inventory: one materialized copy per skill identity.
//!
//! Distribution repos ("pack" layout, e.g. `pbakaus/impeccable`) publish the
//! same skill as a dozen harness-rewritten copies (`.claude/skills/x`,
//! `.cursor/skills/x`, …): same frontmatter `name`, different bytes.
//! Materializing every copy meant fetching every copy's blobs.
//!
//! A treeless partial clone already lists every path, each directory's tree
//! SHA and each `SKILL.md` blob id, so the plan is made before any content is
//! downloaded. Same-basename folders whose trees differ get their `SKILL.md`
//! blobs prefetched in one request and grouped by frontmatter `name`; each
//! identity materializes one representative (`pack_layout::choose_copy`) and
//! the other copies stay deferred until a request selects them. Test-fixture
//! and vendored folders (`pack_layout::IGNORED_DIR_NAMES`) never materialize.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::git::ops as git_ops;
use crate::pack_layout::{CopyRequest, choose_copy, identity_key, is_under_ignored_dir};

/// Where the plan is persisted inside the cache entry. Living under `.git`
/// keeps it out of the worktree, so scans never see it as skill content and
/// `git status` never reports it.
const SIDECAR: &str = ".git/skillstar-inventory.json";

/// Bumped when the sidecar shape changes. `format` has no serde default, so
/// an older sidecar fails to parse and is simply re-planned.
const INVENTORY_FORMAT: u32 = 2;

const PLUGIN_MANIFEST_DIR: &str = ".claude-plugin";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Inventory {
    format: u32,
    /// The HEAD revision the plan was computed from; mismatches force a
    /// rebuild after a fetch/reset moves the checkout.
    pub revision: String,
    /// Identity key → every tracked folder carrying that Skill.
    pub identities: BTreeMap<String, Vec<String>>,
    /// Folders the checkout materializes: one representative per identity,
    /// every copy of a group whose names could not be read and whose bytes
    /// differ, and copies materialized on request.
    pub materialized: Vec<String>,
    /// Skill folders under ignored directories. Never materialized.
    pub ignored: Vec<String>,
    pub manifest_dirs: Vec<String>,
    /// The repo tracks `.claude-plugin/`; keeping it on disk lets filesystem
    /// discovery read the manifest (cone mode only brings root files).
    pub plugin_manifest: bool,
}

impl Inventory {
    /// Tracked Skill folders left unmaterialized on purpose.
    pub(crate) fn deferred(&self) -> Vec<String> {
        let materialized: HashSet<&String> = self.materialized.iter().collect();
        let mut deferred: Vec<String> = self
            .identities
            .values()
            .flatten()
            .filter(|dir| !materialized.contains(dir))
            .cloned()
            .collect();
        deferred.sort();
        deferred.dedup();
        deferred
    }

    /// No Skill folder anywhere: the caller checks out everything, exactly
    /// like the pre-inventory behavior.
    pub(crate) fn is_unfiltered(&self) -> bool {
        self.identities.is_empty() && self.ignored.is_empty()
    }

    /// Cone patterns for the checkout: the materialized set plus `extra`
    /// (installed or pinned folders) plus `on_disk` (folders already checked
    /// out; the set only grows, so Agent links pinned into this cache never
    /// dangle), plus `.claude-plugin`.
    pub(crate) fn sparse_dirs(&self, extra: &[String], on_disk: &[String]) -> Vec<String> {
        let mut dirs: Vec<String> = self
            .materialized
            .iter()
            .chain(extra)
            .chain(on_disk)
            .filter(|dir| !dir.is_empty())
            .cloned()
            .collect();
        dirs.sort();
        dirs.dedup();
        let chosen: HashSet<&String> = dirs.iter().collect();
        let blocked: Vec<String> = self
            .deferred()
            .into_iter()
            .chain(self.ignored.iter().cloned())
            .filter(|dir| !chosen.contains(dir))
            .collect();
        let mut sparse = compact_to_common_parents(&dirs, &blocked);
        if self.plugin_manifest {
            sparse.push(PLUGIN_MANIFEST_DIR.to_string());
            sparse.sort();
            sparse.dedup();
        }
        sparse
    }

    /// The copy of `name` that `request` selects. Identities are frontmatter
    /// names; a group whose names were unreadable is keyed by basename, so
    /// fall back to folders with that basename.
    pub(crate) fn copy_for(&self, name: &str, request: CopyRequest<'_>) -> Option<&str> {
        let key = name.to_lowercase();
        let by_basename: Vec<String>;
        let copies = match self.identities.get(&key) {
            Some(copies) => copies,
            None => {
                by_basename = self
                    .identities
                    .values()
                    .flatten()
                    .filter(|dir| dir.rsplit('/').next().unwrap_or(dir).to_lowercase() == key)
                    .cloned()
                    .collect();
                &by_basename
            }
        };
        choose_copy(copies, |dir| dir.as_str(), request, &self.manifest_dirs)
            .and_then(|dir| {
                self.identities
                    .values()
                    .flatten()
                    .find(|known| *known == dir)
            })
            .map(String::as_str)
    }

    /// Known Skill folders currently checked out.
    fn on_disk(&self, repo_dir: &Path) -> Vec<String> {
        self.identities
            .values()
            .flatten()
            .filter(|dir| repo_dir.join(dir).join("SKILL.md").is_file())
            .cloned()
            .collect()
    }
}

/// Compute (or reload from the sidecar) the inventory for a cache entry.
pub(crate) fn load_or_plan(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Inventory> {
    let revision = git_ops::head_revision(repo_dir)
        .context("Failed to read cache revision while planning sparse checkout")?;
    if let Some(inventory) = load_sidecar(repo_dir)
        && inventory.revision == revision
    {
        return Ok(inventory);
    }
    let inventory = plan_inventory(repo_dir, session, &revision)?;
    save_sidecar(repo_dir, &inventory);
    Ok(inventory)
}

/// Point the checkout at the inventory: `extra` are folders that must stay
/// materialized (installed Skills' `source_folder`s). Anything already on
/// disk stays, so re-applying after a fetch/reset only ever adds.
pub(crate) fn apply(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    extra: &[String],
) -> Result<()> {
    let inventory = load_or_plan(repo_dir, session)?;
    if inventory.is_unfiltered() {
        let _ = git_ops::checkout_in_session(repo_dir, &["sparse-checkout", "disable"], session);
        let _ = git_ops::checkout_in_session(repo_dir, &["checkout"], session);
        return Ok(());
    }
    let dirs = inventory.sparse_dirs(extra, &inventory.on_disk(repo_dir));
    let dir_refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
    git_ops::apply_sparse_checkout_in_session(repo_dir, &dir_refs, session)
}

/// Materialize the one copy `request` selects for each wanted identity.
///
/// Filesystem discovery cannot see a deferred copy, so a harness-specific
/// install must first surface the copy the shared table picks for it — and
/// only that copy.
///
/// Returns `true` when a directory was newly materialized.
pub(crate) fn materialize_for(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    wants: &[(&str, CopyRequest<'_>)],
) -> bool {
    let Ok(inventory) = load_or_plan(repo_dir, session) else {
        return false;
    };
    let dirs: Vec<String> = wants
        .iter()
        .filter_map(|(name, request)| inventory.copy_for(name, *request))
        .map(str::to_string)
        .collect();
    materialize_dirs(repo_dir, session, &dirs)
}

/// Check out `dirs` in addition to what is already materialized and record
/// them in the inventory so every later re-apply keeps them.
///
/// Returns `true` when a directory was newly materialized.
pub(crate) fn materialize_dirs(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    dirs: &[String],
) -> bool {
    if dirs.is_empty() {
        return false;
    }
    let Ok(mut inventory) = load_or_plan(repo_dir, session) else {
        return false;
    };
    let fresh: Vec<String> = dirs
        .iter()
        .filter(|dir| !repo_dir.join(dir).exists())
        .cloned()
        .collect();

    // Record intent even when the files are already on disk: the sparse
    // pattern must keep the directory.
    inventory.materialized.extend(dirs.iter().cloned());
    inventory.materialized.sort();
    inventory.materialized.dedup();
    save_sidecar(repo_dir, &inventory);

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
                inventory.revision = revision;
            }
            save_sidecar(repo_dir, &inventory);
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

/// Carry an inventory computed from the real repository tree over to a
/// tarball-built synthetic cache entry.
///
/// The synthetic tree only contains the extracted directories, so an
/// inventory rebuilt from it would see no deferred copies at all.
/// Re-stamping the revision keeps the real one valid.
pub(crate) fn adopt_plan_for_synthetic_repo(repo_dir: &Path, inventory: &Inventory) {
    let mut inventory = inventory.clone();
    if let Ok(revision) = git_ops::head_revision(repo_dir) {
        inventory.revision = revision;
    }
    save_sidecar(repo_dir, &inventory);
}

fn sidecar_path(repo_dir: &Path) -> PathBuf {
    repo_dir.join(SIDECAR)
}

fn load_sidecar(repo_dir: &Path) -> Option<Inventory> {
    let content = std::fs::read_to_string(sidecar_path(repo_dir)).ok()?;
    serde_json::from_str::<Inventory>(&content)
        .ok()
        .filter(|inventory| inventory.format == INVENTORY_FORMAT)
}

fn save_sidecar(repo_dir: &Path, inventory: &Inventory) {
    let path = sidecar_path(repo_dir);
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_ok()
        && let Ok(content) = serde_json::to_string(inventory)
    {
        // Best effort: a failed sidecar only costs a rebuild on the next call.
        let _ = std::fs::write(path, content);
    }
}

fn plan_inventory(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    revision: &str,
) -> Result<Inventory> {
    let entries = git_ops::list_tree_entries_with_trees(repo_dir, revision)
        .context("Failed to read the repository tree while planning sparse checkout")?;

    let mut dir_shas: HashMap<String, String> = HashMap::new();
    let mut manifest_blobs: HashMap<String, String> = HashMap::new();
    let mut plugin_blobs: HashMap<&str, String> = HashMap::new();
    let mut tracked_files: Vec<&str> = Vec::new();
    for entry in &entries {
        if entry.kind == "tree" {
            dir_shas.insert(entry.path.clone(), entry.sha.clone());
            continue;
        }
        tracked_files.push(&entry.path);
        if matches!(
            entry.path.as_str(),
            ".claude-plugin/marketplace.json" | ".claude-plugin/plugin.json"
        ) {
            plugin_blobs.insert(&entry.path, entry.sha.clone());
        }
        // Repo-root `SKILL.md` is always present in cone mode; whether it is
        // a shim or a genuine skill stays with discovery.
        if entry.path.ends_with("/SKILL.md")
            && let Some(parent) = parent_dir(&entry.path)
        {
            manifest_blobs.insert(parent, entry.sha.clone());
        }
    }

    let mut inventory = Inventory {
        format: INVENTORY_FORMAT,
        revision: revision.to_string(),
        plugin_manifest: tracked_files
            .iter()
            .any(|path| path.starts_with(&format!("{PLUGIN_MANIFEST_DIR}/"))),
        ..Inventory::default()
    };
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for dir in manifest_blobs.keys() {
        if is_under_ignored_dir(dir) {
            inventory.ignored.push(dir.clone());
        } else {
            let base = dir.rsplit('/').next().unwrap_or(dir).to_lowercase();
            groups.entry(base).or_default().push(dir.clone());
        }
    }
    inventory.ignored.sort();
    if groups.is_empty() {
        return Ok(inventory);
    }
    let divergent = |dirs: &[String]| {
        dirs.iter()
            .map(|dir| dir_shas.get(dir))
            .collect::<HashSet<_>>()
            .len()
            > 1
    };
    groups.values_mut().for_each(|dirs| dirs.sort());
    let named_folders: Vec<&String> = groups
        .values()
        .filter(|dirs| divergent(dirs))
        .flatten()
        .collect();
    // Plugin manifests and the SKILL.md files that need a name share one
    // prefetch: one round-trip for the whole plan.
    let blobs = read_blobs(
        repo_dir,
        session,
        plugin_blobs
            .values()
            .chain(named_folders.iter().map(|dir| &manifest_blobs[*dir]))
            .cloned()
            .collect(),
    );
    let plugin_json = |path: &str| plugin_blobs.get(path).and_then(|oid| blobs.get(oid));
    inventory.manifest_dirs = crate::plugin_manifest::declared_skill_dir_strings(
        plugin_json(".claude-plugin/marketplace.json").map(String::as_str),
        plugin_json(".claude-plugin/plugin.json").map(String::as_str),
    );
    let names: HashMap<&String, String> = named_folders
        .into_iter()
        .filter_map(|dir| {
            let content = blobs.get(&manifest_blobs[dir])?;
            let report = crate::validation::inspect_skill_frontmatter_content(content);
            Some((dir, identity_key(report.name.as_deref(), dir)))
        })
        .collect();

    for (base, dirs) in &groups {
        let named: Option<Vec<(String, &String)>> = if divergent(dirs) {
            dirs.iter()
                .map(|dir| names.get(dir).map(|name| (name.clone(), dir)))
                .collect()
        } else {
            Some(dirs.iter().map(|dir| (base.clone(), dir)).collect())
        };
        let Some(named) = named else {
            // ponytail: names unreadable (prefetch or read failed) → D-063's
            // tree-SHA rule for this group: only byte-identical copies defer.
            let representative = pick_representative(dirs, &inventory.manifest_dirs);
            let rep_sha = dir_shas.get(&representative);
            inventory.materialized.extend(
                dirs.iter()
                    .filter(|dir| **dir == representative || dir_shas.get(*dir) != rep_sha)
                    .cloned(),
            );
            inventory.identities.insert(base.clone(), dirs.clone());
            continue;
        };
        let mut by_identity: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (identity, dir) in named {
            by_identity.entry(identity).or_default().push(dir.clone());
        }
        for (identity, copies) in by_identity {
            inventory
                .materialized
                .push(pick_representative(&copies, &inventory.manifest_dirs));
            inventory
                .identities
                .entry(identity)
                .or_default()
                .extend(copies);
        }
    }
    for copies in inventory.identities.values_mut() {
        copies.sort();
        copies.dedup();
    }
    inventory.materialized.sort();
    inventory.materialized.dedup();
    Ok(inventory)
}

fn pick_representative(dirs: &[String], manifest_dirs: &[String]) -> String {
    choose_copy(
        dirs,
        |dir| dir.as_str(),
        CopyRequest::default(),
        manifest_dirs,
    )
    .cloned()
    .expect("identity groups are never empty")
}

/// Blob contents by id. Blobs already local are read directly; the rest
/// arrive in one prefetch. Blobs that stay unreadable are absent.
fn read_blobs(
    repo_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    mut oids: Vec<String>,
) -> HashMap<String, String> {
    oids.sort();
    oids.dedup();
    let read = |oid: &str| {
        git_ops::read_local_blob(repo_dir, oid, crate::validation::MAX_MANIFEST_BYTES).ok()
    };
    let mut contents = HashMap::new();
    let mut missing = Vec::new();
    for oid in oids {
        match read(&oid) {
            Some(content) => {
                contents.insert(oid, content);
            }
            None => missing.push(oid),
        }
    }
    if missing.is_empty() {
        return contents;
    }
    match git_ops::prefetch_blobs_in_session(repo_dir, &missing, session) {
        Ok(()) => {
            for oid in missing {
                if let Some(content) = read(&oid) {
                    contents.insert(oid, content);
                }
            }
        }
        Err(error) => warn!(
            target: "repo_scanner",
            path = %repo_dir.display(),
            blobs = missing.len(),
            error = %error,
            "could not prefetch manifest blobs; same-name copies stay materialized"
        ),
    }
    contents
}

fn parent_dir(path: &str) -> Option<String> {
    let parent = Path::new(path).parent()?.to_string_lossy().to_string();
    (!parent.is_empty()).then_some(parent)
}

/// Batch sibling directories into their shared parent.
///
/// Cone-mode sparse checkout pulls whole directories, so once two chosen
/// directories share a parent there is no extra cost in taking everything
/// under it — and one pattern beats two. A parent that holds any `blocked`
/// folder (deferred copy, ignored fixture) is never taken whole.
pub(crate) fn compact_to_common_parents(dirs: &[String], blocked: &[String]) -> Vec<String> {
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
    let covers_blocked = |parent: &str| {
        blocked
            .iter()
            .any(|dir| dir.starts_with(&format!("{parent}/")))
    };

    let mut result = Vec::new();
    let mut handled: HashSet<String> = HashSet::new();

    for dir in dirs {
        if handled.contains(dir) {
            continue;
        }
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if parent_counts.get(&parent_str).copied().unwrap_or(0) >= 2
                && !parent_str.is_empty()
                && !covers_blocked(&parent_str)
            {
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
