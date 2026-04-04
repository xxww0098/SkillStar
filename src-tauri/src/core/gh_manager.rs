use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::{lockfile, path_env::command_with_path};

// ── Status ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum GhStatus {
    /// gh CLI is not installed
    NotInstalled,
    /// gh CLI is installed but the user is not authenticated
    NotAuthenticated,
    /// gh CLI is installed and authenticated; `username` is the logged-in user
    Ready { username: String },
}

/// Check if GitHub CLI (gh) is installed
pub fn is_gh_installed() -> bool {
    command_with_path("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── Git Status ──────────────────────────────────────────────────────

/// Platform-specific install instruction for Git.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInstallInstruction {
    /// Short label, e.g. "Homebrew", "winget", "apt"
    pub label: String,
    /// Shell command to run, e.g. "brew install git"
    pub command: String,
}

/// Result of checking whether `git` is available.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum GitStatus {
    /// Git is installed. `version` contains the raw version string.
    Installed { version: String },
    /// Git is not found. `os` is the detected platform name.
    /// `install_instructions` lists OS-appropriate install options.
    NotInstalled {
        os: String,
        install_instructions: Vec<GitInstallInstruction>,
        download_url: String,
    },
}

/// Check whether `git` is available on the system.
///
/// Uses the enriched PATH from `command_with_path` so Homebrew / scoop
/// installs are found even in GUI-launched apps.
pub fn check_git_status() -> GitStatus {
    let output = command_with_path("git")
        .arg("--version")
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // `git --version` → "git version 2.44.0"
            let version = raw
                .strip_prefix("git version ")
                .unwrap_or(&raw)
                .to_string();
            GitStatus::Installed { version }
        }
        _ => {
            let (os, instructions, url) = git_install_info();
            GitStatus::NotInstalled {
                os,
                install_instructions: instructions,
                download_url: url,
            }
        }
    }
}

/// Return OS-specific install instructions for Git.
fn git_install_info() -> (String, Vec<GitInstallInstruction>, String) {
    #[cfg(target_os = "macos")]
    {
        (
            "macOS".to_string(),
            vec![
                GitInstallInstruction {
                    label: "Xcode Command Line Tools".to_string(),
                    command: "xcode-select --install".to_string(),
                },
                GitInstallInstruction {
                    label: "Homebrew".to_string(),
                    command: "brew install git".to_string(),
                },
            ],
            "https://git-scm.com/downloads/mac".to_string(),
        )
    }

    #[cfg(target_os = "windows")]
    {
        (
            "Windows".to_string(),
            vec![
                GitInstallInstruction {
                    label: "winget".to_string(),
                    command: "winget install --id Git.Git -e --source winget".to_string(),
                },
                GitInstallInstruction {
                    label: "Scoop".to_string(),
                    command: "scoop install git".to_string(),
                },
            ],
            "https://git-scm.com/downloads/win".to_string(),
        )
    }

    #[cfg(target_os = "linux")]
    {
        (
            "Linux".to_string(),
            vec![
                GitInstallInstruction {
                    label: "apt (Debian/Ubuntu)".to_string(),
                    command: "sudo apt install git".to_string(),
                },
                GitInstallInstruction {
                    label: "dnf (Fedora)".to_string(),
                    command: "sudo dnf install git".to_string(),
                },
                GitInstallInstruction {
                    label: "pacman (Arch)".to_string(),
                    command: "sudo pacman -S git".to_string(),
                },
            ],
            "https://git-scm.com/downloads/linux".to_string(),
        )
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        (
            "Unknown".to_string(),
            vec![],
            "https://git-scm.com/downloads".to_string(),
        )
    }
}

/// Check if gh is authenticated
pub fn is_gh_authenticated() -> Result<bool> {
    let output = command_with_path("gh")
        .args(["auth", "status"])
        .output()
        .context("Failed to run gh auth status")?;
    Ok(output.status.success())
}

/// Get the authenticated GitHub username
fn get_gh_username() -> Option<String> {
    let output = command_with_path("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .ok()?;
    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    } else {
        None
    }
}

/// Combined status check: installed → authenticated → username
pub fn check_status() -> GhStatus {
    if !is_gh_installed() {
        return GhStatus::NotInstalled;
    }
    match is_gh_authenticated() {
        Ok(true) => {
            let username = get_gh_username().unwrap_or_else(|| "unknown".to_string());
            GhStatus::Ready { username }
        }
        _ => GhStatus::NotAuthenticated,
    }
}

// ── List User Repos ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRepo {
    /// e.g. "user/my-skills"
    pub full_name: String,
    /// e.g. "https://github.com/user/my-skills"
    pub url: String,
    /// e.g. "My skills collection"
    pub description: String,
    /// true if public
    pub is_public: bool,
    /// Top-level directories in the repo (for showing existing skill folders)
    pub folders: Vec<String>,
}

/// List user's GitHub repositories that could serve as skill monorepos.
/// Fetches repos owned by the authenticated user and inspects their top-level dirs.
pub fn list_user_repos(limit: u32) -> Result<Vec<UserRepo>> {
    let owner = get_gh_username();
    let mut cmd = command_with_path("gh");
    cmd.arg("repo").arg("list");

    // `gh repo list` expects owner as a positional argument.
    if let Some(ref owner_name) = owner {
        cmd.arg(owner_name);
    }

    cmd.args([
        "--json",
        "nameWithOwner,url,description,isPrivate",
        "--limit",
        &limit.to_string(),
    ]);

    let output = cmd.output().context("Failed to run gh repo list")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "gh repo list failed{}: {}",
            owner
                .as_ref()
                .map(|o| format!(" for owner '{}'", o))
                .unwrap_or_default(),
            err.trim()
        );
    }

    #[derive(Deserialize)]
    struct GhRepoItem {
        #[serde(rename = "nameWithOwner")]
        name_with_owner: String,
        url: String,
        description: Option<String>,
        #[serde(rename = "isPrivate")]
        is_private: bool,
    }

    let repo_entries: Vec<GhRepoItem> =
        serde_json::from_slice(&output.stdout).context("Failed to parse gh repo list output")?;

    let repos = repo_entries
        .into_iter()
        .map(|item| UserRepo {
            full_name: item.name_with_owner,
            url: item.url,
            description: item.description.unwrap_or_default(),
            is_public: !item.is_private,
            folders: Vec::new(), // Filled lazily by inspect_repo
        })
        .collect();

    Ok(repos)
}

/// Inspect a specific repo's top-level directories via GitHub API.
/// Used to show existing skill folders when the user picks a repo.
pub fn inspect_repo_folders(repo_full_name: &str) -> Result<Vec<String>> {
    // Use gh api to list contents at the repo root
    let output = command_with_path("gh")
        .args([
            "api",
            &format!("repos/{}/contents", repo_full_name),
            "--jq",
            r#"[.[] | select(.type == "dir") | .name] | sort | .[]"#,
        ])
        .output()
        .context("Failed to inspect repo contents")?;

    if !output.status.success() {
        // Could be empty repo — return empty list
        return Ok(Vec::new());
    }

    let folders = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('.'))
        .collect();

    Ok(folders)
}

// ── Git Init ────────────────────────────────────────────────────────

/// Ensure the directory is a git repository with at least one commit.
pub fn ensure_git_repo(path: &Path) -> Result<()> {
    if path.join(".git").exists() {
        let has_commits = command_with_path("git")
            .current_dir(path)
            .args(["rev-parse", "HEAD"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !has_commits {
            stage_and_commit(path, "Initial commit")?;
        }

        return Ok(());
    }

    std::fs::create_dir_all(path)?;
    run_git_in(path, &["init"])?;
    stage_and_commit(path, "Initial commit")?;

    Ok(())
}

fn stage_and_commit(path: &Path, message: &str) -> Result<()> {
    run_git_in(path, &["add", "-A"])?;

    let status = command_with_path("git")
        .current_dir(path)
        .args(["diff", "--cached", "--quiet"])
        .output()
        .context("Failed to check git status")?;

    if !status.status.success() {
        run_git_in(
            path,
            &[
                "-c",
                "user.name=SkillStar",
                "-c",
                "user.email=skillstar@local",
                "commit",
                "-m",
                message,
            ],
        )?;
    }

    Ok(())
}

// ── Publish ─────────────────────────────────────────────────────────

/// The local clone cache lives at `~/.agents/.publish-repos/<repo-name>/`
fn get_publish_cache_dir(repo_name: &str) -> PathBuf {
    super::paths::publish_cache_dir(repo_name)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResult {
    /// The GitHub repository URL (e.g. https://github.com/user/my-skills)
    pub url: String,
    /// The git clone URL (e.g. https://github.com/user/my-skills.git)
    pub git_url: String,
    /// The source folder within the repo (e.g. "agent-team-driven-development")
    pub source_folder: String,
}

/// Publish a skill into an existing or new GitHub repository.
///
/// - `existing_repo_url`: If Some, publish into this existing repo.
///   If None, create a new repo named `repo_name`.
/// - `folder_name`: Subfolder name in the repo for this skill.
pub fn publish_skill(
    skill_name: &str,
    repo_name: &str,
    description: &str,
    is_public: bool,
    existing_repo_url: Option<&str>,
    folder_name: &str,
) -> Result<PublishResult> {
    let hub_dir = super::paths::hub_skills_dir();
    let skill_source = hub_dir.join(skill_name);

    if !skill_source.exists() {
        anyhow::bail!("Skill directory '{}' not found", skill_name);
    }

    // Resolve local skill symlinks (skills/ → skills-local/) to their actual directory.
    // Repo-cached symlinks (.repos/) are still rejected — those should be forked first.
    let skill_source_resolved = if super::paths::is_link(&skill_source) {
        let link_target = std::fs::read_link(&skill_source);
        #[cfg(windows)]
        let link_target = link_target.or_else(|_| junction::get_target(&skill_source));
        let target = link_target
            .with_context(|| format!("Failed to read symlink for '{}'", skill_name))?;
        let resolved = if target.is_absolute() {
            target
        } else {
            skill_source.parent().unwrap_or(Path::new(".")).join(target)
        };

        let local_dir = super::paths::local_skills_dir();
        if !resolved.starts_with(&local_dir) {
            anyhow::bail!(
                "Skill '{}' is a repo-cached symlink. Cannot publish.",
                skill_name
            );
        }
        resolved
    } else {
        skill_source.clone()
    };

    // Determine repo URL: either use existing or create new
    let (repo_url, cache_dir) = if let Some(url) = existing_repo_url {
        // Clone/fetch the existing repo
        let sanitized = url
            .rsplit('/')
            .next()
            .unwrap_or("skills")
            .trim_end_matches(".git");
        let cache = get_publish_cache_dir(sanitized);

        if cache.join(".git").exists() {
            // Already cloned — pull latest
            let _ = run_git_in(&cache, &["pull", "--rebase"]);
        } else {
            // Clone fresh
            std::fs::create_dir_all(cache.parent().unwrap_or(Path::new(".")))?;
            run_git_in(
                cache.parent().unwrap_or(Path::new(".")),
                &["clone", url, &cache.to_string_lossy()],
            )?;
        }

        (url.to_string(), cache)
    } else {
        // Create a new repo
        let cache = get_publish_cache_dir(repo_name);
        std::fs::create_dir_all(&cache)?;

        // Create a README
        let readme = format!(
            "# {}\n\nA collection of SkillStar skills.\n\nManaged by [SkillStar](https://github.com/SkillStar).\n",
            repo_name
        );
        std::fs::write(cache.join("README.md"), readme)?;

        // Create .gitignore to exclude OS/editor junk
        ensure_gitignore(&cache)?;

        ensure_git_repo(&cache)?;

        let visibility = if is_public { "--public" } else { "--private" };
        let output = command_with_path("gh")
            .current_dir(&cache)
            .args([
                "repo",
                "create",
                repo_name,
                "--source",
                ".",
                "--push",
                visibility,
                "--description",
                description,
            ])
            .output()
            .context("Failed to run gh repo create")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            let _ = std::fs::remove_dir_all(&cache);
            anyhow::bail!("gh repo create failed: {}", err.trim());
        }

        let created_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let url = if created_url.is_empty() {
            run_git_in(&cache, &["remote", "get-url", "origin"])
                .unwrap_or_default()
                .trim()
                .to_string()
        } else {
            created_url
        };

        (url, cache)
    };

    // Ensure .gitignore exists (covers existing repos that were cloned without one)
    ensure_gitignore(&cache_dir)?;

    // Copy skill into the repo as a subfolder
    let dest = cache_dir.join(folder_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    copy_dir_recursive(&skill_source_resolved, &dest)?;

    // Commit and push
    let commit_msg = format!("publish: {}", folder_name);
    stage_and_commit(&cache_dir, &commit_msg)?;
    run_git_in(&cache_dir, &["push", "-u", "origin", "HEAD"])?;

    // Normalize URL
    let clean_url = repo_url.trim_end_matches('/').to_string();
    let git_url = if clean_url.ends_with(".git") {
        clean_url.clone()
    } else {
        format!("{}.git", clean_url)
    };

    // Update lockfile
    let tree_hash = super::git_ops::compute_tree_hash(&skill_source_resolved).unwrap_or_default();
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    lf.upsert(lockfile::LockEntry {
        name: skill_name.to_string(),
        git_url: git_url.clone(),
        tree_hash,
        installed_at: chrono::Utc::now().to_rfc3339(),
        source_folder: Some(folder_name.to_string()),
    });
    let _ = lf.save(&lock_path);

    Ok(PublishResult {
        url: clean_url,
        git_url,
        source_folder: folder_name.to_string(),
    })
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Ensure a `.gitignore` exists in the repo root with standard OS/editor exclusions.
/// If the file already exists it is left untouched so user edits are preserved.
fn ensure_gitignore(repo_dir: &Path) -> Result<()> {
    let gitignore = repo_dir.join(".gitignore");
    if !gitignore.exists() {
        let content = "\
# macOS
.DS_Store

# Windows
Thumbs.db
desktop.ini

# Editors
*.swp
*.swo
*~
.vscode/
.idea/
";
        std::fs::write(&gitignore, content).context("Failed to write .gitignore")?;
    }
    Ok(())
}

fn run_git_in(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = command_with_path("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), err.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dest.join(&file_name);

        // Explicitly avoid copying git metadata and OS system files
        if file_name == ".git" || file_name == ".DS_Store"
            || file_name == "Thumbs.db" || file_name == "desktop.ini"
        {
            continue;
        }

        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}
