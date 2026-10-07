use super::*;

fn entry(name_updates: &str) -> SkillLockEntry {
    SkillLockEntry {
        source: "owner/repo".into(),
        source_type: SourceType::Github,
        source_url: "https://github.com/owner/repo.git".into(),
        git_ref: Some("main".into()),
        skill_path: Some(format!("skills/{name_updates}")),
        skill_folder_hash: Some("abc123".into()),
        installed_at: "2026-10-04T10:00:00Z".into(),
        updated_at: "2026-10-04T10:00:00Z".into(),
        extra: BTreeMap::new(),
    }
}

#[test]
fn roundtrip_preserves_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".skill-lock.json");
    let mut lock = SkillLock::default();
    lock.upsert("foo", entry("foo"));
    lock.save(&path).unwrap();

    let loaded = SkillLock::load(&path);
    assert_eq!(
        loaded.skills["foo"].skill_path.as_deref(),
        Some("skills/foo")
    );
    assert_eq!(loaded.version, SKILL_LOCK_VERSION);
}

#[test]
fn upsert_preserves_installed_at_and_refreshes_fields() {
    let mut lock = SkillLock::default();
    lock.upsert("foo", entry("foo"));
    let mut refreshed = entry("foo");
    refreshed.installed_at = "2026-10-05T10:00:00Z".into();
    refreshed.skill_folder_hash = Some("def456".into());
    lock.upsert("foo", refreshed);

    assert_eq!(lock.skills["foo"].installed_at, "2026-10-04T10:00:00Z");
    assert_eq!(
        lock.skills["foo"].skill_folder_hash.as_deref(),
        Some("def456")
    );
}

#[test]
fn readers_tolerate_but_writers_refuse_unreadable_or_newer_locks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".skill-lock.json");

    for content in ["not json at all", "{\"version\":99,\"skills\":{\"a\":{}}}"] {
        std::fs::write(&path, content).unwrap();
        assert!(SkillLock::load(&path).skills.is_empty());
        let error = SkillLock::load_for_write(&path).unwrap_err().to_string();
        assert!(error.contains("refusing to rewrite"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
    let backups = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains(".bak."))
        .count();
    assert!(backups >= 2, "each refusal takes a backup first");

    std::fs::write(&path, "{\"version\":2,\"skills\":{\"a\":{}}}").unwrap();
    assert!(SkillLock::load_for_write(&path).unwrap().skills.is_empty());
}

#[test]
fn unknown_fields_and_source_types_survive_a_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".skill-lock.json");
    std::fs::write(
        &path,
        r#"{
  "version": 3,
  "dismissed": {"findSkillsPrompt": true},
  "last_selected_agents": ["claude-code"],
  "skills": {
"docs": {"source": "mintlify/docs", "sourceType": "mintlify", "sourceUrl": "https://docs.example.com", "skillFolderHash": "", "installedAt": "t", "updatedAt": "t", "pluginName": "p"},
"broken": {"sourceType": 7},
"foo": {"source": "o/r", "sourceType": "github", "sourceUrl": "https://github.com/o/r.git", "installedAt": "t", "updatedAt": "t", "futureField": 1}
  }
}"#,
    )
    .unwrap();

    let mut lock = SkillLock::load_for_write(&path).unwrap();
    assert_eq!(lock.skills["docs"].source_type, SourceType::Unknown);
    assert!(!lock.skills["docs"].source_type.is_updatable());
    assert!(!lock.skills.contains_key("broken"));
    assert_eq!(lock.last_selected_agents, ["claude-code"]);
    lock.skills.get_mut("foo").unwrap().updated_at = "t2".into();
    lock.save(&path).unwrap();

    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(written["dismissed"]["findSkillsPrompt"], true);
    assert_eq!(written["lastSelectedAgents"][0], "claude-code");
    assert!(written.get("last_selected_agents").is_none());
    assert_eq!(written["skills"]["docs"]["sourceType"], "mintlify");
    assert_eq!(written["skills"]["docs"]["pluginName"], "p");
    assert_eq!(written["skills"]["broken"]["sourceType"], 7);
    assert_eq!(written["skills"]["foo"]["futureField"], 1);
    assert_eq!(written["skills"]["foo"]["updatedAt"], "t2");

    lock.remove("broken");
    lock.save(&path).unwrap();
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(written["skills"].get("broken").is_none());
}

#[test]
fn mutate_rereads_under_the_lock_and_fails_closed() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    mutate(|lock| lock.upsert("a", entry("a"))).unwrap();
    // Another writer lands between two of our cycles.
    let mut outside = SkillLock::load(&lock_path());
    outside.upsert("b", entry("b"));
    outside.save(&lock_path()).unwrap();
    mutate(|lock| lock.upsert("c", entry("c"))).unwrap();
    let names: Vec<_> = load().skills.into_keys().collect();
    assert_eq!(names, ["a", "b", "c"]);

    std::fs::write(lock_path(), "{\"version\":4,\"skills\":{}}").unwrap();
    assert!(mutate(|lock| lock.remove("a")).is_err());
    assert_eq!(
        std::fs::read_to_string(lock_path()).unwrap(),
        "{\"version\":4,\"skills\":{}}"
    );
}

#[test]
fn groups_keyed_by_source_and_ref() {
    let mut lock = SkillLock::default();
    lock.upsert("a", entry("a"));
    let mut other_ref = entry("b");
    other_ref.git_ref = Some("dev".into());
    lock.upsert("b", other_ref);

    let groups = SkillLock::by_source_group(
        &lock
            .skills
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>(),
    );
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[&(
            "https://github.com/owner/repo.git".into(),
            Some("main".into())
        )]
            .len(),
        1
    );
}

#[test]
fn classify_matches_url_shapes() {
    assert_eq!(
        classify_source("https://github.com/o/r.git"),
        SourceType::Github
    );
    assert_eq!(
        classify_source("git@github.com:o/r.git"),
        SourceType::Github
    );
    assert_eq!(
        classify_source("https://gitlab.com/o/r.git"),
        SourceType::Git
    );
    assert_eq!(classify_source("file:///tmp/x"), SourceType::Local);
}

#[test]
fn ref_uses_the_vercel_field_name_and_reads_the_old_one() {
    let mut written = serde_json::to_value(entry("foo")).unwrap();
    assert_eq!(written["ref"], "main");
    assert!(written.get("gitRef").is_none());
    written.as_object_mut().unwrap().remove("ref");
    written["gitRef"] = Value::from("dev");
    let read: SkillLockEntry = serde_json::from_value(written).unwrap();
    assert_eq!(read.git_ref.as_deref(), Some("dev"));
    assert!(read.extra.is_empty(), "{:?}", read.extra);
}

#[test]
fn folder_lookup_finds_entries_vercel_keyed_by_the_raw_name() {
    let mut lock = SkillLock::default();
    lock.skills.insert("My Skill".into(), entry("my-skill"));
    assert_eq!(folder_for_key("My Skill").as_deref(), Some("my-skill"));
    assert_eq!(folder_for_key("../.."), None);
    let (key, found) = lock.entry_for_folder("my-skill").unwrap();
    assert_eq!(key, "My Skill");
    assert_eq!(found.skill_path.as_deref(), Some("skills/my-skill"));
    assert!(lock.entry_for_folder("other").is_none());

    lock.skills.insert("my-skill".into(), entry("exact"));
    assert_eq!(lock.entry_for_folder("my-skill").unwrap().0, "my-skill");
    assert_eq!(lock.keys_for_folder("my-skill"), ["my-skill", "My Skill"]);

    lock.remove("my-skill");
    assert!(lock.skills.is_empty(), "every key for the folder goes");
}

#[test]
fn upsert_folds_other_keys_for_the_folder_and_keeps_their_extra_fields() {
    let mut lock = SkillLock::default();
    let mut vercel = entry("my-skill");
    vercel.extra.insert("pluginName".into(), Value::from("p"));
    vercel.extra.insert("futureField".into(), Value::from(1));
    vercel.installed_at = "2026-01-01T00:00:00Z".into();
    lock.skills.insert("My Skill".into(), vercel);

    let mut fresh = entry("my-skill");
    fresh.extra.insert("futureField".into(), Value::from(2));
    fresh.installed_at = "2026-10-06T00:00:00Z".into();
    lock.upsert("my-skill", fresh);

    assert_eq!(lock.skills.keys().collect::<Vec<_>>(), ["my-skill"]);
    let merged = &lock.skills["my-skill"];
    assert_eq!(merged.extra["pluginName"], "p");
    assert_eq!(merged.extra["futureField"], 2, "the new entry wins");
    assert_eq!(merged.installed_at, "2026-01-01T00:00:00Z");
}

#[test]
fn vercel_entries_survive_a_skillstar_reinstall() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    std::fs::create_dir_all(lock_path().parent().unwrap()).unwrap();
    std::fs::write(
        lock_path(),
        r#"{"version":3,"skills":{"My Skill":{"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","skillPath":"skills/my-skill","skillFolderHash":"abc","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","pluginName":"plug"}}}"#,
    )
    .unwrap();
    let checkout = tempfile::tempdir().unwrap();
    let skill = checkout.path().join("skills/my-skill");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: My Skill\ndescription: Round trip.\n---\n",
    )
    .unwrap();
    let spec = crate::source_resolver::Source {
        repo_url: "https://github.com/o/r.git".into(),
        short: "o/r".into(),
        git_ref: Some("main".into()),
        subpath: None,
        skill_filter: None,
    };

    let installed = crate::installer::install_units(
        checkout.path(),
        &spec,
        &[crate::installer::InstallUnit {
            id: "My Skill".into(),
            folder_path: "skills/my-skill".into(),
        }],
    )
    .unwrap();
    assert_eq!(installed, ["my-skill"]);

    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(lock_path()).unwrap()).unwrap();
    let skills = written["skills"].as_object().unwrap();
    assert_eq!(skills.keys().collect::<Vec<_>>(), ["my-skill"]);
    let entry = &skills["my-skill"];
    assert_eq!(entry["ref"], "main");
    assert!(entry.get("gitRef").is_none());
    assert_eq!(entry["pluginName"], "plug");
    assert_eq!(entry["installedAt"], "2026-01-01T00:00:00Z");
    let (key, _) = load()
        .entry_for_folder("my-skill")
        .map(|(k, e)| (k.to_string(), e.clone()))
        .unwrap();
    assert_eq!(key, "my-skill");
}

#[test]
fn a_refused_lock_can_be_reset_explicitly_after_a_backup() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    std::fs::create_dir_all(lock_path().parent().unwrap()).unwrap();
    std::fs::write(lock_path(), "{ torn").unwrap();
    let error = ensure_writable().unwrap_err().to_string();
    assert!(error.contains("Nothing was changed"), "{error}");
    assert!(error.contains("reset the lock"), "{error}");
    assert_eq!(std::fs::read_to_string(lock_path()).unwrap(), "{ torn");

    let backup = reset_after_backup().unwrap().unwrap();
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{ torn");
    ensure_writable().unwrap();
    assert!(load().skills.is_empty());
}

#[test]
fn uninstall_checks_the_lock_before_removing_deployments() {
    let sandbox = crate::test_sandbox::Sandbox::new();
    let canonical = ss_core::infra::paths::agents_skill_dir("foo");
    std::fs::create_dir_all(&canonical).unwrap();
    std::fs::write(canonical.join("SKILL.md"), "keep").unwrap();
    let agent = sandbox.home().join(".claude/skills/foo");
    std::fs::create_dir_all(agent.parent().unwrap()).unwrap();
    ss_core::infra::fs_ops::create_symlink(&canonical, &agent).unwrap();
    std::fs::create_dir_all(lock_path().parent().unwrap()).unwrap();
    std::fs::write(lock_path(), "{ torn").unwrap();

    let error = crate::skill_install::uninstall_skill("foo").unwrap_err();
    assert!(error.contains("Nothing was changed"), "{error}");
    assert!(error.contains("reset_after_backup"), "{error}");
    assert!(
        canonical.join("SKILL.md").is_file(),
        "canonical skill must stay when the lock cannot be rewritten"
    );
    assert!(
        ss_core::infra::fs_ops::is_link(&agent),
        "agent deployment must stay when the lock cannot be rewritten"
    );
}
