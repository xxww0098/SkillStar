use std::path::Path;

use super::*;

const REPOSITORY_ID: u64 = 4242;
const ORGANIZATION_ID: u64 = 7;
const COMMIT_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn write_hub_skill(id: &str, description: &str) {
    let root = ss_core::infra::paths::hub_skills_dir().join(id);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        format!("---\nname: {id}\ndescription: {description}\n---\n\n# {id}\n"),
    )
    .unwrap();
}

fn subscribed_skill(id: &str) -> ChannelSubscribedSkill {
    let snapshot = crate::content::snapshot(id).unwrap();
    ChannelSubscribedSkill {
        id: id.to_string(),
        content_root: id.to_string(),
        release_content_hash: snapshot.content_hash.clone(),
        release_content_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
        baseline_hash: snapshot.content_hash.clone(),
        baseline_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: REPOSITORY_ID,
            repository_url: "https://github.com/acme-org/team-channel.git".to_string(),
            git_ref: COMMIT_SHA.to_string(),
            source_folder: id.to_string(),
        },
    }
}

fn descriptor() -> SharedChannelDescriptor {
    SharedChannelDescriptor {
        descriptor_version: CHANNEL_DESCRIPTOR_VERSION,
        repository_id: REPOSITORY_ID,
        organization_id: ORGANIZATION_ID,
        owner: "acme-org".to_string(),
        name: "team-channel".to_string(),
        html_url: "https://github.com/acme-org/team-channel".to_string(),
        clone_url: "https://github.com/acme-org/team-channel.git".to_string(),
        role: SharedChannelRole::Owner,
        status: SharedChannelStatus::Active,
        authorization: SharedChannelAuthorization::default(),
        created_at: "2026-10-07T00:00:00Z".to_string(),
        updated_at: "2026-10-07T00:00:00Z".to_string(),
    }
}

fn subscription(skills: Vec<ChannelSubscribedSkill>) -> ChannelSubscription {
    ChannelSubscription {
        descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
        repository_id: REPOSITORY_ID,
        organization_id: ORGANIZATION_ID,
        repository_url_aliases: Vec::new(),
        target: ChannelReleaseTarget {
            revision: 42,
            tag_name: "channel-v000042".to_string(),
            commit_sha: COMMIT_SHA.to_string(),
        },
        known_skill_ids: skills.iter().map(|skill| skill.id.clone()).collect(),
        skills,
        pins: Vec::new(),
        last_update: None,
        auto_update: ChannelAutoUpdateState::default(),
        remote_state: ChannelSubscriptionRemoteState::default(),
        created_at: "2026-10-07T00:00:00Z".to_string(),
        updated_at: "2026-10-07T00:00:00Z".to_string(),
    }
}

fn write_registries(descriptor: &SharedChannelDescriptor, subscriptions: &[ChannelSubscription]) {
    let channels = SharedChannelStore {
        schema_version: SHARED_CHANNEL_STORE_VERSION,
        channels: vec![descriptor.clone()],
    };
    let store = ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: subscriptions.to_vec(),
    };
    std::fs::create_dir_all(DiskSharedChannelRegistry::path().parent().unwrap()).unwrap();
    std::fs::write(
        DiskSharedChannelRegistry::path(),
        serde_json::to_vec(&channels).unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(DiskChannelSubscriptionRegistry::path().parent().unwrap()).unwrap();
    std::fs::write(
        DiskChannelSubscriptionRegistry::path(),
        serde_json::to_vec(&store).unwrap(),
    )
    .unwrap();
}

#[test]
fn exports_active_subscription_to_a_marketplace_directory() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    write_hub_skill("pr-review", "Review pull requests");
    write_hub_skill("tdd_flow", "Red green refactor");
    write_registries(
        &descriptor(),
        &[subscription(vec![
            subscribed_skill("pr-review"),
            subscribed_skill("tdd_flow"),
        ])],
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("marketplace");
    let exported = export_channel_marketplace(REPOSITORY_ID, &out).unwrap();

    assert_eq!(exported.root, out);
    assert_eq!(exported.revision, 42);
    assert_eq!(exported.plugin_names, ["pr-review", "tdd-flow"]);
    // marketplace.json + 2 plugin.json + 2 SKILL.md
    assert_eq!(exported.files, 5);

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join(".claude-plugin").join("marketplace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"], "team-channel");
    assert_eq!(manifest["owner"]["name"], "acme-org");
    assert_eq!(manifest["version"], "42");
    assert_eq!(manifest["plugins"].as_array().map(Vec::len), Some(2));
    assert_eq!(manifest["plugins"][0]["name"], "pr-review");
    assert_eq!(manifest["plugins"][0]["source"], "./plugins/pr-review");
    assert_eq!(
        manifest["plugins"][0]["description"],
        "Review pull requests"
    );
    // 技能名里的下划线按外部格式净化成连字符,条目名与目录一致。
    assert_eq!(manifest["plugins"][1]["name"], "tdd-flow");
    assert_eq!(manifest["plugins"][1]["source"], "./plugins/tdd-flow");

    let plugin_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            out.join("plugins")
                .join("pr-review")
                .join(".claude-plugin")
                .join("plugin.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(plugin_json["name"], "pr-review");
    assert_eq!(plugin_json["version"], "42");

    let copied = std::fs::read_to_string(
        out.join("plugins")
            .join("tdd-flow")
            .join("skills")
            .join("tdd-flow")
            .join("SKILL.md"),
    )
    .unwrap();
    assert!(copied.contains("tdd_flow"), "内容保持原样,只有目录名净化");
}

#[test]
fn local_edits_and_missing_copies_fail_closed_together() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    write_hub_skill("pr-review", "Review pull requests");
    write_hub_skill("tdd-flow", "Red green refactor");
    let pr = subscribed_skill("pr-review");
    let tdd = subscribed_skill("tdd-flow");
    // tdd-flow 在取完 baseline 后被本地改动;pr-review 的规范副本被整个删掉。
    std::fs::write(
        ss_core::infra::paths::hub_skills_dir()
            .join("tdd-flow")
            .join("SKILL.md"),
        "---\nname: tdd\ndescription: edited\n---\n",
    )
    .unwrap();
    std::fs::remove_dir_all(ss_core::infra::paths::hub_skills_dir().join("pr-review")).unwrap();

    let error = build_channel_marketplace(&descriptor(), &subscription(vec![pr, tdd])).unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert!(
        error.message.contains("'pr-review' is missing"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains("'tdd-flow' was modified locally"),
        "{}",
        error.message
    );
}

#[test]
fn sanitized_name_collision_is_rejected() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    write_hub_skill("pr-review", "Review pull requests");
    write_hub_skill("pr_review", "Another review skill");
    let error = build_channel_marketplace(
        &descriptor(),
        &subscription(vec![
            subscribed_skill("pr-review"),
            subscribed_skill("pr_review"),
        ]),
    )
    .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert!(
        error
            .message
            .contains("both map to plugin name 'pr-review'"),
        "{}",
        error.message
    );
}

#[test]
fn empty_subscription_is_rejected() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let error = build_channel_marketplace(&descriptor(), &subscription(Vec::new())).unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Protocol);
}

#[test]
fn unbound_repository_and_missing_subscription_are_distinct_errors() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    write_hub_skill("pr-review", "Review pull requests");
    let tmp = tempfile::tempdir().unwrap();

    let missing = tmp.path().join("missing-out");
    let error = export_channel_marketplace(999, &missing).unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::RepositoryNotFound);

    write_registries(&descriptor(), &[]);
    let unbound = tmp.path().join("unbound-out");
    let error = export_channel_marketplace(REPOSITORY_ID, &unbound).unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::SubscriptionNotFound);
}

#[test]
fn inactive_channel_cannot_export() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    write_hub_skill("pr-review", "Review pull requests");
    let mut pending = descriptor();
    pending.status = SharedChannelStatus::AwaitingAppInstallation;
    write_registries(
        &pending,
        &[subscription(vec![subscribed_skill("pr-review")])],
    );
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let error = export_channel_marketplace(REPOSITORY_ID, &out).unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Protocol);
}

#[test]
fn non_empty_output_directory_maps_to_storage_error() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    write_hub_skill("pr-review", "Review pull requests");
    write_registries(
        &descriptor(),
        &[subscription(vec![subscribed_skill("pr-review")])],
    );
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("occupied");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), "keep").unwrap();
    let error = export_channel_marketplace(REPOSITORY_ID, &out).unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert!(out.join("keep.txt").is_file());
}

#[test]
fn prepared_payloads_reuse_verified_snapshots_without_rewriting_sources() {
    // 直接驱动 write_prepared_marketplace,验证 build → write 两段可独立复用。
    let _sandbox = crate::test_sandbox::Sandbox::new();
    write_hub_skill("pr-review", "Review pull requests");
    let prepared = build_channel_marketplace(
        &descriptor(),
        &subscription(vec![subscribed_skill("pr-review")]),
    )
    .unwrap();
    assert_eq!(prepared.plugins.len(), 1);
    let tmp = tempfile::tempdir().unwrap();
    let out: &Path = &tmp.path().join("out");
    let written = write_prepared_marketplace(out, &prepared).unwrap();
    assert_eq!(written.plugins, ["pr-review"]);
    assert!(
        out.join(".claude-plugin")
            .join("marketplace.json")
            .is_file()
    );
}
