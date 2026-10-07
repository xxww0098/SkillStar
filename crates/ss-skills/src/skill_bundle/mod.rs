use anyhow::{Context, Result};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};

// ── Types ───────────────────────────────────────────────────────────

mod import;

pub use import::{import_bundle, import_multi_bundle};

const FORMAT_VERSION: u32 = 1;
const MANIFEST_NAME: &str = "manifest.json";
const MULTI_MANIFEST_NAME: &str = "multi_manifest.json";

/// Reject archive entry paths that could escape the extraction root.
///
/// Tar entries are spec-mandated to use forward slashes, so a backslash (or a
/// Windows drive prefix like `C:\`) only appears in a maliciously crafted or
/// non-standard archive. Rejecting them is pure defense-in-depth — legitimate
/// `.ags`/`.agd` bundles written by this module always use `/`-delimited
/// entries — but it prevents a path-traversal entry that targets Windows-style
/// `..\` traversal or an absolute `C:\` path from being joined onto the temp
/// extraction dir.
fn is_unsafe_archive_path(path: &str) -> bool {
    path.starts_with('/')
        || path.contains('\\')
        || path.contains("..")
        // Windows drive letter prefix, e.g. `C:` / `c:`.
        || {
            let bytes = path.as_bytes();
            bytes.len() >= 2
                && bytes[1] == b':'
                && bytes[0].is_ascii_alphabetic()
        }
}

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
    crate::content::validate_skill_name(skill_name)
        .map_err(|error| anyhow::anyhow!("Invalid Skill name: {error}"))?;
    let hub = ss_core::infra::paths::hub_skills_dir();
    let skill_dir = hub.join(skill_name);

    if !skill_dir.exists() {
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let effective_dir = if ss_core::infra::fs_ops::is_link(&skill_dir) {
        ss_core::infra::fs_ops::read_link_resolved(&skill_dir).unwrap_or_else(|_| skill_dir.clone())
    } else {
        skill_dir.clone()
    };

    // Frontmatter quality gate: only valid skills may be exported as bundles
    // (mirrors Anthropic's package-before-validate flow).
    crate::validation::ensure_installable(&effective_dir).map_err(anyhow::Error::msg)?;

    // Collect files (exclude .git)
    let mut files: Vec<String> = Vec::new();
    collect_files(&effective_dir, &effective_dir, &mut files);
    files.sort();

    // Compute checksum over sorted file contents
    let checksum = compute_content_checksum(&effective_dir, &files)?;

    // Extract description from SKILL.md frontmatter
    let description = ss_core::types::extract_skill_description(&effective_dir);

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
        let entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();
        if path == MANIFEST_NAME {
            let mut content = String::new();
            entry
                .take(import::MAX_ENTRY_BYTES)
                .read_to_string(&mut content)?;
            let manifest: BundleManifest =
                serde_json::from_str(&content).context("Invalid manifest.json in bundle")?;
            return Ok(manifest);
        }
    }

    anyhow::bail!("Bundle does not contain manifest.json")
}

// ── Unified entry ───────────────────────────────────────────────────

/// What [`import_any_bundle`] installed — one `.ags` skill or every skill of
/// an `.agd` deck bundle.
#[derive(Debug, Clone)]
pub enum AnyBundleImport {
    Single(ImportBundleResult),
    Multi(ImportMultiBundleResult),
}

/// Import a picked bundle file without asking the caller to know its flavour.
///
/// UIs that accept both `.ags` and `.agd` in one file picker (the GPUI import
/// dialog, the card toolbar) must not route a deck bundle into the
/// single-skill importer — its manifest is `multi_manifest.json`, so the
/// single importer reports a missing `manifest.json` and installs nothing.
/// Dispatch on the extension; the multi importer still falls back to a
/// single-skill import when the archive actually contains `manifest.json`.
pub fn import_any_bundle(file_path: &str, force: bool) -> Result<AnyBundleImport> {
    let is_deck_bundle = Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("agd"));
    if is_deck_bundle {
        import_multi_bundle(file_path, force).map(AnyBundleImport::Multi)
    } else {
        import_bundle(file_path, force).map(AnyBundleImport::Single)
    }
}

/// Deck display name for a picked bundle file. Exports are named
/// `<deck>-bundle-<timestamp>.agd`, so strip that suffix (the React
/// importer's `-bundle-[\d-T]+$` rule) and a re-imported deck keeps its
/// original name instead of wearing the timestamp.
pub fn deck_name_from_bundle_path(file_path: &str) -> String {
    let stem = Path::new(file_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if let Some(pos) = stem.rfind("-bundle-") {
        let suffix = &stem[pos + "-bundle-".len()..];
        let is_timestamp = !suffix.is_empty()
            && suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'-'..=b'T').contains(&b));
        if is_timestamp {
            return stem[..pos].to_string();
        }
    }
    stem.to_string()
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

    let mut unique_names = std::collections::HashSet::new();
    for skill_name in skill_names {
        crate::content::validate_skill_name(skill_name)
            .map_err(|error| anyhow::anyhow!("Invalid Skill name: {error}"))?;
        if !unique_names.insert(skill_name.to_ascii_lowercase()) {
            anyhow::bail!("Duplicate Skill name in bundle export: {skill_name}");
        }
    }

    let hub = ss_core::infra::paths::hub_skills_dir();
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

        let effective_dir = if ss_core::infra::fs_ops::is_link(&skill_dir) {
            ss_core::infra::fs_ops::read_link_resolved(&skill_dir)
                .unwrap_or_else(|_| skill_dir.clone())
        } else {
            skill_dir.clone()
        };

        // Frontmatter quality gate: only valid skills may be exported.
        crate::validation::ensure_installable(&effective_dir).map_err(anyhow::Error::msg)?;

        let mut files: Vec<String> = Vec::new();
        collect_files(&effective_dir, &effective_dir, &mut files);
        files.sort();

        let description = ss_core::types::extract_skill_description(&effective_dir);
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
    tar.append_data(&mut header, MULTI_MANIFEST_NAME, manifest_bytes)?;

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

/// Preview a `.agd` multi-bundle manifest without extracting.
pub fn preview_multi_bundle(file_path: &str) -> Result<MultiManifest> {
    let file = std::fs::File::open(file_path)
        .with_context(|| format!("Cannot open bundle: {}", file_path))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();
        if path == MULTI_MANIFEST_NAME {
            let mut content = String::new();
            entry
                .take(import::MAX_ENTRY_BYTES)
                .read_to_string(&mut content)?;
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

        // Never follow symlinks when collecting export content: the target may
        // live outside the skill root (e.g. ~/.ssh/id_rsa) and must not be
        // bundled. Mirrors content::snapshot's record-don't-follow behaviour.
        match entry.file_type() {
            Ok(ty) if ty.is_symlink() => continue,
            _ => {}
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

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    struct Sandbox(crate::test_sandbox::Sandbox);

    impl Sandbox {
        fn new() -> Self {
            Self(crate::test_sandbox::Sandbox::new())
        }

        fn bundle_path(&self, name: &str) -> String {
            self.0.root().join(name).to_string_lossy().to_string()
        }
    }

    fn hub_skill(name: &str) {
        let dir = ss_core::infra::paths::hub_skills_dir().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} does things\n---\n\n# {name}\n"),
        )
        .unwrap();
    }

    fn remove_hub_skill(name: &str) {
        std::fs::remove_dir_all(ss_core::infra::paths::hub_skills_dir().join(name)).unwrap();
    }

    #[test]
    fn multi_bundle_roundtrip_reinstalls_every_skill() {
        let sandbox = Sandbox::new();
        hub_skill("alpha");
        hub_skill("beta");
        let bundle = sandbox.bundle_path("deck.agd");
        export_multi_bundle(&["alpha".into(), "beta".into()], &bundle).unwrap();

        remove_hub_skill("alpha");
        remove_hub_skill("beta");

        let result = import_multi_bundle(&bundle, false).unwrap();
        assert_eq!(result.skill_names, ["alpha", "beta"]);
        let hub = ss_core::infra::paths::hub_skills_dir();
        assert!(hub.join("alpha/SKILL.md").is_file());
        assert!(hub.join("beta/SKILL.md").is_file());
    }

    #[test]
    fn single_importer_rejects_an_agd_multi_bundle() {
        // The GPUI import dialog used to route every picked file through the
        // single-skill importer, so a deck bundle died on a missing
        // manifest.json instead of installing its skills.
        let sandbox = Sandbox::new();
        hub_skill("alpha");
        hub_skill("beta");
        let bundle = sandbox.bundle_path("deck.agd");
        export_multi_bundle(&["alpha".into(), "beta".into()], &bundle).unwrap();
        remove_hub_skill("alpha");
        remove_hub_skill("beta");

        let error = import_bundle(&bundle, false).unwrap_err().to_string();
        assert!(error.contains("manifest.json"), "unexpected error: {error}");

        let imported = import_any_bundle(&bundle, false).unwrap();
        assert!(matches!(imported, AnyBundleImport::Multi(ref r) if r.skill_names.len() == 2));
    }

    #[test]
    fn any_bundle_routes_single_bundles() {
        let sandbox = Sandbox::new();
        hub_skill("alpha");
        let bundle = sandbox.bundle_path("alpha.ags");
        export_bundle("alpha", Some(&bundle)).unwrap();
        remove_hub_skill("alpha");

        let imported = import_any_bundle(&bundle, false).unwrap();
        assert!(matches!(imported, AnyBundleImport::Single(ref r) if r.name == "alpha"));
        assert!(
            ss_core::infra::paths::hub_skills_dir()
                .join("alpha/SKILL.md")
                .is_file()
        );
    }

    #[test]
    fn deck_names_strip_the_export_timestamp() {
        // Export layout: `<deck>-bundle-<ISO ts with - separators>.agd`.
        assert_eq!(
            deck_name_from_bundle_path("/tmp/My Deck-bundle-2026-10-06T12-34-56.agd"),
            "My Deck"
        );
        // A plain rename keeps the whole stem; the suffix guard must not eat
        // names that merely contain `-bundle-`.
        assert_eq!(
            deck_name_from_bundle_path("/tmp/my-bundle-of-skills.agd"),
            "my-bundle-of-skills"
        );
        assert_eq!(deck_name_from_bundle_path("/tmp/beta.AGD"), "beta");
    }
}

#[cfg(test)]
mod import_tests;

#[cfg(test)]
mod tests {
    use super::is_unsafe_archive_path;

    #[test]
    fn safe_relative_paths_are_accepted() {
        // Legitimate bundle entries — forward slashes, no traversal.
        assert!(!is_unsafe_archive_path("skills/pdf-tools/SKILL.md"));
        assert!(!is_unsafe_archive_path("pdf-tools/scripts/setup.sh"));
        assert!(!is_unsafe_archive_path("a/b/c/d/e.txt"));
        assert!(!is_unsafe_archive_path("manifest.json"));
    }

    #[test]
    fn unix_absolute_and_traversal_are_rejected() {
        assert!(is_unsafe_archive_path("/etc/passwd"));
        assert!(is_unsafe_archive_path("../escape"));
        assert!(is_unsafe_archive_path("skills/../../escape"));
        assert!(is_unsafe_archive_path("foo/../bar/../../etc"));
    }

    #[test]
    fn windows_style_paths_are_rejected() {
        // Backslash separators (only present in malicious/non-standard tar).
        assert!(is_unsafe_archive_path("skills\\pdf-tools\\SKILL.md"));
        assert!(is_unsafe_archive_path("..\\escape"));
        // Windows drive-letter absolute paths.
        assert!(is_unsafe_archive_path("C:\\Users\\evil"));
        assert!(is_unsafe_archive_path("c:evil"));
        assert!(is_unsafe_archive_path("D:/abs/path"));
    }
}
