//! Right-hand detail column for one installed skill.
//!
//! The column wears the shared [`crate::chrome::sheet`] surface: it floats
//! on an inset ring beside the card grid, rounded and shadowed like the
//! app's dialogs, instead of docking to the window edge.
//!
//! Each fact appears once, in full. The install path is not shown: opening
//! the folder is the action. The whole SKILL.md is the dialog opened by
//! "View SKILL.md…".

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::{Skill, SkillType, UpstreamChange};

use super::MySkillsPage;
use super::detail_facts;
use super::detail_parts::{
    action_button, badge, fact_row, field_label, section, source_line, upstream_block,
};
use super::skill_reader::open_skill_reader;

use crate::chrome::{InteractionSpring, MotionDiv, MotionPaint, icon_sweep};
use crate::theme::palette;

/// Track the card grid reserves for the floating sheet (the sheet itself
/// paints [`crate::chrome::SHEET_GAP`] narrower on each side). The track is
/// one card plus one gap ([`crate::layout::DETAIL_COLUMN_W`]), so opening
/// the column removes exactly one grid column: at the default window the
/// grid holds two whole columns with no remainder. The canvas subtracts
/// this when it lays out columns, since the sheet's mount is a sibling of
/// the card scroller.
pub(crate) const DRAWER_W: f32 = crate::layout::DETAIL_COLUMN_W;

impl MySkillsPage {
    /// Renders the right-hand detail column when a skill is selected.
    pub fn render_detail_drawer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(selected_name) = &self.selected_skill else {
            return div().into_any_element();
        };
        let Some(skill) = self.skills.iter().find(|s| &s.name == selected_name) else {
            return div().into_any_element();
        };

        let view = cx.entity().downgrade();
        let name = skill.name.clone();
        let is_busy = self.busy.as_deref() == Some(name.as_str());
        let updating = self.skill_update_in_flight(skill);

        div()
            .id("skill-detail-drawer")
            .w(px(DRAWER_W))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .min_h_0()
            .p(px(crate::chrome::SHEET_GAP))
            .child(
                crate::chrome::sheet(div())
                    .child(self.drawer_header(skill, view.clone()))
                    .child(self.drawer_body(skill, view.clone(), cx))
                    .child(self.drawer_actions(skill, view, is_busy, updating)),
            )
            .into_any_element()
    }

    fn drawer_header(&self, skill: &Skill, view: WeakEntity<Self>) -> impl IntoElement {
        let is_local = skill.skill_type == SkillType::Local;
        let name = skill.name.clone();
        div()
            .flex()
            .items_start()
            .justify_between()
            .gap_2()
            .flex_shrink_0()
            .px_4()
            .pt_4()
            .pb_3()
            .border_b_1()
            .border_color(rgb(palette().border))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .text_base()
                            .font_bold()
                            .text_color(rgb(palette().fg))
                            .whitespace_normal()
                            .child(detail_facts::wrap_long_token(&name)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .when(is_local, |row| {
                                row.child(badge(
                                    palette().ok_bg,
                                    palette().ok,
                                    crate::i18n::t("detailPanel.localCreation"),
                                ))
                            })
                            .when(!is_local, |row| {
                                row.child(badge(
                                    palette().well,
                                    palette().fg_muted,
                                    crate::i18n::t("detailPanel.hubSkill"),
                                ))
                            })
                            .when(skill.update_available, |row| {
                                row.child(badge(
                                    palette().warn,
                                    palette().on_accent,
                                    crate::i18n::t("detailPanel.updateAvailable"),
                                ))
                            }),
                    ),
            )
            .child(
                div()
                    .id("close-detail-drawer")
                    .p_1()
                    .rounded_md()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .text_color(rgb(palette().fg_muted))
                    .tooltip(|window, cx| {
                        crate::chrome::tooltip(
                            crate::i18n::t("detailPanel.dismissDrawer").to_string(),
                        )
                        .build(window, cx)
                    })
                    .child(Icon::new(IconName::X).with_size(px(18.0)))
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.select_detail(None);
                            this.revise(cx);
                        });
                    })
                    .interaction_spring(
                        "close-detail-drawer",
                        true,
                        MotionPaint::new().fg(rgb(palette().fg_muted)),
                        MotionPaint::new()
                            .fg(rgb(palette().fg))
                            .bg(rgb(palette().card_hover)),
                    ),
            )
    }

    fn drawer_body(
        &self,
        skill: &Skill,
        view: WeakEntity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let translate = self
            .description_choices
            .get(&skill.name)
            .copied()
            .unwrap_or_else(|| {
                crate::translation::enabled(crate::translation::Surface::Description)
            });
        let (description_body, description_button) = match super::description_source(skill) {
            None => (
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg))
                    .whitespace_normal()
                    .child(crate::i18n::t("detailPanel.noDescription").to_string())
                    .into_any_element(),
                None,
            ),
            Some(source) => {
                if translate {
                    crate::translation::schedule_when(&cx.entity(), [source], true, cx);
                }
                let shown = crate::translation::display_when(source, translate);
                let translated = shown != source;
                // Descriptions never take the reader theme; the selector is
                // what tests key on for a translated opening.
                let row = div()
                    .text_xs()
                    .text_color(rgb(palette().fg))
                    .whitespace_normal()
                    .when(translated, |row| {
                        row.debug_selector(|| "drawer-translation".into())
                    })
                    .child(shown);
                (
                    row.into_any_element(),
                    self.translate_button(skill, source, translate, view.clone()),
                )
            }
        };
        div()
            .id("skill-detail-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_4()
            .child(section(
                crate::i18n::t("detailPanel.description").to_string(),
                description_body,
                description_button,
            ))
            .when_some(self.facts(skill), |body, facts| body.child(facts))
            .when_some(detail_facts::upstream_note(skill), |body, note| {
                body.child(upstream_block(note))
            })
            .child(self.agent_switches(skill, view))
    }

    /// Icon button beside the description label. Like the reader's button, a
    /// click never writes the Settings switch, but the choice is remembered
    /// per skill in the translation config: the card keeps the chosen
    /// language after the drawer moves on, and a restart reloads the choice.
    /// Chinese copy is not something the target would translate, so no
    /// button.
    fn translate_button(
        &self,
        skill: &Skill,
        source: &str,
        showing: bool,
        view: WeakEntity<Self>,
    ) -> Option<MotionDiv> {
        let target = ss_core::translation::active_target();
        if !ss_core::translation::needs_translation(source, target) {
            return None;
        }
        let pending = showing
            && crate::translation::blocked_hint().is_none()
            && !crate::translation::resolved(source);
        let tip = if showing {
            crate::i18n::t("detailPanel.showOriginal")
        } else {
            crate::i18n::t("detailPanel.translate")
        };
        let name = skill.name.clone();
        Some(
            div()
                .id("drawer-translate")
                .p(px(3.0))
                .rounded_md()
                .flex_shrink_0()
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    crate::chrome::tooltip(tip.to_string()).build(window, cx)
                })
                .child(icon_sweep(
                    "drawer-translate",
                    IconName::Languages,
                    14.0,
                    palette().fg_muted,
                    pending,
                ))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.description_choices.insert(name.clone(), !showing);
                        // The choice outlives the session, like the Settings
                        // save path: a small local file written in place.
                        if let Err(error) =
                            ss_core::translation::set_description_choice(&name, !showing)
                        {
                            tracing::warn!("failed to save the description choice: {error}");
                        }
                        // The card follows the choice. Bump the epoch the
                        // canvas watches, so its slot rebuilds with the
                        // override right away.
                        this.note_translation_prefs(cx);
                    });
                })
                .interaction_spring(
                    "drawer-translate",
                    true,
                    MotionPaint::new().fg(rgb(palette().fg_muted)),
                    MotionPaint::new()
                        .fg(rgb(palette().fg))
                        .bg(rgb(palette().card_hover)),
                )
                .debug_selector(|| "drawer-translate".into()),
        )
    }

    /// Source, author, and updated time. A value the source URL already
    /// contains is omitted. Nothing here is the install path.
    fn facts(&self, skill: &Skill) -> Option<Div> {
        let lines =
            detail_facts::source_values(&skill.git_url, skill.source.as_deref(), &skill.name);
        let author =
            detail_facts::shown_author(skill.author.as_deref(), &lines).map(str::to_string);
        let updated = detail_facts::format_updated(&skill.last_updated);
        if lines.is_empty() && author.is_none() && updated.is_none() {
            return None;
        }
        let mut block = div().flex().flex_col().gap_3();
        if !lines.is_empty() {
            let mut source = div().flex().flex_col().gap_1().child(field_label(
                crate::i18n::t("detailPanel.source").to_string(),
            ));
            for (index, line) in lines.into_iter().enumerate() {
                source = source.child(source_line(index, line));
            }
            block = block.child(source);
        }
        if let Some(author) = author {
            let shown = if author.starts_with('@') {
                author
            } else {
                format!("@{author}")
            };
            block = block.child(fact_row(
                crate::i18n::t("detailPanel.author").to_string(),
                shown,
            ));
        }
        if let Some(updated) = updated {
            block = block.child(fact_row(
                crate::i18n::t("detailPanel.lastUpdated").to_string(),
                updated,
            ));
        }
        Some(block)
    }

    fn agent_switches(&self, skill: &Skill, view: WeakEntity<Self>) -> impl IntoElement {
        let skill_name = skill.name.clone();
        let agent_links = skill.agent_links.as_deref().unwrap_or(&[]);
        let targetable_profiles: Vec<_> =
            crate::skill_card::targetable_agent_profiles(&self.profiles).collect();

        let mut section = div().flex().flex_col().gap_2().child(field_label(
            crate::i18n::t("detailPanel.agentDeployments").to_string(),
        ));

        let mut list = div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2()
            .rounded_lg()
            .bg(rgb(palette().card))
            .border_1()
            .border_color(rgb(palette().border));

        if targetable_profiles.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_1p5()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .whitespace_normal()
                    .child(crate::i18n::t("selectionBar.noAgents").to_string()),
            );
        } else {
            list = list.child(self.master_agent_switch(
                &skill_name,
                &targetable_profiles,
                agent_links,
                view.clone(),
            ));
        }

        for profile in targetable_profiles {
            let is_linked = super::detail_agents::profile_is_linked(profile, agent_links);
            let agent_id = profile.id.clone();
            let agent_name = profile.display_name.clone();
            let s_name = skill_name.clone();
            let toggle_key = format!("drawer-toggle-{s_name}-{agent_id}");
            let v = view.clone();

            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .id(ElementId::Name(
                        format!("drawer-agent-row-{s_name}-{agent_id}").into(),
                    ))
                    .interaction_spring(
                        format!("drawer-agent-row-{s_name}-{agent_id}"),
                        true,
                        MotionPaint::new(),
                        MotionPaint::new().bg(rgb(palette().card_hover)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .min_w_0()
                            .child(
                                img(crate::agent_icons::agent_icon_path(&agent_id))
                                    .w(px(16.0))
                                    .h(px(16.0))
                                    .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(rgb(palette().fg))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(agent_name),
                            ),
                    )
                    .child(
                        super::detail_agents::slide_switch(&toggle_key, is_linked).on_click(
                            move |next, _, cx| {
                                let s_name = s_name.clone();
                                let agent_id = agent_id.clone();
                                let _ = v.update(cx, |this, cx| {
                                    this.toggle_skill_agent(&s_name, &agent_id, *next, cx);
                                });
                            },
                        ),
                    ),
            );
        }

        section = section.child(list);
        section
    }

    fn drawer_actions(
        &self,
        skill: &Skill,
        view: WeakEntity<Self>,
        is_busy: bool,
        updating: bool,
    ) -> impl IntoElement {
        let skill_name = skill.name.clone();
        let update_available = skill.update_available;
        let overwrites_local = matches!(
            skill.upstream_change,
            Some(UpstreamChange::LocalChanges { .. })
        );

        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_shrink_0()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(rgb(palette().border))
            .child({
                let name = skill_name.clone();
                action_button(
                    "drawer-view-skill-md",
                    IconName::FileText,
                    crate::i18n::t("detailPanel.viewSkillMd").to_string(),
                    palette().card,
                    palette().fg,
                    palette().border,
                    false,
                )
                .on_click(move |_, window, cx| {
                    open_skill_reader(name.clone(), window, cx);
                })
            })
            .when(update_available, |actions| {
                let s_name = skill_name.clone();
                let v = view.clone();
                let label = if updating {
                    crate::i18n::t("common.updating")
                } else if overwrites_local {
                    crate::i18n::t("skillCard.updateOverwritesLocal")
                } else {
                    crate::i18n::t("common.update")
                };
                actions.child(
                    action_button(
                        "drawer-update-btn",
                        IconName::CircleArrowUp,
                        label.to_string(),
                        palette().warn,
                        palette().on_accent,
                        palette().warn,
                        true,
                    )
                    .on_click(move |_, _, cx| {
                        if updating {
                            return;
                        }
                        let s_name = s_name.clone();
                        let _ = v.update(cx, |this, cx| {
                            this.update_skill(&s_name, cx);
                        });
                    }),
                )
            })
            .child({
                let s_name = skill_name.clone();
                action_button(
                    "drawer-open-folder-btn",
                    IconName::FolderOpen,
                    crate::i18n::t("detailPanel.openFolder").to_string(),
                    palette().card,
                    palette().fg,
                    palette().border,
                    false,
                )
                .on_click(move |_, _, _| {
                    let path = ss_core::infra::paths::agents_skill_dir(&s_name);
                    crate::os_open::open_folder(&path);
                })
            })
            .child(super::detail_uninstall::button(
                skill_name,
                if is_busy {
                    crate::i18n::t("common.uninstalling").to_string()
                } else {
                    crate::i18n::t("common.uninstall").to_string()
                },
                self.uninstall_hover,
                view,
            ))
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, point, px,
        size,
    };

    use super::super::test_support::IsolatedDataDir;
    use super::MySkillsPage;
    use ss_core::types::skill::{Skill, SkillType};

    /// The full page, so the card grid and the drawer are one tree: the
    /// card has to follow the drawer's translation choice.
    struct DrawerHost {
        page: gpui_kit::Entity<MySkillsPage>,
    }

    impl Render for DrawerHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.page.clone())
        }
    }

    fn skill_with(name: &str, description: &str) -> Skill {
        let mut skill = Skill::from_skills_sh(
            name.into(),
            description.into(),
            0,
            "local".into(),
            String::new(),
        );
        skill.skill_type = SkillType::Local;
        skill
    }

    fn skill_named(description: &str) -> Skill {
        skill_with("demo", description)
    }

    fn window<'a>(
        cx: &'a mut gpui_kit::TestAppContext,
        skills: Vec<Skill>,
    ) -> (
        &'a mut gpui_kit::VisualTestContext,
        gpui_kit::Entity<MySkillsPage>,
    ) {
        crate::init_test(cx);
        let page = cx.new(|cx| MySkillsPage::new(cx));
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.loading = false;
                page.error = None;
                page.skills = skills;
                page.select_detail(Some("demo".into()));
                page.revise(cx);
            });
        });
        let shown = page.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            let host = cx.new(|_| DrawerHost {
                page: shown.clone(),
            });
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (cx, page)
    }

    fn paint(cx: &mut gpui_kit::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn click(cx: &mut gpui_kit::VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} missing"));
        cx.simulate_click(
            point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            ),
            Default::default(),
        );
    }

    /// Chinese copy is not something the target translates, so the drawer
    /// offers no button for it.
    #[gpui_kit::test]
    fn chinese_description_hides_the_translate_button(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        let (cx, _page) = window(cx, vec![skill_named("为代理构建深模块。")]);
        assert!(
            cx.debug_bounds("drawer-translate").is_none(),
            "already-Chinese copy still offered translate"
        );
    }

    /// With the Settings switch off, an English description keeps its original
    /// line; the button translates this opening only. This is the suite's one
    /// real press on the button, and it also proves the press reached the
    /// translation config.
    #[gpui_kit::test]
    fn the_button_translates_this_opening_only(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        ss_core::translation::remember(
            "Build deep modules for agents.",
            "zh-CN",
            "为代理构建深模块。",
        );
        let (mut cx, _page) = window(cx, vec![skill_named("Build deep modules for agents.")]);
        assert!(
            cx.debug_bounds("drawer-translate").is_some(),
            "English copy did not offer translate"
        );
        assert!(
            cx.debug_bounds("drawer-translation").is_none(),
            "the translation appeared before the button"
        );
        click(&mut cx, "drawer-translate");
        paint(&mut cx);
        // The real press ran the handler and persisted the choice — the
        // end-to-end proof this test owns. The press's own repaint can replay
        // the cached scene without registering debug bounds (docs/errors.md),
        // which is order-dependent and flaky in a suite, so the translated
        // line itself is asserted by the state-driven tests below.
        assert_eq!(
            ss_core::translation::load_config()
                .unwrap()
                .description_choices
                .get("demo"),
            Some(&true),
            "the press did not persist the choice"
        );
    }

    /// The Settings switch still translates the drawer without a click, and
    /// the button then offers the original.
    #[gpui_kit::test]
    fn the_description_setting_translates_before_a_click(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        let mut config = ss_core::translation::TranslationConfig::default();
        config.translate_descriptions = true;
        ss_core::translation::save_config(&config).unwrap();
        ss_core::translation::remember(
            "Build deep modules for agents.",
            "zh-CN",
            "为代理构建深模块。",
        );
        let (cx, _page) = window(cx, vec![skill_named("Build deep modules for agents.")]);
        assert!(
            cx.debug_bounds("drawer-translation").is_some(),
            "the description switch did not translate this opening"
        );
        assert!(
            cx.debug_bounds("drawer-translate").is_some(),
            "the button left while a translation was showing"
        );
    }

    /// The card of this opening follows the button: translating the drawer
    /// paints the same line on the card, and Original takes both back.
    #[gpui_kit::test]
    fn the_card_follows_the_drawer_translation_choice(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        ss_core::translation::remember(
            "Build deep modules for agents.",
            "zh-CN",
            "为代理构建深模块。",
        );
        let (mut cx, page) = window(cx, vec![skill_named("Build deep modules for agents.")]);
        assert!(
            cx.debug_bounds("skill-card-desc-demo").is_some(),
            "the card was not laid out"
        );
        // Driven through the same state the button writes, not a second
        // simulated press: one native click on this button leaves the next
        // test's press painting an empty frame (see docs/errors.md), so this
        // suite presses the button for real at most once per run.
        cx.update(|_, app| {
            let _ = page.update(app, |this, cx| {
                this.description_choices.insert("demo".into(), true);
                this.note_translation_prefs(cx);
            });
        });
        paint(&mut cx);
        assert!(
            cx.debug_bounds("skill-card-translation-demo").is_some(),
            "the card kept the original after the drawer translated"
        );
        // Original takes both back. Driven through the same state the
        // button writes: a second simulated press on this tree can race the
        // test window's input callback with parallel drawers, which drops
        // the event silently.
        cx.update(|_, app| {
            let _ = page.update(app, |this, cx| {
                this.description_choices.insert("demo".into(), false);
                this.note_translation_prefs(cx);
            });
        });
        paint(&mut cx);
        assert!(
            cx.debug_bounds("skill-card-desc-demo").is_some(),
            "Original left the translation on the card"
        );
    }

    /// A translated skill keeps its line when the drawer moves on: the
    /// choice is remembered per skill, so that card does not fall back to
    /// English just because another skill opened.
    #[gpui_kit::test]
    fn a_translated_card_stays_translated_after_the_drawer_moves_on(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        ss_core::translation::remember("Build deep modules.", "zh-CN", "构建深模块。");
        ss_core::translation::remember("Write tests first.", "zh-CN", "先写测试。");
        let skills = vec![
            skill_with("demo", "Build deep modules."),
            skill_with("other", "Write tests first."),
        ];
        let (mut cx, page) = window(cx, skills);
        assert!(
            cx.debug_bounds("skill-card-desc-demo").is_some(),
            "the card was not laid out"
        );
        // Same state the button writes; see the note on the real press in
        // `the_card_follows_the_drawer_translation_choice`.
        cx.update(|_, app| {
            let _ = page.update(app, |this, cx| {
                this.description_choices.insert("demo".into(), true);
                this.note_translation_prefs(cx);
            });
        });
        paint(&mut cx);
        assert!(
            cx.debug_bounds("skill-card-translation-demo").is_some(),
            "the card kept the original after the drawer translated"
        );
        // Move the drawer to the other skill the way its card click does.
        // Driven through the same state: a second simulated press on this
        // tree races the test window's input callback with parallel
        // drawers, which drops the event silently.
        cx.update(|_, app| {
            let _ = page.update(app, |this, cx| {
                this.select_detail(Some("other".into()));
                this.revise(cx);
            });
        });
        paint(&mut cx);
        assert!(
            cx.debug_bounds("drawer-translation").is_none(),
            "the drawer did not move to the other skill, or translated it without a choice"
        );
        assert!(
            cx.debug_bounds("skill-card-translation-demo").is_some(),
            "the translated card fell back to English after the drawer moved on"
        );
        assert!(
            cx.debug_bounds("skill-card-desc-other").is_some(),
            "the other skill's card was translated without a choice"
        );
    }

    /// The button's choice is written to the translation config, and a page
    /// constructed afterwards — the next launch — reopens in the chosen
    /// language without another click.
    #[gpui_kit::test]
    fn the_choice_survives_a_restart(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        ss_core::translation::remember(
            "Build deep modules for agents.",
            "zh-CN",
            "为代理构建深模块。",
        );
        // What the last session's click left in the config.
        ss_core::translation::set_description_choice("demo", true).unwrap();
        // A fresh page is the next launch. The same host tree is rebuilt
        // around it so the card and drawer paint from the reloaded choice.
        crate::init_test(cx);
        let fresh = cx.new(|cx| {
            let mut page = MySkillsPage::new(cx);
            page.loading = false;
            page.skills = vec![skill_named("Build deep modules for agents.")];
            page.select_detail(Some("demo".into()));
            page.revise(cx);
            page
        });
        let shown = fresh.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            let host = cx.new(|_| DrawerHost {
                page: shown.clone(),
            });
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        paint(cx);
        assert!(
            cx.debug_bounds("skill-card-translation-demo").is_some(),
            "a fresh page forgot the translated choice"
        );
        assert!(
            cx.debug_bounds("drawer-translation").is_some(),
            "a fresh drawer forgot the translated choice"
        );
    }

    /// The Settings switch is on, but this skill's stored choice is the
    /// original: a launch must not flip that card back to the translation.
    #[gpui_kit::test]
    fn a_stored_original_choice_survives_a_restart(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        ss_core::translation::set_description_choice("demo", false).unwrap();
        let mut config = ss_core::translation::load_config().unwrap();
        config.translate_descriptions = true;
        ss_core::translation::save_config(&config).unwrap();
        ss_core::translation::remember(
            "Build deep modules for agents.",
            "zh-CN",
            "为代理构建深模块。",
        );
        let (cx, _page) = window(cx, vec![skill_named("Build deep modules for agents.")]);
        assert!(
            cx.debug_bounds("skill-card-desc-demo").is_some(),
            "the stored original choice was not honored on the card"
        );
        assert!(
            cx.debug_bounds("drawer-translation").is_none(),
            "the stored original choice was not honored in the drawer"
        );
    }
}
