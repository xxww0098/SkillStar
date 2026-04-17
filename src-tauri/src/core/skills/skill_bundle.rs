use anyhow::{Context, Result};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};

use super::security_scan;

// ── Types ───────────────────────────────────────────────────────────

const FORMAT_VERSION: u32 = 1;
const MANIFEST_NAME: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: u32,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub created_at: String,
    pub files: Vec<String>,
    /// SHA-256 hex digest of all file contents (sorted, concatenated)
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportBundleResult {
    pub name: String,
    pub description: String,
    pub file_count: usize,
    /// true if a skill with the same name already existed and was replaced
    pub replaced: bool,
}

// ── Export ───────────────────────────────────────────────────────────

/// Export a skill as a `.ags` bundle.
///
/// The output file is written to the specified path, or defaults to the
/// downloads directory. Returns the absolute path of the generated file.
pub fn export_bundle(skill_name: &str, output_path: Option<&str>) -> Result<PathBuf> {
    let hub = crate::core::infra::paths::hub_skills_dir();
    let skill_dir = hub.join(skill_name);

    if !skill_dir.exists() {
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let effective_dir = if crate::core::infra::fs_ops::is_link(&skill_dir) {
        crate::core::infra::fs_ops::read_link_resolved(&skill_dir)
            .unwrap_or_else(|_| skill_dir.clone())
    } else {
        skill_dir.clone()
    };

    // Collect files (exclude .git)
    let mut files: Vec<String> = Vec::new();
    collect_files(&effective_dir, &effective_dir, &mut files);
    files.sort();

    // Compute checksum over sorted file contents
    let checksum = compute_content_checksum(&effective_dir, &files)?;

    // Extract description from SKILL.md frontmatter
    let description = super::skill::extract_skill_description(&effective_dir);

    let manifest = BundleManifest {
        format_version: FORMAT_VERSION,
        name: skill_name.to_string(),
        description,
        version: "1.0.0".to_string(),
        author: String::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
        files: files.clone(),
        checksum,
    };

    // Determine output path
    let out_path = match output_path {
        Some(p) => PathBuf::from(p),
        None => {
            let out_dir = dirs::download_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("."));
            out_dir.join(format!("{}.ags", skill_name))
        }
    };
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Build tar.gz
    let file = std::fs::File::create(&out_path)
        .with_context(|| format!("Cannot create output file: {}", out_path.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    // Write manifest.json first
    let manifest_bytes = serde_json::to_string_pretty(&manifest)?;
    let manifest_bytes = manifest_bytes.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, MANIFEST_NAME, manifest_bytes)?;

    // Write each file
    for rel_path in &files {
        let abs = effective_dir.join(rel_path);
        let metadata = std::fs::metadata(&abs)?;
        let mut f = std::fs::File::open(&abs)?;

        let mut header = tar::Header::new_gnu();
        header.set_size(metadata.len());
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, rel_path, &mut f)?;
    }

    tar.into_inner()?.finish()?;

    Ok(out_path)
}

// ── Preview ─────────────────────────────────────────────────────────

/// Read only the manifest from a `.ags` file without extracting.
pub fn preview_bundle(file_path: &str) -> Result<BundleManifest> {
    let file = std::fs::File::open(file_path)
        .with_context(|| format!("Cannot open bundle: {}", file_path))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();
        if path == MANIFEST_NAME {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            let manifest: BundleManifest =
                serde_json::from_str(&content).context("Invalid manifest.json in bundle")?;
            return Ok(manifest);
        }
    }

    anyhow::bail!("Bundle does not contain manifest.json")
}

// ── Import ──────────────────────────────────────────────────────────

/// Import a `.ags` file into the hub.
///
/// If `force` is true, replaces an existing skill with the same name.
pub fn import_bundle(file_path: &str, force: bool) -> Result<ImportBundleResult> {
    // First pass: read and validate manifest
    let manifest = preview_bundle(file_path)?;

    if manifest.format_version > FORMAT_VERSION {
        anyhow::bail!(
            "Bundle format version {} is not supported (max: {})",
            manifest.format_version,
            FORMAT_VERSION
        );
    }

    let hub = crate::core::infra::paths::hub_skills_dir();
    let target_dir = hub.join(&manifest.name);
    let replaced = target_dir.exists();

    if replaced && !force {
        anyhow::bail!("CONFLICT:{}", manifest.name);
    }

    // Extract to a temp directory first, then move atomically
    let temp_dir = hub.join(format!(".importing-{}", manifest.name));
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Second pass: extract files
    let file = std::fs::File::open(file_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();

        // Skip manifest.json — we already read it
        if path == MANIFEST_NAME {
            continue;
        }

        // Security: reject absolute paths and path traversal
        if path.starts_with('/') || path.contains("..") {
            continue;
        }

        let dest = temp_dir.join(&path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
        std::fs::write(&dest, &content)?;
    }

    // Verify checksum
    let mut extracted_files: Vec<String> = Vec::new();
    collect_files(&temp_dir, &temp_dir, &mut extracted_files);
    extracted_files.sort();

    let actual_checksum = compute_content_checksum(&temp_dir, &extracted_files)?;
    if actual_checksum != manifest.checksum {
        let _ = std::fs::remove_dir_all(&temp_dir);
        anyhow::bail!(
            "Checksum mismatch: bundle may be corrupted (expected {}, got {})",
            &manifest.checksum[..12],
            &actual_checksum[..12]
        );
    }

    // Replace existing if needed
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)?;
    }
    std::fs::rename(&temp_dir, &target_dir)?;

    // Invalidate security scan cache — imported content may differ from prior scan
    security_scan::invalidate_skill_cache(&manifest.name);

    Ok(ImportBundleResult {
        name: manifest.name,
        description: manifest.description,
        file_count: manifest.files.len(),
        replaced,
    })
}

// ── Multi-skill export ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiManifestEntry {
    pub name: String,
    pub description: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiManifest {
    pub format_version: u32,
    pub created_at: String,
    pub skills: Vec<MultiManifestEntry>,
    pub checksum: String,
}

/// Export multiple skills into a single `.agd` bundle archive.
///
/// Each skill is stored under `<skill_name>/` prefix inside the tar.gz.
/// A top-level `multi_manifest.json` describes all contained skills.
pub fn export_multi_bundle(skill_names: &[String], output_path: &str) -> Result<PathBuf> {
    use std::io::Read;

    let hub = crate::core::infra::paths::hub_skills_dir();
    let out = PathBuf::from(output_path);

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(&out)
        .with_context(|| format!("Cannot create output file: {}", out.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    let mut manifest_entries: Vec<MultiManifestEntry> = Vec::new();
    let mut global_hasher = Sha256::new();

    for skill_name in skill_names {
        let skill_dir = hub.join(skill_name);
        if !skill_dir.exists() {
            continue;
        }

        let effective_dir = if crate::core::infra::fs_ops::is_link(&skill_dir) {
            crate::core::infra::fs_ops::read_link_resolved(&skill_dir)
                .unwrap_or_else(|_| skill_dir.clone())
        } else {
            skill_dir.clone()
        };

        let mut files: Vec<String> = Vec::new();
        collect_files(&effective_dir, &effective_dir, &mut files);
        files.sort();

        let description = super::skill::extract_skill_description(&effective_dir);
        manifest_entries.push(MultiManifestEntry {
            name: skill_name.clone(),
            description,
            file_count: files.len(),
        });

        for rel_path in &files {
            let abs = effective_dir.join(rel_path);
            let metadata = std::fs::metadata(&abs)?;
            let mut f = std::fs::File::open(&abs)?;

            // Read content for checksum
            let mut content = Vec::new();
            f.read_to_end(&mut content)?;
            global_hasher.update(&content);

            let archive_path = format!("{}/{}", skill_name, rel_path);
            let mut header = tar::Header::new_gnu();
            header.set_size(metadata.len());
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, &archive_path, content.as_slice())?;
        }
    }

    let hash = global_hasher.finalize();
    let checksum: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

    let manifest = MultiManifest {
        format_version: FORMAT_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        skills: manifest_entries,
        checksum: format!("sha256:{}", checksum),
    };

    let manifest_bytes = serde_json::to_string_pretty(&manifest)?;
    let manifest_bytes = manifest_bytes.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "multi_manifest.json", manifest_bytes)?;

    tar.into_inner()?.finish()?;
    Ok(out)
}

// ── Multi-skill import ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportMultiBundleResult {
    /// Names of all skills that were imported
    pub skill_names: Vec<String>,
    /// Total number of files extracted
    pub total_file_count: usize,
    /// Number of skills that replaced existing ones
    pub replaced_count: usize,
}

/// Import a `.agd` multi-bundle into the hub.
///
/// Each skill directory inside the archive is extracted into the hub.
/// If `force` is true, existing skills with the same name are replaced.
pub fn import_multi_bundle(file_path: &str, force: bool) -> Result<ImportMultiBundleResult> {
    let file = std::fs::File::open(file_path)
        .with_context(|| format!("Cannot open bundle: {}", file_path))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    // First pass: read multi_manifest.json
    let mut manifest: Option<MultiManifest> = None;
    let mut entries_data: Vec<(String, Vec<u8>)> = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();

        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;

        if path == "multi_manifest.json" {
            manifest = Some(
                serde_json::from_slice(&content)
                    .context("Invalid multi_manifest.json in bundle")?,
            );
        } else if path == MANIFEST_NAME {
            // Single-skill bundle opened as multi — fallback to single import
            drop(entries_data);
            return import_bundle(file_path, force).map(|r| ImportMultiBundleResult {
                skill_names: vec![r.name],
                total_file_count: r.file_count,
                replaced_count: if r.replaced { 1 } else { 0 },
            });
        } else {
            entries_data.push((path, content));
        }
    }

    let manifest = manifest.ok_or_else(|| {
        anyhow::anyhow!("Bundle does not contain multi_manifest.json or manifest.json")
    })?;

    let hub = crate::core::infra::paths::hub_skills_dir();
    let known_skills: std::collections::HashSet<String> =
        manifest.skills.iter().map(|s| s.name.clone()).collect();

    // Check for conflicts if not forcing
    if !force {
        for skill_name in &known_skills {
            let target = hub.join(skill_name);
            if target.exists() {
                anyhow::bail!("CONFLICT:{}", skill_name);
            }
        }
    }

    // Extract into temp dirs, then move atomically
    let mut imported_names: Vec<String> = Vec::new();
    let mut total_files = 0usize;
    let mut replaced_count = 0usize;

    // Group entries by skill name (first path component)
    let mut skill_files: std::collections::HashMap<String, Vec<(String, Vec<u8>)>> =
        std::collections::HashMap::new();
    for (path, content) in entries_data {
        // Security: reject absolute paths and path traversal
        if path.starts_with('/') || path.contains("..") {
            continue;
        }
        // First component is the skill name
        if let Some(slash_pos) = path.find('/') {
            let skill_name = path[..slash_pos].to_string();
            let rel_path = path[slash_pos + 1..].to_string();
            if !rel_path.is_empty() && known_skills.contains(&skill_name) {
                skill_files
                    .entry(skill_name)
                    .or_default()
                    .push((rel_path, content));
            }
        }
    }

    for skill_name in &manifest.skills {
        let name = &skill_name.name;
        let files = match skill_files.remove(name) {
            Some(f) => f,
            None => continue,
        };

        let target_dir = hub.join(name);
        let was_replaced = target_dir.exists();

        let temp_dir = hub.join(format!(".importing-{}", name));
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir)?;
        }
        std::fs::create_dir_all(&temp_dir)?;

        for (rel_path, content) in &files {
            let dest = temp_dir.join(rel_path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, content)?;
        }

        // Replace existing if needed
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)?;
            replaced_count += 1;
        }
        std::fs::rename(&temp_dir, &target_dir)?;

        // Invalidate security scan cache
        security_scan::invalidate_skill_cache(name);

        total_files += files.len();
        imported_names.push(name.clone());

        if was_replaced {
            // Already counted above
        }
    }

    crate::core::installed_skill::invalidate_cache();

    Ok(ImportMultiBundleResult {
        skill_names: imported_names,
        total_file_count: total_files,
        replaced_count,
    })
}

/// Preview a `.agd` multi-bundle manifest without extracting.
pub fn preview_multi_bundle(file_path: &str) -> Result<MultiManifest> {
    let file = std::fs::File::open(file_path)
        .with_context(|| format!("Cannot open bundle: {}", file_path))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();
        if path == "multi_manifest.json" {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            let manifest: MultiManifest =
                serde_json::from_str(&content).context("Invalid multi_manifest.json in bundle")?;
            return Ok(manifest);
        }
    }

    anyhow::bail!("Bundle does not contain multi_manifest.json")
}

// ── Helpers ─────────────────────────────────────────────────────────

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden files/dirs and .git
        if name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            collect_files(root, &path, files);
        } else if let Ok(rel) = path.strip_prefix(root) {
            files.push(rel.to_string_lossy().to_string());
        }
    }
}

fn compute_content_checksum(root: &Path, sorted_files: &[String]) -> Result<String> {
    use std::io::Read;
    let mut hasher = Sha256::new();
    // Reuse a single 64 KB buffer across all files — zero extra allocation per file.
    let mut buf = vec![0u8; 64 * 1024];
    for rel_path in sorted_files {
        let abs = root.join(rel_path);
        let file = std::fs::File::open(&abs)
            .with_context(|| format!("Failed to open file for checksum: {}", abs.display()))?;
        let mut reader = std::io::BufReader::new(file);
        loop {
            let bytes_read = reader
                .read(&mut buf)
                .with_context(|| format!("Failed to read {}", abs.display()))?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buf[..bytes_read]);
        }
    }
    let hash = hasher.finalize();
    let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(format!("sha256:{}", hex))
}
