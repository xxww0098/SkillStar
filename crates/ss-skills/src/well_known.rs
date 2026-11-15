//! Well-known skill discovery.
//!
//! HTTP(S) URLs that are not a known Git host are tried as an agentskills.io
//! discovery index before Git clone. Discovery 0.2.0 requires a sha256 digest.
//! A path-scoped miss never falls back to the host root index.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::source_resolver::Source;

mod archive;
mod index;

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_INDEX_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARTIFACT_BYTES: usize = archive::MAX_UNPACKED_BYTES as usize;
const USER_AGENT: &str = concat!("SkillStar/", env!("CARGO_PKG_VERSION"));

pub(crate) struct Acquired {
    pub spec: Source,
    pub revision: String,
    root: PathBuf,
}

impl Acquired {
    pub(crate) fn dir(&self) -> &Path {
        &self.root
    }
}

impl Drop for Acquired {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn is_candidate(input: &str) -> bool {
    index::is_candidate(input)
}

/// Fetch a well-known index and materialize the skills it can prove.
///
/// `Ok(None)` means this URL is not a discovery source, so Git clone may
/// continue. A scoped miss is an error and must not continue.
pub(crate) fn acquire(input: &str) -> Result<Option<Acquired>> {
    let input = input.trim();
    if !is_candidate(input) {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to start well-known discovery")?;
    runtime.block_on(acquire_async(input))
}

async fn acquire_async(input: &str) -> Result<Option<Acquired>> {
    let scope = index::scope_of(input);
    let candidates = index::index_candidates(input);
    let client = ss_core::infra::http_client::probe_http_client_no_redirect(TIMEOUT)?;
    let mut hits = Vec::new();
    for candidate in &candidates {
        match fetch_index(&client, candidate).await? {
            Some(index::ParsedIndex::Rejected) => {
                bail!("Unsupported well-known discovery schema");
            }
            Some(index::ParsedIndex::Entries(entries)) => {
                hits.push(index::FetchedIndex {
                    resolved_base: candidate.resolved_base.clone(),
                    entries,
                });
            }
            None => {}
        }
    }
    if hits.is_empty() {
        return Ok(None);
    }

    let allowed = index::indexes_to_try(scope.as_ref(), &hits);
    for hit in allowed {
        if let Some(acquired) = materialize(input, hit).await? {
            return Ok(Some(acquired));
        }
    }
    if let Some(miss) = index::scope_miss_if_root_published(scope.as_ref(), &hits) {
        bail!(miss.message());
    }
    bail!("Well-known index did not contain an installable skill");
}

async fn fetch_index(
    client: &reqwest::Client,
    candidate: &index::IndexCandidate,
) -> Result<Option<index::ParsedIndex>> {
    let response = match client
        .get(&candidate.index_url)
        .header("user-agent", USER_AGENT)
        .header("accept", "application/json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(
                target: "well_known",
                url = %index::display_url(&candidate.index_url),
                "well-known index request failed: {error}"
            );
            return Ok(None);
        }
    };
    if response.status() != reqwest::StatusCode::OK {
        return Ok(None);
    }
    if !same_resource(&candidate.index_url, response.url()) {
        return Ok(None);
    }
    let Some(body) = read_capped(response, MAX_INDEX_BYTES).await? else {
        return Ok(None);
    };
    let Ok(raw) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return Ok(None);
    };
    Ok(index::parse_index(
        &raw,
        &candidate.index_url,
        &candidate.resolved_base,
        &candidate.well_known_path,
    ))
}

async fn materialize(input: &str, hit: &index::FetchedIndex) -> Result<Option<Acquired>> {
    let root = temp_root()?;
    let client = ss_core::infra::http_client::probe_http_client(TIMEOUT)?;
    let mut wrote = false;
    for entry in &hit.entries {
        let dir = root.join(entry.name());
        if std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let installed = install_entry(&client, entry, &dir).await.unwrap_or(false);
        if installed && dir.join("SKILL.md").is_file() {
            wrote = true;
        } else {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    if !wrote {
        let _ = std::fs::remove_dir_all(&root);
        return Ok(None);
    }
    Ok(Some(Acquired {
        spec: source_for(input),
        revision: revision_of(&hit.entries),
        root,
    }))
}

async fn install_entry(
    client: &reqwest::Client,
    entry: &index::IndexEntry,
    dir: &Path,
) -> Result<bool> {
    match entry {
        index::IndexEntry::V2 {
            kind,
            artifact_url,
            digest,
            ..
        } => {
            let Some(bytes) = fetch_artifact(client, artifact_url).await? else {
                return Ok(false);
            };
            if sha256_prefixed(&bytes) != *digest {
                return Ok(false);
            }
            match kind {
                index::ArtifactKind::SkillMd => {
                    std::fs::write(dir.join("SKILL.md"), &bytes)?;
                }
                index::ArtifactKind::Archive => {
                    archive::unpack_archive(&bytes, artifact_url, dir)?;
                }
            }
            Ok(dir.join("SKILL.md").is_file())
        }
        index::IndexEntry::V1 {
            name,
            files,
            base_url,
            well_known_path,
        } => {
            let client = ss_core::infra::http_client::probe_http_client_no_redirect(TIMEOUT)?;
            let mut wrote_skill = false;
            for file in files {
                let url = format!(
                    "{}/{}/{}/{}",
                    base_url.trim_end_matches('/'),
                    well_known_path,
                    name,
                    file
                );
                let Some(bytes) = fetch_exact(&client, &url, MAX_ARTIFACT_BYTES).await? else {
                    continue;
                };
                let rel = archive::safe_archive_path(file).context("Unsafe legacy skill path")?;
                let path = dir.join(&rel);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, &bytes)?;
                if rel.eq_ignore_ascii_case("SKILL.md") {
                    wrote_skill = true;
                }
            }
            Ok(wrote_skill)
        }
    }
}

async fn fetch_artifact(client: &reqwest::Client, url: &str) -> Result<Option<Vec<u8>>> {
    let Ok(response) = client
        .get(url)
        .header("user-agent", USER_AGENT)
        .send()
        .await
    else {
        return Ok(None);
    };
    if !response.status().is_success() {
        return Ok(None);
    }
    let final_url = response.url().clone();
    if final_url.scheme() != "http" && final_url.scheme() != "https" {
        return Ok(None);
    }
    read_capped(response, MAX_ARTIFACT_BYTES).await
}

async fn fetch_exact(client: &reqwest::Client, url: &str, max: usize) -> Result<Option<Vec<u8>>> {
    let Ok(response) = client
        .get(url)
        .header("user-agent", USER_AGENT)
        .send()
        .await
    else {
        return Ok(None);
    };
    if response.status() != reqwest::StatusCode::OK || !same_resource(url, response.url()) {
        return Ok(None);
    }
    read_capped(response, max).await
}

async fn read_capped(mut response: reqwest::Response, max: usize) -> Result<Option<Vec<u8>>> {
    if response
        .content_length()
        .is_some_and(|len| len > max as u64)
    {
        return Ok(None);
    }
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if out.len().saturating_add(chunk.len()) > max {
            return Ok(None);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(Some(out))
}

fn source_for(input: &str) -> Source {
    let url = url::Url::parse(input).ok();
    let host = url
        .as_ref()
        .and_then(|url| url.host_str())
        .unwrap_or("host");
    let path = url
        .as_ref()
        .map(|url| url.path().trim_end_matches('/'))
        .filter(|path| !path.is_empty())
        .unwrap_or("");
    Source {
        repo_url: index::display_url(input),
        short: format!("wellknown/{host}{path}"),
        git_ref: None,
        subpath: None,
        skill_filter: None,
        well_known: true,
    }
}

fn revision_of(entries: &[index::IndexEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.identity().as_bytes());
        hasher.update([0xff]);
    }
    format_digest(&hasher.finalize())
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    format_digest(&Sha256::digest(bytes))
}

fn format_digest(digest: &[u8]) -> String {
    let mut out = String::from("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn same_resource(expected: &str, actual: &reqwest::Url) -> bool {
    let Ok(expected) = url::Url::parse(expected) else {
        return false;
    };
    expected.scheme() == actual.scheme()
        && expected.host_str() == actual.host_str()
        && expected.port() == actual.port()
        && expected.path() == actual.path()
}

fn temp_root() -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("skillstar-wellknown-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&path).context("Failed to create well-known staging directory")?;
    Ok(path)
}

#[cfg(test)]
#[path = "well_known_tests.rs"]
mod tests;
