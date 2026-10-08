//! My Skills — installed skills management page.
//! React source: `src/pages/MySkills.tsx` + `features/my-skills/`.

mod canvas;
mod commands;
mod detail_agents;
pub mod detail_drawer;
mod detail_facts;
mod detail_parts;
mod detail_uninstall;
pub mod empty_state;
mod filters;
mod github_sign_in;
mod import_modal;
mod repo_source;
pub mod selection_bar;
mod skill_card;
pub(crate) mod skill_reader;
pub mod toolbar;
pub mod types;
mod updates;
mod view;

use std::collections::HashSet;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_skills::agents::AgentProfile;

use crate::nav::{GroupsChanged, SelectPage};
use crate::spawn_domain;

use self::types::{MySkillsScope, SortOption, SourceFilter};

struct AgentLinkSnap {
    name: String,
    display_name: String,
    was_linked: bool,
}

pub struct MySkillsPage {
    pub(crate) skills: Vec<Skill>,
    pub(crate) profiles: Vec<AgentProfile>,
    pub(crate) scope: MySkillsScope,
    pub(crate) sort_by: SortOption,
    pub(crate) source_filter: SourceFilter,
    pub(crate) repo_filter: Option<String>,
    /// Repo source whose reinstall is in flight. Other rows stay idle.
    pub(crate) reinstalling_repo: Option<String>,
    pub(crate) agent_filter: Option<String>,
    pub(crate) only_updates: bool,
    pub(crate) view_list: bool,
    pub(crate) search: Option<Entity<InputState>>,
    pub(crate) search_query: String,
    pub(crate) selected_skill: Option<String>,
    /// Per-skill description-translation choice made on the drawer's button.
    /// A skill not in the map follows the Settings switch; a skill the user
    /// translated keeps its line in the chosen language even after the
    /// drawer moves to another skill. The map is loaded from and written back
    /// to the translation config, so choices survive a restart; the switch
    /// itself is never written.
    description_choices: std::collections::HashMap<String, bool>,
    /// Pointer is on the detail-column uninstall control. Hover frames read
    /// this and notify the canvas; they do not bump [`Self::content_epoch`].
    uninstall_hover: bool,
    pub(crate) selected_batch: HashSet<String>,
    pub(crate) link_menu_open: bool,
    pub(crate) loading: bool,
    pub(crate) busy: Option<String>,
    /// Skill names captured when a toolbar or selection-bar update starts.
    /// The card ellipsis follows this set, not a later selection change.
    updating_batch: HashSet<String>,
    pub(crate) checking_updates: bool,
    /// Start of the last upstream check; throttles the background check.
    pub(crate) last_update_check: Option<std::time::Instant>,
    /// `{skill}::{agent id}` while a carousel install or toggle is in flight.
    pub(crate) pending_agents: HashSet<String>,
    pub(crate) error: Option<String>,
    /// GitHub 会话状态,频道空状态的登录卡消费。`None` = 尚未读到。
    pub(crate) github: Option<ss_skills::github_auth::GitHubConnectionStatus>,
    pub(crate) _subscription: Option<Subscription>,
    /// Bumped when Settings reloads profiles, so an in-flight `refresh`
    /// cannot paint the pre-toggle snapshot back over the carousel.
    profiles_epoch: u64,
    /// Data paints. Animation frames notify this page without bumping it,
    /// so the cached grid does not rebuild on every refresh tick.
    content_epoch: u64,
    /// Translation fill. Separate from [`Self::content_epoch`] so a finished
    /// batch repaints cards once.
    translation_epoch: u64,
    /// Virtual list offset. Grid and list mode must not share one offset.
    skills_scroll: UniformListScrollHandle,
    canvas: Option<Entity<canvas::SkillsCanvas>>,
}

impl MySkillsPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let this = Self {
            skills: Vec::new(),
            profiles: Vec::new(),
            scope: MySkillsScope::Local,
            sort_by: SortOption::Updated,
            source_filter: SourceFilter::All,
            repo_filter: None,
            reinstalling_repo: None,
            agent_filter: None,
            only_updates: false,
            view_list: false,
            search: None,
            search_query: String::new(),
            selected_skill: None,
            description_choices: ss_core::translation::load_config()
                .map(|config| config.description_choices)
                .unwrap_or_default(),
            uninstall_hover: false,
            selected_batch: HashSet::new(),
            link_menu_open: false,
            loading: true,
            busy: None,
            updating_batch: HashSet::new(),
            checking_updates: false,
            last_update_check: None,
            pending_agents: HashSet::new(),
            error: None,
            github: None,
            _subscription: None,
            profiles_epoch: 0,
            content_epoch: 0,
            translation_epoch: 0,
            skills_scroll: UniformListScrollHandle::new(),
            canvas: None,
        };
        // The entity is not inserted until `new` returns, so this must not
        // read `profiles_epoch` off the handle.
        Self::refresh_from(cx, 0);
        this
    }

    /// Refresh installed skills and agent profiles.
    ///
    /// Call from inside this page's `update`. The entity is already leased,
    /// so `cx.entity().read` panics; the epoch has to come off `self`.
    pub fn refresh(&self, cx: &mut Context<Self>) {
        Self::refresh_from(cx, self.profiles_epoch);
    }

    fn refresh_from(cx: &mut Context<Self>, profiles_epoch: u64) {
        let view = cx.entity();
        let fut = async {
            let skills_res = ss_skills::installed_skill::list_installed_skills().await;
            let profiles = tokio::task::spawn_blocking(ss_skills::agents::list_profiles)
                .await
                .unwrap_or_default();
            (skills_res, profiles)
        };

        spawn_domain(&view, cx, fut, move |this, cx, (skills_res, profiles)| {
            this.loading = false;
            // A Settings toggle may have landed while this listing was in
            // flight. Keep that newer snapshot; its SVG is already due.
            if this.profiles_epoch == profiles_epoch {
                this.apply_agent_profiles(profiles);
            }
            match skills_res {
                Ok(skills) => {
                    this.skills = skills;
                    this.error = None;
                    this.maybe_check_updates(cx);
                }
                Err(err) => {
                    this.error = Some(err.to_string());
                }
            }
            this.revise(cx);
        });

        // GitHub 会话:频道空状态的登录卡与登录对话框共用同一 facade,
        // 身份缓存在两次刷新之间存活,避免每次刷新都打一次 GitHub API。
        let auth = github_sign_in::shared_auth_facade();
        spawn_domain(
            &view,
            cx,
            async move { auth.status().await.map_err(anyhow::Error::new) },
            |this, cx, status| {
                if let Ok(status) = status {
                    this.github = Some(status);
                    this.revise(cx);
                }
            },
        );
    }

    /// Settings changed which agents are enabled. Synchronous: the carousel
    /// must include the brand SVG on the next skills paint, not after the
    /// installed-skill listing returns.
    pub(crate) fn reload_agent_profiles(&mut self, cx: &mut Context<Self>) {
        self.profiles_epoch = self.profiles_epoch.wrapping_add(1);
        self.apply_agent_profiles(ss_skills::agents::list_profiles());
        self.revise(cx);
    }

    pub(super) fn replay_epochs(&self) -> (u64, u64) {
        (self.content_epoch, self.translation_epoch)
    }

    /// Open or close the detail column. A different skill drops the uninstall
    /// hover, so the next button starts at rest instead of the previous face.
    /// A skill's description-translation choice is remembered: moving the
    /// drawer to another skill does not put a translated card back to
    /// English.
    pub(super) fn select_detail(&mut self, name: Option<String>) {
        if self.selected_skill != name {
            self.uninstall_hover = false;
        }
        self.selected_skill = name;
    }

    /// Detail-column uninstall hover. Notifies the canvas so the spring can
    /// step, and leaves the grid epoch alone.
    pub(super) fn set_uninstall_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.uninstall_hover == hovered {
            return;
        }
        self.uninstall_hover = hovered;
        if let Some(canvas) = self.canvas.clone() {
            canvas.update(cx, |_, cx| cx.notify());
        }
    }

    /// Page data changed. Animation frames call `cx.notify` on this page and
    /// must not come through here.
    pub(crate) fn revise(&mut self, cx: &mut Context<Self>) {
        self.content_epoch = self.content_epoch.wrapping_add(1);
        cx.notify();
    }

    /// Grid and list are different `uniform_list` ids. A leftover offset
    /// would open the other mode mid-list.
    pub(super) fn reset_list_scroll(&mut self) {
        self.skills_scroll = UniformListScrollHandle::new();
    }

    fn apply_agent_profiles(&mut self, profiles: Vec<AgentProfile>) {
        self.profiles = profiles;
        let filter_gone = self.agent_filter.as_ref().is_some_and(|id| {
            !self
                .profiles
                .iter()
                .any(|profile| profile.id == *id && profile.enabled)
        });
        if filter_gone {
            self.agent_filter = None;
        }
    }

    /// Create search InputState on first render.
    pub fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            return;
        }
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("toolbar.searchPlaceholder"))
        });
        self._subscription = Some(cx.subscribe_in(
            &search,
            window,
            |this, _state, event: &InputEvent, _window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some(search) = &this.search else { return };
                let query = search.read(cx).value().to_string();
                this.search_query = query;
                this.revise(cx);
            },
        ));
        self.search = Some(search);
    }

    pub fn sync_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = &self.search {
            crate::i18n::sync_placeholder(
                search,
                crate::i18n::t("toolbar.searchPlaceholder"),
                window,
                cx,
            );
        }
        self.revise(cx);
    }

    /// Clears search query and all active filters.
    pub fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.search_query.clear();
        self.source_filter = SourceFilter::All;
        self.repo_filter = None;
        self.agent_filter = None;
        self.only_updates = false;
        self.revise(cx);
    }
}

impl MySkillsPage {
    /// Settings changed a translation switch. The canvas watches this epoch
    /// and resyncs, so the next paint reads the cache or queues new lines.
    pub(crate) fn note_translation_prefs(&mut self, cx: &mut Context<Self>) {
        self.translation_epoch = self.translation_epoch.wrapping_add(1);
        cx.notify();
    }
}

impl crate::translation::TranslationHost for MySkillsPage {
    fn translations_arrived(&mut self) {
        self.translation_epoch = self.translation_epoch.wrapping_add(1);
    }
}

pub(super) fn description_source(skill: &Skill) -> Option<&str> {
    let text = skill
        .localized_description
        .as_deref()
        .filter(|text| !text.is_empty())
        .unwrap_or(skill.description.as_str())
        .trim();
    if text.is_empty() { None } else { Some(text) }
}

impl Render for MySkillsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        let canvas = self.ensure_canvas(cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .size_full()
                    .debug_selector(|| "skills-body".into())
                    .child(crate::chrome::replay_view(canvas.into())),
            )
    }
}
impl EventEmitter<SelectPage> for MySkillsPage {}

impl EventEmitter<GroupsChanged> for MySkillsPage {}

#[cfg(test)]
pub(super) mod test_support {
    pub(super) use crate::test_support::{IsolatedDataDir, data_dir_lock};
}
