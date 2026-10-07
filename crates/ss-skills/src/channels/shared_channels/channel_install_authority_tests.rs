//! Channel flows write channel-managed Skills that the generic mutation gate
//! refuses; [`ChannelInstallAuthority`] is the only way through, and only for
//! the channel that owns them.

use super::*;
use crate::channels::shared_channels::{
    CHANNEL_RELEASE_MANIFEST_VERSION, ChannelPublisherIdentity, ChannelReleaseSkill,
    ChannelSkillReleaseStatus, ChannelSubscribedSkill, RepositoryPermissions,
};
use std::path::Path;

const CHANNEL_URL: &str = "https://github.com/acme/channel.git";

fn git(repo: &Path, args: &[&str]) -> String {
    crate::pack_fixture::git(repo, args)
}

fn write_skill(root: &Path, id: &str) {
    let dir = root.join("skills").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {id}\ndescription: {id}\n---\n# {id}\n"),
    )
    .unwrap();
}

fn hash(root: &Path, id: &str) -> String {
    crate::content::snapshot_path(id, &root.join("skills").join(id))
        .unwrap()
        .content_hash
}

fn repository() -> RemoteRepository {
    RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "channel".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url: CHANNEL_URL.into(),
        private: true,
        permissions: RepositoryPermissions {
            admin: false,
            maintain: false,
            push: false,
            pull: true,
        },
    }
}

fn released(id: &str, content_hash: String) -> ChannelReleaseSkill {
    ChannelReleaseSkill {
        id: id.into(),
        content_root: format!("skills/{id}"),
        content_hash,
        content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        status: ChannelSkillReleaseStatus::Added,
    }
}

fn subscribed(id: &str, content_hash: &str, commit: &str) -> ChannelSubscribedSkill {
    ChannelSubscribedSkill {
        id: id.into(),
        content_root: format!("skills/{id}"),
        release_content_hash: content_hash.into(),
        release_content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        baseline_hash: content_hash.into(),
        baseline_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: 42,
            repository_url: CHANNEL_URL.into(),
            git_ref: commit.into(),
            source_folder: format!("skills/{id}"),
        },
    }
}

fn persist_subscription(skill: ChannelSubscribedSkill, commit: &str) {
    DiskChannelSubscriptionRegistry
        .save(&ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![ChannelSubscription {
                descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
                repository_id: 42,
                organization_id: 7,
                repository_url_aliases: Vec::new(),
                target: ChannelReleaseTarget {
                    revision: 1,
                    tag_name: revision_tag(1),
                    commit_sha: commit.into(),
                },
                known_skill_ids: vec![skill.id.clone()],
                skills: vec![skill],
                pins: Vec::new(),
                last_update: None,
                auto_update: Default::default(),
                remote_state: Default::default(),
                created_at: "2026-08-05T00:00:00Z".into(),
                updated_at: "2026-08-05T00:00:00Z".into(),
            }],
        })
        .unwrap();
}

/// Install-and-track a second Skill from a repository a persisted
/// subscription already owns: the generic gate refuses the repository, the
/// channel installer must not.
#[test]
fn tracking_another_skill_of_a_subscribed_channel_passes_the_gate() {
    let sandbox = crate::pack_fixture::Sandbox::new();
    let origin = tempfile::tempdir().unwrap();
    write_skill(origin.path(), "writer");
    write_skill(origin.path(), "reader");
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "tests@skillstar.local"],
        &["config", "user.name", "SkillStar Tests"],
        &["config", "commit.gpgsign", "false"],
        &["add", "."],
        &["commit", "-qm", "release one"],
    ] {
        git(origin.path(), args);
    }
    let commit = git(origin.path(), &["rev-parse", "HEAD"]);
    sandbox.map_github_url(CHANNEL_URL, origin.path());
    let writer_hash = hash(origin.path(), "writer");
    let reader_hash = hash(origin.path(), "reader");
    persist_subscription(subscribed("writer", &writer_hash, &commit), &commit);
    assert!(
        crate::skill_mutation::policy()
            .ensure_repository_mutation_allowed(CHANNEL_URL)
            .is_err(),
        "the generic gate owns this repository"
    );

    let installer =
        GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::from_file_store());
    let receipt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(ChannelSubscriptionInstaller::install(
            &installer,
            ChannelInstallRequest {
                repository: repository(),
                manifest: ChannelReleaseManifest {
                    schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
                    repository_id: 42,
                    organization_id: 7,
                    revision: 1,
                    tag_name: revision_tag(1),
                    commit_sha: commit.clone(),
                    publisher: ChannelPublisherIdentity {
                        id: 9,
                        login: "alice".into(),
                    },
                    published_at: "2026-08-05T00:00:00Z".into(),
                    title: "Release one".into(),
                    notes: String::new(),
                    skills: vec![
                        released("writer", writer_hash),
                        released("reader", reader_hash.clone()),
                    ],
                },
                selected_skill_ids: vec!["reader".into()],
            },
        ))
        .unwrap();

    assert_eq!(receipt.newly_installed_skill_ids, ["reader"]);
    assert_eq!(
        crate::content::snapshot("reader").unwrap().content_hash,
        reader_hash
    );
    let entry = crate::skill_lock::load().skills["reader"].clone();
    assert_eq!(entry.git_ref.as_deref(), Some(commit.as_str()));
}

/// The authority is per channel: it cannot overwrite a Skill another
/// channel manages.
#[test]
fn another_channels_authority_cannot_overwrite_a_managed_skill() {
    let _sandbox = crate::pack_fixture::Sandbox::new();
    let origin = tempfile::tempdir().unwrap();
    write_skill(origin.path(), "writer");
    let writer_hash = hash(origin.path(), "writer");
    let commit = "a".repeat(40);
    persist_subscription(subscribed("writer", &writer_hash, &commit), &commit);

    let spec = crate::source_resolver::Source {
        repo_url: "https://github.com/other/channel.git".into(),
        short: "other/channel".into(),
        git_ref: None,
        subpath: None,
        skill_filter: None,
    };
    let unit = crate::installer::InstallUnit {
        id: "writer".into(),
        folder_path: "skills/writer".into(),
    };
    let error = crate::installer::install_units_for_channel(
        origin.path(),
        &spec,
        std::slice::from_ref(&unit),
        &ChannelInstallAuthority::for_repository(99),
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("managed by shared channel 42"),
        "{error:#}"
    );
    assert!(!ss_core::infra::paths::agents_skill_dir("writer").exists());

    crate::installer::install_units_for_channel(
        origin.path(),
        &spec,
        &[unit],
        &ChannelInstallAuthority::for_repository(42),
    )
    .unwrap();
    assert!(ss_core::infra::paths::agents_skill_dir("writer").is_dir());
}
