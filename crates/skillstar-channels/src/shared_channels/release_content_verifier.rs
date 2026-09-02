use super::git_read::git_read_error;
use super::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelReleaseManifest, ChannelSkillReleaseStatus,
    RemoteRepository, SharedChannelError, SharedChannelErrorCode,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) fn verify_release_content_blocking(
    git: &skillstar_skills::git_skill::GitSkillFacade,
    repository: &RemoteRepository,
    manifest: &ChannelReleaseManifest,
) -> Result<(), SharedChannelError> {
    let _guard =
        skillstar_skills::skill_update::acquire_update_transaction_lock().map_err(|error| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionUpdateFailed,
                format!("Unable to lock channel verification: {error}"),
            )
        })?;
    // Verification must never reuse the installed ref cache: Hub Skills may
    // point into that checkout, and fetching it would reset user edits before
    // divergence handling can offer preserve/discard choices.
    let verification_source = format!(
        "channel-verify-{}-{}",
        repository.id,
        skillstar_skills::repo_scanner::cache_dir_name(&repository.clone_url)
    );
    let repo_dir = skillstar_skills::repo_scanner::clone_or_fetch_repo_at_in_session(
        &repository.clone_url,
        &verification_source,
        Some(&manifest.commit_sha),
        git.session(),
    )
    .map_err(release_content_git_error)?;
    verify_release_checkout(&repo_dir, &repository.name, manifest)
}

fn verify_release_checkout(
    repo_dir: &Path,
    repository_name: &str,
    manifest: &ChannelReleaseManifest,
) -> Result<(), SharedChannelError> {
    verify_checkout_head(repo_dir, &manifest.commit_sha)?;

    let default_skill_id = root_skill_default_id(repository_name, &manifest.skills);
    let discovered = skillstar_skills::discovery::collapse_pack_identity_copies(
        skillstar_skills::discovery::discover_skills_without_dedup(
            repo_dir,
            true,
            Some(default_skill_id),
        ),
    )
    .map_err(|_| content_integrity_error())?
    .into_iter()
    .map(|skill| (skill.id.to_ascii_lowercase(), skill))
    .collect::<BTreeMap<_, _>>();
    let released = manifest
        .skills
        .iter()
        .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
        .collect::<Vec<_>>();
    let released_ids = released
        .iter()
        .map(|skill| skill.id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    if discovered.len() != released.len() || discovered.keys().any(|id| !released_ids.contains(id))
    {
        return Err(content_integrity_error());
    }
    for skill in released {
        let Some(target) = discovered.get(&skill.id.to_ascii_lowercase()) else {
            return Err(content_integrity_error());
        };
        if target.id != skill.id
            || target.folder_path != skill.content_root
            || skill.content_hash_version != CHANNEL_CONTENT_HASH_VERSION
        {
            return Err(content_integrity_error());
        }
        let target_root = if skill.content_root.is_empty() {
            repo_dir.to_path_buf()
        } else {
            repo_dir.join(&skill.content_root)
        };
        let snapshot = skillstar_skills::content::snapshot_path(&skill.id, &target_root)
            .map_err(|_| content_integrity_error())?;
        if snapshot.content_hash != skill.content_hash {
            return Err(content_integrity_error());
        }
    }
    verify_checkout_head(repo_dir, &manifest.commit_sha)
}

fn root_skill_default_id<'a>(
    repository_name: &'a str,
    skills: &'a [super::ChannelReleaseSkill],
) -> &'a str {
    skills
        .iter()
        .find(|skill| {
            skill.content_root.is_empty() && skill.status != ChannelSkillReleaseStatus::Removed
        })
        .map(|skill| skill.id.as_str())
        .unwrap_or(repository_name)
}

fn release_content_git_error(error: anyhow::Error) -> SharedChannelError {
    use skillstar_skills::git::transport::GitTransportErrorCode;

    let missing_exact_object = error
        .chain()
        .find_map(|cause| {
            cause.downcast_ref::<skillstar_skills::git::transport::GitTransportError>()
        })
        .is_some_and(|transport| {
            transport.code == GitTransportErrorCode::Other
                && [
                    "not our ref",
                    "couldn't find remote ref",
                    "reference is not a tree",
                    "bad object",
                    "unadvertised object",
                ]
                .iter()
                .any(|needle| transport.message.to_ascii_lowercase().contains(needle))
        });
    if missing_exact_object {
        return SharedChannelError::new(
            SharedChannelErrorCode::Integrity,
            format!(
                "Unable to verify channel release content because its exact published commit is unavailable: {error:#}"
            ),
        );
    }
    git_read_error(error, "Unable to verify channel release content")
}

fn verify_checkout_head(repo_dir: &Path, expected: &str) -> Result<(), SharedChannelError> {
    let head = skillstar_skills::git::ops::rev_parse(repo_dir, "HEAD")
        .map_err(|_| content_integrity_error())?;
    if head.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(content_integrity_error())
    }
}

fn content_integrity_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Integrity,
        "The published channel release content does not match its exact commit, Skill inventory, paths, or content hashes",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_skills::git::transport::{GitTransportError, GitTransportErrorCode};
    use std::fs;

    fn transport_error(code: GitTransportErrorCode, message: &str) -> anyhow::Error {
        anyhow::Error::new(GitTransportError {
            code,
            message: message.into(),
            session_id: "test".into(),
        })
    }

    #[test]
    fn missing_exact_commit_is_integrity_but_repository_access_loss_stays_revoked() {
        assert_eq!(
            release_content_git_error(transport_error(
                GitTransportErrorCode::Other,
                "fatal: remote error: upload-pack: not our ref abc",
            ))
            .code,
            SharedChannelErrorCode::Integrity
        );
        assert_eq!(
            release_content_git_error(transport_error(
                GitTransportErrorCode::AppNotInstalled,
                "repository not found",
            ))
            .code,
            SharedChannelErrorCode::AppRepositoryAccessRequired
        );
    }

    #[test]
    fn repository_rename_keeps_the_manifest_identity_for_a_root_skill() {
        let skill = super::super::ChannelReleaseSkill {
            id: "old-repository-name".into(),
            content_root: String::new(),
            content_hash: format!("sha256:{}", "a".repeat(64)),
            content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            status: ChannelSkillReleaseStatus::Updated,
        };

        assert_eq!(
            root_skill_default_id("renamed-repository", &[skill]),
            "old-repository-name"
        );
    }

    #[test]
    fn exact_checkout_verifier_rejects_extra_missing_and_modified_skill_content() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path();
        let skill_root = repository.join("skills/writer");
        fs::create_dir_all(&skill_root).unwrap();
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: writer\ndescription: Test writer\n---\n# Writer\n",
        )
        .unwrap();
        git(repository, &["init", "-q"]);
        git(
            repository,
            &["config", "user.email", "test@skillstar.local"],
        );
        git(repository, &["config", "user.name", "SkillStar Test"]);
        git(repository, &["add", "."]);
        git(repository, &["commit", "-qm", "initial"]);
        let commit = git_output(repository, &["rev-parse", "HEAD"]);
        let snapshot = skillstar_skills::content::snapshot_path("writer", &skill_root).unwrap();
        let manifest = manifest(&commit, &snapshot.content_hash);

        verify_release_checkout(repository, "renamed-channel", &manifest).unwrap();

        let harness = repository.join(".cursor/skills/writer");
        fs::create_dir_all(&harness).unwrap();
        fs::write(
            harness.join("SKILL.md"),
            "---\nname: writer\ndescription: Test writer\n---\n# Writer\n",
        )
        .unwrap();
        verify_release_checkout(repository, "renamed-channel", &manifest)
            .expect("catalog + harness copy is one Skill, not a second identity");
        fs::remove_dir_all(repository.join(".cursor")).unwrap();

        let extra = repository.join("skills/extra");
        fs::create_dir_all(&extra).unwrap();
        fs::write(
            extra.join("SKILL.md"),
            "---\nname: extra\ndescription: Extra\n---\n# Extra\n",
        )
        .unwrap();
        assert_eq!(
            verify_release_checkout(repository, "renamed-channel", &manifest)
                .unwrap_err()
                .code,
            SharedChannelErrorCode::Integrity
        );
        fs::remove_dir_all(extra).unwrap();

        fs::write(skill_root.join("SKILL.md"), "# tampered\n").unwrap();
        assert_eq!(
            verify_release_checkout(repository, "renamed-channel", &manifest)
                .unwrap_err()
                .code,
            SharedChannelErrorCode::Integrity
        );
        fs::remove_dir_all(skill_root).unwrap();
        assert_eq!(
            verify_release_checkout(repository, "renamed-channel", &manifest)
                .unwrap_err()
                .code,
            SharedChannelErrorCode::Integrity
        );
    }

    fn manifest(commit: &str, content_hash: &str) -> ChannelReleaseManifest {
        ChannelReleaseManifest {
            schema_version: super::super::CHANNEL_RELEASE_MANIFEST_VERSION,
            repository_id: 42,
            organization_id: 7,
            revision: 1,
            tag_name: super::super::revision_tag(1),
            commit_sha: commit.into(),
            publisher: super::super::ChannelPublisherIdentity {
                id: 9,
                login: "alice".into(),
            },
            published_at: "2026-08-05T00:00:00Z".into(),
            title: "Release one".into(),
            notes: "Initial release".into(),
            skills: vec![super::super::ChannelReleaseSkill {
                id: "writer".into(),
                content_root: "skills/writer".into(),
                content_hash: content_hash.into(),
                content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
                status: ChannelSkillReleaseStatus::Added,
            }],
        }
    }

    fn git(repository: &Path, args: &[&str]) {
        let status = skillstar_core::infra::path_env::command_with_path("git")
            .current_dir(repository)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git command failed: {args:?}");
    }

    fn git_output(repository: &Path, args: &[&str]) -> String {
        let output = skillstar_core::infra::path_env::command_with_path("git")
            .current_dir(repository)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git command failed: {args:?}");
        String::from_utf8(output.stdout).unwrap().trim().into()
    }
}
