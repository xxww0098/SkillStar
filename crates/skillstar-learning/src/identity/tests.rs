use super::*;

fn sha(nibble: char) -> String {
    format!("sha256:{}", nibble.to_string().repeat(64))
}

fn commit(nibble: char) -> String {
    nibble.to_string().repeat(40)
}

#[test]
fn identity_keys_are_stable_and_domain_separated() {
    let git_main = SkillIdentity::git(
        "https://github.com/owner/repo.git",
        GitTrackingRef::DefaultBranch,
        "",
    )
    .unwrap();
    let git_named = SkillIdentity::git(
        "https://github.com/owner/repo.git",
        GitTrackingRef::Named {
            name: "release".to_string(),
        },
        "",
    )
    .unwrap();
    let git_folder = SkillIdentity::git(
        "https://github.com/owner/repo.git",
        GitTrackingRef::DefaultBranch,
        "skills/demo",
    )
    .unwrap();
    let other_repo = SkillIdentity::git(
        "https://github.com/owner/other.git",
        GitTrackingRef::DefaultBranch,
        "",
    )
    .unwrap();
    let local = SkillIdentity::local(uuid::Uuid::from_u128(1)).unwrap();
    let channel = SkillIdentity::channel(42, "skills/demo").unwrap();

    assert!(git_main.key.as_str().starts_with("ski:v1:"));
    assert_eq!(git_main.key.as_str().len(), "ski:v1:".len() + 64);
    assert_ne!(git_main.key, git_named.key);
    assert_ne!(git_main.key, git_folder.key);
    assert_ne!(git_main.key, other_repo.key);
    assert_ne!(git_main.key, local.key);
    assert_ne!(channel.key, git_folder.key);
    assert_eq!(
        git_main.key.storage_segment(),
        git_main.key.as_str().replace(':', "-")
    );
}

#[test]
fn display_name_and_optional_release_do_not_change_keys() {
    let identity = SkillIdentity::channel(7, "skills/writer").unwrap();
    let content = ContentRevision::new(2, sha('a')).unwrap();
    let without_release =
        SkillRevision::channel(&identity, commit('b'), None, content.clone()).unwrap();
    let with_release = SkillRevision::channel(
        &identity,
        commit('b'),
        Some(ChannelReleaseRef {
            revision: 12,
            tag_name: "channel-v000012".to_string(),
        }),
        content,
    )
    .unwrap();
    assert_eq!(without_release.key, with_release.key);

    let resolved_a = ResolvedSkill::new(
        identity.clone(),
        without_release.clone(),
        "Writer",
        Some("writer".to_string()),
    )
    .unwrap();
    let resolved_b = ResolvedSkill::new(identity, without_release, "Other Label", None).unwrap();
    assert_eq!(resolved_a.identity.key, resolved_b.identity.key);
    assert_eq!(resolved_a.revision.key, resolved_b.revision.key);
}

#[test]
fn stored_key_mismatch_and_variant_mismatch_fail_closed() {
    let identity = SkillIdentity::local(uuid::Uuid::from_u128(9)).unwrap();
    let mut tampered = identity.clone();
    tampered.key = SkillIdentityKey::from_digest(&[0; 32]);
    assert!(tampered.verified().is_err());

    let content = ContentRevision::new(2, sha('c')).unwrap();
    assert!(SkillRevision::git(&identity, commit('d'), commit('e'), content).is_err());
    assert!(ContentRevision::new(1, sha('c')).is_err());
    assert!(normalize_content_root("../escape").is_err());
    assert!(normalize_content_root("skills/demo").unwrap() == "skills/demo");
    assert_eq!(normalize_content_root("/").unwrap(), "");
}

#[test]
fn local_edit_keeps_identity_and_changes_revision() {
    let identity = SkillIdentity::git(
        "https://github.com/owner/repo.git",
        GitTrackingRef::DefaultBranch,
        "skills/demo",
    )
    .unwrap();
    let first = SkillRevision::git(
        &identity,
        commit('1'),
        commit('2'),
        ContentRevision::new(2, sha('a')).unwrap(),
    )
    .unwrap();
    let edited = SkillRevision::git(
        &identity,
        commit('1'),
        commit('2'),
        ContentRevision::new(2, sha('b')).unwrap(),
    )
    .unwrap();
    assert_eq!(first.skill_key, edited.skill_key);
    assert_ne!(first.key, edited.key);
}
