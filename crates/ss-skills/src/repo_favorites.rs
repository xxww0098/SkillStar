//! Repositories the user marked as favorites on this machine.
//!
//! The set is user intent. It cannot be rebuilt from the install lock or the
//! marketplace, so it lives at `data/skills/repo-favorites.json`. Uninstalling
//! a repository does not drop its favorite. A file this build does not
//! understand is left untouched.

use std::collections::BTreeSet;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedFavorites {
    Ready(BTreeSet<String>),
    /// Written by a newer SkillStar. Do not project it as empty and do not
    /// overwrite it.
    NewerVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteWrite {
    Stored,
    /// A later edit from this process already landed.
    Stale,
    NewerVersion,
}

#[derive(Debug, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    sources: BTreeSet<String>,
}

struct Gate {
    applied: u64,
}

static GATE: Mutex<Gate> = Mutex::new(Gate { applied: 0 });

fn usable(source: &str) -> bool {
    !source.is_empty() && !source.chars().any(|ch| ch.is_control())
}

fn read_file() -> LoadedFavorites {
    let path = ss_core::infra::paths::repo_favorites_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return LoadedFavorites::Ready(BTreeSet::new());
    };
    let Ok(file) = serde_json::from_str::<File>(&content) else {
        tracing::warn!(
            target: "skills",
            path = %path.display(),
            "repo favorites file is unreadable; treating it as empty"
        );
        return LoadedFavorites::Ready(BTreeSet::new());
    };
    if file.version > VERSION {
        return LoadedFavorites::NewerVersion;
    }
    LoadedFavorites::Ready(
        file.sources
            .into_iter()
            .filter(|source| usable(source))
            .collect(),
    )
}

/// Favorites currently on disk. A missing or broken file is an empty set.
pub fn load() -> LoadedFavorites {
    read_file()
}

/// Replace the favorite set. `generation` orders overlapping saves from one
/// process: an older generation does not clobber a newer one. A file with a
/// higher `version` is not written.
pub fn save(generation: u64, sources: BTreeSet<String>) -> Result<FavoriteWrite, String> {
    let mut gate = GATE.lock().unwrap_or_else(|poison| poison.into_inner());
    if generation < gate.applied {
        return Ok(FavoriteWrite::Stale);
    }
    if matches!(read_file(), LoadedFavorites::NewerVersion) {
        return Ok(FavoriteWrite::NewerVersion);
    }
    let file = File {
        version: VERSION,
        sources: sources
            .into_iter()
            .filter(|source| usable(source))
            .collect(),
    };
    let content = serde_json::to_string_pretty(&file).map_err(|err| err.to_string())?;
    ss_core::infra::fs_ops::atomic_write(
        &ss_core::infra::paths::repo_favorites_path(),
        content.as_bytes(),
    )
    .map_err(|err| err.to_string())?;
    gate.applied = generation;
    Ok(FavoriteWrite::Stored)
}
