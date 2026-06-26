//! Data model for S3 cloud sync: target definitions and the cloud manifest.
//!
//! `S3TargetDef` stores **only non-sensitive metadata**. The
//! `secret_access_key` lives in the system keyring (see [`crate::store`]),
//! keyed by the target `id`, so the on-disk TOML is safe to back up.

use serde::{Deserialize, Serialize};

/// A user-defined S3-compatible sync target (Cloudflare R2, Backblaze B2,
/// 七牛云, 阿里云 OSS, 腾讯云 COS, AWS S3, MinIO, …). Non-sensitive fields only.
///
/// `id` is stable across renames and is the keyring credential key. New targets
/// should get an auto-generated id; see [`crate::store::TargetsStore::fresh_id`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3TargetDef {
    /// Stable unique id (e.g. `s3_<unix_ms>`). Used as the keyring account name.
    pub id: String,
    /// Human-friendly label shown in the UI.
    pub display_name: String,
    /// S3-compatible endpoint URL. For AWS S3 this may be empty (region-based).
    /// Required for R2 (`https://<account>.r2.cloudflarestorage.com`), MinIO,
    /// and other S3-compatible services.
    #[serde(default)]
    pub endpoint_url: String,
    /// Region (e.g. `us-east-1`, `auto` for R2, `cn-east-1` for 七牛/OSS).
    #[serde(default = "default_region")]
    pub region: String,
    /// Bucket name.
    pub bucket: String,
    /// Key prefix inside the bucket (e.g. `skillstar/`). The manifest lands at
    /// `<prefix>manifest.json` and tarballs at `<prefix>tarballs/<name>/<sha>.tar.gz`.
    /// Normalised to end with `/` on save.
    #[serde(default)]
    pub prefix: String,
    /// Access key id (public-ish; safe to persist in the TOML).
    pub access_key_id: String,
    /// When `true`, use path-style addressing (`endpoint/bucket/key`) instead of
    /// virtual-host-style (`bucket.endpoint/key`). Required for MinIO and some
    /// on-prem S3 services; harmless for R2/B2.
    #[serde(default)]
    pub force_path_style: bool,
}

fn default_region() -> String {
    "us-east-1".to_string()
}

impl S3TargetDef {
    /// Normalised prefix guaranteed to end with `/` (empty → `""`).
    pub fn normalised_prefix(&self) -> String {
        let p = self.prefix.trim();
        if p.is_empty() {
            String::new()
        } else if p.ends_with('/') {
            p.to_string()
        } else {
            format!("{p}/")
        }
    }

    /// Full object key for the manifest under this target's prefix.
    pub fn manifest_key(&self) -> String {
        format!("{}manifest.json", self.normalised_prefix())
    }

    /// Full object key for a local-skill tarball under this target's prefix.
    pub fn tarball_key(&self, skill_name: &str, sha256: &str) -> String {
        format!(
            "{}tarballs/{skill_name}/{sha256}.tar.gz",
            self.normalised_prefix()
        )
    }
}

// ── Cloud manifest ──────────────────────────────────────────────────

/// The top-level manifest persisted at `<prefix>manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    /// RFC3339 timestamp of when this manifest was generated.
    pub generated_at: String,
    /// Origin device id (hostname + suffix).
    pub device_id: String,
    pub skills: Vec<ManifestEntry>,
}

impl Manifest {
    pub const CURRENT_VERSION: u32 = 1;
}

/// A single skill record in the manifest. Dispatched at restore time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ManifestEntry {
    /// Skill with a git origin — only metadata recorded, no files uploaded.
    /// Device B restores it via the normal git install path.
    Hub {
        name: String,
        git_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_folder: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tree_hash: Option<String>,
        #[serde(default)]
        description: String,
    },
    /// User-authored local skill — packed as a content-addressed tarball.
    /// Device B downloads and unpacks it into the local hub.
    Local {
        name: String,
        /// Object key of the tarball under this target's bucket.
        tarball_key: String,
        /// sha256 of the tarball bytes (content-addressed key component).
        sha256: String,
        size_bytes: u64,
        #[serde(default)]
        description: String,
        /// RFC3339 timestamp of when the tarball was uploaded.
        uploaded_at: String,
    },
}

impl ManifestEntry {
    pub fn name(&self) -> &str {
        match self {
            ManifestEntry::Hub { name, .. } => name,
            ManifestEntry::Local { name, .. } => name,
        }
    }
}

/// Result of probing a target's bucket with HeadBucket.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionTestResult {
    pub latency_ms: u64,
}

/// Per-skill restore outcome, mirroring `ShareSkillOutcome`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InstallOutcome {
    /// Already installed in the local hub before this run.
    Existing { name: String },
    /// Freshly installed from a git repo (hub kind).
    Installed { name: String },
    /// Local skill tarball downloaded and unpacked.
    Restored { name: String },
    /// Skipped because neither git url nor tarball resolved.
    Skipped { name: String, reason: String },
}

/// Aggregate restore result, mirroring `ShareCodeInstallSummary`.
#[derive(Debug, Clone, Serialize)]
pub struct InstallSummary {
    pub requested_count: usize,
    pub installed_names: Vec<String>,
    pub existing_names: Vec<String>,
    pub restored_names: Vec<String>,
    pub skipped_names: Vec<String>,
    pub outcomes: Vec<InstallOutcome>,
}

/// A manifest entry annotated with whether it is installed on this device —
/// returned by `pull_cloud_manifest` for the UI to badge skills.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestEntryView {
    #[serde(flatten)]
    pub entry: ManifestEntry,
    pub installed_locally: bool,
}

/// Push summary returned to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushSummary {
    pub hub_count: usize,
    pub local_count: usize,
    pub tarballs_uploaded: usize,
    pub tarballs_skipped: usize,
    pub manifest_uploaded: bool,
}
