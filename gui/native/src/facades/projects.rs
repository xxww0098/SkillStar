//! Projects page facade — the whole area in one file
//! (`crates/ss-gpui/src/projects/`: mod.rs rail + toolbar, detail.rs cards,
//! agent_item.rs rows, actions.rs mutations, upkeep.rs copy status,
//! dialogs.rs registry dialogs). Every call is a thin pass-through to
//! `ss_skills::projects` / `ss_skills::agents` / `ss_skills::installer` —
//! the same functions the GPUI page's `spawn_domain` jobs run — with the
//! same UI-thread courtesy: file-touching jobs (sync, retry, forget,
//! registry writes, the copy inspection that takes the project write lock)
//! ride the blocking pool; the cheap registry/profile/hub reads run inline
//! (the deck facades' precedent).

use std::collections::HashMap;

use nsh_core::JsonValue;
use nsh_core::file_dialog::OpenDialog;
use nsh_core::prelude::*;
use ss_skills::projects::{ProjectCopyState, ProjectDeployMode, ProjectEntry};

// The crate cannot depend on serde_json directly, so the JSON builders go
// through nsh-core's re-exports (same trick as the sibling facades).
use nsh_core::element::__private::JsonMap;

fn obj(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
    let mut map = JsonMap::new();
    for (key, value) in pairs {
        map.insert(key.to_owned(), value);
    }
    JsonValue::Object(map)
}

fn str(value: &str) -> JsonValue {
    JsonValue::String(value.to_owned())
}

fn num(value: usize) -> JsonValue {
    JsonValue::from(value as u64)
}

/// A string-keyed JSON object (the facades' JsonMap needs explicit type
/// params at collect sites, so this helper keeps the call sites short).
fn str_map(pairs: Vec<(String, JsonValue)>) -> JsonValue {
    let mut map = JsonMap::new();
    for (key, value) in pairs {
        map.insert(key, value);
    }
    JsonValue::Object(map)
}

/// The projects page seam (`Projects` in TSX).
#[module]
pub struct Projects;

/// `ProjectCopyState` as the page's snake_case tag (upkeep.rs `state_key`'s
/// Rust-side twin; the TSX maps the tag to the `projects.copyState*` copy).
fn copy_state_tag(state: ProjectCopyState) -> &'static str {
    match state {
        ProjectCopyState::Current => "current",
        ProjectCopyState::Stale => "stale",
        ProjectCopyState::ProjectMissing => "project_missing",
        ProjectCopyState::Missing => "missing",
        ProjectCopyState::Modified => "modified",
        ProjectCopyState::Unmanaged => "unmanaged",
        ProjectCopyState::SourceMissing => "source_missing",
        ProjectCopyState::ReadFailed => "read_failed",
        ProjectCopyState::BrokenLink => "broken_link",
    }
}

/// `ProjectDeployMode` serde tag (`snake_case`, types.rs:11-19).
fn deploy_mode_tag(mode: ProjectDeployMode) -> &'static str {
    match mode {
        ProjectDeployMode::Symlink => "symlink",
        ProjectDeployMode::Copy => "copy",
    }
}

fn parse_deploy_mode(value: &JsonValue) -> Result<ProjectDeployMode, String> {
    match value.as_str() {
        Some("symlink") => Ok(ProjectDeployMode::Symlink),
        Some("copy") => Ok(ProjectDeployMode::Copy),
        other => Err(format!("unknown deploy mode: {other:?}")),
    }
}

fn entry_json(entry: &ProjectEntry) -> JsonValue {
    obj(vec![
        ("name", str(&entry.name)),
        ("path", str(&entry.path)),
        ("createdAt", str(&entry.created_at)),
    ])
}

fn copy_record_json(record: &ss_skills::projects::ProjectCopyRecord) -> JsonValue {
    obj(vec![
        ("projectName", str(record.project_name())),
        ("relativePath", str(record.relative_path())),
        ("skillName", str(record.skill_name())),
        ("state", str(copy_state_tag(record.state()))),
        ("detail", record.detail().map(str).unwrap_or(JsonValue::Null)),
    ])
}

fn skills_list_json(list: &ss_skills::projects::SkillsList) -> JsonValue {
    let agents = str_map(
        list.agents
            .iter()
            .map(|(agent, skills)| {
                (
                    agent.clone(),
                    JsonValue::Array(skills.iter().map(|s| str(s)).collect()),
                )
            })
            .collect(),
    );
    let modes = str_map(
        list.deploy_modes
            .iter()
            .map(|(rel, mode)| (rel.clone(), str(deploy_mode_tag(*mode))))
            .collect(),
    );
    obj(vec![("agents", agents), ("deployModes", modes)])
}

/// Parse the `agents` + `deployModes` payload `save_and_sync` expects. The
/// mode map is keyed by `project_skills_rel` exactly like the manifest
/// (`skills-list.json`), so the page just echoes the rows it loaded.
fn parse_skills_list(
    agents: &JsonValue,
    deploy_modes: &JsonValue,
) -> Result<(HashMap<String, Vec<String>>, HashMap<String, ProjectDeployMode>), String> {
    let Some(agent_map) = agents.as_object() else {
        return Err("agents must be an object".to_owned());
    };
    let mut parsed_agents = HashMap::new();
    for (agent, skills) in agent_map {
        let Some(list) = skills.as_array() else {
            return Err(format!("skills for {agent} must be an array"));
        };
        let mut names = Vec::new();
        for skill in list {
            let Some(name) = skill.as_str() else {
                return Err(format!("skill for {agent} must be a string"));
            };
            names.push(name.to_owned());
        }
        parsed_agents.insert(agent.clone(), names);
    }
    let mut parsed_modes = HashMap::new();
    if let Some(mode_map) = deploy_modes.as_object() {
        for (rel, mode) in mode_map {
            parsed_modes.insert(rel.clone(), parse_deploy_mode(mode)?);
        }
    } else if !deploy_modes.is_null() {
        return Err("deployModes must be an object".to_owned());
    }
    Ok((parsed_agents, parsed_modes))
}

// ---- Blocking bodies ----------------------------------------------------------
//
// The `#[module]` methods wrap these in `blocking` (`tokio::spawn_blocking`),
// which needs a runtime the unit-test host does not start — the suite drives
// the plain bodies instead (the repo's facade-test precedent: test the seam's
// body, never spin a blocking-pool future).

fn profiles_array_json() -> JsonValue {
    let profiles = ss_skills::agents::list_profile_specs();
    JsonValue::Array(
        profiles
            .iter()
            .map(|profile| {
                obj(vec![
                    ("id", str(&profile.id)),
                    ("displayName", str(&profile.display_name)),
                    ("icon", str(&profile.icon)),
                    ("projectSkillsRel", str(&profile.project_skills_rel)),
                    (
                        "hasProjectSkills",
                        JsonValue::Bool(profile.has_project_skills()),
                    ),
                    ("enabled", JsonValue::Bool(profile.enabled)),
                ])
            })
            .collect(),
    )
}

fn registry_job() -> Result<JsonValue, String> {
    let projects = ss_skills::projects::list_projects();
    let mut hub = ss_skills::installer::installed_names();
    hub.sort();
    Ok(obj(vec![
        (
            "projects",
            JsonValue::Array(projects.iter().map(entry_json).collect()),
        ),
        ("profiles", profiles_array_json()),
        ("hubSkills", JsonValue::Array(hub.iter().map(|n| str(n)).collect())),
    ]))
}

fn profiles_job() -> JsonValue {
    profiles_array_json()
}

fn skills_list_job(project: &str) -> JsonValue {
    let list = ss_skills::projects::load_skills_list(project).unwrap_or_default();
    skills_list_json(&list)
}

/// `refresh_copy_status` body: `inspect_project_copy_status` under the
/// project lock — config and deployment facts observed together.
fn copy_status_job(project: &str) -> Result<JsonValue, String> {
    let snapshot = ss_skills::projects::inspect_project_copy_status(project)
        .map_err(|err| format!("{err:#}"))?;
    let list = skills_list_json(&snapshot.skills_list);
    Ok(obj(vec![
        ("project", entry_json(&snapshot.project)),
        ("agents", list["agents"].clone()),
        ("deployModes", list["deployModes"].clone()),
        (
            "copies",
            JsonValue::Array(snapshot.copies.iter().map(copy_record_json).collect()),
        ),
        (
            "rules",
            JsonValue::Array(snapshot.rules.iter().map(|r| str(r)).collect()),
        ),
    ]))
}

/// `save_and_sync_current` body: persist the working state and deploy.
fn save_and_sync_job(
    path: &str,
    agents: &JsonValue,
    deploy_modes: &JsonValue,
) -> Result<JsonValue, String> {
    let (parsed_agents, parsed_modes) = parse_skills_list(agents, deploy_modes)?;
    let (_, count) =
        ss_skills::projects::save_and_sync(path, parsed_agents, parsed_modes)
            .map_err(|err| format!("{err:#}"))?;
    Ok(obj(vec![("count", num(count as usize))]))
}

/// `retry_copies` body: reconcile the tracked copy deployments with the hub.
fn retry_copies_job(project: &str) -> Result<JsonValue, String> {
    let records = ss_skills::projects::retry_project_copies(project)
        .map_err(|err| format!("{err:#}"))?;
    Ok(JsonValue::Array(records.iter().map(copy_record_json).collect()))
}

/// `forget_copy` body: metadata-only cleanup; files on both ends stay.
fn forget_copy_job(project: &str, relative_path: &str, skill: &str) -> Result<(), String> {
    ss_skills::projects::forget_project_copy(project, relative_path, skill)
        .map_err(|err| format!("{err:#}"))
}

/// The register dialog's commit body.
fn register_job(path: &str) -> Result<JsonValue, String> {
    ss_skills::projects::register_project(path)
        .map(|entry| entry_json(&entry))
        .map_err(|err| format!("{err:#}"))
}

/// The change-path dialog's commit body.
fn update_path_job(name: &str, path: &str) -> Result<(), String> {
    ss_skills::projects::update_project_path(name, path)
        .map(|_| ())
        .map_err(|err| format!("{err:#}"))
}

/// The remove dialog's commit body.
fn remove_job(name: &str) -> Result<(), String> {
    ss_skills::projects::remove_project(name).map_err(|err| format!("{err:#}"))
}

fn approval_source_tag(source: ss_app::project_skill_approval::ApprovalSource) -> &'static str {
    match source {
        ss_app::project_skill_approval::ApprovalSource::Elicitation => "elicitation",
        ss_app::project_skill_approval::ApprovalSource::Skillstar => "skillstar",
    }
}

fn pending_plan_json(plan: &ss_app::project_skill_approval::PendingProjectSkillPlan) -> JsonValue {
    let ops = plan
        .operations
        .iter()
        .map(|op| {
            let action = match op.action {
                ss_app::project_skill_approval::PlanAction::Create => "create",
                ss_app::project_skill_approval::PlanAction::Already => "already",
            };
            obj(vec![
                ("name", str(&op.name)),
                ("action", str(action)),
                ("skillPath", str(&op.skill_path)),
            ])
        })
        .collect();
    let agents = plan.affected_agents.iter().map(|a| str(a)).collect();
    let approval_source = plan
        .approval_source
        .map(|s| str(approval_source_tag(s)))
        .unwrap_or(JsonValue::Null);
    obj(vec![
        ("planId", str(&plan.plan_id)),
        ("planHash", str(&plan.plan_hash)),
        ("canonicalRoot", str(&plan.canonical_root)),
        ("isRegistered", JsonValue::Bool(plan.is_registered)),
        ("owner", str(&plan.owner)),
        ("affectedAgents", JsonValue::Array(agents)),
        ("operations", JsonValue::Array(ops)),
        ("expiresAt", str(&plan.expires_at.to_rfc3339())),
        ("approved", JsonValue::Bool(plan.approved)),
        ("approvalSource", approval_source),
    ])
}

fn pending_project_skill_plan_job(project_root: &str) -> Result<JsonValue, String> {
    ss_app::project_skill_approval::pending_project_skill_plan(std::path::Path::new(project_root))
        .map(|maybe| match maybe {
            Some(plan) => pending_plan_json(&plan),
            None => JsonValue::Null,
        })
        .map_err(|err| format!("{err:#}"))
}

fn approve_project_skill_plan_job(plan_id: &str, plan_hash: &str) -> Result<JsonValue, String> {
    ss_app::project_skill_approval::approve_project_skill_plan(plan_id, plan_hash)
        .map(|record| {
            obj(vec![
                ("planId", str(&record.plan_id)),
                ("planHash", str(&record.plan_hash)),
                ("source", str(approval_source_tag(record.source))),
                ("recordedAt", str(&record.recorded_at.to_rfc3339())),
            ])
        })
        .map_err(|err| format!("{err:#}"))
}

#[module]
impl Projects {
    /// The page's one refresh payload (mod.rs `refresh`): the registry
    /// (`list_projects`), the agent profiles (`list_profile_specs` — the page
    /// filters `has_project_skills() && enabled` itself, detail.rs:190), and
    /// the hub skill names (`installer::installed_names`, sorted —
    /// mod.rs `load_hub_skill_names`). Runs on the blocking pool.
    pub async fn registry() -> Result<JsonValue, String> {
        blocking(registry_job)
            .await
            .map_err(|join| format!("registry task failed: {join}"))?
    }

    /// Lightweight profile roster reload (after Settings toggles an agent).
    /// Avoids re-reading `projects.json` or scanning Hub skills.
    pub async fn profiles() -> Result<JsonValue, String> {
        blocking(profiles_job)
            .await
            .map_err(|join| format!("profiles task failed: {join}"))
    }

    /// `set_active_project` (mod.rs:138): the project's saved skills list
    /// (`load_skills_list` — `None` degrades to the empty list like the
    /// GPUI `unwrap_or_default`). Runs on the blocking pool.
    pub async fn skills_list(project: String) -> Result<JsonValue, String> {
        blocking(move || skills_list_job(&project))
            .await
            .map_err(|join| format!("skills list task failed: {join}"))
    }

    /// `refresh_copy_status` (upkeep.rs:13): config and deployment facts
    /// observed under the same project lock. Blocking pool (the call takes
    /// the write lock). The page reconciles the snapshot into its working
    /// state only when it has no unsaved edits (upkeep.rs:46-50).
    pub async fn copy_status(project: String) -> Result<JsonValue, String> {
        blocking(move || copy_status_job(&project))
            .await
            .map_err(|join| format!("copy status task failed: {join}"))?
    }

    /// `save_and_sync_current` (actions.rs:10): persist the working state
    /// and deploy (`save_and_sync`). Blocking pool like the GPUI
    /// `spawn_domain` job. Resolves `{count}` — the deployment count the
    /// footer status reads (`projects.deploySuccess`); Err rejects with the
    /// domain chain (`projects.deployFailed`).
    pub async fn save_and_sync(
        path: String,
        agents: JsonValue,
        deploy_modes: JsonValue,
    ) -> Result<JsonValue, String> {
        blocking(move || save_and_sync_job(&path, &agents, &deploy_modes))
            .await
            .map_err(|join| format!("save and sync task failed: {join}"))?
    }

    /// `retry_copies` (actions.rs:56): `retry_project_copies` — reconcile
    /// the tracked copy deployments with the hub without an upstream
    /// update. Blocking pool. Resolves the fresh record list the page
    /// stores verbatim (the GPUI callback keeps `pending` as page state).
    pub async fn retry_copies(project: String) -> Result<JsonValue, String> {
        blocking(move || retry_copies_job(&project))
            .await
            .map_err(|join| format!("retry copies task failed: {join}"))?
    }

    /// `forget_copy` (actions.rs:110): `forget_project_copy` — metadata-only
    /// cleanup; files on both ends are preserved. Blocking pool.
    pub async fn forget_copy(
        project: String,
        relative_path: String,
        skill: String,
    ) -> Result<(), String> {
        blocking(move || forget_copy_job(&project, &relative_path, &skill))
            .await
            .map_err(|join| format!("forget copy task failed: {join}"))?
    }

    /// The register dialog's commit (dialogs.rs `PathAction::Register`):
    /// `register_project`. Blocking pool. Resolves the new entry so the page
    /// can refresh and select it (dialogs.rs:376-379).
    pub async fn register(path: String) -> Result<JsonValue, String> {
        blocking(move || register_job(&path))
            .await
            .map_err(|join| format!("register task failed: {join}"))?
    }

    /// The change-path dialog's commit (dialogs.rs `PathAction::Change`):
    /// `update_project_path`. Blocking pool.
    pub async fn update_path(name: String, path: String) -> Result<(), String> {
        blocking(move || update_path_job(&name, &path))
            .await
            .map_err(|join| format!("update path task failed: {join}"))?
    }

    /// The remove dialog's commit (dialogs.rs `open_remove`):
    /// `remove_project`. Blocking pool. Unregistering never touches files in
    /// the project directory (`projects.unregisterHint`).
    pub async fn remove(name: String) -> Result<(), String> {
        blocking(move || remove_job(&name))
            .await
            .map_err(|join| format!("remove task failed: {join}"))?
    }

    /// System folder picker for registering or re-pointing a project root
    /// (`dialogs.rs` `PathAction`). Resolves with the picked directory path,
    /// or `None` when cancelled / unavailable on the platform.
    pub async fn pick_folder(prompt: String) -> Option<String> {
        let dialog = OpenDialog::folders().title(prompt);
        match pick_paths(dialog).await {
            Ok(Some(paths)) => paths
                .into_iter()
                .next()
                .map(|path| path.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    /// Inspect whether `project_root` has an unexpired, unapplied project-skill
    /// MCP plan waiting for desktop review. Runs on the blocking pool.
    pub async fn pending_project_skill_plan(project_root: String) -> Result<JsonValue, String> {
        blocking(move || pending_project_skill_plan_job(&project_root))
            .await
            .map_err(|join| format!("pending plan lookup failed: {join}"))?
    }

    /// Verify `plan_id` matches `plan_hash` and write a SkillStar desktop
    /// approval record without deploying skills or registering the project.
    pub async fn approve_project_skill_plan(
        plan_id: String,
        plan_hash: String,
    ) -> Result<JsonValue, String> {
        blocking(move || approve_project_skill_plan_job(&plan_id, &plan_hash))
            .await
            .map_err(|join| format!("approve plan task failed: {join}"))?
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::JsonValue;
    use nsh_core::element::__private::JsonMap;
    use ss_skills::projects::ProjectDeployMode;
    use std::collections::HashMap;

    /// The shared sandbox guard (facades/mod.rs TEST_ENV_LOCK): every facade
    /// suite that flips SKILLSTAR_DATA_DIR borrows it so the cargo threads
    /// serialize — module-local locks cannot (wave-1 lesson).
    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-projects-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
            Self { _lock: lock, previous, root }
        }
    }

    impl Drop for IsolatedDataDir {
        fn drop(&mut self) {
            unsafe {
                match self.previous.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// One registered project with a copy-deployed hub skill — the GPUI
    /// `ProjectSandbox::project` fixture (test_support.rs:78-89) with the
    /// hub source rooted at the isolated data dir. Returns the entry name.
    fn sandbox_project(iso: &IsolatedDataDir, name: &str) -> String {
        let hub = ss_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::create_dir_all(&hub).unwrap();
        std::fs::write(hub.join("SKILL.md"), "v1").unwrap();
        let root = iso.root.join(name);
        std::fs::create_dir_all(&root).unwrap();
        let agents = JsonValue::Object({
            let mut map = JsonMap::new();
            map.insert(
                "codex".to_owned(),
                JsonValue::Array(vec![JsonValue::String("demo".to_owned())]),
            );
            map
        });
        let modes = JsonValue::Object({
            let mut map = JsonMap::new();
            map.insert(".agents/skills".to_owned(), JsonValue::String("copy".to_owned()));
            map
        });
        super::save_and_sync_job(root.to_str().unwrap(), &agents, &modes)
            .expect("seed the sandbox deployment");
        let entry = super::register_job(root.to_str().unwrap())
            .expect("register the sandbox project");
        entry["name"].as_str().unwrap().to_owned()
    }

    #[test]
    fn registry_starts_empty_but_carries_the_builtin_roster() {
        let _iso = IsolatedDataDir::new();
        let registry = super::registry_job().expect("registry");
        assert_eq!(
            registry["projects"].as_array().map(Vec::len),
            Some(0),
            "fresh isolated store has no projects"
        );
        let profiles = registry["profiles"].as_array().expect("profiles");
        assert!(!profiles.is_empty(), "the built-in roster always exists");
        for profile in profiles {
            assert!(profile["id"].is_string());
            assert!(profile["projectSkillsRel"].is_string());
            assert!(profile["hasProjectSkills"].is_boolean());
        }
        assert!(registry["hubSkills"].as_array().is_some());
    }

    #[test]
    fn skills_list_degrades_to_empty_for_an_unregistered_name() {
        let _iso = IsolatedDataDir::new();
        let list = super::skills_list_job("ghost");
        assert_eq!(list["agents"].as_object().map(JsonMap::len), Some(0));
        assert_eq!(list["deployModes"].as_object().map(JsonMap::len), Some(0));
    }

    #[test]
    fn copy_status_round_trips_the_saved_deployment() {
        let iso = IsolatedDataDir::new();
        let name = sandbox_project(&iso, "round");
        let status = super::copy_status_job(&name).expect("copy status");
        assert_eq!(status["project"]["name"].as_str(), Some("round"));
        // The saved list reconciles verbatim: codex → [demo], copy mode.
        assert_eq!(
            status["agents"]["codex"].as_array().map(|list| list.len()),
            Some(1)
        );
        assert_eq!(status["agents"]["codex"][0].as_str(), Some("demo"));
        assert_eq!(status["deployModes"][".agents/skills"].as_str(), Some("copy"));
        let copies = status["copies"].as_array().expect("copies");
        assert_eq!(copies.len(), 1, "records dedupe by path, not by agent");
        assert_eq!(copies[0]["state"].as_str(), Some("current"));
        assert!(status["rules"].as_array().is_some());
    }

    #[test]
    fn retry_updates_a_stale_copy_and_the_state_tag_flips() {
        let iso = IsolatedDataDir::new();
        let name = sandbox_project(&iso, "stale");
        let source = ss_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::write(source.join("SKILL.md"), "v2").unwrap();
        let before = super::copy_status_job(&name).expect("status");
        assert_eq!(before["copies"][0]["state"].as_str(), Some("stale"));

        let records = super::retry_copies_job(&name).expect("retry");
        assert_eq!(records[0]["state"].as_str(), Some("current"));
        let target = iso.root.join("stale/.agents/skills/demo/SKILL.md");
        assert_eq!(std::fs::read_to_string(target).unwrap(), "v2");
    }

    #[test]
    fn save_and_sync_reports_the_deployment_count_and_forget_clears_the_record() {
        let iso = IsolatedDataDir::new();
        let name = sandbox_project(&iso, "lifecycle");
        let entry = super::copy_status_job(&name).expect("status");
        let path = entry["project"]["path"].as_str().unwrap().to_owned();
        let agents = JsonValue::Object({
            let mut map = JsonMap::new();
            map.insert(
                "codex".to_owned(),
                JsonValue::Array(vec![JsonValue::String("demo".to_owned())]),
            );
            map
        });
        let modes = JsonValue::Object({
            let mut map = JsonMap::new();
            map.insert(".agents/skills".to_owned(), JsonValue::String("copy".to_owned()));
            map
        });
        let result =
            super::save_and_sync_job(&path, &agents, &modes).expect("save and sync");
        assert_eq!(result["count"].as_u64(), Some(1));

        // Metadata-only cleanup: the record goes away, both files stay.
        super::forget_copy_job(&name, ".agents/skills", "demo").expect("forget");
        let after = super::copy_status_job(&name).expect("status after forget");
        assert_eq!(
            after["copies"].as_array().map(Vec::len),
            Some(0),
            "the record is gone"
        );
        assert!(
            ss_core::infra::paths::hub_skills_dir().join("demo/SKILL.md").exists(),
            "the hub file is preserved"
        );
        let project_copy = iso.root.join("lifecycle/.agents/skills/demo/SKILL.md");
        assert!(project_copy.exists(), "the project file is preserved");
    }

    #[test]
    fn a_missing_project_root_reports_project_missing_without_writing() {
        let iso = IsolatedDataDir::new();
        let name = sandbox_project(&iso, "moved");
        let entry = super::copy_status_job(&name).expect("status");
        let path = std::path::PathBuf::from(entry["project"]["path"].as_str().unwrap());
        let moved = iso.root.join("moved-away");
        std::fs::rename(&path, &moved).unwrap();
        let status = super::copy_status_job(&name).expect("status over the missing root");
        assert_eq!(status["copies"][0]["state"].as_str(), Some("project_missing"));
        assert_eq!(
            std::fs::read_to_string(moved.join(".agents/skills/demo/SKILL.md")).unwrap(),
            "v1",
            "the moved files are untouched"
        );
    }

    #[test]
    fn remove_unregisters_without_touching_the_project_directory() {
        let iso = IsolatedDataDir::new();
        let name = sandbox_project(&iso, "bye");
        super::remove_job(&name).expect("remove");
        let registry = super::registry_job().expect("registry");
        assert_eq!(registry["projects"].as_array().map(Vec::len), Some(0));
        assert!(
            iso.root.join("bye/.agents/skills/demo/SKILL.md").exists(),
            "unregistering keeps the project directory"
        );
    }

    #[test]
    fn update_path_relinks_the_registry_entry() {
        let iso = IsolatedDataDir::new();
        let name = sandbox_project(&iso, "relink");
        let moved = iso.root.join("relink-new");
        std::fs::rename(iso.root.join("relink"), &moved).unwrap();
        super::update_path_job(&name, moved.to_str().unwrap()).expect("update path");
        let status = super::copy_status_job(&name).expect("status");
        assert_eq!(status["project"]["path"].as_str(), Some(moved.to_str().unwrap()));
        assert_eq!(status["copies"][0]["state"].as_str(), Some("current"));
    }

    #[test]
    fn unknown_deploy_modes_reject_instead_of_guessing() {
        let bad = JsonValue::Object({
            let mut map = JsonMap::new();
            map.insert(
                ".agents/skills".to_owned(),
                JsonValue::String("auto".to_owned()),
            );
            map
        });
        let err = super::save_and_sync_job(
            "/tmp/unused",
            &JsonValue::Object(JsonMap::new()),
            &bad,
        )
        .expect_err("auto is not a mode the manifest knows");
        assert!(err.contains("deploy mode"));
    }

    /// The GPUI sandbox also seeds `codex` via the manifest directly; the
    /// mode map the page echoes back must survive the round trip unchanged
    /// (types.rs prune rule keeps modes for enabled project agents).
    #[test]
    fn the_manifest_round_trips_the_copy_mode_exactly() {
        let _iso = IsolatedDataDir::new();
        let _ = HashMap::<String, ProjectDeployMode>::new();
        let modes = ss_skills::projects::load_skills_list("ghost").unwrap_or_default();
        assert!(modes.deploy_modes.is_empty());
    }

    #[test]
    fn pending_plan_returns_null_when_absent_and_approve_rejects_unknown_plan() {
        let iso = IsolatedDataDir::new();
        let project_dir = iso.root.join("clean-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        let pending =
            super::pending_project_skill_plan_job(project_dir.to_str().unwrap()).unwrap();
        assert_eq!(pending, JsonValue::Null);

        let err = super::approve_project_skill_plan_job("missing-plan", "sha256:bad")
            .expect_err("unknown plan must fail");
        assert!(!err.is_empty());
    }
}
