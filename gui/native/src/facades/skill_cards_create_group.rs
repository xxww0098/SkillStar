//! Create-deck facade — the `skill_cards/create_group.rs` slice's seam
//! (member picker load, duplicate-name guard input, create/save commit).
//! Methods surface in TSX as `SkillCardsCreateGroup.*`.
//!
//! Parity targets (crates/ss-gpui/src/skill_cards/create_group.rs):
//! `CreateGroupDialog::new` loads installed skills and existing deck names
//! in the background; `create` commits through `skill_group::create_group`
//! or `update_group`. Editing passes `None` for `skill_sources` and
//! `agent_links` so the store keeps the deck's identity, source map and
//! agent rail untouched (create_group_tests.rs
//! `editing_keeps_missing_members_sources_and_agent_links`).

use nsh_core::prelude::*;
use ss_skills::skill_group;

/// One member-picker row: an installed skill's name and one-line
/// description (the filter matches either).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct DeckSkillRow {
    /// The skill's name — deck members are these strings.
    pub name: String,
    /// One-line description, the picker filter's second match target.
    pub description: String,
}

/// The create-deck seam (`SkillCardsCreateGroup` in TSX).
#[module]
pub struct SkillCardsCreateGroup;

#[module]
impl SkillCardsCreateGroup {
    /// `(name, description)` of every installed skill — the member
    /// picker's rows. Read-only. A listing failure is an empty list, the
    /// same swallow the GPUI dialog's background load does
    /// (`unwrap_or_default`), so an unreadable hub never blocks deck
    /// editing.
    pub async fn installed_skill_rows() -> Vec<DeckSkillRow> {
        ss_skills::installed_skill::list_installed_skills()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|skill| DeckSkillRow {
                name: skill.name,
                description: skill.description,
            })
            .collect()
    }

    /// Names of the decks that already exist — the duplicate-name guard's
    /// input. `excludeId` drops the deck being edited (its own name is
    /// legal to keep). Read-only; an unreadable store reads as no decks.
    pub async fn existing_deck_names(exclude_id: Option<String>) -> Vec<String> {
        skill_group::list_groups()
            .into_iter()
            .filter(|group| Some(group.id.as_str()) != exclude_id.as_deref())
            .map(|group| group.name)
            .collect()
    }

    /// Create a deck (`editingId` null) or save the edits to one (set).
    /// Members replace the deck's member list wholesale (missing members
    /// included — removal from a deck never uninstalls); `skill_sources`
    /// and `agent_links` are left as the store holds them. A rejected
    /// promise carries the domain message (duplicate name, store failure)
    /// — the dialog keeps its input and shows it.
    pub async fn commit_deck(
        editing_id: Option<String>,
        name: String,
        description: String,
        icon: String,
        members: Vec<String>,
    ) -> Result<(), String> {
        match editing_id {
            Some(id) => skill_group::update_group(
                id,
                Some(name),
                Some(description),
                Some(icon),
                Some(members),
                None,
                None,
            )
            .map(|_| ()),
            None => skill_group::create_group(name, description, icon, members, Default::default())
                .map(|_| ()),
        }
        .map_err(|error| format!("{error:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::SkillCardsCreateGroup;
    use ss_skills::skill_group;

    /// The ss-gpui test-support pattern (IsolatedDataDir): point
    /// `SKILLSTAR_DATA_DIR` at a temp dir for the test's lifetime so deck
    /// writes never touch the real `~/.skillstar`. The guard borrows the
    /// shared TEST_ENV_LOCK (facades/mod.rs) so every facade suite that
    /// flips the variable serializes against each other — module-local
    /// locks cannot, and the racing threads abort the test binary.
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
                "skillstar-nsh-create-group-{}-{}",
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

    /// The facade futures are domain calls over synchronous IO (no inner
    /// spawns), so a minimal park/unpark block-on drives them — the crate
    /// deliberately carries no async runtime of its own.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};
        struct ThreadWaker(std::thread::Thread);
        impl Wake for ThreadWaker {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::park(),
            }
        }
    }

    #[test]
    fn commit_deck_creates_and_reports_duplicate_names() {
        let _dir = IsolatedDataDir::new();
        block_on(async {
            SkillCardsCreateGroup::commit_deck(
                None,
                "demo".into(),
                "d".into(),
                "📦".into(),
                vec!["alpha".into()],
            )
            .await
            .unwrap();
            let groups = skill_group::list_groups();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].name, "demo");
            assert_eq!(groups[0].skills, vec!["alpha".to_string()]);

            // The domain's duplicate-name bail reaches JS as the message.
            let error = SkillCardsCreateGroup::commit_deck(
                None,
                "demo".into(),
                String::new(),
                "📦".into(),
                vec!["beta".into()],
            )
            .await
            .unwrap_err();
            assert!(error.contains("already exists"), "got: {error}");
        });
    }

    #[test]
    fn commit_deck_edit_keeps_sources_and_agent_links() {
        let _dir = IsolatedDataDir::new();
        block_on(async {
            let group = skill_group::create_group(
                "demo".into(),
                "old".into(),
                "🚀".into(),
                vec!["alpha".into(), "missing".into()],
                [("missing".into(), "https://github.com/example/skills".into())].into(),
            )
            .unwrap();
            let group = skill_group::update_group(
                group.id.clone(),
                None,
                None,
                None,
                None,
                None,
                Some(vec!["claude".into()]),
            )
            .unwrap();

            SkillCardsCreateGroup::commit_deck(
                Some(group.id.clone()),
                "demo".into(),
                "new".into(),
                "🚀".into(),
                vec!["alpha".into(), "beta".into(), "missing".into()],
            )
            .await
            .unwrap();

            let saved = skill_group::list_groups();
            assert_eq!(saved.len(), 1);
            assert_eq!(saved[0].id, group.id);
            assert_eq!(saved[0].description, "new");
            assert_eq!(saved[0].skills, vec!["alpha", "beta", "missing"]);
            assert_eq!(saved[0].skill_sources, group.skill_sources);
            assert_eq!(saved[0].agent_links, group.agent_links);
            assert_eq!(saved[0].created_at, group.created_at);
        });
    }

    #[test]
    fn existing_deck_names_excludes_the_edited_deck() {
        let _dir = IsolatedDataDir::new();
        block_on(async {
            let demo = skill_group::create_group(
                "demo".into(),
                String::new(),
                "📦".into(),
                vec!["alpha".into()],
                Default::default(),
            )
            .unwrap();
            skill_group::create_group(
                "taken".into(),
                String::new(),
                "📦".into(),
                vec!["beta".into()],
                Default::default(),
            )
            .unwrap();

            let all = SkillCardsCreateGroup::existing_deck_names(None).await;
            assert_eq!(all, vec!["demo".to_string(), "taken".to_string()]);

            let editing = SkillCardsCreateGroup::existing_deck_names(Some(demo.id.clone())).await;
            assert_eq!(editing, vec!["taken".to_string()]);
        });
    }
}
