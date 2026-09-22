//! Codeload tarball fallback: plain HTTPS when the git smart protocol fails.
//!
//! The sparse-clone path moves bytes over `git clone --filter=blob:none` plus
//! lazy blob fetches — several smart-protocol round-trips that ghproxy-style
//! mirrors frequently break (HTTP/2 framing, truncated packs). A codeload
//! archive is one ordinary HTTPS GET, which the anonymous mirror chain can
//! accelerate like any other GitHub-family download. Only the planned skill
//! directories are extracted, and a synthetic local commit gives the cache
//! entry the git shape (status, tree hash, reset) every downstream path
//! already expects. Marked via `git config skillstar.transport tarball`.

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use flate2::read::GzDecoder;
use tar::Archive;

use skillstar_core::infra::path_env::command_with_path;

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
/// Hard caps so a hostile mirror cannot exhaust disk or memory.
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_FILES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Tarball transport exists only for GitHub's codeload host.
pub(crate) fn supports_tarball(repo_url: &str) -> bool {
    repo_url.starts_with("https://github.com/")
}

/// Is this cache entry a tarball-built synthetic repository?
pub(crate) fn is_tarball_cache(repo_dir: &Path) -> bool {
    run_local_git(repo_dir, &["config", "--get", "skillstar.transport"])
        .map(|value| value.trim() == "tarball")
        .unwrap_or(false)
}

/// Rebuild a cache entry from the codeload tarball, extracting only `dirs`.
///
/// `dest` must not exist; the entry is assembled in a staging sibling and
/// atomically renamed into place so a failed download leaves no residue.
pub(crate) fn rebuild_cache_from_tarball(
    repo_url: &str,
    git_ref: Option<&str>,
    dest: &Path,
    dirs: &[String],
) -> Result<()> {
    let staging = staging_dir_for(dest);
    let archive_path = spool_path_for(&staging);
    let outcome = (|| -> Result<()> {
        if let Some(parent) = staging.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let reference = git_ref.unwrap_or("HEAD");
        let url = archive_url(repo_url, reference)?;
        let archive = download_to_file(&url, &archive_path)?;
        assemble_from_archive(&archive, &staging, dirs, repo_url)
    })();
    match outcome {
        Ok(()) => move_staging_into_place(&staging, dest),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            let _ = std::fs::remove_file(&archive_path);
            Err(error)
        }
    }
}

/// Assemble a synthetic cache entry from an archive already on disk.
fn assemble_from_archive(
    archive_path: &Path,
    staging: &Path,
    dirs: &[String],
    repo_url: &str,
) -> Result<()> {
    extract_subset(archive_path, staging, dirs)?;
    init_synthetic_repo(staging, repo_url)
}

fn move_staging_into_place(staging: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create the repo cache parent '{}'", parent.display())
        })?;
    }
    std::fs::rename(staging, dest).with_context(|| {
        format!("Failed to move the tarball cache into '{}'", dest.display())
    })
}

/// `https://codeload.github.com/<owner>/<repo>/tar.gz/<ref>` for GitHub URLs.
fn archive_url(repo_url: &str, reference: &str) -> Result<String> {
    let parsed = crate::source_resolver::Source::parse(repo_url)
        .with_context(|| format!("Cannot parse repository URL '{repo_url}'"))?;
    Ok(format!(
        "https://codeload.github.com/{}/tar.gz/{}",
        parsed.short, reference
    ))
}

/// Extend an existing tarball cache with more directories.
///
/// Tarball caches hold no git objects for deferred copies, so on-demand
/// materialization re-downloads the archive and commits the new directories.
pub(crate) fn add_dirs_via_tarball(repo_dir: &Path, dirs: &[String]) -> Result<()> {
    if dirs.iter().any(|dir| dir == ".git" || dir.starts_with(".git/")) {
        bail!("Refusing to extract archive entries into the repository metadata");
    }
    let repo_url = crate::git::ops::remote_origin_url(repo_dir)
        .context("Tarball cache has no origin remote")?;
    fetch_and_extract(&repo_url, None, repo_dir, dirs)?;
    commit_all_local(repo_dir, "SkillStar tarball extension")?;
    Ok(())
}

fn staging_dir_for(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    dest.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{name}.tarball-{}", std::process::id()))
}

// ── Download and extract ────────────────────────────────────────────

/// Download the archive and extract `dirs` into `target`.
///
/// Anonymous only: codeload ignores credentials, and a GitHub App token must
/// never transit a public accelerator (D-014 / D-050).
fn fetch_and_extract(
    repo_url: &str,
    git_ref: Option<&str>,
    target: &Path,
    dirs: &[String],
) -> Result<()> {
    if dirs.is_empty() {
        bail!("The tarball fallback requires at least one directory to extract");
    }
    if dirs.iter().any(|dir| dir == ".git" || dir.starts_with(".git/")) {
        bail!("Refusing to extract archive entries into the repository metadata");
    }
    let spool = spool_path_for(target);
    let url = archive_url(repo_url, git_ref.unwrap_or("HEAD"))?;
    let archive = download_to_file(&url, &spool)?;
    let extracted = extract_subset(&archive, target, dirs);
    let _ = std::fs::remove_file(&archive);
    extracted
}

fn spool_path_for(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    target
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{name}.skillstar-archive"))
}

/// One plain HTTPS GET streamed to disk, bounded by [`MAX_ARCHIVE_BYTES`].
///
/// Bridges the async `reqwest` response (which honors the proxy-configured
/// client) into the synchronous install pipeline with a short-lived runtime —
/// the same pattern `git::gh_rest` uses. Must only run on a blocking thread.
fn download_to_file(url: &str, dest: &Path) -> Result<PathBuf> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Unable to start the archive download runtime")?;
    runtime.block_on(async {
        let mut response = skillstar_core::infra::github_http::get_anonymous(
            url,
            DOWNLOAD_TIMEOUT,
        )
        .await
        .with_context(|| format!("Failed to download '{url}'"))?;
        if !response.status().is_success() {
            bail!("Archive download returned HTTP {}", response.status().as_u16());
        }
        let mut file = std::fs::File::create(dest)
            .with_context(|| format!("Failed to create archive spool '{}'", dest.display()))?;
        let mut downloaded: u64 = 0;
        while let Some(chunk) = response
            .chunk()
            .await
            .context("Archive download stream failed")?
        {
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > MAX_ARCHIVE_BYTES {
                bail!(
                    "Repository archive exceeds the {} MB download cap",
                    MAX_ARCHIVE_BYTES / (1024 * 1024)
                );
            }
            file.write_all(&chunk)
                .with_context(|| format!("Failed to spool archive to '{}'", dest.display()))?;
        }
        if downloaded == 0 {
            bail!("Repository archive was empty");
        }
        Ok(())
    })?;
    Ok(dest.to_path_buf())
}

/// Extract only the paths under `dirs` from a `tar.gz` archive on disk.
fn extract_subset(archive_path: &Path, target: &Path, dirs: &[String]) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive '{}'", archive_path.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));

    let mut extracted_bytes: u64 = 0;
    let mut extracted_files: usize = 0;
    for entry in archive
        .entries()
        .context("Failed to read the repository archive")?
    {
        let mut entry = entry.context("Corrupt entry in the repository archive")?;
        let raw_path = entry
            .path()
            .context("Archive entry has an invalid path")?
            .to_string_lossy()
            .into_owned();
        // GitHub archives wrap everything in a single "<repo>-<ref>" directory.
        let Some(rest) = strip_archive_prefix(&raw_path) else {
            continue;
        };
        if rest.is_empty() || !is_under_dirs(&rest, dirs) {
            continue;
        }
        let rel = Path::new(&rest);
        // Defense in depth: the tar crate also rejects escapes, but only
        // plain normal components are ever acceptable here.
        if rel
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            continue;
        }
        // Read the header facts up front: the entry must be free for the
        // mutable read below.
        let (entry_type, size, file_mode) = {
            let header = entry.header();
            (header.entry_type(), header.size().unwrap_or(0), header.mode().unwrap_or(0o644))
        };
        let out_path = target.join(rel);
        if entry_type.is_dir() {
            std::fs::create_dir_all(&out_path).with_context(|| {
                format!("Failed to create directory '{}'", out_path.display())
            })?;
            continue;
        }
        // Symlinks and hardlinks are skipped: skill payloads are plain files,
        // and a link target is one more escape hatch not worth guarding.
        if !entry_type.is_file() {
            continue;
        }
        if size > MAX_FILE_BYTES {
            bail!(
                "Archive file '{rest}' is {size} bytes, above the per-file cap"
            );
        }
        extracted_bytes = extracted_bytes.saturating_add(size);
        extracted_files += 1;
        if extracted_bytes > MAX_EXTRACTED_BYTES || extracted_files > MAX_EXTRACTED_FILES {
            bail!("Archive extraction exceeded the safety caps");
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&out_path)
            .with_context(|| format!("Failed to create '{}'", out_path.display()))?;
        std::io::copy(&mut entry, &mut out)
            .with_context(|| format!("Failed to extract '{rest}'"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if file_mode & 0o111 != 0 {
                let permissions = std::fs::Permissions::from_mode(file_mode & 0o777);
                std::fs::set_permissions(&out_path, permissions)?;
            }
        }
    }
    if extracted_files == 0 {
        bail!("No files matched the planned directories in the repository archive");
    }
    Ok(())
}

/// Drop the leading `<repo>-<ref>` segment of a GitHub archive path.
fn strip_archive_prefix(path: &str) -> Option<String> {
    let mut segments = path.split('/');
    segments.next()?;
    Some(segments.collect::<Vec<_>>().join("/"))
}

fn is_under_dirs(rest: &str, dirs: &[String]) -> bool {
    dirs.iter()
        .any(|dir| rest == dir || rest.starts_with(&format!("{dir}/")))
}

// ── Synthetic repository ────────────────────────────────────────────

/// Give the extracted tree a local commit and the real origin remote so
/// status/tree-hash/reset/update paths work unchanged.
fn init_synthetic_repo(dir: &Path, repo_url: &str) -> Result<()> {
    run_local_git_checked(dir, &["init", "-q"])?;
    run_local_git_checked(dir, &["config", "user.name", "SkillStar"])?;
    run_local_git_checked(dir, &["config", "user.email", "skillstar-cache@local"])?;
    run_local_git_checked(dir, &["config", "skillstar.transport", "tarball"])?;
    run_local_git_checked(dir, &["remote", "add", "origin", repo_url])?;
    commit_all_local(dir, "SkillStar tarball snapshot")
}

fn commit_all_local(dir: &Path, message: &str) -> Result<()> {
    run_local_git_checked(dir, &["add", "-A"])?;
    run_local_git_checked(
        dir,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
            "--allow-empty",
        ],
    )
}

fn run_local_git_checked(dir: &Path, args: &[&str]) -> Result<()> {
    let output = command_with_path("git")
        .current_dir(dir)
        .args(args)
        .output()
        .with_context(|| format!("Failed to execute git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn run_local_git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = command_with_path("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests;
