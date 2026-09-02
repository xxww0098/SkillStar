//! Exact-commit scanner for immutable shared-channel publication.

use super::release::{cancelled_error, integrity_error, valid_content_root, validate_commit_sha};
use super::{
    ChannelDraftScanner, ChannelDraftSkill, ChannelDraftSnapshot, RemoteRepository,
    SharedChannelError, SharedChannelErrorCode,
};
use std::io::Read;
use std::path::{Component, Path};
use std::process::Stdio;

#[derive(Clone)]
pub struct GitChannelDraftScanner {
    facade: skillstar_skills::git_skill::GitSkillFacade,
}

impl GitChannelDraftScanner {
    pub fn new(facade: skillstar_skills::git_skill::GitSkillFacade) -> Self {
        Self { facade }
    }
}

impl ChannelDraftScanner for GitChannelDraftScanner {
    fn scan(
        &self,
        repository: &RemoteRepository,
    ) -> Result<ChannelDraftSnapshot, SharedChannelError> {
        let session = self.facade.session();
        if session.is_cancelled() {
            return Err(cancelled_error());
        }
        let checkout = tempfile::tempdir().map_err(|_| archive_error())?;
        let repository_dir = checkout.path().join("repository");
        let destination = repository_dir.to_str().ok_or_else(|| {
            integrity_error("The isolated channel checkout path is not valid UTF-8")
        })?;
        let mut command = skillstar_core::infra::path_env::command_with_path("git");
        skillstar_skills::git::transport::execute_remote_command(
            &mut command,
            None,
            &[
                "clone",
                "--depth",
                "1",
                "--filter=blob:none",
                "--no-checkout",
                "--single-branch",
                "--branch",
                &repository.default_branch,
                "--",
                &repository.clone_url,
                destination,
            ],
            &repository.clone_url,
            session,
        )
        .map_err(|_| scanner_error(session))?;
        session.emit(
            skillstar_skills::git::transport::GitOperationPhase::Running,
            &repository.clone_url,
        );
        let commit_sha = skillstar_skills::git::ops::rev_parse(&repository_dir, "HEAD")
            .map_err(|_| scanner_error(session))?;
        validate_commit_sha(&commit_sha)?;
        let snapshot = snapshot_published_tree(
            &repository_dir,
            &repository.clone_url,
            &repository.name,
            &commit_sha,
            session,
        )?;
        if session.is_cancelled() {
            session.emit(
                skillstar_skills::git::transport::GitOperationPhase::Cancelled,
                &repository.clone_url,
            );
            return Err(cancelled_error());
        }
        session.emit(
            skillstar_skills::git::transport::GitOperationPhase::Completed,
            &repository.clone_url,
        );
        Ok(snapshot)
    }
}

fn snapshot_published_tree(
    repository_dir: &Path,
    remote: &str,
    repository_name: &str,
    commit_sha: &str,
    session: &skillstar_skills::git::transport::GitOperationSession,
) -> Result<ChannelDraftSnapshot, SharedChannelError> {
    const MAX_TRACKED_ENTRIES: usize = 16_384;
    const MAX_TRACKED_BYTES: u64 = 256 * 1024 * 1024;

    let entries = skillstar_skills::git::ops::list_tree_entries_at(repository_dir, commit_sha)
        .map_err(|_| scanner_error(session))?;
    for entry in &entries {
        if session.is_cancelled() {
            return Err(cancelled_error());
        }
        let relative_path = Path::new(&entry.path);
        if relative_path.is_absolute()
            || entry.path.contains('\\')
            || relative_path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(integrity_error(
                "The channel draft contains an unsafe tracked path",
            ));
        }
    }
    let files = entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    let root_skill = files.contains(&"SKILL.md");
    let mut roots = files
        .iter()
        .filter(|path| path.ends_with("/SKILL.md"))
        .filter_map(|path| Path::new(path).parent())
        .filter_map(Path::to_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    if !root_skill && roots.is_empty() {
        return Ok(ChannelDraftSnapshot {
            commit_sha: commit_sha.to_string(),
            skills: Vec::new(),
        });
    }
    if roots.iter().any(|path| !valid_content_root(path)) {
        return Err(integrity_error(
            "The repository contains an unsafe Skill content root",
        ));
    }
    let relevant_entries = entries
        .iter()
        .filter(|entry| {
            root_skill
                || roots.iter().any(|root| {
                    entry.path == root.as_str() || entry.path.starts_with(&format!("{root}/"))
                })
        })
        .collect::<Vec<_>>();
    if relevant_entries.len() > MAX_TRACKED_ENTRIES {
        return Err(integrity_error(
            "The channel draft exceeds the bounded publication snapshot limits",
        ));
    }
    if relevant_entries.iter().any(|entry| {
        entry.kind != "blob" || !matches!(entry.mode.as_str(), "100644" | "100755" | "120000")
    }) {
        return Err(integrity_error(
            "The channel draft contains an unsupported tracked object",
        ));
    }

    // Git archive normally honors export-ignore/export-subst and text/eol
    // conversion from the commit or the host (`core.autocrlf=true` on
    // Windows). This isolated checkout overrides those attributes at Git's
    // highest-precedence info layer so the archive is a raw, complete
    // projection of the exact tree.
    let info_dir = repository_dir.join(".git/info");
    std::fs::create_dir_all(&info_dir).map_err(|_| archive_error())?;
    std::fs::write(
        info_dir.join("attributes"),
        b"** -export-ignore -export-subst -text\n",
    )
    .map_err(|_| archive_error())?;

    let staging = tempfile::tempdir().map_err(|_| archive_error())?;
    let extraction = staging.path().join("repository");
    std::fs::create_dir(&extraction).map_err(|_| archive_error())?;
    let mut command = skillstar_core::infra::path_env::command_with_path("git");
    command.current_dir(repository_dir);
    skillstar_skills::git::transport::configure_remote_command(&mut command, remote, session)
        .map_err(|_| scanner_error(session))?;
    command
        .args(["-c", "core.autocrlf=false", "-c", "core.eol=lf"])
        .arg("archive")
        .arg("--format=tar")
        .arg(commit_sha)
        .arg("--");
    if !root_skill {
        command.args(&roots);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    skillstar_skills::git::transport::configure_process_group(&mut command);
    let mut child = command.spawn().map_err(|_| archive_error())?;
    let stdout = child.stdout.take().ok_or_else(archive_error)?;
    let mut stderr = child.stderr.take().ok_or_else(archive_error)?;
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let extraction_for_unpack = extraction.clone();
    let unpack_session = session.clone();
    let (unpack_sender, unpack_receiver) = std::sync::mpsc::channel();
    let unpack_reader = std::thread::spawn(move || {
        let result = (|| -> Result<(), SharedChannelError> {
            let mut archive = tar::Archive::new(stdout);
            let entries = archive.entries().map_err(|_| archive_error())?;
            let mut entry_count = 0usize;
            let mut total_bytes = 0u64;
            for entry in entries {
                if unpack_session.is_cancelled() {
                    return Err(cancelled_error());
                }
                let mut entry = entry.map_err(|_| archive_error())?;
                entry_count += 1;
                total_bytes = total_bytes
                    .checked_add(entry.size())
                    .ok_or_else(archive_error)?;
                if entry_count > MAX_TRACKED_ENTRIES || total_bytes > MAX_TRACKED_BYTES {
                    return Err(integrity_error(
                        "The channel draft exceeds the bounded publication snapshot limits",
                    ));
                }
                let path = entry.path().map_err(|_| archive_error())?;
                if path.is_absolute()
                    || path.components().any(|component| {
                        !matches!(component, Component::Normal(_) | Component::CurDir)
                    })
                {
                    return Err(integrity_error(
                        "The channel draft archive contains an unsafe path",
                    ));
                }
                if !entry
                    .unpack_in(&extraction_for_unpack)
                    .map_err(|_| archive_error())?
                {
                    return Err(integrity_error(
                        "The channel draft archive escaped its staging root",
                    ));
                }
            }
            Ok(())
        })();
        let _ = unpack_sender.send(result);
    });
    let mut unpack_result = None;
    let status = loop {
        if unpack_result.is_none() {
            match unpack_receiver.try_recv() {
                Ok(Ok(())) => unpack_result = Some(Ok(())),
                Ok(Err(error)) => {
                    skillstar_skills::git::transport::terminate_child_tree(&mut child);
                    let _ = child.wait();
                    let _ = unpack_reader.join();
                    let _ = stderr_reader.join();
                    return Err(error);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    skillstar_skills::git::transport::terminate_child_tree(&mut child);
                    let _ = child.wait();
                    let _ = unpack_reader.join();
                    let _ = stderr_reader.join();
                    return Err(archive_error());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if session.is_cancelled() {
            skillstar_skills::git::transport::terminate_child_tree(&mut child);
            let _ = child.wait();
            let _ = unpack_reader.join();
            let _ = stderr_reader.join();
            session.emit(
                skillstar_skills::git::transport::GitOperationPhase::Cancelled,
                remote,
            );
            return Err(cancelled_error());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(_) => {
                skillstar_skills::git::transport::terminate_child_tree(&mut child);
                let _ = child.wait();
                let _ = unpack_reader.join();
                let _ = stderr_reader.join();
                return Err(archive_error());
            }
        }
    };
    let unpack_result = match unpack_result {
        Some(result) => result,
        None => unpack_receiver.recv().map_err(|_| archive_error())?,
    };
    unpack_reader.join().map_err(|_| archive_error())?;
    let _ = stderr_reader.join();
    unpack_result?;
    if !status.success() {
        return Err(scanner_error(session));
    }

    let discovered = skillstar_skills::discovery::collapse_pack_identity_copies(
        skillstar_skills::discovery::discover_skills_without_dedup(
            &extraction,
            true,
            Some(repository_name),
        ),
    )
    .map_err(|_| integrity_error("The channel draft contains duplicate Skill identities"))?;
    let mut skills = Vec::with_capacity(discovered.len());
    for skill in discovered {
        if session.is_cancelled() {
            return Err(cancelled_error());
        }
        let root = if skill.folder_path.is_empty() {
            extraction.clone()
        } else {
            extraction.join(&skill.folder_path)
        };
        let snapshot =
            skillstar_skills::content::snapshot_path(&skill.id, &root).map_err(|_| {
                integrity_error("A Skill in the channel draft could not be hashed safely")
            })?;
        skills.push(ChannelDraftSkill {
            id: skill.id,
            content_root: skill.folder_path,
            content_hash: snapshot.content_hash,
        });
    }
    skills.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ChannelDraftSnapshot {
        commit_sha: commit_sha.to_string(),
        skills,
    })
}

fn scanner_error(
    session: &skillstar_skills::git::transport::GitOperationSession,
) -> SharedChannelError {
    if session.is_cancelled() {
        cancelled_error()
    } else {
        SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "Unable to scan the channel draft with the secure Git session",
        )
    }
}

fn archive_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Protocol,
        "Unable to stage the exact channel commit for publication",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_commit_snapshot_uses_the_shared_known_content_hash_and_ignores_untracked_files() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("skills/writer/scripts")).unwrap();
        std::fs::write(
            repo.join("skills/writer/SKILL.md"),
            b"---\nname: writer\n---\n",
        )
        .unwrap();
        std::fs::write(repo.join("skills/writer/scripts/run.sh"), b"echo ok\n").unwrap();
        init_lf_git(&repo);
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "fixture"]);
        let commit = skillstar_skills::git::ops::rev_parse(&repo, "HEAD").unwrap();
        let expected =
            skillstar_skills::content::snapshot_path("writer", &repo.join("skills/writer"))
                .unwrap()
                .content_hash;
        std::fs::write(repo.join("skills/writer/untracked.txt"), b"must not hash").unwrap();

        let snapshot = snapshot_published_tree(
            &repo,
            "https://github.com/acme/repo.git",
            "repo",
            &commit,
            &skillstar_skills::git::transport::GitOperationSession::public(),
        )
        .unwrap();

        assert_eq!(snapshot.commit_sha, commit);
        assert_eq!(snapshot.skills.len(), 1);
        assert_eq!(snapshot.skills[0].id, "writer");
        assert_eq!(snapshot.skills[0].content_root, "skills/writer");
        assert_eq!(snapshot.skills[0].content_hash, expected);
        assert_eq!(
            expected,
            "sha256:6e8b30c29c269c5375c2149f4834f8f6d289e5842b6d75f0f912749605a537f7",
            "LF-pinned fixture must keep the shared content-hash algorithm"
        );
    }

    #[test]
    fn exact_commit_snapshot_collapses_catalog_and_harness_copies() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("skills/writer")).unwrap();
        std::fs::create_dir_all(repo.join(".cursor/skills/writer")).unwrap();
        std::fs::write(
            repo.join("skills/writer/SKILL.md"),
            b"---\nname: writer\n---\n",
        )
        .unwrap();
        std::fs::write(
            repo.join(".cursor/skills/writer/SKILL.md"),
            b"---\nname: writer\n---\n",
        )
        .unwrap();
        init_lf_git(&repo);
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "pack"]);
        let commit = skillstar_skills::git::ops::rev_parse(&repo, "HEAD").unwrap();

        let snapshot = snapshot_published_tree(
            &repo,
            "https://github.com/acme/repo.git",
            "repo",
            &commit,
            &skillstar_skills::git::transport::GitOperationSession::public(),
        )
        .unwrap();

        assert_eq!(snapshot.skills.len(), 1);
        assert_eq!(snapshot.skills[0].id, "writer");
        assert_eq!(snapshot.skills[0].content_root, "skills/writer");
    }

    #[test]
    fn exact_commit_snapshot_rejects_case_insensitive_duplicate_skill_identities() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("skills/one")).unwrap();
        std::fs::create_dir_all(repo.join("skills/two")).unwrap();
        std::fs::write(
            repo.join("skills/one/SKILL.md"),
            b"---\nname: Writer\n---\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("skills/two/SKILL.md"),
            b"---\nname: writer\n---\n",
        )
        .unwrap();
        init_lf_git(&repo);
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "duplicates"]);
        let commit = skillstar_skills::git::ops::rev_parse(&repo, "HEAD").unwrap();

        let error = snapshot_published_tree(
            &repo,
            "https://github.com/acme/repo.git",
            "repo",
            &commit,
            &skillstar_skills::git::transport::GitOperationSession::public(),
        )
        .unwrap_err();

        assert_eq!(error.code, SharedChannelErrorCode::Integrity);
        assert!(error.message.contains("duplicate Skill identities"));
    }

    #[test]
    fn exact_commit_snapshot_disables_export_ignore_and_export_subst() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("skills/writer")).unwrap();
        std::fs::write(
            repo.join("skills/writer/SKILL.md"),
            b"---\nname: writer\n---\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("skills/writer/template.txt"),
            b"commit=$Format:%H$\n",
        )
        .unwrap();
        std::fs::write(
            repo.join(".gitattributes"),
            b"* text=auto\nskills/writer/SKILL.md export-ignore\nskills/writer/template.txt export-subst\n",
        )
        .unwrap();
        init_lf_git(&repo);
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "attributes"]);
        let commit = skillstar_skills::git::ops::rev_parse(&repo, "HEAD").unwrap();
        let expected =
            skillstar_skills::content::snapshot_path("writer", &repo.join("skills/writer"))
                .unwrap()
                .content_hash;

        let snapshot = snapshot_published_tree(
            &repo,
            "https://github.com/acme/repo.git",
            "repo",
            &commit,
            &skillstar_skills::git::transport::GitOperationSession::public(),
        )
        .unwrap();

        assert_eq!(snapshot.skills.len(), 1);
        assert_eq!(snapshot.skills[0].content_hash, expected);
    }

    #[test]
    fn scanner_uses_the_api_default_branch_in_an_isolated_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        init_lf_git(&source);
        std::fs::write(source.join("SKILL.md"), b"---\nname: legacy-skill\n---\n").unwrap();
        run_git(&source, &["add", "."]);
        run_git(&source, &["commit", "-m", "legacy"]);
        run_git(&source, &["branch", "-M", "legacy"]);
        run_git(&source, &["checkout", "-b", "main"]);
        std::fs::write(source.join("SKILL.md"), b"---\nname: main-skill\n---\n").unwrap();
        run_git(&source, &["add", "."]);
        run_git(&source, &["commit", "-m", "main"]);
        let remote = temp.path().join("remote.git");
        run_git(&source, &["clone", "--bare", ".", remote.to_str().unwrap()]);
        let scanner =
            GitChannelDraftScanner::new(skillstar_skills::git_skill::GitSkillFacade::new(
                skillstar_skills::git::transport::GitOperationSession::public(),
            ));
        let repository = RemoteRepository {
            id: 42,
            owner_id: 7,
            owner_login: "acme".into(),
            owner_type: "Organization".into(),
            name: "shared".into(),
            default_branch: "legacy".into(),
            html_url: "https://github.com/acme/shared".into(),
            clone_url: remote.to_string_lossy().into_owned(),
            private: true,
            permissions: crate::shared_channels::RepositoryPermissions {
                admin: true,
                maintain: true,
                push: true,
                pull: true,
            },
        };

        let snapshot = scanner.scan(&repository).unwrap();

        assert_eq!(snapshot.skills.len(), 1);
        assert_eq!(snapshot.skills[0].id, "legacy-skill");
    }

    fn init_lf_git(directory: &Path) {
        if !directory.join(".git").exists() {
            run_git(directory, &["init"]);
        }
        run_git(directory, &["config", "user.email", "test@example.com"]);
        run_git(directory, &["config", "user.name", "SkillStar Test"]);
        run_git(directory, &["config", "core.autocrlf", "false"]);
        run_git(directory, &["config", "core.eol", "lf"]);
        let attributes = directory.join(".gitattributes");
        if !attributes.exists() {
            std::fs::write(&attributes, b"* -text\n").unwrap();
        }
    }

    fn run_git(directory: &Path, args: &[&str]) {
        let output = skillstar_core::infra::path_env::command_with_path("git")
            .current_dir(directory)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
