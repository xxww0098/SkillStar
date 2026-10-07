//! Skill Cards — deck list (`skill_group::list_groups`). React source:
//! `src/pages/SkillCards.tsx` + `features/my-skills/components/DeckCard.tsx`.

mod deck_import;
mod group_card;
mod query;

use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;
use ss_skills::agents::{AgentProfile, list_profiles};
use ss_skills::skill_group::{SkillGroup, create_group, list_groups};
use ss_skills::workflows::agent_links::AgentLinkReport;
use ss_skills::workflows::skill_group_links::{link_deck_to_enabled_agents, set_deck_agent};

use crate::chrome::{
    InteractionSpring, MotionPaint, bar_count, bar_primary, bar_secondary, icon, page_chrome,
    page_toolbar, segment_track, toolbar_search, view_toggle_button,
};
use crate::layout::CARD_ROW_W;
use crate::nav::NavPage;
use crate::spawn_domain;
use crate::theme::palette;

use self::query::matches_deck_query;

fn load_installed_skills() -> HashSet<String> {
    ss_skills::installer::installed_names()
        .into_iter()
        .collect()
}

/// Decks (skill groups) read straight from `skill_group::list_groups`.
pub struct SkillCardsPage {
    groups: Vec<SkillGroup>,
    installed_skills: HashSet<String>,
    profiles: Vec<AgentProfile>,
    search: Option<Entity<InputState>>,
    search_query: String,
    view_list: bool,
    import_notice: Option<String>,
    expanded: Option<String>,
    _subscription: Option<Subscription>,
}

impl SkillCardsPage {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            groups: list_groups(),
            installed_skills: load_installed_skills(),
            profiles: list_profiles()
                .into_iter()
                .filter(|p| p.has_global_skills())
                .collect(),
            search: None,
            search_query: String::new(),
            view_list: false,
            import_notice: None,
            expanded: None,
            _subscription: None,
        }
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.groups = list_groups();
        self.installed_skills = load_installed_skills();
        self.profiles = list_profiles()
            .into_iter()
            .filter(|p| p.has_global_skills())
            .collect();
        cx.notify();
    }

    fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            return;
        }
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("skillCards.searchPlaceholder"))
        });
        self._subscription = Some(cx.subscribe_in(
            &search,
            window,
            |this, _state, event: &InputEvent, _window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some(search) = &this.search else { return };
                this.search_query = search.read(cx).value().to_string();
                cx.notify();
            },
        ));
        self.search = Some(search);
    }

    pub fn sync_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = &self.search {
            crate::i18n::sync_placeholder(
                search,
                crate::i18n::t("skillCards.searchPlaceholder"),
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn toggle_agent_for_deck(
        &mut self,
        group: &SkillGroup,
        agent_id: &str,
        cx: &mut Context<Self>,
    ) {
        let enable = !group
            .agent_links
            .as_ref()
            .is_some_and(|links| links.iter().any(|id| id == agent_id));
        let group_id = group.id.clone();
        let agent_id = agent_id.to_string();
        self.run_link_job(cx, move || set_deck_agent(&group_id, &agent_id, enable));
    }

    fn deploy_all_for_deck(&mut self, group: &SkillGroup, cx: &mut Context<Self>) {
        let group_id = group.id.clone();
        self.run_link_job(cx, move || link_deck_to_enabled_agents(&group_id));
    }

    fn run_link_job(
        &mut self,
        cx: &mut Context<Self>,
        job: impl FnOnce() -> anyhow::Result<AgentLinkReport> + Send + 'static,
    ) {
        self.import_notice = None;
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    // Deck-rail clicks already left the UI thread. Do not wait
                    // on the Skill transaction; show OperationInProgress instead.
                    let _guard = ss_skills::skill_update::try_acquire_update_transaction_lock(
                        std::time::Duration::ZERO,
                    )?;
                    job()
                })
                .await
                .map_err(anyhow::Error::new)
                .and_then(|result| result)
            },
            |this, cx, result: anyhow::Result<AgentLinkReport>| {
                this.import_notice = match result {
                    Ok(report) => crate::notify::agent_link_notice(&report),
                    Err(err) => Some(format!("{err:#}")),
                };
                this.refresh(cx);
            },
        );
    }

    pub(crate) fn import_share_code(&mut self, text: String, cx: &mut Context<Self>) {
        self.run_import(cx, move || deck_import::apply_share_code(&text));
    }

    pub(crate) fn import_bundle_path(&mut self, path: String, cx: &mut Context<Self>) {
        self.run_import(cx, move || deck_import::apply_bundle(&path));
    }

    fn run_import(
        &mut self,
        cx: &mut Context<Self>,
        job: impl FnOnce() -> anyhow::Result<Option<String>> + Send + 'static,
    ) {
        self.import_notice = None;
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(job)
                    .await
                    .map_err(anyhow::Error::new)
                    .and_then(|result| result)
            },
            |this, cx, result: anyhow::Result<Option<String>>| {
                this.import_notice = result.unwrap_or_else(|err| Some(format!("{err:#}")));
                this.refresh(cx);
            },
        );
    }
}

impl Render for SkillCardsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        let view = cx.entity().downgrade();

        let mut toolbar =
            page_toolbar(crate::i18n::t("sidebar.groups")).drag_id("skill-cards-toolbar-drag");

        if let Some(search) = &self.search {
            toolbar = toolbar.search(toolbar_search(search, 224.0));
        }

        let filtered_groups: Vec<&SkillGroup> = self
            .groups
            .iter()
            .filter(|g| matches_deck_query(g, &self.search_query))
            .collect();

        toolbar = toolbar.filter(bar_count(
            IconName::Layers,
            filtered_groups.len().to_string(),
        ));

        if let Some(notice) = &self.import_notice {
            toolbar = toolbar.action(
                div()
                    .max_w(px(180.0))
                    .text_xs()
                    .text_color(rgb(palette().danger))
                    .child(notice.clone()),
            );
        }

        let import_view = view.clone();
        toolbar = toolbar.action(
            bar_secondary(
                "skill-cards-import",
                IconName::Download,
                crate::i18n::t("common.import"),
            )
            .on_click(move |_, window, cx| {
                deck_import::open_share_import(import_view.clone(), window, cx);
            }),
        );
        let file_view = view.clone();
        toolbar = toolbar.action(
            bar_secondary(
                "skill-cards-import-file",
                IconName::Package,
                crate::i18n::t("toolbar.importFile"),
            )
            .on_click(move |_, window, cx| {
                deck_import::open_bundle_import(file_view.clone(), window, cx);
            }),
        );

        let new_deck_view = view.clone();
        toolbar = toolbar.action(
            bar_primary(
                "skill-cards-new",
                IconName::Plus,
                crate::i18n::t("skillCards.newGroup"),
            )
            .on_click(move |_, window, cx| {
                let view = new_deck_view.clone();
                let name_input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(crate::i18n::t("createGroupModal.groupName"))
                });
                let desc_input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(crate::i18n::t("createGroupModal.description"))
                });
                let icon_input = cx.new(|cx| {
                    InputState::new(window, cx).placeholder("Emoji icon (e.g. 📦, ⚡, 🚀, 🧠)")
                });
                let name_ok = name_input.clone();
                let desc_ok = desc_input.clone();
                let icon_ok = icon_input.clone();
                crate::chrome::open_form_dialog(
                    window,
                    cx,
                    crate::i18n::t("createGroupModal.newGroup"),
                    crate::i18n::t("createGroupModal.create"),
                    false,
                    240.0,
                    move |_, _| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(Input::new(&name_input))
                            .child(Input::new(&desc_input))
                            .child(Input::new(&icon_input))
                            .into_any_element()
                    },
                    move |_, cx| {
                        let name = name_ok.read(cx).value().trim().to_string();
                        let desc = desc_ok.read(cx).value().trim().to_string();
                        let icon = icon_ok.read(cx).value().trim().to_string();
                        if name.is_empty() {
                            return false;
                        }
                        let final_icon = if icon.is_empty() {
                            "📦".to_string()
                        } else {
                            icon
                        };
                        let _ = view.update(cx, |this, cx| {
                            if let Ok(_) =
                                create_group(name, desc, final_icon, Vec::new(), Default::default())
                            {
                                this.refresh(cx);
                            }
                        });
                        true
                    },
                );
            }),
        );
        let grid_view = view.clone();
        let list_view = view.clone();
        let view_list = self.view_list;
        toolbar = toolbar.action(
            segment_track()
                .child(
                    view_toggle_button("skill-cards-view-grid", IconName::LayoutGrid, !view_list)
                        .on_click(move |_, _, cx| {
                            let _ = grid_view.update(cx, |this, cx| {
                                this.view_list = false;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    view_toggle_button("skill-cards-view-list", IconName::List, view_list)
                        .on_click(move |_, _, cx| {
                            let _ = list_view.update(cx, |this, cx| {
                                this.view_list = true;
                                cx.notify();
                            });
                        }),
                ),
        );
        let toolbar = toolbar.build();

        // Pane fills under the toolbar. Empty states are a flex_1 child of
        // this column (not of a wrap). The deck wrap is not flex_1: a
        // wrapping line with default align-content stretch grows one card
        // to the full pane height.
        let mut pane = div().flex().flex_col().flex_1().min_h_0().w_full();

        if self.groups.is_empty() {
            let empty_view = view.clone();
            pane = pane.child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(56.0))
                            .rounded_2xl()
                            .bg(rgb(palette().accent_soft))
                            .border_1()
                            .border_color(rgb(palette().accent))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(NavPage::SkillCards.icon(), 28.0, palette().accent_fg)),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette().fg))
                            .child(crate::i18n::t("skillCards.emptyTitle")),
                    )
                    .child(
                        div()
                            .max_w(px(360.0))
                            .text_sm()
                            .text_align(TextAlign::Center)
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("skillCards.emptyDesc")),
                    )
                    .child(
                        div()
                            .id("empty-create-deck-btn")
                            .px_4()
                            .py(px(8.0))
                            .rounded_lg()
                            .bg(rgb(palette().accent))
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().on_accent))
                            .cursor_pointer()
                            .interaction_spring(
                                "empty-create-deck-btn",
                                true,
                                MotionPaint::new().opacity(1.0),
                                MotionPaint::new().opacity(0.9),
                            )
                            .child(crate::i18n::t("skillCards.createFirst"))
                            .on_click(move |_, window, cx| {
                                let view = empty_view.clone();
                                let name_input = cx.new(|cx| {
                                    InputState::new(window, cx)
                                        .placeholder(crate::i18n::t("createGroupModal.groupName"))
                                });
                                let desc_input = cx.new(|cx| {
                                    InputState::new(window, cx)
                                        .placeholder(crate::i18n::t("createGroupModal.description"))
                                });
                                let name_ok = name_input.clone();
                                let desc_ok = desc_input.clone();
                                crate::chrome::open_form_dialog(
                                    window,
                                    cx,
                                    crate::i18n::t("createGroupModal.newGroup"),
                                    crate::i18n::t("createGroupModal.create"),
                                    false,
                                    200.0,
                                    move |_, _| {
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(Input::new(&name_input))
                                            .child(Input::new(&desc_input))
                                            .into_any_element()
                                    },
                                    move |_, cx| {
                                        let name = name_ok.read(cx).value().trim().to_string();
                                        let desc = desc_ok.read(cx).value().trim().to_string();
                                        if name.is_empty() {
                                            return false;
                                        }
                                        let _ = view.update(cx, |this, cx| {
                                            if let Ok(_) = create_group(
                                                name,
                                                desc,
                                                "📦".into(),
                                                Vec::new(),
                                                Default::default(),
                                            ) {
                                                this.refresh(cx);
                                            }
                                        });
                                        true
                                    },
                                );
                            }),
                    ),
            );
        } else if filtered_groups.is_empty() {
            pane = pane.child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(icon(IconName::Search, 32.0, palette().fg_muted))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(crate::i18n::t("skillCards.noMatching")),
                    )
                    .child(
                        div()
                            .max_w(px(360.0))
                            .text_xs()
                            .text_align(TextAlign::Center)
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("skillCards.tryDifferent")),
                    ),
            );
        } else {
            // Shared skill-card box. The wrap stops at one card row.
            let mut grid = div()
                .w_full()
                .max_w(px(CARD_ROW_W))
                .flex_grow_0()
                .flex_shrink_0()
                .flex()
                .items_start()
                .content_start()
                .justify_start();
            grid = if self.view_list {
                grid.flex_col().gap(px(8.0))
            } else {
                grid.flex_row().flex_wrap().gap(px(16.0))
            };
            for group in filtered_groups {
                grid = grid.child(self.render_group_card(group, view.clone()));
            }
            pane = pane.child(
                div()
                    .flex()
                    .flex_col()
                    .justify_start()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scrollbar()
                    .p(px(20.0))
                    .child(
                        div()
                            .w_full()
                            .flex_grow_0()
                            .flex_shrink_0()
                            .flex()
                            .flex_row()
                            .justify_center()
                            .items_start()
                            .child(grid),
                    ),
            );
        }

        page_chrome(toolbar, pane)
    }
}
