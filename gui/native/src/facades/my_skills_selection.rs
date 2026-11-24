//! Pre-registered facade slot for the my-skills SELECTION BAR slice
//! (`my_skills/selection_bar.rs`). Registered empty by the wave-3 page base
//! so the slice worker never touches a shared registry file: add `#[module]`
//! methods on `MySkillsSelection` here and surface them in TSX as
//! `MySkillsSelection.*`.
//!
//! The page base owns the selection state model (`src/pages/my_skills/
//! state.ts`: selectedBatch, linkMenuOpen, busy tokens) and the five batch
//! commands the pill mounts on:
//! - Link to agent — `batch_link_skills_to_agent`;
//! - Unlink all — `remove_skill_from_all_agents` per name;
//! - Translate descriptions — `MySkills.setDescriptionChoices` (the same
//!   per-skill store the cards and the drawer read);
//! - Update selected — `MySkills.updateSkills(names)` (busy token
//!   "batch_update", the click-time name snapshot drives the card
//!   ellipses);
//! - Uninstall — `MySkills.uninstallSkill` per name via
//!   `MySkills.uninstallRepoSkills`'s failure-collecting shape, behind the
//!   shared uninstall confirm.
//! This slot is for those two agent-link batch calls and anything else only
//! the pill runs.
//!
//! Page mount point: `src/pages/my_skills/selectionBarSlot.tsx`.

use nsh_core::prelude::*;
use nsh_core::JsonValue;

// Same JSON-builder trick as the my_skills facade (the crate manifest is
// frozen for the parallel waves; nsh-core re-exports the map type).
use nsh_core::element::__private::JsonMap;

fn obj(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
    let mut map = JsonMap::new();
    for (key, value) in pairs {
        map.insert(key.to_owned(), value);
    }
    JsonValue::Object(map)
}

/// The selection-bar seam (`MySkillsSelection` in TSX).
#[module]
pub struct MySkillsSelection;

#[module]
impl MySkillsSelection {
    /// Batch-link the selected skills to one agent (commands.rs
    /// `batch_link_to_agent`: `deployment::batch_link_skills_to_agent`).
    /// Runs on the blocking pool like the GPUI `spawn_domain` job — the call
    /// takes the update transaction lock and walks both directory trees.
    /// Resolves with the number of NEW links (`linked`); already-linked and
    /// not-installed names are skipped by the domain, and an unknown or
    /// disabled agent fails closed with the domain reason.
    pub async fn link_to_agent(
        names: Vec<String>,
        agent_id: String,
    ) -> Result<JsonValue, String> {
        blocking(move || link_job(&names, &agent_id))
            .await
            .map_err(|join| format!("link task failed: {join}"))?
    }

    /// Remove every selected skill's SkillStar deployments from all agents
    /// (commands.rs `batch_unlink_selected`: one blocking job, one
    /// `deployment::remove_skill_from_all_agents` per name, failures
    /// collected as `name: err` lines joined with newlines — the same
    /// failure shape as `MySkills::uninstall_repo_skills`). Resolves with
    /// the number of agents a removal came back from (`removed`).
    pub async fn unlink_from_all_agents(names: Vec<String>) -> Result<JsonValue, String> {
        blocking(move || unlink_job(&names))
            .await
            .map_err(|join| format!("unlink task failed: {join}"))?
    }
}

/// The link batch body (commands.rs `batch_unlink_selected`'s sibling in
/// `batch_link_to_agent`): one domain call, the new-link count out. Kept as
/// a plain fn so the #[cfg(test)] suite can drive it without a tokio
/// runtime (`blocking` is `spawn_blocking` — the repo's facade tests never
/// spin a blocking-pool future, they test the body).
fn link_job(names: &[String], agent_id: &str) -> Result<JsonValue, String> {
    let outcome = ss_skills::deployment::batch_link_skills_to_agent_with_report(names, agent_id)
        .map_err(|err| format!("{err:#}"))?;
    let skipped = outcome
        .skipped
        .into_iter()
        .map(|item| {
            obj(vec![
                ("skill", JsonValue::String(item.skill_name)),
                ("code", JsonValue::String(item.code)),
                ("path", JsonValue::String(item.path)),
            ])
        })
        .collect::<Vec<_>>();
    Ok(obj(vec![
        ("linked", JsonValue::from(outcome.linked as u64)),
        ("skipped", JsonValue::Array(skipped)),
    ]))
}

/// The unlink batch body (commands.rs `batch_unlink_selected`): one
/// `remove_skill_from_all_agents` per name, failures collected as
/// `name: err` lines joined with newlines — the same failure shape as
/// `MySkills::uninstall_repo_skills`.
fn unlink_job(names: &[String]) -> Result<JsonValue, String> {
    let mut failures = Vec::new();
    let mut removed = 0usize;
    for name in names {
        match ss_skills::deployment::remove_skill_from_all_agents(name) {
            Ok(agents) => removed += agents.len(),
            Err(err) => failures.push(format!("{name}: {err:#}")),
        }
    }
    if failures.is_empty() {
        Ok(obj(vec![("removed", JsonValue::from(removed as u64))]))
    } else {
        Err(failures.join("\n"))
    }
}

#[cfg(test)]
mod tests {
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
                "skillstar-nsh-my-skills-selection-{}-{}",
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

    /// The `#[module]` methods wrap their batch bodies in `blocking`
    /// (`tokio::spawn_blocking`), which needs a runtime the unit-test host
    /// does not start — the suite drives the plain bodies instead (the
    /// repo's facade-test precedent: test the seam's body, never spin a
    /// blocking-pool future).

    /// An unknown agent id fails closed before anything is touched on disk
    /// (deployment/mod.rs `require_enabled_global_profile`).
    #[test]
    fn link_to_agent_fails_closed_for_an_unknown_agent() {
        let _iso = IsolatedDataDir::new();
        let err = super::link_job(&["alpha".to_owned()], "no-such-agent")
            .expect_err("an unknown agent id must not link anything");
        assert!(!err.is_empty());
    }

    /// A fresh store has every built-in agent disabled (registry prefs
    /// default false), so a real id fails closed too — handling a stale
    /// request must not provision an agent directory (deployment/mod.rs:83).
    #[test]
    fn link_to_agent_fails_closed_when_no_agent_is_enabled() {
        let _iso = IsolatedDataDir::new();
        let err = super::link_job(&[], "cursor")
            .expect_err("a disabled agent must not accept a batch link");
        assert!(!err.is_empty());
    }

    /// An invalid skill name collects into the `name: err` failure line and
    /// rejects the whole unlink (commands.rs `batch_uninstall_selected`
    /// failure shape).
    #[test]
    fn unlink_collects_invalid_names_as_failure_lines() {
        let _iso = IsolatedDataDir::new();
        let err = super::unlink_job(&["bad/name".to_owned()])
            .expect_err("an invalid name is not a removable deployment");
        assert!(err.contains("bad/name"), "failure line names the skill: {err}");
    }

    /// A well-formed name with no deployment anywhere unlinks cleanly: every
    /// profile answers Missing, nothing is written, removed is zero.
    #[test]
    fn unlink_of_an_undeployed_name_is_a_clean_zero() {
        let _iso = IsolatedDataDir::new();
        let result =
            super::unlink_job(&["zz-never-deployed".to_owned()])
                .expect("removing nothing is not an error");
        assert_eq!(result["removed"].as_u64(), Some(0));
    }
}
