//! Skill publishing: local repository preparation plus the GitHub side of it.
//!
//! Everything remote goes through one identity and one policy: REST through
//! [`super::gh_rest`] with the SkillStar GitHub App credential (D-013), and
//! Git through an operation session so the token only ever reaches an askpass
//! child process (D-014). The `gh` CLI is no longer part of this path — it
//! authenticated as the machine's global GitHub login and inherited the
//! launcher's proxy environment, which is a second identity and a second proxy
//! behaviour inside the same app.
//!
//! Local-only Git (`init`, `add`, `commit`, `remote add`) stays on a plain
//! subprocess: it touches no remote, so an operation session would add policy
//! without a boundary to enforce.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use skillstar_core::infra::path_env::command_with_path;

use crate::git::transport::{
    GitAuthMaterial, GitOperationSession, NoopGitProgressSink, execute_remote_command,
};
use skillstar_github_auth::{
    GitHubAuthFacade, KeyringCredentialStore, ProductionGitHubGateway, SystemClock,
};

use super::gh_rest::{GhRestClient, GhRestErrorCode};

/// Committer identity for publish commits.
///
/// A machine with no global Git identity would otherwise fail the commit, and
/// publishing must not depend on how the user configured Git outside SkillStar.
const COMMITTER_ARGS: [&str; 4] = [
    "-c",
    "user.name=SkillStar",
    "-c",
    "user.email=skillstar@local",
];

/// Shown when the credential is valid but GitHub could not be asked for the
/// login (network or rate limit). Publishing still works; only the label is
/// unknown.
const UNKNOWN_LOGIN: &str = "unknown";

// ── Status ──────────────────────────────────────────────────────────

/// Whether the publish flow can run, in the three states the UI branches on.
///
/// The publish path no longer needs the `gh` CLI, but it still needs `git` for
/// the local repository work, so the states now describe git plus the SkillStar
/// GitHub identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum GhStatus {
    /// `git` — required for the local publish repository — is not installed
    NotInstalled,
    /// `git` is installed but SkillStar has no usable GitHub identity
    NotAuthenticated,
    /// Ready to publish; `username` is the signed-in GitHub App user
    Ready { username: String },
}

/// Check if GitHub CLI (gh) is installed.
///
/// Kept for the Settings environment check only — no publish step depends on
/// it any more.
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
    let output = command_with_path("git").arg("--version").output();

    match output {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // `git --version` → "git version 2.44.0"
            let version = raw.strip_prefix("git version ").unwrap_or(&raw).to_string();
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

/// Is `git` — the only external binary publishing still needs — available?
fn is_git_installed() -> bool {
    matches!(check_git_status(), GitStatus::Installed { .. })
}

/// Who SkillStar would publish as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PublishIdentity {
    /// No stored credential, or GitHub rejected the one we have.
    SignedOut,
    SignedIn { login: String },
}

/// Resolve the publishing identity from the stored GitHub App credential.
fn publish_identity() -> PublishIdentity {
    let Ok(client) = GhRestClient::from_keyring() else {
        return PublishIdentity::SignedOut;
    };
    match client.current_login() {
        Ok(login) => PublishIdentity::SignedIn { login },
        // A stored credential GitHub refuses is not an identity the user can
        // publish with; they have to sign in again.
        Err(error)
            if matches!(
                error.code,
                GhRestErrorCode::NotAuthenticated | GhRestErrorCode::Unauthorized
            ) =>
        {
            PublishIdentity::SignedOut
        }
        // Network or rate-limit failures say nothing about the credential.
        // Reporting "not signed in" here would send a signed-in user to the
        // login screen for what is really a connectivity problem.
        Err(_) => PublishIdentity::SignedIn {
            login: UNKNOWN_LOGIN.to_string(),
        },
    }
}

pub(super) fn map_publish_status(git_installed: bool, identity: PublishIdentity) -> GhStatus {
    if !git_installed {
        return GhStatus::NotInstalled;
    }
    match identity {
        PublishIdentity::SignedOut => GhStatus::NotAuthenticated,
        PublishIdentity::SignedIn { login } => GhStatus::Ready { username: login },
    }
}

/// Combined publish readiness: git present → SkillStar signed in → login.
pub fn check_status() -> GhStatus {
    map_publish_status(is_git_installed(), publish_identity())
}

/// One operation session for every remote Git command in a publish.
fn publish_session() -> GitOperationSession {
    let auth = GitHubAuthFacade::new(
        ProductionGitHubGateway::from_environment(),
        KeyringCredentialStore,
        SystemClock,
    );
    GitOperationSession::new(
        uuid::Uuid::new_v4().to_string(),
        auth.git_auth_material()
            .unwrap_or_else(|error| GitAuthMaterial::unavailable(error.to_string())),
        Arc::new(NoopGitProgressSink),
    )
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

/// List GitHub repositories that could serve as skill monorepos.
///
/// Covers personal, collaborator and organization-member repositories, so an
/// organization repository can finally be chosen as a publish target — the
/// previous `gh repo list <login>` call could only ever see personal ones.
pub fn list_user_repos(limit: u32) -> Result<Vec<UserRepo>> {
    let client = GhRestClient::from_keyring()?;
    Ok(client
        .list_repositories(limit)?
        .into_iter()
        .map(|repo| UserRepo {
            full_name: repo.full_name,
            url: repo.html_url,
            description: repo.description,
            is_public: !repo.private,
            folders: Vec::new(), // Filled lazily by inspect_repo_folders
        })
        .collect())
}

/// Inspect the skill folders inside a repo's top-level `skills/` directory.
/// Used to show existing skill folders (and detect name clashes) when the user
/// picks a repo. Skills always publish under `skills/<name>`, so we list that
/// directory rather than the repo root.
pub fn inspect_repo_folders(repo_full_name: &str) -> Result<Vec<String>> {
    let client = GhRestClient::from_keyring()?;
    Ok(client.list_skill_folders(repo_full_name)?)
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
        let mut args = COMMITTER_ARGS.to_vec();
        args.extend_from_slice(&["commit", "-m", message]);
        run_git_in(path, &args)?;
    }

    Ok(())
}

// ── Publish ─────────────────────────────────────────────────────────

/// The local clone cache lives at `~/.agents/.publish-repos/<repo-name>/`
fn get_publish_cache_dir(repo_name: &str) -> PathBuf {
    skillstar_core::infra::paths::publish_cache_dir(repo_name)
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

pub enum PublishLockfileMode<'a> {
    Commit(&'a Path),
    ValidateOnly(&'a Path),
}

impl PublishLockfileMode<'_> {
    fn path(&self) -> &Path {
        match self {
            Self::Commit(path) | Self::ValidateOnly(path) => path,
        }
    }

    fn should_commit(&self) -> bool {
        matches!(self, Self::Commit(_))
    }
}

/// Whether publishing this Skill produces an independent copy rather than
/// moving the local Skill's provenance to the destination repository.
///
/// True for anything backed by a repository checkout: the user is sharing a
/// Skill they installed, and their own copy must keep following the source it
/// came from. False for locally authored Skills, whose publication *is* their
/// graduation into Git.
///
/// Split out from [`publish_skill`] because it decides whether the lockfile is
/// rewritten, and getting it wrong is silent — the Skill keeps working and
/// simply stops receiving upstream updates.
pub(super) fn publish_copies_content(
    was_link: bool,
    resolved: &Path,
    local_skills_dir: &Path,
) -> bool {
    was_link && !resolved.starts_with(local_skills_dir)
}

/// Publish a skill into an existing or new GitHub repository.
///
/// - `existing_repo_url`: If Some, publish into this existing repo.
///   If None, create a new repo named `repo_name`.
/// - `folder_name`: Subfolder name in the repo for this skill.
///
/// `lockfile_mode` supplies the app-specific lockfile location and decides
/// whether publication commits provenance immediately or defers it to a staged
/// local-to-Git graduation.
pub fn publish_skill(
    skill_name: &str,
    repo_name: &str,
    description: &str,
    is_public: bool,
    existing_repo_url: Option<&str>,
    folder_name: &str,
    lockfile_mode: PublishLockfileMode<'_>,
) -> Result<PublishResult> {
    crate::content::validate_skill_name(skill_name).map_err(anyhow::Error::from)?;
    crate::content::validate_skill_name(folder_name).map_err(anyhow::Error::from)?;
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()
        .context("Unable to lock Skill publication")?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(skill_name)?;
    if let Some(repository_url) = existing_repo_url {
        crate::skill_mutation::policy().ensure_repository_mutation_allowed(repository_url)?;
    }
    // Validate persisted state before any remote commit/push. Reload again for
    // the final write so a concurrent install is not overwritten.
    {
        let _lock = crate::lockfile::get_mutex()
            .lock()
            .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
        let lockfile = crate::lockfile::Lockfile::load(lockfile_mode.path())
            .context("Failed to validate Skill lockfile before publishing")?;
        lockfile
            .save(lockfile_mode.path())
            .context("Skill lockfile is not writable; publish was not started")?;
    }
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let skill_source = hub_dir.join(skill_name);

    if !skill_source.exists() {
        anyhow::bail!("Skill directory '{}' not found", skill_name);
    }

    // Hub entries are symlinks: locally authored Skills point into skills-local/,
    // installed ones into a repository checkout. Both can be published — most of
    // the library is installed, so refusing those made "share the Skill I use"
    // impossible — but they mean different things afterwards, which
    // `publish_copies_content` decides.
    let (skill_source_resolved, was_link) =
        if skillstar_core::infra::fs_ops::is_link(&skill_source) {
            let resolved = skillstar_core::infra::fs_ops::read_link_resolved(&skill_source)
                .with_context(|| format!("Failed to read symlink for '{}'", skill_name))?;
            (resolved, true)
        } else {
            (skill_source.clone(), false)
        };
    let publishes_a_copy = publish_copies_content(
        was_link,
        &skill_source_resolved,
        &skillstar_core::infra::paths::local_skills_dir(),
    );
    let content_hash = crate::content::snapshot(skill_name)
        .with_context(|| format!("Failed to capture content baseline for '{skill_name}'"))?
        .content_hash;

    // Every remote step below — clone, pull, push — runs inside this session,
    // so the token reaches Git only through the askpass child and SkillStar's
    // proxy setting is the one that applies.
    let session = publish_session();

    // Determine repo URL: either use existing or create new
    let (repo_url, remote_url, cache_dir, created_new) = if let Some(url) = existing_repo_url {
        // Clone/fetch the existing repo
        let sanitized = url
            .rsplit('/')
            .next()
            .unwrap_or("skills")
            .trim_end_matches(".git");
        let cache = get_publish_cache_dir(sanitized);

        if cache.join(".git").exists() {
            // Already cloned — pull latest. A stale cache must not block the
            // publish; the push below is the authority on whether the local
            // copy can move the remote forward.
            let mut args = COMMITTER_ARGS.to_vec();
            args.extend_from_slice(&["pull", "--rebase"]);
            let _ = run_remote_git(&cache, &args, url, &session);
        } else {
            // Clone fresh
            let parent = cache.parent().unwrap_or(Path::new(".")).to_path_buf();
            std::fs::create_dir_all(&parent)?;
            let destination = cache.to_string_lossy().into_owned();
            if let Err(error) =
                run_remote_git(&parent, &["clone", url, &destination], url, &session)
            {
                // A half-written clone would look like a usable cache to the
                // next attempt, which would then publish into it.
                let _ = std::fs::remove_dir_all(&cache);
                return Err(error);
            }
        }

        (url.to_string(), url.to_string(), cache, false)
    } else {
        // Create a new repo
        let client = GhRestClient::from_keyring()?;
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

        let created = match client.create_repository(repo_name, description, !is_public) {
            Ok(created) => created,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&cache);
                return Err(error.into());
            }
        };

        // The remote is attached locally; the push that fills it happens in the
        // shared tail below, together with the Skill commit.
        if let Err(error) = run_git_in(&cache, &["remote", "add", "origin", &created.clone_url]) {
            let _ = std::fs::remove_dir_all(&cache);
            return Err(error);
        }

        (created.html_url, created.clone_url, cache, true)
    };

    // Skills always live under a top-level `skills/` directory in the repo, so
    // the scanner's priority-dir discovery picks them up on re-import.
    let repo_rel_path = format!("skills/{}", folder_name);

    let staged = (|| -> Result<()> {
        // Ensure .gitignore exists (covers existing repos that were cloned without one)
        ensure_gitignore(&cache_dir)?;

        // Copy skill into the repo under skills/<folder_name>
        let dest = cache_dir.join("skills").join(folder_name);
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        copy_dir_recursive(&skill_source_resolved, &dest)?;

        // Commit and push
        let commit_msg = format!("publish: {}", repo_rel_path);
        stage_and_commit(&cache_dir, &commit_msg)?;
        run_remote_git(
            &cache_dir,
            &["push", "-u", "origin", "HEAD"],
            &remote_url,
            &session,
        )?;
        Ok(())
    })();

    if let Err(error) = staged {
        // A repository created moments ago has no other content worth keeping,
        // and leaving its cache behind would make the retry try to create the
        // same repository again.
        if created_new {
            let _ = std::fs::remove_dir_all(&cache_dir);
        }
        return Err(error);
    }

    // Normalize URL
    let clean_url = repo_url.trim_end_matches('/').to_string();
    let git_url = if clean_url.ends_with(".git") {
        clean_url.clone()
    } else {
        format!("{}.git", clean_url)
    };

    // A local Skill is not Git-managed until its staged graduation succeeds.
    // GUI callers therefore defer this write and let the installer commit the
    // new provenance atomically with the checkout replacement.
    //
    // Publishing an *installed* Skill is a copy, not a move: the local one keeps
    // following the source it was installed from. Rewriting its provenance here
    // would silently repoint it at the publisher's repository, so a Skill the
    // user only meant to share with a teammate would stop receiving upstream
    // updates — with nothing in the UI saying so.
    if lockfile_mode.should_commit() && !publishes_a_copy {
        use crate::lockfile::{LockEntry, Lockfile};
        let tree_hash = skillstar_git::ops::compute_tree_hash(&skill_source_resolved).unwrap_or_default();
        let _lock = crate::lockfile::get_mutex()
            .lock()
            .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
        let mut lf = Lockfile::load(lockfile_mode.path())?;
        lf.upsert(LockEntry {
            name: skill_name.to_string(),
            git_url: git_url.clone(),
            git_ref: None,
            tree_hash,
            content_hash: Some(content_hash),
            content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
            installed_at: chrono::Utc::now().to_rfc3339(),
            source_folder: Some(repo_rel_path.clone()),
        });
        lf.save(lockfile_mode.path())?;
    }

    Ok(PublishResult {
        url: clean_url,
        git_url,
        source_folder: repo_rel_path,
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

/// Run one *remote* Git command under the operation session.
///
/// The session owns authentication, proxy, cancellation and output redaction.
/// A bare `git` subprocess here would instead inherit the launcher's proxy and
/// whatever credential helper the machine has configured.
fn run_remote_git(
    cwd: &Path,
    args: &[&str],
    remote: &str,
    session: &GitOperationSession,
) -> Result<String> {
    let mut command = command_with_path("git");
    run_remote_git_command(&mut command, cwd, args, remote, session)
}

/// Test seam: production injects `git`; a test can inject a stub that reports
/// what it received and prove the token reaches neither argv nor output.
pub(super) fn run_remote_git_command(
    command: &mut Command,
    cwd: &Path,
    args: &[&str],
    remote: &str,
    session: &GitOperationSession,
) -> Result<String> {
    let output = execute_remote_command(command, Some(cwd), args, remote, session)
        .with_context(|| format!("git {} failed", args.join(" ")))?;
    Ok(output.stdout.trim().to_string())
}

/// Run one *local* Git command. Nothing here touches a remote.
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
        if file_name == ".git"
            || file_name == ".DS_Store"
            || file_name == "Thumbs.db"
            || file_name == "desktop.ini"
        {
            continue;
        }

        // Never follow symlinks when publishing: the target may live outside
        // the skill root (e.g. ~/.ssh/id_rsa) and must not be pushed to a
        // (possibly public) repository. Mirrors content::snapshot.
        if ty.is_symlink() {
            tracing::warn!(
                target: "gh_manager",
                path = %src_path.display(),
                "skipping symlink during publish — its target may live outside the skill root"
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn copy_dir_recursive_copies_nested_content_but_never_repo_metadata() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        std::fs::create_dir_all(src.join(".git")).unwrap();
        std::fs::create_dir_all(src.join("scripts")).unwrap();
        std::fs::write(src.join("SKILL.md"), "# skill body").unwrap();
        std::fs::write(src.join(".git/config"), "[core]").unwrap();
        std::fs::write(src.join(".DS_Store"), "junk").unwrap();
        std::fs::write(src.join("scripts/run.sh"), "echo hi").unwrap();
        std::fs::write(src.join("scripts/Thumbs.db"), "junk").unwrap();
        std::fs::write(src.join("scripts/desktop.ini"), "junk").unwrap();

        copy_dir_recursive(&src, &dest).unwrap();

        // Real content survives with bytes intact, including nested dirs.
        assert_eq!(
            std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
            "# skill body"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("scripts/run.sh")).unwrap(),
            "echo hi"
        );
        // Git metadata and OS junk must not leak into the published copy,
        // even below the top level.
        assert!(!dest.join(".git").exists());
        assert!(!dest.join(".DS_Store").exists());
        assert!(!dest.join("scripts/Thumbs.db").exists());
        assert!(!dest.join("scripts/desktop.ini").exists());
    }

    #[test]
    fn ensure_gitignore_seeds_os_exclusions_once_but_preserves_user_edits() {
        let temp = TempDir::new().unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let seeded = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(seeded.contains(".DS_Store"));
        assert!(seeded.contains("Thumbs.db"));

        // A user-edited .gitignore must be left byte-identical.
        std::fs::write(temp.path().join(".gitignore"), "custom-only\n").unwrap();
        ensure_gitignore(temp.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(temp.path().join(".gitignore")).unwrap(),
            "custom-only\n"
        );
    }

    /// The `status` serde tag is the discriminant the frontend switches on;
    /// renaming a variant would silently break the UI's status handling.
    #[test]
    fn status_enums_serialize_with_the_status_tag_frontend_contract() {
        let ready = serde_json::to_value(GhStatus::Ready {
            username: "octocat".into(),
        })
        .unwrap();
        assert_eq!(ready["status"], "Ready");
        assert_eq!(ready["username"], "octocat");

        let missing = serde_json::to_value(GhStatus::NotInstalled).unwrap();
        assert_eq!(missing["status"], "NotInstalled");

        let git = serde_json::to_value(GitStatus::Installed {
            version: "2.44.0".into(),
        })
        .unwrap();
        assert_eq!(git["status"], "Installed");
        assert_eq!(git["version"], "2.44.0");
    }
}
