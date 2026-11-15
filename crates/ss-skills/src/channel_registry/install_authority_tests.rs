use super::{
    CHANNEL_CONTENT_HASH_VERSION, CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
    CHANNEL_SUBSCRIPTION_STORE_VERSION, ChannelInstallAuthority, ChannelReleaseTarget,
    ChannelSkillProvenance, ChannelSubscribedSkill, ChannelSubscription,
    ChannelSubscriptionRegistry, ChannelSubscriptionStore, DiskChannelSubscriptionRegistry,
    revision_tag,
};
use std::path::Path;

const CHANNEL_URL: &str = "https://github.com/acme/channel.git";

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
        well_known: false,
    };
    let unit = crate::installer::InstallUnit {
        id: "writer".into(),
        folder_path: "skills/writer".into(),
    };
    let error = crate::installer::install_units_for_channel(
        origin.path(),
        &spec,
        std::slice::from_ref(&unit),
        &ChannelInstallAuthority::for_repository(42 + 57),
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
