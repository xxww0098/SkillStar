//! Right-hand detail column for one installed skill.
//!
//! Each fact appears once, in full. The install path is not shown: opening
//! the folder is the action. The whole SKILL.md is the dialog opened by
//! "View SKILL.md…".

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::link::Link;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::{Skill, SkillType, UpstreamChange};

use super::MySkillsPage;
use super::detail_facts::{self, SourceValue, UpstreamNote};
use super::skill_reader::open_skill_reader;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

/// Column width. The skills grid subtracts it when it lays out columns, since
/// the column is a sibling of the card scroller.
///
/// At the default window two skill cards fit with four pixels to spare. That
/// gap is the last card's border and shadow; the scroller clips anything
/// past the pane. Filling those pixels puts the border under the clip.
pub(crate) const DRAWER_W: f32 = 370.0;

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
            .overflow_hidden()
            .border_l_1()
            .border_color(rgb(palette().border))
            .child(self.drawer_header(skill, view.clone()))
            .child(self.drawer_body(skill, view.clone()))
            .child(self.drawer_actions(skill, view, is_busy, updating))
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

    fn drawer_body(&self, skill: &Skill, view: WeakEntity<Self>) -> impl IntoElement {
        let (description, translated) = match super::description_source(skill) {
            None => (
                crate::i18n::t("detailPanel.noDescription").to_string(),
                false,
            ),
            Some(source) => {
                let shown =
                    crate::translation::display(&source, crate::translation::Surface::Description);
                let translated = shown != source;
                (shown, translated)
            }
        };
        let description_body = {
            let row = div()
                .text_xs()
                .text_color(rgb(palette().fg))
                .whitespace_normal()
                .child(description);
            if translated {
                crate::translation::paint_card(row)
            } else {
                row.into_any_element()
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
            ))
            .when_some(self.facts(skill), |body, facts| body.child(facts))
            .when_some(detail_facts::upstream_note(skill), |body, note| {
                body.child(upstream_block(note))
            })
            .child(self.agent_switches(skill, view))
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
            super::skill_card::targetable_agent_profiles(&self.profiles).collect();

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
                        super::detail_agents::slide_switch(&toggle_key, is_linked)
                            .on_click(move |_, _, cx| {
                                let s_name = s_name.clone();
                                let agent_id = agent_id.clone();
                                let _ = v.update(cx, |this, cx| {
                                    this.toggle_skill_agent(&s_name, &agent_id, !is_linked, cx);
                                });
                            })
                            .interaction_spring(
                                toggle_key,
                                true,
                                MotionPaint::new().opacity(1.0),
                                MotionPaint::new().opacity(1.0),
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

fn badge(bg: u32, fg: u32, label: impl Into<SharedString>) -> Div {
    div()
        .px_1p5()
        .py(px(1.0))
        .rounded_sm()
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .font_semibold()
        .child(label.into())
}

fn field_label(label: String) -> Div {
    div()
        .text_xs()
        .font_semibold()
        .text_color(rgb(palette().fg_muted))
        .child(label)
}

fn section(label: String, body: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(field_label(label))
        .child(body)
}

fn fact_row(label: String, value: String) -> Div {
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .min_w_0()
        .child(field_label(label))
        .child(
            div()
                .min_w_0()
                .text_xs()
                .text_color(rgb(palette().fg))
                .whitespace_normal()
                .text_right()
                .child(value),
        )
}

fn source_line(index: usize, line: SourceValue<'_>) -> AnyElement {
    match line {
        SourceValue::Link(url) => {
            let shown = detail_facts::wrap_long_token(url);
            Link::new(ElementId::Name(
                format!("skill-detail-source-{index}").into(),
            ))
            .href(url.to_string())
            .text_xs()
            .whitespace_normal()
            .child(shown)
            .into_any_element()
        }
        SourceValue::Text(text) => div()
            .text_xs()
            .text_color(rgb(palette().fg))
            .whitespace_normal()
            .child(detail_facts::wrap_long_token(text))
            .into_any_element(),
    }
}

fn upstream_block(note: UpstreamNote) -> Div {
    let (title, body, tone) = match note {
        UpstreamNote::Removed { source, folder } => {
            let body = if source.is_empty() {
                crate::i18n::t("skillCard.upstreamRemovedHint").to_string()
            } else {
                crate::i18n::tf(
                    "detailPanel.upstreamRemovedDesc",
                    &[("source", &source), ("folder", &folder)],
                )
                .to_string()
            };
            (
                crate::i18n::t("detailPanel.upstreamRemovedTitle").to_string(),
                body,
                palette().danger,
            )
        }
        UpstreamNote::Renamed { from, to } => (
            crate::i18n::t("skillCard.upstreamRenamed").to_string(),
            crate::i18n::tf(
                "mySkills.updateIdentityChanged",
                &[("name", &from), ("upstream", &to)],
            )
            .to_string(),
            palette().danger,
        ),
        UpstreamNote::LocalEdits { baseline_missing } => {
            let body = if baseline_missing {
                crate::i18n::t("detailPanel.localChangesUnknown")
            } else {
                crate::i18n::t("detailPanel.localChanges")
            };
            (
                crate::i18n::t("detailPanel.localChangesTitle").to_string(),
                body.to_string(),
                palette().warn,
            )
        }
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(rgb(tone))
                .whitespace_normal()
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg))
                .whitespace_normal()
                .child(body),
        )
}

fn action_button(
    id: &'static str,
    icon: IconName,
    label: String,
    bg: u32,
    fg: u32,
    border: u32,
    emphasize: bool,
) -> crate::chrome::MotionDiv {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .w_full()
        .py_2()
        .px_2()
        .rounded_lg()
        .border_1()
        .border_color(rgb(border))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .text_xs()
        .font_weight(if emphasize {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .cursor_pointer()
        .child(Icon::new(icon).with_size(px(14.0)))
        .child(div().whitespace_normal().text_center().child(label))
        .interaction_spring(
            id,
            true,
            if emphasize {
                MotionPaint::new().opacity(1.0)
            } else {
                MotionPaint::new().bg(rgb(bg))
            },
            if emphasize {
                MotionPaint::new().opacity(0.92)
            } else {
                MotionPaint::new().bg(rgb(palette().card_hover))
            },
        )
}
