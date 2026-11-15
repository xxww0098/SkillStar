//! Deck-detail-drawer facade — the `skill_cards/detail_drawer.rs` slice's
//! seam to the ss-skills domain. Thin wrappers over the same workflows the
//! GPUI page drives (`skill_cards/mod.rs` `run_link_job` / `run_group_job`):
//! the per-agent deck toggle, the deploy-all batch the card cannot express,
//! and the duplicate/delete commands behind the drawer's more menu.
//!
//! Follows the deck-page facade's bridge conventions (facades/skill_cards.rs):
//! JSON through `nsh_core::JsonValue` (the manifest stays frozen for the
//! parallel waves), async exports as nsh tasks, and the link report shaped
//! exactly like `SkillCards.toggleDeckAgent`'s — `applied` as
//! `[agent, skill]` pairs, `skipped`/`failed` as `{agentId, skill, reason}`
//! — so the page and the drawer share one notice assembler. The update
//! transaction lock is tried with a zero wait, like the GPUI `spawn_domain`
//! closure (skill_cards/mod.rs:170-181): a busy Skill update fails fast
//! with its message instead of freezing the drawer.

use std::time::Duration;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use ss_skills::skill_group;
use ss_skills::workflows::agent_links::AgentLinkReport;
use ss_skills::workflows::skill_group_links;

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

/// The deck-detail-drawer seam (`SkillCardsDrawer` in TSX).
#[module]
pub struct SkillCardsDrawer;

#[module]
impl SkillCardsDrawer {
    /// Link (`enable` true) or unlink every installed member of the deck
    /// `deckId` on the agent `agentId`, then record the deck's rail for that
    /// agent — `skill_group_links::set_deck_agent` (the drawer's per-agent
    /// switch, detail_drawer.rs `toggle_agent_for_deck`). The rail keeps its
    /// old state for the agent when every link attempt failed or was
    /// skipped. Resolves with the link report the notice assembler reads.
    pub async fn set_deck_agent(
        deck_id: String,
        agent_id: String,
        enable: bool,
    ) -> Result<JsonValue, String> {
        run_link_job(move || skill_group_links::set_deck_agent(&deck_id, &agent_id, enable)).await
    }

    /// Deploy-all: link the deck `deckId` into every enabled global agent
    /// and add each agent that received at least one skill to the rail —
    /// `skill_group_links::link_deck_to_enabled_agents` (the drawer's
    /// 「全部链接」 button, detail_drawer.rs `deploy_all_for_deck`). The
    /// batch the card footer cannot express. Resolves with the link report.
    pub async fn link_deck_to_enabled_agents(deck_id: String) -> Result<JsonValue, String> {
        run_link_job(move || skill_group_links::link_deck_to_enabled_agents(&deck_id)).await
    }

    /// Duplicate the deck `deckId` (`skill_group::duplicate_group`, the
    /// drawer's more-menu 「复制卡组」). Resolves with the copy's id.
    pub async fn duplicate_deck(deck_id: String) -> Result<JsonValue, String> {
        run_group_job(move || skill_group::duplicate_group(&deck_id).map(|g| str(&g.id))).await
    }

    /// Delete the deck `deckId` (`skill_group::delete_group`, the drawer's
    /// more-menu 「删除卡组…」 after the name-confirm dialog). Hub skills
    /// stay intact — the domain only drops the group record. Resolves when
    /// the store is written.
    pub async fn delete_deck(deck_id: String) -> Result<(), String> {
        run_group_job(move || skill_group::delete_group(&deck_id).map(|_| ())).await
    }
}

/// The link-command tail (skill_cards/mod.rs `run_link_job`): acquire the
/// update transaction lock with a zero wait, then run the link job and shape
/// the report as JSON. Errors keep anyhow's alternate-chain formatting.
async fn run_link_job<E: std::fmt::Display>(
    job: impl FnOnce() -> Result<AgentLinkReport, E>,
) -> Result<JsonValue, String> {
    let _guard = ss_skills::skill_update::try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    let report = job().map_err(|err| format!("{err:#}"))?;
    Ok(report_to_json(&report))
}

/// The group-command tail (skill_cards/mod.rs `run_group_job`).
async fn run_group_job<T, E: std::fmt::Display>(
    job: impl FnOnce() -> Result<T, E>,
) -> Result<T, String> {
    job().map_err(|err| format!("{err:#}"))
}

fn report_to_json(report: &AgentLinkReport) -> JsonValue {
    obj(vec![
        (
            "applied",
            JsonValue::Array(
                report
                    .applied
                    .iter()
                    .map(|(agent, skill)| JsonValue::Array(vec![str(agent), str(skill)]))
                    .collect(),
            ),
        ),
        (
            "skipped",
            JsonValue::Array(
                report.skipped.iter().map(link_issue_json).collect::<Vec<_>>(),
            ),
        ),
        (
            "failed",
            JsonValue::Array(
                report.failed.iter().map(link_issue_json).collect::<Vec<_>>(),
            ),
        ),
    ])
}

fn link_issue_json(issue: &ss_skills::workflows::agent_links::AgentLinkIssue) -> JsonValue {
    obj(vec![
        ("agentId", str(&issue.agent_id)),
        ("skill", str(&issue.skill)),
        ("reason", str(&issue.reason)),
    ])
}

#[cfg(test)]
mod tests {
    use super::SkillCardsDrawer;
    use ss_skills::skill_group;

    /// The GPUI suites' IsolatedDataDir pattern (ss-gpui test_support.rs):
    /// keep test writes off the real `~/.skillstar` by re-rooting
    /// `SKILLSTAR_DATA_DIR` for the duration. The guard borrows the shared
    /// TEST_ENV_LOCK (facades/mod.rs) so every facade suite that flips the
    /// variable serializes against each other — module-local locks cannot,
    /// and the racing threads abort the test binary.
    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-drawer-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
            Self {
                _lock: lock,
                previous,
                root,
            }
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

    fn seed_deck(members: &[&str]) -> skill_group::SkillGroup {
        skill_group::create_group(
            "demo".into(),
            "A deck under test".into(),
            "\u{1f4bb}".into(),
            members.iter().map(|m| m.to_string()).collect(),
            Default::default(),
        )
        .expect("seed deck")
    }

    /// The #[module] macro turns `async fn` into a task-start call in JS;
    /// from Rust tests the same bodies run as plain futures (the deck-page
    /// facade's `spin` pattern).
    fn spin<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => unreachable!("domain calls finish on first poll"),
        }
    }

    /// One store-writing test on purpose: the sibling deck facades each
    /// sandbox `SKILLSTAR_DATA_DIR` behind their own module lock, and cargo
    /// runs the suites in parallel — every extra env-flipping test widens
    /// the cross-module race window (an abort under cargo's threaded
    /// runner). All write-path domains run here back to back under one
    /// sandbox lease; read-only and in-memory checks live below without one.
    #[test]
    fn drawer_jobs_write_the_store() {
        let _iso = IsolatedDataDir::new();
        let group = seed_deck(&["alpha"]);

        // Duplicate: the copy lands beside its source with the same members.
        let copy_id = spin(SkillCardsDrawer::duplicate_deck(group.id.clone()))
            .expect("duplicate in the isolated store");
        let copy_id = copy_id.as_str().expect("duplicate returns the new id").to_owned();
        assert_ne!(copy_id, group.id);
        assert_eq!(skill_group::list_groups().len(), 2);

        // The rail toggle on a deck with no installed members still records
        // the rail (nothing was attempted for the agent — `rail_may_change`),
        // the domain rule the drawer's switch relies on.
        let report = spin(SkillCardsDrawer::set_deck_agent(
            group.id.clone(),
            "claude".into(),
            true,
        ))
        .expect("toggle is not an error on an empty install");
        assert_eq!(report["applied"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["skipped"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["failed"].as_array().map(Vec::len), Some(0));
        let links = skill_group::list_groups()[0]
            .agent_links
            .clone()
            .unwrap_or_default();
        assert!(links.iter().any(|id| id == "claude"), "rail recorded");

        // Toggling back off clears the rail entry the same way.
        spin(SkillCardsDrawer::set_deck_agent(
            group.id.clone(),
            "claude".into(),
            false,
        ))
        .expect("untoggle");
        let links = skill_group::list_groups()[0]
            .agent_links
            .clone()
            .unwrap_or_default();
        assert!(!links.iter().any(|id| id == "claude"), "rail cleared");

        // Deploy-all with no enabled agents is a no-op, never an error.
        let report = spin(SkillCardsDrawer::link_deck_to_enabled_agents(group.id.clone()))
            .expect("deploy-all without agents is not an error");
        assert_eq!(report["applied"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["skipped"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["failed"].as_array().map(Vec::len), Some(0));

        // Delete: the round trip empties the store (Hub skills untouched —
        // the domain only drops group records).
        spin(SkillCardsDrawer::delete_deck(copy_id)).expect("delete the copy");
        spin(SkillCardsDrawer::delete_deck(group.id)).expect("delete the original");
        assert!(skill_group::list_groups().is_empty());
    }

    /// Read-only: a missing deck surfaces the domain's error verbatim (the
    /// store is only read, so no sandbox is needed).
    #[test]
    fn missing_deck_jobs_surface_the_domain_error() {
        let err = spin(SkillCardsDrawer::duplicate_deck("no-such-deck".into()))
            .expect_err("duplicating a missing deck fails");
        assert!(err.contains("no-such-deck"), "error names the deck: {err}");

        let err = spin(SkillCardsDrawer::delete_deck("no-such-deck".into()))
            .expect_err("deleting a missing deck fails");
        assert!(err.contains("no-such-deck"), "error names the deck: {err}");
    }
}
