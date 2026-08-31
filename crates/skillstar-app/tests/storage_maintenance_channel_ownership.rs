//! Global storage maintenance must respect shared-channel ownership.
//!
//! The mutation gate is per-Skill, but `storage_maintenance` deletes whole
//! directories — so nothing in the type system proves these paths keep the
//! subscription store aligned with the hub. These assertions are that proof:
//! the destructive resets prune what they delete, and routine housekeeping
//! does not touch channel-owned Skills at all.

use skillstar_channels::shared_channels::{
    CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION, CHANNEL_SUBSCRIPTION_STORE_VERSION,
    ChannelAutoUpdateState, ChannelReleaseTarget, ChannelSkillProvenance, ChannelSubscribedSkill,
    ChannelSubscription, ChannelSubscriptionRegistry, ChannelSubscriptionRemoteState,
    ChannelSubscriptionStore, DiskChannelSubscriptionRegistry, DiskSharedChannelRegistry,
};
use skillstar_core::infra::paths;
use skillstar_skills::lockfile;
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
        let previous = assignments
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                std::fs::create_dir_all(value).unwrap();
                unsafe { std::env::set_var(key, value) };
                (*key, previous)
            })
            .collect();
        skillstar_channels::policy::install_global_policy();
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
        release_content_hash_version: skillstar_skills::content::SNAPSHOT_HASH_VERSION,
        baseline_hash: hash,
        baseline_hash_version: skillstar_skills::content::SNAPSHOT_HASH_VERSION,
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
    let lock = lockfile::Lockfile {
        skills: entries
            .iter()
            .map(|(name, git_url)| lockfile::LockEntry {
                name: (*name).to_string(),
                git_url: (*git_url).to_string(),
                git_ref: Some("a".repeat(40)),
                tree_hash: "0".repeat(40),
                content_hash: None,
                content_hash_version: None,
                installed_at: "2026-08-05T00:00:00Z".into(),
                source_folder: None,
            })
            .collect(),
        ..Default::default()
    };
    lock.save(&lockfile::lockfile_path()).unwrap();
}

fn lock_entry_names() -> Vec<String> {
    lockfile::Lockfile::load(&lockfile::lockfile_path())
        .unwrap()
        .skills
        .into_iter()
        .map(|entry| entry.name)
        .collect()
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
    skillstar_skills::skill_mutation::skill_is_channel_managed(name).unwrap()
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

    let removed = skillstar_app::storage_maintenance::force_delete_installed_skills()
        .await
        .unwrap();

    assert_eq!(removed, 2);
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

    skillstar_app::storage_maintenance::force_delete_repo_caches()
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

    let fixed = skillstar_app::storage_maintenance::clean_broken_skills()
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
    let preference = paths::config_dir().join("model_providers.json");
    std::fs::write(&preference, "{}").unwrap();

    let removed = skillstar_app::storage_maintenance::force_delete_app_config()
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
    std::fs::write(paths::config_dir().join("mcp_servers.json"), "{}").unwrap();

    let removed = skillstar_app::storage_maintenance::force_delete_app_config()
        .await
        .unwrap();

    assert_eq!(removed, 1);
    assert!(DiskSharedChannelRegistry::path().exists());
}
