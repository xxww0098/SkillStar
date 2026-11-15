//! Settings storage facade — the storage / skill-repair / about slice owns
//! this file (`SettingsStorage` in TSX; GPUI sources
//! `crates/ss-gpui/src/settings/storage/` (`mod.rs` + `rows.rs` +
//! `parts.rs`), `settings/skill_repair.rs` and `settings/about.rs`, TSX
//! sections `settings-storage` and `settings-about`, slots
//! `src/pages/settings/sections/storage.tsx` and `sections/about.tsx`).
//! Pre-registered empty by the wave-7 wiring pass so the slice never
//! touches a shared registry — it only adds methods inside this file. Do
//! not edit `settings.rs` (the base seam) or the `lib.rs` /
//! `facades/mod.rs` registration.
//!
//! Domain seams this slice wires:
//! - `ss_app::storage_maintenance` (joined this wave, see the manifest):
//!   `get_storage_overview`, `clear_all_caches`,
//!   `preview_force_delete_installed_skills` /
//!   `force_delete_installed_skills` (the two-step hub reset — the delete
//!   only takes entries SkillStar can prove it owns: install-lock entries
//!   with a recognizable source plus hub links to local skills; everything
//!   else is reported and kept), `force_delete_repo_caches`,
//!   `preview_skill_repair` / `apply_skill_repair` (ownership-proven
//!   repair only) and `preview_agent_intake` / `apply_agent_intake`.
//! - `ss_skills::git::gh_manager::{check_git_status, is_gh_installed}` and
//!   `ss_core::infra::paths::data_root` — the About card's environment
//!   snapshot (about.rs parity; the version chip's source lives below).
//! - `ss_core::infra::release_check::{run_check, last_record}` — the
//!   manual "check for updates" button. `run_check` hits the real network
//!   (GitHub releases API): unit tests must not call it — the wire is
//!   asserted over a hand-built record instead.
//!
//! Test discipline (wave-1 lesson): any test touching the
//! `SKILLSTAR_DATA_DIR` sandbox borrows the shared `TEST_ENV_LOCK` from
//! `facades/mod.rs` for the whole sandbox lifetime — never a module-private
//! lock, never `--test-threads=1`.

use nsh_core::prelude::*;
use nsh_core::JsonValue;
use nsh_core::element::__private::JsonMap;

use ss_app::storage_maintenance;

/// The storage + skill-repair + about settings seam (`SettingsStorage` in
/// TSX).
#[module]
pub struct SettingsStorage;

/// The product version the GPUI binary takes from the repo-root
/// `[package]` (`product_version()` in ss-gpui/src/lib.rs). This crate has
/// its own (meaningless) package version, so the root manifest is read at
/// compile time instead — by `build.rs`, which owns that job (it parses the
/// `[package]` table precisely) and exposes the result as `ROOT_VERSION`
/// for exactly this seam (about pane, release check).
fn product_version() -> &'static str {
    crate::ROOT_VERSION
}

/// `SkillRepairPreview` / `SkillRepairApplyReport` / `AgentIntakePreview`
/// are plain structs (no Serialize), so their wire lives here — the slot's
/// "wire serialization in this file" rule. Field names are the ones the
/// TSX DTO mirrors.
fn repair_preview_wire(preview: &storage_maintenance::SkillRepairPreview) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert(
        "issues".to_owned(),
        JsonValue::from(preview.issues as u64),
    );
    map.insert(
        "steps".to_owned(),
        JsonValue::Array(preview.steps.iter().map(|s| JsonValue::String(s.clone())).collect()),
    );
    map.insert(
        "untouched".to_owned(),
        JsonValue::Array(
            preview
                .untouched
                .iter()
                .map(|s| JsonValue::String(s.clone()))
                .collect(),
        ),
    );
    JsonValue::Object(map)
}

fn apply_report_wire(applied: usize, failed: &[String]) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert("applied".to_owned(), JsonValue::from(applied as u64));
    map.insert(
        "failed".to_owned(),
        JsonValue::Array(failed.iter().map(|s| JsonValue::String(s.clone())).collect()),
    );
    JsonValue::Object(map)
}

fn intake_preview_wire(preview: &storage_maintenance::AgentIntakePreview) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert(
        "steps".to_owned(),
        JsonValue::Array(preview.steps.iter().map(|s| JsonValue::String(s.clone())).collect()),
    );
    map.insert(
        "reported".to_owned(),
        JsonValue::Array(
            preview
                .reported
                .iter()
                .map(|s| JsonValue::String(s.clone()))
                .collect(),
        ),
    );
    map.insert(
        "would_apply".to_owned(),
        JsonValue::from(preview.would_apply as u64),
    );
    JsonValue::Object(map)
}

#[module]
impl SettingsStorage {
    /// Aggregated storage usage (`get_storage_overview` parity,
    /// storage/mod.rs `load_storage`): per-root bytes/counts, health
    /// findings and agent intake rows, field-for-field the serde shape of
    /// `StorageOverview`.
    pub async fn storage_overview() -> Result<JsonValue, String> {
        let overview = storage_maintenance::get_storage_overview()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(serde_json::to_value(&overview).unwrap_or(JsonValue::Null))
    }

    /// Routine cache clean (`clear_all_caches` parity, storage/mod.rs
    /// `clean_caches`). Resolves `{repos_removed, history_cleared}`.
    pub async fn clear_all_caches() -> Result<JsonValue, String> {
        let cleaned = storage_maintenance::clear_all_caches()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(serde_json::to_value(&cleaned).unwrap_or(JsonValue::Null))
    }

    /// The names a hub reset would uninstall
    /// (`preview_force_delete_installed_skills` parity, storage/mod.rs
    /// `preview_hub_delete`) — the same selection the delete takes:
    /// install-lock entries whose source SkillStar recognizes plus hub
    /// links of local skills. Read-only. Drives the second-click confirm
    /// copy; an empty list means nothing to delete.
    pub async fn preview_hub_delete() -> Result<JsonValue, String> {
        let names = storage_maintenance::preview_force_delete_installed_skills()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(JsonValue::Array(names.into_iter().map(JsonValue::String).collect()))
    }

    /// The two-step hub reset's second click
    /// (`force_delete_installed_skills` parity, storage/mod.rs
    /// `storage_action`). Resolves `{removed[], failed[], kept[]}` —
    /// folders SkillStar has no provenance for land in `kept`, still on
    /// disk.
    pub async fn force_delete_hub() -> Result<JsonValue, String> {
        let report = storage_maintenance::force_delete_installed_skills()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(serde_json::to_value(&report).unwrap_or(JsonValue::Null))
    }

    /// Force-delete every repo cache (`force_delete_repo_caches` parity).
    /// Resolves the removed count.
    pub async fn force_delete_caches() -> Result<u64, String> {
        let count = storage_maintenance::force_delete_repo_caches()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(count as u64)
    }

    /// Read-only repair preview (`preview_skill_repair` parity,
    /// skill_repair.rs `preview_skill_repair`): issues found, the steps
    /// repair can prove it owns, and the findings left untouched. Resolves
    /// `{issues, steps[], untouched[]}`.
    pub async fn preview_repair() -> Result<JsonValue, String> {
        let preview = storage_maintenance::preview_skill_repair()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(repair_preview_wire(&preview))
    }

    /// Apply the repair plan (`apply_skill_repair` parity, skill_repair.rs
    /// `repair_skills`). Never adopts an external Agent folder. Resolves
    /// `{applied, failed[]}`.
    pub async fn apply_repair() -> Result<JsonValue, String> {
        let report = storage_maintenance::apply_skill_repair()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(apply_report_wire(report.applied, &report.failed))
    }

    /// Read-only agent-intake preview (`preview_agent_intake` parity,
    /// skill_repair.rs `preview_agent_intake`). Resolves
    /// `{steps[], reported[], would_apply}`.
    pub async fn preview_intake() -> Result<JsonValue, String> {
        let preview = storage_maintenance::preview_agent_intake()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(intake_preview_wire(&preview))
    }

    /// Adopt agent-installed skills (`apply_agent_intake` parity,
    /// skill_repair.rs `adopt_agent_skills`); conflicts are reported, never
    /// overwritten. Resolves `{applied, failed[]}`.
    pub async fn apply_intake() -> Result<JsonValue, String> {
        let report = storage_maintenance::apply_agent_intake()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(apply_report_wire(report.applied, &report.failed))
    }

    /// The About card's environment snapshot (about.rs constructor parity):
    /// `{version, data_root, gh_installed, git_status, release_check}`.
    /// `git_status` is the serde shape of `GitStatus` (tag = "status"),
    /// `release_check` the last persisted `ReleaseCheckRecord` or null.
    /// Read-only; `check_git_status` shells out to `git --version`.
    pub fn about_snapshot() -> JsonValue {
        let mut map = JsonMap::new();
        map.insert(
            "version".to_owned(),
            JsonValue::String(product_version().to_owned()),
        );
        map.insert(
            "data_root".to_owned(),
            JsonValue::String(
                ss_core::infra::paths::data_root().display().to_string(),
            ),
        );
        map.insert(
            "gh_installed".to_owned(),
            JsonValue::Bool(ss_skills::git::gh_manager::is_gh_installed()),
        );
        map.insert(
            "git_status".to_owned(),
            serde_json::to_value(ss_skills::git::gh_manager::check_git_status())
                .unwrap_or(JsonValue::Null),
        );
        map.insert(
            "release_check".to_owned(),
            ss_core::infra::release_check::last_record()
                .and_then(|record| serde_json::to_value(&record).ok())
                .unwrap_or(JsonValue::Null),
        );
        map.insert("gh_install_command".to_owned(), gh_install_command());
        JsonValue::Object(map)
    }

    /// The manual update check (about.rs `run_release_check` parity).
    /// Resolves the fresh `ReleaseCheckRecord` wire (`{last_checked_unix,
    /// current_version, outcome}`; `outcome` tagged `kind`:
    /// `up_to_date` / `no_published_release` / `available{tag,url}` /
    /// `failed{reason}`) and persists it for the next launch. Hits the
    /// real network — never call from unit tests.
    pub async fn run_release_check() -> Result<JsonValue, String> {
        let current = product_version();
        let outcome = ss_core::infra::release_check::run_check(current).await;
        let record = ss_core::infra::release_check::ReleaseCheckRecord {
            last_checked_unix: ss_core::infra::github_api_cooldown::now_unix(),
            current_version: current.to_owned(),
            outcome,
        };
        serde_json::to_value(&record).map_err(|err| format!("{err:#}"))
    }

    /// Reveal a path in the platform file manager (os_open::open_folder
    /// parity — the storage rows' folder buttons and the About data-path
    /// row): `explorer` on Windows, `/usr/bin/open` on macOS, `xdg-open`
    /// elsewhere. Fire-and-forget like the GPUI helper (a spawn failure is
    /// ignored, nothing rejects); browser URLs never land here — they go
    /// through the nsh `openUrl` primitive on the TSX side.
    pub fn reveal_path(path: String) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(&path).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("/usr/bin/open").arg(&path).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
        #[cfg(not(any(unix, windows)))]
        let _ = &path;
        Ok(())
    }

    /// List rolling backups in `data/skills/backups/`, newest first. Blocking pool.
    pub async fn list_backups() -> Result<JsonValue, String> {
        blocking(list_backups_job)
            .await
            .map_err(|join| format!("list-backups task failed: {join}"))?
    }

    /// Restore one backup snapshot into `data/skills/installed/`. Blocking pool.
    pub async fn restore_backup(id: String) -> Result<String, String> {
        blocking(move || restore_backup_job(&id))
            .await
            .map_err(|join| format!("restore-backup task failed: {join}"))?
    }

    /// Remove all rolling backups in `data/skills/backups/`. Blocking pool.
    pub async fn clear_backups() -> Result<JsonValue, String> {
        blocking(clear_backups_job)
            .await
            .map_err(|join| format!("clear-backups task failed: {join}"))?
    }
}

fn list_backups_job() -> Result<JsonValue, String> {
    let entries = ss_skills::backup::list_backups().map_err(|err| format!("{err:#}"))?;
    Ok(serde_json::to_value(&entries).unwrap_or(JsonValue::Array(Vec::new())))
}

fn restore_backup_job(id: &str) -> Result<String, String> {
    ss_skills::backup::restore_backup(id).map_err(|err| format!("{err:#}"))
}

fn clear_backups_job() -> Result<JsonValue, String> {
    let (removed_count, freed_bytes) =
        ss_skills::backup::clear_all_backups().map_err(|err| format!("{err:#}"))?;
    let mut map = JsonMap::new();
    map.insert("removed_count".to_owned(), JsonValue::from(removed_count as u64));
    map.insert("freed_bytes".to_owned(), JsonValue::from(freed_bytes));
    Ok(JsonValue::Object(map))
}

/// about.rs `gh_install_command` parity: this platform's `gh` install
/// command, `null` where none is known. Compile-time per platform, exactly
/// like the GPUI cfg ladder.
fn gh_install_command() -> JsonValue {
    #[cfg(target_os = "macos")]
    {
        JsonValue::String("brew install gh".into())
    }
    #[cfg(target_os = "windows")]
    {
        JsonValue::String("winget install --id GitHub.cli".into())
    }
    #[cfg(target_os = "linux")]
    {
        JsonValue::String("sudo apt install gh".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        JsonValue::Null
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsStorage;
    use nsh_core::JsonValue;

    /// The GPUI suites' IsolatedDataDir pattern (facades/mod.rs), widened
    /// to the ss-app `MaintenanceEnv` set: `SKILLSTAR_DATA_DIR` /
    /// `SKILLSTAR_HUB_DIR` re-root the storage tree, and `HOME` re-roots
    /// the Agent directories the overview's intake rows scan (read-only,
    /// but a developer shell must not leak a real Agent config into the
    /// assertions). Everything runs under the shared TEST_ENV_LOCK so
    /// every facade suite that flips these variables serializes against
    /// each other (racing threads abort the binary). Never touches the
    /// real $HOME.
    struct StorageSandbox {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
        root: std::path::PathBuf,
    }

    impl StorageSandbox {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-settings-storage-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(root.join("home")).unwrap();
            let assignments: Vec<(&'static str, Option<std::path::PathBuf>)> = vec![
                ("SKILLSTAR_DATA_DIR", Some(root.join("data"))),
                ("SKILLSTAR_HUB_DIR", Some(root.join("hub"))),
                ("SKILLSTAR_TOOL_SYNC_HOME", Some(root.join("tool-home"))),
                ("HOME", Some(root.join("home"))),
                ("USERPROFILE", Some(root.join("home"))),
                ("XDG_CONFIG_HOME", Some(root.join("home/.config"))),
                ("XDG_STATE_HOME", None),
                ("CLAUDE_CONFIG_DIR", None),
                ("AUTOHAND_HOME", None),
                ("DSH_HOME", None),
                ("GROK_HOME", None),
                ("HERMES_HOME", None),
                ("VIBE_HOME", None),
            ];
            let previous = assignments
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in &assignments {
                    match value {
                        Some(path) => {
                            std::fs::create_dir_all(path).unwrap();
                            std::env::set_var(key, path)
                        }
                        None => std::env::remove_var(key),
                    }
                }
            }
            Self { _lock: lock, previous, root }
        }

        /// A hub folder with minimal skill content.
        fn hub_skill(&self, name: &str) -> std::path::PathBuf {
            let dir = ss_core::infra::paths::hub_skills_dir().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("SKILL.md"),
                "---\nname: demo\ndescription: test\n---\n# test\n",
            )
            .unwrap();
            dir
        }

        /// Claim a hub folder through the install lock — the ownership
        /// proof the reset needs rides the source type: `Github` is a source
        /// SkillStar can re-fetch, `Unknown` is another writer's entry the
        /// reset must not touch.
        fn claim(&self, name: &str, source_type: ss_skills::skill_lock::SourceType) {
            ss_skills::skill_lock::mutate(|lock| {
                lock.upsert(
                    name,
                    ss_skills::skill_lock::SkillLockEntry {
                        source: "owner/repo".into(),
                        source_type,
                        source_url: "https://github.com/owner/repo".into(),
                        git_ref: None,
                        skill_path: None,
                        skill_folder_hash: None,
                        installed_at: String::new(),
                        updated_at: String::new(),
                        extra: std::collections::BTreeMap::new(),
                    },
                );
            })
            .unwrap();
        }
    }

    impl Drop for StorageSandbox {
        fn drop(&mut self) {
            for (key, previous) in self.previous.drain(..) {
                // SAFETY: still holding the same shared lock.
                unsafe {
                    match previous {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn str_vec(value: &JsonValue, key: &str) -> Vec<String> {
        value
            .get(key)
            .and_then(|item| item.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The two-step hub reset's ownership rule, end to end through the
    /// seam: the preview lists only provably-owned names, the delete
    /// removes exactly those, and folders SkillStar cannot prove it owns
    /// survive — placed by hand, or locked by another writer with an
    /// unrecognized source — reported in `kept`, still on disk
    /// (deletion_targets.rs + storage_maintenance.rs
    /// `force_delete_installed_skills`).
    #[tokio::test]
    async fn hub_reset_takes_only_provable_owned_entries() {
        let sandbox = StorageSandbox::new();
        sandbox.claim("owned-skill", ss_skills::skill_lock::SourceType::Github);
        sandbox.hub_skill("owned-skill");
        let manual = sandbox.hub_skill("manual-skill");
        sandbox.claim("foreign-skill", ss_skills::skill_lock::SourceType::Unknown);
        let foreign = sandbox.hub_skill("foreign-skill");

        // Step 1 (preview): only the recognized-source entry is listed —
        // neither the hand-placed folder nor the unknown-source claim.
        let preview = SettingsStorage::preview_hub_delete().await.unwrap();
        let names = preview.as_array().unwrap().iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        assert_eq!(names, ["owned-skill"], "only the provably-owned name is listed");

        // Step 2 (delete): the owned entry goes, both unprovable ones stay.
        let report = SettingsStorage::force_delete_hub().await.unwrap();
        let removed = str_vec(&report, "removed");
        let kept = str_vec(&report, "kept");
        assert_eq!(removed, ["owned-skill"]);
        assert!(
            kept.contains(&"manual-skill".to_owned()),
            "a folder with no provenance is reported as kept: {kept:?}"
        );
        assert!(
            kept.contains(&"foreign-skill".to_owned()),
            "an unrecognized-source lock entry is reported as kept: {kept:?}"
        );
        assert!(
            manual.join("SKILL.md").is_file(),
            "the user's own folder is left on disk"
        );
        assert!(
            foreign.is_dir(),
            "another writer's locked folder is left on disk"
        );
        assert!(
            !ss_core::infra::paths::hub_skills_dir().join("owned-skill").exists(),
            "the owned entry is uninstalled"
        );

        // A second preview after the reset: nothing left to delete.
        let empty = SettingsStorage::preview_hub_delete().await.unwrap();
        assert_eq!(empty.as_array().map(Vec::len), Some(0));
        drop(sandbox);
    }

    /// An unreadable lock aborts both steps before anything is deleted
    /// (deletion_targets.rs: Missing/Outdated degrade to an empty lock,
    /// Corrupt is the same error the delete would abort on).
    #[tokio::test]
    async fn corrupt_lock_aborts_the_reset_before_any_delete() {
        let sandbox = StorageSandbox::new();
        let hub = ss_core::infra::paths::hub_skills_dir();
        let folder = sandbox.hub_skill("demo-skill");
        let lock_path = ss_skills::skill_lock::lock_path();
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        std::fs::write(&lock_path, "{ not json").unwrap();

        let preview = SettingsStorage::preview_hub_delete().await;
        assert!(preview.is_err(), "the preview refuses a corrupt lock");
        let deleted = SettingsStorage::force_delete_hub().await;
        assert!(deleted.is_err(), "the delete refuses a corrupt lock");
        assert!(
            folder.join("SKILL.md").is_file(),
            "nothing was deleted off a corrupt lock"
        );
        assert!(hub.join("demo-skill").is_dir());
        drop(sandbox);
    }

    /// The cache reset sweeps the cache directories and the hub links that
    /// point into them, and leaves every non-cache-backed skill alone
    /// (storage_maintenance.rs `force_delete_repo_caches`).
    #[cfg(unix)]
    #[tokio::test]
    async fn cache_reset_removes_cache_links_and_spares_plain_skills() {
        let sandbox = StorageSandbox::new();
        let hub = ss_core::infra::paths::hub_skills_dir();
        let cache = ss_core::infra::paths::repos_cache_dir();
        std::fs::create_dir_all(cache.join("repo-one")).unwrap();
        let plain = sandbox.hub_skill("plain-skill");
        std::os::unix::fs::symlink(cache.join("repo-one"), hub.join("cached-skill")).unwrap();

        let removed = SettingsStorage::force_delete_caches().await.unwrap();
        assert!(removed >= 1, "the cached repo is counted, got {removed}");
        assert!(
            !hub.join("cached-skill").exists(),
            "the cache-backed hub link is gone"
        );
        assert!(!cache.join("repo-one").exists(), "the cache dir is swept");
        assert!(
            plain.join("SKILL.md").is_file(),
            "a plain folder skill is untouched"
        );
        drop(sandbox);
    }

    /// The manual release check under an active rate-limit cooldown
    /// resolves before any socket opens: a failed outcome carrying this
    /// build's version, while the previously persisted record stays intact
    /// (release_check.rs `run_check` cooldown arm — the only no-network
    /// path a test may drive).
    #[tokio::test]
    async fn release_check_cooldown_fails_fast_and_keeps_the_last_record() {
        let sandbox = StorageSandbox::new();
        let state_path = ss_core::infra::paths::app_release_check_state_path();
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        std::fs::write(
            &state_path,
            r#"{"last_checked_unix":1,"current_version":"0.0.1","outcome":{"kind":"available","tag":"v9.9.9","url":"https://example.com/released"}}"#,
        )
        .unwrap();
        let now = ss_core::infra::github_api_cooldown::now_unix();
        ss_core::infra::github_api_cooldown::record(now + 3600);

        let record = SettingsStorage::run_release_check().await.unwrap();
        assert_eq!(
            record.get("outcome").and_then(|o| o.get("kind")).and_then(|k| k.as_str()),
            Some("failed")
        );
        assert_eq!(
            record.get("current_version").and_then(|v| v.as_str()),
            Some(crate::ROOT_VERSION)
        );
        let snapshot = SettingsStorage::about_snapshot();
        let persisted = snapshot.get("release_check").unwrap();
        assert_eq!(
            persisted
                .get("outcome")
                .and_then(|o| o.get("tag"))
                .and_then(|t| t.as_str()),
            Some("v9.9.9"),
            "the cooldown path keeps the previous record"
        );
        drop(sandbox);
    }

    /// The overview counts both folders (it is a usage report, not a
    /// deletion plan) and its paths stay inside the sandbox.
    #[tokio::test]
    async fn overview_counts_entries_without_claiming_them() {
        let sandbox = StorageSandbox::new();
        sandbox.claim("owned-skill", ss_skills::skill_lock::SourceType::Github);
        sandbox.hub_skill("owned-skill");
        sandbox.hub_skill("manual-skill");

        let overview = SettingsStorage::storage_overview().await.unwrap();
        assert_eq!(overview.get("hub_count").and_then(|v| v.as_u64()), Some(2));
        let hub_path = overview.get("hub_path").and_then(|v| v.as_str()).unwrap();
        assert!(hub_path.starts_with(sandbox.root.to_str().unwrap()));
        drop(sandbox);
    }

    /// `about_snapshot` wire: the product version from the root manifest,
    /// the sandbox-resolved data root, a tagged git status and a release
    /// check that is null before any check ran in the sandbox.
    #[test]
    fn about_snapshot_carries_version_paths_and_git_status() {
        let sandbox = StorageSandbox::new();
        let snapshot = SettingsStorage::about_snapshot();
        let version = snapshot.get("version").and_then(|v| v.as_str()).unwrap();
        assert_eq!(version, crate::ROOT_VERSION, "build.rs's root [package] version");
        assert_ne!(version, "0.1.0", "never the bridge crate's own version");
        let data_root = snapshot.get("data_root").and_then(|v| v.as_str()).unwrap();
        assert!(data_root.starts_with(sandbox.root.to_str().unwrap()));
        let git = snapshot.get("git_status").expect("git_status present");
        assert!(
            git.get("status").and_then(|v| v.as_str()).is_some(),
            "GitStatus serializes tagged by status"
        );
        assert!(
            snapshot.get("gh_installed").and_then(|v| v.as_bool()).is_some(),
            "gh_installed is a bool"
        );
        assert!(
            snapshot.get("release_check").map(|v| v.is_null()).unwrap_or(false),
            "no persisted release record inside the sandbox"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            snapshot.get("gh_install_command").and_then(|v| v.as_str()),
            Some("brew install gh")
        );
    }

    #[test]
    fn backups_jobs_list_restore_and_clear() {
        let sandbox = StorageSandbox::new();
        sandbox.hub_skill("demo-skill");
        let entry = ss_skills::backup::archive_skill(
            "demo-skill",
            &ss_core::infra::paths::hub_skills_dir().join("demo-skill"),
            ss_skills::backup::BackupReason::Uninstall,
        )
        .unwrap()
        .expect("backup entry created");
        std::fs::remove_dir_all(ss_core::infra::paths::hub_skills_dir().join("demo-skill")).unwrap();

        let listed = super::list_backups_job().unwrap();
        let arr = listed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"].as_str(), Some(entry.id.as_str()));
        assert_eq!(arr[0]["skillName"].as_str(), Some("demo-skill"));

        let restored_name = super::restore_backup_job(&entry.id).unwrap();
        assert_eq!(restored_name, "demo-skill");
        assert!(
            ss_core::infra::paths::hub_skills_dir()
                .join("demo-skill/SKILL.md")
                .is_file()
        );

        let cleared = super::clear_backups_job().unwrap();
        assert_eq!(cleared["removed_count"].as_u64(), Some(1));
        assert!(super::list_backups_job().unwrap().as_array().unwrap().is_empty());
        drop(sandbox);
    }
}
