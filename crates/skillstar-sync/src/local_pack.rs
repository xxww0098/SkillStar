//! Pack and unpack user-authored local skills as content-addressed tarballs.
//!
//! Packing reads `~/.skillstar/hub/local/<name>/`, skips noise (`.git`,
//! `.DS_Store`), streams it through `tar` + `flate2` into memory while hashing
//! with sha256 — so the digest is computed in a single pass.
//!
//! Unpacking writes into `~/.skillstar/hub/local/<name>/` and creates the hub
//! symlink `~/.skillstar/hub/skills/<name>` → local dir, mirroring
//! `local_skill::create`. The tarball is authored so each entry's path is
//! relative to the skill dir (no leading `<name>/`), so the same archive can be
//! unpacked under any destination name.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// A packed local skill: gzipped-tar bytes + its sha256 digest + byte length.
pub struct PackedSkill {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Filenames skipped when packing (mirrors `copy_dir_recursive` in skills crate).
const SKIP_NAMES: &[&str] = &[".git", ".DS_Store", "Thumbs.db"];

/// Pack a local skill directory at `~/.skillstar/hub/local/<name>` into a
/// content-addressed `.tar.gz`.
pub fn pack_skill(name: &str) -> Result<PackedSkill> {
    let local_dir = skillstar_core::infra::paths::local_skills_dir().join(name);
    if !local_dir.is_dir() {
        anyhow::bail!(
            "Local skill directory '{}' does not exist",
            local_dir.display()
        );
    }

    let mut buf: Vec<u8> = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        tar.follow_symlinks(false);
        append_dir(&mut tar, &local_dir, "")?;
        tar.into_inner()?.finish()?;
    }

    let size_bytes = buf.len() as u64;
    let sha256 = hex_digest(&buf);
    Ok(PackedSkill {
        bytes: buf,
        sha256,
        size_bytes,
    })
}

fn append_dir(tar: &mut tar::Builder<impl Write>, root: &Path, rel: &str) -> Result<()> {
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).context(format!("read_dir {}", dir.display())),
    };
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if SKIP_NAMES.iter().any(|s| *s == name) {
            continue;
        }
        let path = entry.path();
        let archive_path = if rel.is_empty() {
            PathBuf::from(&*name)
        } else {
            Path::new(rel).join(&*name)
        };
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            // Record the directory entry so empty dirs survive the round-trip.
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o755);
            header.set_mtime(
                meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            );
            header.set_cksum();
            tar.append_data(&mut header, &archive_path, std::io::empty())?;
            append_dir(tar, root, &archive_path.to_string_lossy())?;
        } else if meta.is_symlink() {
            // Record symlink target instead of following it.
            let target = std::fs::read_link(&path).unwrap_or_default();
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_mtime(
                meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            );
            header.set_cksum();
            let link = target.to_string_lossy();
            tar.append(&header, link.as_bytes())?;
        } else {
            // Regular file (skip any other special types).
            if meta.is_file() {
                tar.append_path_with_name(&path, &archive_path)
                    .with_context(|| format!("append file {}", path.display()))?;
            }
        }
    }
    Ok(())
}

trait Write: std::io::Write {}
impl<W: std::io::Write> Write for W {}

/// Unpack a downloaded local-skill tarball into `~/.skillstar/hub/local/<name>/`
/// and create the hub symlink. Fails if the skill already exists (no overwrite).
pub fn unpack_skill(name: &str, bytes: &[u8]) -> Result<()> {
    let local_dir = skillstar_core::infra::paths::local_skills_dir().join(name);
    let hub_link = skillstar_core::infra::paths::hub_skills_dir().join(name);

    if hub_link.symlink_metadata().is_ok() {
        anyhow::bail!("Skill '{}' already exists", name);
    }
    if local_dir.symlink_metadata().is_ok() {
        anyhow::bail!("Skill '{}' already exists in skills-local", name);
    }

    std::fs::create_dir_all(&local_dir)
        .with_context(|| format!("create local dir {}", local_dir.display()))?;
    std::fs::create_dir_all(skillstar_core::infra::paths::hub_skills_dir())
        .context("create hub skills dir")?;

    // Stream gzip → tar → filesystem, writing entries under local_dir.
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes.to_vec()));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry.context("read tar entry")?;
        // Defence in depth: strip any leading `..` or absolute components.
        let rel = entry.path()?.into_owned();
        if !is_safe_relative(&rel) {
            tracing::warn!(target: "sync", path = ?rel, "skipping unsafe tar entry");
            continue;
        }
        entry.unpack_in(&local_dir).with_context(|| {
            format!("unpack {} into {}", rel.display(), local_dir.display())
        })?;
    }

    // Mirror local_skill::create: symlink hub/skills/<name> → hub/local/<name>.
    skillstar_core::infra::fs_ops::create_symlink(&local_dir, &hub_link)
        .with_context(|| format!("create hub symlink for '{name}'"))?;

    Ok(())
}

fn is_safe_relative(p: &Path) -> bool {
    let mut depth: i32 = 0;
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            // Prefix (e.g. `C:`) or RootDir → absolute path.
            _ => return false,
        }
    }
    true
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_relative_rejects_traversal() {
        assert!(is_safe_relative(Path::new("SKILL.md")));
        assert!(is_safe_relative(Path::new("sub/file.txt")));
        assert!(!is_safe_relative(Path::new("../escape")));
        assert!(!is_safe_relative(Path::new("a/../../b")));
        assert!(!is_safe_relative(Path::new("/etc/passwd")));
    }

    #[test]
    fn hex_digest_known_vector() {
        // sha256 of empty input
        assert_eq!(
            hex_digest(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn pack_unpack_round_trip() {
        let _g = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: held under env_lock() so concurrent tests can't race.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", dir.path());
        }

        // Build a local skill: SKILL.md + a scripts/ subdir + a file.
        let local_root = skillstar_core::infra::paths::local_skills_dir();
        let skill_dir = local_root.join("demo");
        std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# demo\n").unwrap();
        std::fs::write(skill_dir.join("scripts").join("run.sh"), "echo hi\n").unwrap();
        // Noise that must be skipped.
        std::fs::write(skill_dir.join(".DS_Store"), "noise").unwrap();

        let packed = pack_skill("demo").unwrap();
        assert!(!packed.bytes.is_empty());
        assert_eq!(packed.size_bytes, packed.bytes.len() as u64);
        assert_eq!(packed.sha256.len(), 64);

        // Remove the source, then unpack into a fresh skill name.
        std::fs::remove_dir_all(&skill_dir).unwrap();
        // Re-create the parent dir since remove_dir_all on the only skill may
        // leave local_root empty (that's fine) — pack again would fail, but we
        // only unpack here.
        unpack_skill("demo", &packed.bytes).unwrap();

        // Files restored and noise excluded.
        let restored = local_root.join("demo");
        assert_eq!(std::fs::read_to_string(restored.join("SKILL.md")).unwrap(), "# demo\n");
        assert_eq!(
            std::fs::read_to_string(restored.join("scripts").join("run.sh")).unwrap(),
            "echo hi\n"
        );
        assert!(!restored.join(".DS_Store").exists());
        // Hub symlink created.
        let hub_link = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        assert!(hub_link.symlink_metadata().is_ok());
    }
}
