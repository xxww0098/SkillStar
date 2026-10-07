//! Global storage maintenance must respect shared-channel ownership.
//!
//! The mutation gate is per-Skill, but `storage_maintenance` deletes whole
//! directories — so nothing in the type system proves these paths keep the
//! subscription store aligned with the hub. These assertions are that proof:
//! the destructive resets prune what they delete, and routine housekeeping
//! does not touch channel-owned Skills at all.

use ss_core::infra::paths;
use ss_skills::channels::shared_channels::{
    CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION, CHANNEL_SUBSCRIPTION_STORE_VERSION,
    ChannelAutoUpdateState, ChannelReleaseTarget, ChannelSkillProvenance, ChannelSubscribedSkill,
    ChannelSubscription, ChannelSubscriptionRegistry, ChannelSubscriptionRemoteState,
    ChannelSubscriptionStore, DiskChannelSubscriptionRegistry, DiskSharedChannelRegistry,
};
use ss_skills::skill_lock;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

const CHANNEL_REPOSITORY_ID: u64 = 42;
const CHANNEL_URL: &str = "https://github.com/acme/channel.git";

/// Serializes the process-global env vars these tests sandbox.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Redirects every storage root at a temp dir — including `HOME`, because the
/// hub reset unlinks Skills from agent profile directories under the home dir.
struct Sandbox {
    _guard: MutexGuard<'static, ()>,
    _temp: tempfile::TempDir,
    previous: Vec<(&'static str, Option<OsString>)>,
}

impl Sandbox {
    fn new() -> Self {
        let guard = env_lock();
        let temp = tempfile::tempdir().unwrap();
        let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let assignments = [
            ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
            ("SKILLSTAR_HUB_DIR", temp.path().join("hub")),
            (home_var, temp.path().join("home")),
        ];
        let mut previous: Vec<(&'static str, Option<OsString>)> = assignments
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                std::fs::create_dir_all(value).unwrap();
                unsafe { std::env::set_var(key, value) };
                (*key, previous)
            })
            .collect();
        let home = temp.path().join("home");
        let previous_sync = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
        unsafe { std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", &home) };
        previous.push(("SKILLSTAR_TOOL_SYNC_HOME", previous_sync));
        for variable in [
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
            "AUTOHAND_HOME",
            "DSH_HOME",
            "GROK_HOME",
            "HERMES_HOME",
            "VIBE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
        ] {
            previous.push((variable, std::env::var_os(variable)));
            unsafe { std::env::remove_var(variable) };
        }
        std::fs::create_dir_all(paths::hub_skills_dir()).unwrap();
        std::fs::create_dir_all(paths::config_dir()).unwrap();
        Self {
            _guard: guard,
            _temp: temp,
            previous,
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for (key, previous) in self.previous.drain(..) {
            match previous {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
    }
}

fn subscribed_skill(id: &str, digest: char) -> ChannelSubscribedSkill {
    let hash = format!("sha256:{}", digest.to_string().repeat(64));
    ChannelSubscribedSkill {
        id: id.into(),
        content_root: format!("skills/{id}"),
        release_content_hash: hash.clone(),
        release_content_hash_version: ss_skills::content::SNAPSHOT_HASH_VERSION,
        baseline_hash: hash,
        baseline_hash_version: ss_skills::content::SNAPSHOT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: CHANNEL_REPOSITORY_ID,
            repository_url: CHANNEL_URL.into(),
            git_ref: "a".repeat(40),
            source_folder: format!("skills/{id}"),
        },
    }
}

/// Writes a subscription that tracks `channel_skill_ids`.
fn save_subscription(channel_skill_ids: &[&str]) {
    DiskChannelSubscriptionRegistry
        .save(&ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![ChannelSubscription {
                descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
                repository_id: CHANNEL_REPOSITORY_ID,
                organization_id: 7,
                repository_url_aliases: Vec::new(),
                target: ChannelReleaseTarget {
                    revision: 1,
                    tag_name: "channel-v000001".into(),
                    commit_sha: "a".repeat(40),
                },
                skills: channel_skill_ids
                    .iter()
                    .map(|id| subscribed_skill(id, 'b'))
                    .collect(),
                known_skill_ids: channel_skill_ids
                    .iter()
                    .map(|id| (*id).to_string())
                    .collect(),
                pins: Vec::new(),
                last_update: None,
                auto_update: ChannelAutoUpdateState::default(),
                remote_state: ChannelSubscriptionRemoteState::default(),
                created_at: "2026-08-05T00:00:00Z".into(),
                updated_at: "2026-08-05T00:00:00Z".into(),
            }],
        })
        .unwrap();
}

fn save_lock_entries(entries: &[(&str, &str)]) {
    let mut lock = skill_lock::SkillLock::default();
    for (name, git_url) in entries {
        lock.upsert(
            name,
            skill_lock::SkillLockEntry {
                source: name.to_string(),
                source_type: skill_lock::SourceType::Github,
                source_url: (*git_url).to_string(),
                git_ref: Some("a".repeat(40)),
                skill_path: None,
                skill_folder_hash: None,
                installed_at: "2026-08-05T00:00:00Z".into(),
                updated_at: "2026-08-05T00:00:00Z".into(),
                extra: Default::default(),
            },
        );
    }
    lock.save(&skill_lock::lock_path()).unwrap();
}

fn lock_entry_names() -> Vec<String> {
    skill_lock::load().skills.keys().cloned().collect()
}

fn tracked_skill_ids() -> Vec<String> {
    DiskChannelSubscriptionRegistry
        .list_views()
        .unwrap()
        .into_iter()
        .flat_map(|view| view.selected_skill_ids)
        .collect()
}

fn is_channel_managed(name: &str) -> bool {
    ss_skills::skill_mutation::skill_is_channel_managed(name).unwrap()
}

fn install_hub_directory(name: &str) -> PathBuf {
    let path = paths::hub_skills_dir().join(name);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("SKILL.md"), "# channel-owned\n").unwrap();
    path
}

#[cfg(unix)]
fn link(target: &Path, link_path: &Path) {
    std::os::unix::fs::symlink(target, link_path).unwrap();
}

/// An explicit hub reset may delete channel-owned Skills, but it must tell the
/// subscription store — otherwise the store keeps claiming names with nothing
/// behind them and they stay permanently immutable.
#[tokio::test]
async fn force_delete_installed_skills_prunes_the_subscription() {
    let _sandbox = Sandbox::new();
    install_hub_directory("writer");
    install_hub_directory("reader");
    save_lock_entries(&[("writer", CHANNEL_URL), ("reader", CHANNEL_URL)]);
    save_subscription(&["writer", "reader"]);
    assert!(is_channel_managed("writer"));

    let removed = ss_app::storage_maintenance::force_delete_installed_skills()
        .await
        .unwrap();

    let mut removed_names = removed.removed.clone();
    removed_names.sort();
    assert_eq!(
        removed_names,
        vec!["reader".to_string(), "writer".to_string()]
    );
    assert!(removed.failed.is_empty(), "{removed:?}");
    assert!(
        tracked_skill_ids().is_empty(),
        "the store must stop tracking Skills the reset deleted"
    );
    assert!(!is_channel_managed("writer"));
    assert!(lock_entry_names().is_empty());
    assert_eq!(
        std::fs::read_dir(paths::hub_skills_dir()).unwrap().count(),
        0
    );
}

/// The cache reset drops hub symlinks that point into the repo cache — which
/// is exactly where channel Skills are checked out.
#[cfg(unix)]
#[tokio::test]
async fn force_delete_repo_caches_prunes_cache_backed_channel_skills() {
    let _sandbox = Sandbox::new();
    let checkout = paths::repos_cache_dir().join("acme_channel").join("skills");
    std::fs::create_dir_all(checkout.join("writer")).unwrap();
    std::fs::write(checkout.join("writer").join("SKILL.md"), "# owned\n").unwrap();
    link(
        &checkout.join("writer"),
        &paths::hub_skills_dir().join("writer"),
    );
    save_lock_entries(&[("writer", CHANNEL_URL)]);
    save_subscription(&["writer"]);

    ss_app::storage_maintenance::force_delete_repo_caches()
        .await
        .unwrap();

    assert!(tracked_skill_ids().is_empty());
    assert!(!is_channel_managed("writer"));
    assert!(lock_entry_names().is_empty());
    assert!(
        paths::hub_skills_dir()
            .join("writer")
            .symlink_metadata()
            .is_err()
    );
}

/// Routine housekeeping is not a reset. A channel Skill whose checkout
/// vanished is repaired through the channel controls, so cleaning must leave
/// both its hub entry and its lock entry in place.
#[cfg(unix)]
#[tokio::test]
async fn clean_broken_skills_skips_channel_owned_skills() {
    let _sandbox = Sandbox::new();
    let missing = paths::repos_cache_dir().join("gone");
    link(&missing, &paths::hub_skills_dir().join("writer"));
    link(&missing, &paths::hub_skills_dir().join("stray"));
    save_lock_entries(&[("writer", CHANNEL_URL), ("stray", "https://example.com/x")]);
    save_subscription(&["writer"]);

    let fixed = ss_app::storage_maintenance::clean_broken_skills()
        .await
        .unwrap();

    // Two fixes for `stray` — its broken link and its now-orphaned lock entry
    // — and none for `writer`.
    assert_eq!(fixed, 2, "only the unowned Skill is repaired");
    assert!(
        paths::hub_skills_dir()
            .join("writer")
            .symlink_metadata()
            .is_ok(),
        "a channel-owned broken link is left for the channel controls"
    );
    assert!(
        paths::hub_skills_dir()
            .join("stray")
            .symlink_metadata()
            .is_err()
    );
    assert_eq!(lock_entry_names(), vec!["writer".to_string()]);
    assert_eq!(tracked_skill_ids(), vec!["writer".to_string()]);
    assert!(is_channel_managed("writer"));
}

#[cfg(unix)]
#[tokio::test]
async fn repair_keeps_conflicting_installations_out_of_broken_link_cleanup() {
    let _sandbox = Sandbox::new();
    let agent = paths::home_dir().join("repair-fixture/skills");
    ss_skills::agents::add_custom_profile(ss_skills::agents::CustomProfileDef {
        id: "repair-fixture".into(),
        display_name: "Repair fixture".into(),
        global_skills_dir: agent.to_string_lossy().into_owned(),
        project_skills_rel: ".repair-fixture/skills".into(),
        icon_data_uri: None,
    })
    .unwrap();
    ss_skills::agents::toggle_profile("repair-fixture").unwrap();
    let local = paths::local_skills_dir().join("alpha");
    let external = agent.join("alpha");
    for (path, text) in [(&local, "user edits"), (&external, "external update")] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            "---\nname: alpha\ndescription: Migration fixture.\n---\n",
        )
        .unwrap();
        std::fs::write(path.join("script.sh"), text).unwrap();
    }
    let hub = paths::hub_skills_dir().join("alpha");
    link(&paths::home_dir().join("missing"), &hub);
    let report = ss_app::storage_maintenance::repair_skills().await.unwrap();
    assert_eq!(report.repaired, 0, "{report:?}");
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.name == "alpha" && issue.reason.contains("Content conflict"))
    );
    assert!(
        hub.is_symlink(),
        "conflict entry must not reach broken-link cleanup"
    );
    assert!(!external.is_symlink());
    assert_eq!(
        std::fs::read_to_string(local.join("script.sh")).unwrap(),
        "user edits"
    );
    assert_eq!(
        std::fs::read_to_string(external.join("script.sh")).unwrap(),
        "external update"
    );
}

/// Intake may copy an Agent-installed skill into local storage. It must not
/// turn an external checkout (here, a hub link to `~/installer/alpha`) into a
/// symlink, and it must not touch a channel-owned skill.
#[cfg(unix)]
#[tokio::test]
async fn repair_migrates_external_content_but_preserves_channel_ownership() {
    let _sandbox = Sandbox::new();
    let origin = paths::home_dir().join("installer/alpha");
    std::fs::create_dir_all(&origin).unwrap();
    std::fs::write(
        origin.join("SKILL.md"),
        "---\nname: alpha\ndescription: Migration fixture.\n---\n",
    )
    .unwrap();
    link(&origin, &paths::hub_skills_dir().join("alpha"));
    link(
        &paths::home_dir().join("missing"),
        &paths::hub_skills_dir().join("writer"),
    );
    save_lock_entries(&[("writer", CHANNEL_URL)]);
    save_subscription(&["writer"]);
    let external = paths::home_dir().join(".claude/skills/beta");
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(
        external.join("SKILL.md"),
        "---\nname: beta\ndescription: Migration fixture.\n---\n",
    )
    .unwrap();
    std::fs::write(external.join("notes.txt"), b"keep me").unwrap();

    let report = ss_app::storage_maintenance::repair_skills().await.unwrap();
    assert!(
        report.issues.iter().all(|issue| issue.name != "writer"),
        "{report:?}"
    );
    assert_eq!(report.repaired, 1, "{report:?}");
    assert!(
        !origin.is_symlink(),
        "a checkout linked from the hub is not moved"
    );
    assert_eq!(
        paths::hub_skills_dir()
            .join("alpha")
            .canonicalize()
            .unwrap(),
        origin.canonicalize().unwrap()
    );
    let beta_link = std::fs::read_link(&external).unwrap();
    assert!(beta_link.is_relative(), "{beta_link:?}");
    assert_eq!(
        std::fs::read(external.join("notes.txt")).unwrap(),
        b"keep me"
    );
    assert!(!paths::local_skills_dir().join("beta").is_symlink());
    assert_eq!(
        std::fs::read(paths::local_skills_dir().join("beta/notes.txt")).unwrap(),
        b"keep me"
    );
    assert!(paths::hub_skills_dir().join("writer").is_symlink());
    assert_eq!(tracked_skill_ids(), vec!["writer".to_string()]);
    let names = lock_entry_names();
    assert!(names.contains(&"writer".to_string()), "{names:?}");
    assert!(names.contains(&"beta".to_string()), "{names:?}");
    assert!(!names.iter().any(|name| name == "alpha"), "{names:?}");
    assert_eq!(
        ss_app::storage_maintenance::repair_skills()
            .await
            .unwrap()
            .repaired,
        0
    );
}

/// A config reset must not destroy the record of what is installed. Deleting
/// the subscription store while its Skills are still in the hub would strand
/// them: no longer recognised as channel-owned, so the ordinary update path
/// would start fetching a private repository anonymously.
#[tokio::test]
async fn force_delete_app_config_preserves_channel_provenance() {
    let _sandbox = Sandbox::new();
    install_hub_directory("writer");
    save_lock_entries(&[("writer", CHANNEL_URL)]);
    save_subscription(&["writer"]);
    let preference = paths::ai_config_path();
    std::fs::write(&preference, "{}").unwrap();

    let removed = ss_app::storage_maintenance::force_delete_app_config()
        .await
        .unwrap();

    assert_eq!(
        removed, 1,
        "only the preference file is a config reset target"
    );
    assert!(!preference.exists());
    assert!(DiskChannelSubscriptionRegistry::path().exists());
    assert_eq!(tracked_skill_ids(), vec!["writer".to_string()]);
    assert!(
        is_channel_managed("writer"),
        "the Skill is still installed, so it must stay recognised as owned"
    );
}

/// The registry file is preserved for the same reason as the subscription
/// store — it is the other half of the channel-ownership record.
#[tokio::test]
async fn force_delete_app_config_preserves_the_channel_registry_file() {
    let _sandbox = Sandbox::new();
    std::fs::write(DiskSharedChannelRegistry::path(), "{}").unwrap();
    std::fs::write(paths::proxy_config_path(), "{}").unwrap();

    let removed = ss_app::storage_maintenance::force_delete_app_config()
        .await
        .unwrap();

    assert_eq!(removed, 1);
    assert!(DiskSharedChannelRegistry::path().exists());
}

/// The confirm list is the same set the reset deletes. A `github` lock entry
/// is included; an `unknown` source and a
/// folder with no lock entry stay off the list.
#[tokio::test]
async fn force_delete_preview_lists_npx_shaped_lock_entries() {
    let _sandbox = Sandbox::new();
    install_hub_directory("npx-writer");
    install_hub_directory("hand-placed");
    save_lock_entries(&[("npx-writer", "https://github.com/acme/npx-writer.git")]);
    let mut lock = skill_lock::load();
    lock.upsert(
        "mystery",
        skill_lock::SkillLockEntry {
            source: "mystery".into(),
            source_type: skill_lock::SourceType::Unknown,
            source_url: String::new(),
            git_ref: None,
            skill_path: None,
            skill_folder_hash: None,
            installed_at: "2026-08-05T00:00:00Z".into(),
            updated_at: "2026-08-05T00:00:00Z".into(),
            extra: Default::default(),
        },
    );
    lock.save(&skill_lock::lock_path()).unwrap();

    let names = ss_app::storage_maintenance::preview_force_delete_installed_skills()
        .await
        .unwrap();

    assert_eq!(names, vec!["npx-writer".to_string()]);
    let report = ss_app::storage_maintenance::force_delete_installed_skills()
        .await
        .unwrap();
    assert_eq!(report.removed, vec!["npx-writer".to_string()]);
    assert!(report.kept.contains(&"hand-placed".to_string()));
    assert!(lock_entry_names().contains(&"mystery".to_string()));
    assert!(!lock_entry_names().contains(&"npx-writer".to_string()));
}

/// "Delete installed Skills" uninstalls what SkillStar installed. A folder in
/// the canonical root with no lock entry was put there by hand or by another
/// tool, and a real directory in an Agent dir belongs to the user.
#[tokio::test]
async fn force_delete_installed_skills_only_removes_what_skillstar_installed() {
    let _sandbox = Sandbox::new();
    install_hub_directory("tracked");
    install_hub_directory("hand-placed");
    save_lock_entries(&[("tracked", "https://github.com/acme/tracked.git")]);
    let agent_dir = paths::home_dir().join(".claude/skills");
    let user_copy = agent_dir.join("tracked");
    std::fs::create_dir_all(&user_copy).unwrap();
    std::fs::write(user_copy.join("SKILL.md"), "# my own edits\n").unwrap();

    let report = ss_app::storage_maintenance::force_delete_installed_skills()
        .await
        .unwrap();

    assert_eq!(report.removed, vec!["tracked".to_string()]);
    assert_eq!(report.kept, vec!["hand-placed".to_string()]);
    assert!(paths::hub_skills_dir().join("hand-placed").exists());
    assert!(!paths::hub_skills_dir().join("tracked").exists());
    assert!(
        user_copy.join("SKILL.md").exists(),
        "user folder must survive"
    );
}

/// A config reset clears preferences and rebuildable state, never the
/// records nothing can rebuild.
#[tokio::test]
async fn force_delete_app_config_keeps_unrebuildable_records() {
    let _sandbox = Sandbox::new();
    std::fs::create_dir_all(paths::state_dir()).unwrap();
    let kept = [
        paths::groups_path(),
        paths::projects_manifest_path(),
        paths::team_store_path(),
        paths::profiles_config_path(),
        paths::ssh_known_hosts_path(),
        paths::config_dir().join("unknown_future_file.json"),
    ];
    for path in &kept {
        std::fs::write(path, "{}").unwrap();
    }
    std::fs::write(paths::repo_history_path(), "[]").unwrap();

    let removed = ss_app::storage_maintenance::force_delete_app_config()
        .await
        .unwrap();

    assert_eq!(removed, 1);
    assert!(!paths::repo_history_path().exists());
    for path in &kept {
        assert!(
            path.exists(),
            "{} must survive a config reset",
            path.display()
        );
    }
}
