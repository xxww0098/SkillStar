//! Settings — the Tauri page, in GPUI.
//!
//! React source: `src/pages/Settings.tsx` and `SettingsSidebarNav.tsx`.
//! One scroll column holds every section (max 720px). The rail is a card
//! in the left gutter, shifted by half the column inset so the gap beside
//! the sidebar matches the gap before the section content. A click scrolls
//! that section to the top and the highlight follows the scroll position.
//! Section chrome matches
//! `SettingsSectionHeader`: primary icon well, title, optional meta chip
//! and action. Bodies are cards, not stretched panes.

mod about;
mod agent_connections;
mod general;
mod gh_mirror;
mod managed_skills;
mod network_doctor;
mod parts;
mod skill_repair;
mod skill_updates;
mod storage;
mod translation;
#[cfg(test)]
mod translation_tests;

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::combobox::{ComboboxEvent, ComboboxState};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::switch::Switch;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_app::storage_maintenance::StorageOverview;
use ss_core::config::github_mirror::{self as core_gh_mirror, GitHubMirrorConfig};
use ss_core::config::network_doctor::NetworkDiagnosis;
use ss_core::config::proxy::{ProxyConfig, load_config};
use ss_core::config::skill_updates::SkillUpdateConfig;
use ss_core::infra::paths;
use ss_skills::agents::{self as agent_registry, AgentProfile};
use ss_skills::git::gh_manager::{self, GitStatus, check_git_status};

use crate::chrome::{InteractionSpring, MotionPaint, page_chrome, page_toolbar};
use crate::prefs::{self, GuiPrefs};
use crate::theme::palette;

pub(crate) use parts::{
    card, choice_pills, collapse_chevron, field_label, format_bytes, meta_chip, section_shell,
};

/// One rail entry per Settings section. Order matches
/// `SETTINGS_SECTIONS` in `SettingsSidebarNav.tsx`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SettingsSection {
    #[default]
    AgentConnections,
    Proxy,
    GitHubMirror,
    NetworkDoctor,
    BackgroundRun,
    SkillUpdates,
    Appearance,
    Language,
    Translation,
    Storage,
    About,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 11] = [
        SettingsSection::AgentConnections,
        SettingsSection::Proxy,
        SettingsSection::GitHubMirror,
        SettingsSection::NetworkDoctor,
        SettingsSection::BackgroundRun,
        SettingsSection::SkillUpdates,
        SettingsSection::Appearance,
        SettingsSection::Language,
        SettingsSection::Translation,
        SettingsSection::Storage,
        SettingsSection::About,
    ];

    fn dom_id(self) -> &'static str {
        match self {
            SettingsSection::AgentConnections => "settings-agents",
            SettingsSection::Proxy => "settings-proxy",
            SettingsSection::GitHubMirror => "settings-mirror",
            SettingsSection::NetworkDoctor => "settings-network-doctor",
            SettingsSection::BackgroundRun => "settings-background",
            SettingsSection::SkillUpdates => "settings-skill-updates",
            SettingsSection::Appearance => "settings-appearance",
            SettingsSection::Language => "settings-language",
            SettingsSection::Translation => "settings-translation",
            SettingsSection::Storage => "settings-storage",
            SettingsSection::About => "settings-about",
        }
    }

    fn label(self) -> SharedString {
        crate::i18n::t(match self {
            SettingsSection::AgentConnections => "settings.agentConnections",
            SettingsSection::Proxy => "settings.networkProxy",
            SettingsSection::GitHubMirror => "settings.githubMirror",
            SettingsSection::NetworkDoctor => "settings.networkDoctor",
            SettingsSection::BackgroundRun => "settings.backgroundRun",
            SettingsSection::SkillUpdates => "settings.skillUpdates",
            SettingsSection::Appearance => "settings.backgroundStyle",
            SettingsSection::Language => "settings.language",
            SettingsSection::Translation => "settings.translation",
            SettingsSection::Storage => "settings.storage",
            SettingsSection::About => "settings.about",
        })
    }

    /// Lucide picks from `SettingsSidebarNav.tsx`.
    fn icon(self) -> IconName {
        match self {
            SettingsSection::AgentConnections => IconName::Unlink,
            SettingsSection::Proxy => IconName::Globe,
            SettingsSection::GitHubMirror => IconName::Zap,
            SettingsSection::NetworkDoctor => IconName::Activity,
            SettingsSection::BackgroundRun => IconName::EyeOff,
            SettingsSection::SkillUpdates => IconName::RefreshCw,
            SettingsSection::Appearance => IconName::Paintbrush,
            SettingsSection::Language => IconName::Languages,
            SettingsSection::Translation => IconName::FileText,
            SettingsSection::Storage => IconName::HardDrive,
            SettingsSection::About => IconName::Terminal,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|item| *item == self).unwrap_or(0)
    }
}

/// Activation narrowing for the agent list. Same three values as
/// `AgentStatusFilter` in `agentFilters.ts`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AgentFilter {
    #[default]
    All,
    Enabled,
    Disabled,
}

/// One force-delete target in the Storage section.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeleteTarget {
    Hub,
    Cache,
}

/// Direct child of the settings scroller. One slot per section so
/// `ScrollHandle::scroll_to_top_of_item` lines up with `SettingsSection::ALL`.
fn section_slot(body: impl IntoElement) -> Div {
    div()
        .w_full()
        .flex_none()
        .pb(px(32.0))
        .flex()
        .justify_center()
        .child(div().w_full().max_w(px(720.0)).child(body))
}

pub struct SettingsPage {
    pub(crate) section: SettingsSection,
    scroll: ScrollHandle,
    pub(crate) git_status: Option<GitStatus>,
    pub(crate) gh_installed: bool,
    pub(crate) data_root: String,
    pub(crate) copied: Option<String>,
    pub(crate) release_check: Option<ss_core::infra::release_check::ReleaseCheckRecord>,
    pub(crate) release_checking: bool,
    pub(crate) prefs: GuiPrefs,
    pub(crate) proxy: ProxyConfig,
    pub(crate) proxy_host: Entity<InputState>,
    pub(crate) proxy_port: Entity<InputState>,
    pub(crate) proxy_user: Entity<InputState>,
    pub(crate) proxy_pass: Entity<InputState>,
    pub(crate) proxy_bypass: Entity<InputState>,
    pub(crate) proxy_status: Option<String>,
    pub(crate) proxy_expanded: bool,
    pub(crate) proxy_type_open: bool,
    pub(crate) gh_mirror: GitHubMirrorConfig,
    pub(crate) gh_mirror_status: Option<String>,
    pub(crate) gh_mirror_custom_url: Entity<InputState>,
    pub(crate) gh_expanded: bool,
    pub(crate) gh_testing_id: Option<String>,
    pub(crate) gh_test_results: HashMap<String, Result<u64, String>>,
    pub(crate) diagnosis: Option<NetworkDiagnosis>,
    pub(crate) diagnosing: bool,
    pub(crate) diagnosis_error: Option<String>,
    pub(crate) storage: Option<StorageOverview>,
    pub(crate) storage_loading: bool,
    pub(crate) storage_busy: Option<&'static str>,
    pub(crate) storage_confirm: Option<DeleteTarget>,
    pub(crate) cleaning: bool,
    pub(crate) repairing: bool,
    pub(crate) previewing: bool,
    pub(crate) adopting: bool,
    pub(crate) previewing_intake: bool,
    pub(crate) path_structure_open: bool,
    pub(crate) storage_status: Option<String>,
    pub(crate) profiles: Vec<AgentProfile>,
    pub(crate) expanded_agent: Option<String>,
    pub(crate) linked_skills: HashMap<String, Vec<String>>,
    pub(crate) managed: managed_skills::ManagedSkillsUi,
    pub(crate) skill_updates: SkillUpdateConfig,
    pub(crate) translation: ss_core::translation::TranslationConfig,
    pub(crate) reader_styles_open: bool,
    /// Model ids from the last successful fetch. Kept for `llm_models_for`.
    pub(crate) llm_models: Vec<String>,
    pub(crate) llm_models_for: String,
    pub(crate) llm_models_loading: bool,
    pub(crate) llm_models_error: Option<String>,
    /// Model dropdown state. Items and selection are re-synced from
    /// `llm_models` during render — async fetch callbacks carry no `Window`.
    pub(crate) llm_model_state: Entity<ComboboxState<SearchableVec<String>>>,
    /// Bumped after each successful fetch so the render sync notices refetches
    /// that leave the account, count, and current model unchanged.
    pub(crate) llm_fetch_seq: u64,
    /// Sync key of the combobox items/selection last applied.
    pub(crate) llm_models_synced: Option<String>,
    pub(crate) agent_search: Entity<InputState>,
    pub(crate) agent_query: String,
    pub(crate) agent_filter: AgentFilter,
    pub(crate) show_all_agents: bool,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let proxy = load_config().unwrap_or_default();
        let gh_mirror_cfg = core_gh_mirror::load_config().unwrap_or_default();
        let proxy_host = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("127.0.0.1")
                .default_value(proxy.host.clone())
        });
        let proxy_port = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("7897")
                .default_value(proxy.port.to_string())
        });
        let proxy_user = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(crate::i18n::t("common.optional"))
                .default_value(proxy.username.clone().unwrap_or_default())
        });
        let proxy_pass = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(crate::i18n::t("common.optional"))
                .default_value(proxy.password.clone().unwrap_or_default())
        });
        let proxy_bypass = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(crate::i18n::t("settings.proxyBypassPlaceholder"))
                .default_value(proxy.bypass.clone().unwrap_or_default())
        });
        let gh_mirror_custom_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://your-mirror.example/")
                .default_value(gh_mirror_cfg.custom_url.clone().unwrap_or_default())
        });
        let agent_search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("settings.searchAgents"))
        });
        let mut translation_config = ss_core::translation::load_config().unwrap_or_default();
        if translation_config.engine == ss_core::translation::Engine::Llm
            && translation::ensure_llm_default(&mut translation_config)
        {
            let _ = ss_core::translation::save_config(&translation_config);
        }
        let llm_model_state = cx.new(|cx| {
            ComboboxState::new(
                SearchableVec::new(Vec::<String>::new()),
                Vec::new(),
                window,
                cx,
            )
            .searchable(true)
        });

        let mut subscriptions = Vec::new();
        for input in [
            &proxy_host,
            &proxy_port,
            &proxy_user,
            &proxy_pass,
            &proxy_bypass,
        ] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => {
                        this.proxy_status = None;
                        cx.notify();
                    }
                    InputEvent::Blur => this.save_proxy(cx),
                    _ => {}
                },
            ));
        }
        subscriptions.push(cx.subscribe_in(
            &gh_mirror_custom_url,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.save_gh_mirror(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &agent_search,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.agent_query = this.agent_search.read(cx).value().to_string();
                    cx.notify();
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &llm_model_state,
            window,
            |this,
             _: &Entity<ComboboxState<SearchableVec<String>>>,
             event: &ComboboxEvent<SearchableVec<String>>,
             _,
             cx| {
                if let ComboboxEvent::Change(values) = event
                    && let Some(model) = values.first()
                {
                    this.select_llm_model(model.clone(), cx);
                }
            },
        ));
        let mut page = Self {
            section: SettingsSection::AgentConnections,
            scroll: ScrollHandle::new(),
            git_status: Some(check_git_status()),
            gh_installed: gh_manager::is_gh_installed(),
            data_root: paths::data_root().display().to_string(),
            copied: None,
            // Sync domain call, same shape as `check_git_status` above: one
            // tiny local read, no spawn — constructors stay deterministic
            // for the strict test scheduler.
            release_check: ss_core::infra::release_check::last_record(),
            release_checking: false,
            prefs: prefs::load(),
            proxy_status: None,
            proxy_expanded: false,
            proxy_type_open: false,
            proxy_host,
            proxy_port,
            proxy_user,
            proxy_pass,
            proxy_bypass,
            gh_mirror_custom_url,
            agent_search,
            proxy,
            gh_mirror: gh_mirror_cfg,
            gh_mirror_status: None,
            gh_expanded: false,
            gh_testing_id: None,
            gh_test_results: HashMap::new(),
            diagnosis: None,
            diagnosing: false,
            diagnosis_error: None,
            storage: None,
            storage_loading: false,
            storage_busy: None,
            storage_confirm: None,
            cleaning: false,
            repairing: false,
            previewing: false,
            adopting: false,
            previewing_intake: false,
            path_structure_open: false,
            storage_status: None,
            profiles: agent_registry::list_profiles(),
            expanded_agent: None,
            linked_skills: HashMap::new(),
            managed: managed_skills::ManagedSkillsUi::default(),
            skill_updates: ss_core::config::skill_updates::load_config().unwrap_or_default(),
            translation: translation_config,
            reader_styles_open: true,
            llm_models: Vec::new(),
            llm_models_for: String::new(),
            llm_models_loading: false,
            llm_models_error: None,
            llm_model_state,
            llm_fetch_seq: 0,
            llm_models_synced: None,
            agent_query: String::new(),
            agent_filter: AgentFilter::All,
            show_all_agents: false,
            _subscriptions: subscriptions,
        };
        page.load_storage(cx);
        page.preload_managed_skills(cx);
        page
    }

    fn focus_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.section = section;
        self.scroll.scroll_to_top_of_item(section.index());
        cx.notify();
    }

    fn sync_active_from_scroll(&mut self, cx: &mut Context<Self>) {
        let max = self.scroll.max_offset().y;
        let y = self.scroll.offset().y;
        let at_bottom = max > px(24.0) && max + y < px(24.0);
        let index = if at_bottom {
            SettingsSection::ALL.len() - 1
        } else {
            self.scroll.top_item()
        };
        let Some(section) = SettingsSection::ALL.get(index).copied() else {
            return;
        };
        if self.section != section {
            self.section = section;
            cx.notify();
        }
    }

    pub(crate) fn save_prefs(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = prefs::save(&self.prefs) {
            tracing::warn!("failed to save gui prefs: {err}");
        }
        cx.notify();
    }

    pub(crate) fn sync_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::i18n::sync_placeholder(
            &self.proxy_user,
            crate::i18n::t("common.optional"),
            window,
            cx,
        );
        crate::i18n::sync_placeholder(
            &self.proxy_pass,
            crate::i18n::t("common.optional"),
            window,
            cx,
        );
        crate::i18n::sync_placeholder(
            &self.proxy_bypass,
            crate::i18n::t("settings.proxyBypassPlaceholder"),
            window,
            cx,
        );
        crate::i18n::sync_placeholder(
            &self.agent_search,
            crate::i18n::t("settings.searchAgents"),
            window,
            cx,
        );
        cx.notify();
    }

    fn render_nav(&self, view: WeakEntity<Self>, labels: bool) -> impl IntoElement {
        let mut rail = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(6.0))
            .py_3()
            .rounded_xl()
            .border_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().card))
            .when(labels, |d| d.w(px(176.0)))
            .when(!labels, |d| d.w(px(48.0)));

        for section in SettingsSection::ALL {
            let active = section == self.section;
            let color = if active {
                palette().accent
            } else {
                palette().fg_muted
            };
            let v = view.clone();
            rail = rail.child(
                div()
                    .id(ElementId::Name(section.dom_id().into()))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(px(10.0))
                    .h(px(36.0))
                    .w_full()
                    .rounded_xl()
                    .cursor_pointer()
                    .flex_none()
                    .when(labels, |d| d.justify_start().px_3())
                    .when(active, |d| d.bg(rgb(palette().accent_soft)))
                    .child(
                        Icon::new(section.icon())
                            .size(px(18.0))
                            .text_color(rgb(color)),
                    )
                    .when(labels, |d| {
                        d.child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(color))
                                .child(section.label()),
                        )
                    })
                    .on_click(move |_, _, cx| {
                        let _ = v.update(cx, |this, cx| this.focus_section(section, cx));
                    })
                    .interaction_spring(
                        section.dom_id(),
                        true,
                        if active {
                            MotionPaint::new()
                                .bg(rgb(palette().accent_soft))
                                .fg(rgb(color))
                        } else {
                            MotionPaint::new().fg(rgb(color))
                        },
                        if active {
                            MotionPaint::new()
                                .bg(rgb(palette().accent_soft))
                                .fg(rgb(color))
                        } else {
                            MotionPaint::new()
                                .bg(rgb(palette().card_hover))
                                .fg(rgb(palette().fg))
                        },
                    ),
            );
        }
        rail
    }

    /// Shared kit Switch used by several sections. The click stops
    /// propagation: the proxy and GitHub-mirror cards sit the switch inside
    /// a clickable collapse header.
    pub(crate) fn toggle(
        id: &str,
        enabled: bool,
        view: WeakEntity<Self>,
        apply: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Switch {
        Switch::new(ElementId::Name(id.to_string().into()))
            .checked(enabled)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let _ = view.update(cx, |this, cx| {
                    apply(this, cx);
                    cx.notify();
                });
            })
    }
}

impl EventEmitter<crate::nav::AgentsChanged> for SettingsPage {}

impl EventEmitter<crate::nav::TranslationPrefsChanged> for SettingsPage {}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width;
        let show_nav = width >= px(1024.0);
        let nav_labels = width >= px(1280.0);
        let view = cx.entity().downgrade();
        if self.translation.engine == ss_core::translation::Engine::Llm
            && translation::llm_default_needs_apply(&self.translation)
        {
            let page = view.clone();
            cx.defer(move |cx| {
                let _ = page.update(cx, |this, cx| {
                    if translation::ensure_llm_default(&mut this.translation) {
                        this.save_translation(cx);
                    }
                });
            });
        }

        let sections = [
            self.render_agents(view.clone()).into_any_element(),
            self.render_proxy(view.clone()).into_any_element(),
            self.render_github_mirror(view.clone()).into_any_element(),
            self.render_network_doctor(view.clone()).into_any_element(),
            self.render_background_run(view.clone()).into_any_element(),
            self.render_skill_updates(view.clone()).into_any_element(),
            self.render_appearance(view.clone()).into_any_element(),
            self.render_language(view.clone()).into_any_element(),
            self.render_translation(window, cx, view.clone())
                .into_any_element(),
            self.render_storage(view.clone()).into_any_element(),
            self.render_about(view.clone()).into_any_element(),
        ];

        let mut column = div()
            .id("settings-scroll")
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px_6()
            .py_6();
        for section in sections {
            column = column.child(section_slot(section));
        }

        let spy = view.clone();
        let handle = self.scroll.clone();
        column = column.on_scroll_wheel(move |_, window, _cx| {
            let spy = spy.clone();
            let handle = handle.clone();
            window.on_next_frame(move |_, cx| {
                let _ = handle.offset();
                let _ = spy.update(cx, |this, cx| this.sync_active_from_scroll(cx));
            });
        });

        let scroller = div()
            .relative()
            .h_full()
            .min_h_0()
            .when(show_nav, |d| d.w(px(768.0)).flex_none())
            .when(!show_nav, |d| d.flex_1().min_w_0())
            .child(column)
            .child(Scrollbar::vertical(&self.scroll));

        let mut row = div().flex().flex_row().flex_1().min_h_0().w_full();
        if show_nav {
            row = row.child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        // Column uses px_6 (1.5rem). Shift by half of that
                        // so the sidebar gap matches the gap before the
                        // visible sections. Relative offset does not move
                        // the content column.
                        div().left_3().child(self.render_nav(view, nav_labels)),
                    ),
            );
        }
        row = row.child(scroller);
        if show_nav {
            row = row.child(div().flex_1().h_full());
        }

        page_chrome(
            page_toolbar(crate::i18n::t("settings.title"))
                .drag_id("settings-toolbar-drag")
                .build(),
            row,
        )
    }
}
