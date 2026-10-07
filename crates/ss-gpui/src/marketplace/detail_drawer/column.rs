//! The marketplace skill column. Same chrome as the skill drawer: fixed
//! width, left border, title, close. The install action stays pinned so a
//! long SKILL.md does not scroll the decision off screen.

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::{Skill, SkillCategory};
use ss_marketplace::SecurityAudit;

use super::{DRAWER_W, DetailPhase, MarketSkillId};
use crate::chrome::{InteractionSpring, MotionPaint, icon_spin};
use crate::marketplace::types::{format_installs, skill_blurb};
use crate::theme::palette;

pub(crate) fn market_detail_column(
    prefix: &'static str,
    skill: &Skill,
    phase: &DetailPhase,
    busy: bool,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_toggle_install: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = MarketSkillId::from_skill(skill);
    let description = shown_description(skill, phase);
    let readme = shown_readme(phase, description.as_deref());
    let audits = phase_audits(phase);
    let weekly = phase_text(phase, |details| details.weekly_installs.as_deref());
    let first_seen = phase_text(phase, |details| details.first_seen.as_deref());
    let github_stars = match phase {
        DetailPhase::Ready(details) => details.github_stars.filter(|stars| *stars > 0),
        _ => None,
    };
    let failed = match phase {
        DetailPhase::Failed(detail) => Some(detail.clone()),
        _ => None,
    };
    let loading = matches!(phase, DetailPhase::Loading);
    let source_url = id
        .source
        .as_ref()
        .map(|source| format!("https://skills.sh/{source}/{}", skill.name));

    div()
        .id(ElementId::Name(format!("{prefix}-detail-drawer").into()))
        .w(px(DRAWER_W))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .min_h_0()
        .overflow_hidden()
        .border_l_1()
        .border_color(rgb(palette().border))
        .child(detail_header(prefix, skill, on_close))
        .child(
            div()
                .id(ElementId::Name(format!("{prefix}-detail-scroll").into()))
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .gap_4()
                .child(description_section(
                    prefix,
                    description.as_deref(),
                    loading,
                    failed.as_deref(),
                    on_retry,
                ))
                .when(
                    has_metadata(
                        skill,
                        id.source.as_deref(),
                        weekly.as_deref(),
                        github_stars,
                        first_seen.as_deref(),
                    ),
                    |body| {
                        body.child(metadata_card(
                            prefix,
                            skill,
                            id.source.as_deref(),
                            weekly.as_deref(),
                            github_stars,
                            first_seen.as_deref(),
                        ))
                    },
                )
                .when(!audits.is_empty(), |body| {
                    body.child(audit_section(&audits))
                })
                .when_some(readme, |body, readme| {
                    body.child(skill_md_button(prefix, skill.name.clone(), readme))
                }),
        )
        .child(detail_actions(
            prefix,
            skill,
            busy,
            source_url,
            on_toggle_install,
        ))
        .into_any_element()
}

fn detail_header(
    prefix: &'static str,
    skill: &Skill,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let name = skill.name.clone();
    let show_badges = skill.installed
        || skill.category == SkillCategory::Hot
        || skill.rank.is_some_and(|rank| rank > 0 && rank <= 100);
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
                        .id(ElementId::Name(format!("{prefix}-detail-name").into()))
                        .min_w_0()
                        .text_base()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(palette().fg))
                        .truncate()
                        .tooltip(move |window, cx| {
                            crate::chrome::tooltip(name.clone()).build(window, cx)
                        })
                        .child(skill.name.clone()),
                )
                .when(show_badges, |col| col.child(badge_row(skill))),
        )
        .child(close_button(prefix, on_close))
}

fn close_button(
    prefix: &'static str,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::chrome::MotionDiv {
    div()
        .id(ElementId::Name(format!("{prefix}-detail-close").into()))
        .p_1()
        .rounded_md()
        .flex_shrink_0()
        .cursor_pointer()
        .text_color(rgb(palette().fg_muted))
        .tooltip(|window, cx| {
            crate::chrome::tooltip(crate::i18n::t("detailPanel.dismissDrawer").to_string())
                .build(window, cx)
        })
        .child(Icon::new(IconName::X).with_size(px(18.0)))
        .on_click(on_close)
        .interaction_spring(
            format!("{prefix}-detail-close"),
            true,
            MotionPaint::new().fg(rgb(palette().fg_muted)),
            MotionPaint::new()
                .bg(rgb(palette().card_hover))
                .fg(rgb(palette().fg)),
        )
}

fn badge_row(skill: &Skill) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_1()
        .when(
            skill.rank.is_some_and(|rank| rank > 0 && rank <= 100),
            |row| {
                let rank = skill.rank.unwrap_or(0);
                row.child(chip(palette().warn_bg, palette().warn, format!("#{rank}")))
            },
        )
        .when(skill.category == SkillCategory::Hot, |row| {
            row.child(chip(
                palette().danger_bg,
                palette().danger,
                crate::i18n::t("marketplace.hot").to_string(),
            ))
        })
        .when(skill.installed, |row| {
            row.child(chip(
                palette().ok_bg,
                palette().ok,
                crate::i18n::t("marketplace.detailInstalled").to_string(),
            ))
        })
}

fn chip(bg: u32, fg: u32, label: String) -> Div {
    div()
        .px_1p5()
        .py(px(1.0))
        .rounded_sm()
        .bg(rgb(bg))
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(fg))
        .child(label)
}

fn description_section(
    prefix: &'static str,
    description: Option<&str>,
    loading: bool,
    failed: Option<&str>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let failed = failed.map(str::to_string);
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_label(
            crate::i18n::t("detailPanel.description").to_string(),
        ))
        .when_some(description.map(str::to_string), |section, text| {
            let shown =
                crate::translation::display(&text, crate::translation::Surface::Description);
            let block = text_block(&shown, palette().fg);
            section.child(if shown != text {
                crate::translation::paint_card(block)
            } else {
                block.into_any_element()
            })
        })
        .when(description.is_none() && loading, |section| {
            section.child(muted_line(
                crate::i18n::t("marketplace.detailLoading").to_string(),
            ))
        })
        .when(
            description.is_none() && !loading && failed.is_none(),
            |section| {
                section.child(muted_line(
                    crate::i18n::t("detailPanel.noDescription").to_string(),
                ))
            },
        )
        .when(loading && description.is_some(), |section| {
            section.child(muted_line(
                crate::i18n::t("marketplace.detailLoading").to_string(),
            ))
        })
        .when_some(failed, |section, detail| {
            section.child(failure_row(prefix, &detail, on_retry))
        })
}

fn failure_row(
    prefix: &'static str,
    detail: &str,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let detail = detail.to_string();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(rgb(palette().danger))
                .child(crate::i18n::t("marketplace.detailFailed").to_string()),
        )
        .when(!detail.trim().is_empty(), |row| {
            let tip = detail.clone();
            row.child(
                div()
                    .id(ElementId::Name(format!("{prefix}-detail-error").into()))
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .line_clamp(2)
                    .tooltip(move |window, cx| {
                        crate::chrome::tooltip(tip.clone()).build(window, cx)
                    })
                    .child(detail.clone()),
            )
        })
        .child(
            div()
                .id(ElementId::Name(format!("{prefix}-detail-retry").into()))
                .flex()
                .items_center()
                .gap_1()
                .self_start()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette().border))
                .bg(rgb(palette().card))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(palette().fg))
                .cursor_pointer()
                .child(crate::i18n::t("common.retry").to_string())
                .on_click(on_retry)
                .interaction_spring(
                    format!("{prefix}-detail-retry"),
                    true,
                    MotionPaint::new().bg(rgb(palette().card)),
                    MotionPaint::new().bg(rgb(palette().card_hover)),
                ),
        )
}

fn has_metadata(
    skill: &Skill,
    source: Option<&str>,
    weekly: Option<&str>,
    github_stars: Option<u32>,
    first_seen: Option<&str>,
) -> bool {
    source.is_some()
        || skill
            .author
            .as_deref()
            .is_some_and(|author| !author.is_empty())
        || skill.stars > 0
        || weekly.is_some()
        || github_stars.is_some()
        || first_seen.is_some()
        || !skill.last_updated.is_empty()
        || !skill.git_url.trim().is_empty()
}

fn metadata_card(
    prefix: &'static str,
    skill: &Skill,
    source: Option<&str>,
    weekly: Option<&str>,
    github_stars: Option<u32>,
    first_seen: Option<&str>,
) -> Div {
    let git_url = skill.git_url.trim().to_string();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded_lg()
        .bg(rgb(palette().card))
        .border_1()
        .border_color(rgb(palette().border))
        .when_some(source.map(str::to_string), |card, source| {
            card.child(meta_row(
                prefix,
                "source",
                crate::i18n::t("marketplace.detailSource").to_string(),
                source,
            ))
        })
        .when_some(
            skill.author.clone().filter(|author| !author.is_empty()),
            |card, author| {
                card.child(meta_row(
                    prefix,
                    "author",
                    crate::i18n::t("detailPanel.author").to_string(),
                    format!("@{author}"),
                ))
            },
        )
        .when(skill.stars > 0, |card| {
            card.child(meta_row(
                prefix,
                "installs",
                crate::i18n::t("marketplace.detailInstalls").to_string(),
                format_installs(skill.stars),
            ))
        })
        .when_some(weekly.map(str::to_string), |card, weekly| {
            card.child(meta_row(
                prefix,
                "weekly",
                crate::i18n::t("marketplace.detailWeeklyInstalls").to_string(),
                weekly,
            ))
        })
        .when_some(github_stars, |card, stars| {
            card.child(meta_row(
                prefix,
                "stars",
                crate::i18n::t("marketplace.detailGithubStars").to_string(),
                format_installs(stars),
            ))
        })
        .when_some(first_seen.map(str::to_string), |card, seen| {
            card.child(meta_row(
                prefix,
                "seen",
                crate::i18n::t("marketplace.detailFirstSeen").to_string(),
                seen,
            ))
        })
        .when(!skill.last_updated.is_empty(), |card| {
            card.child(meta_row(
                prefix,
                "updated",
                crate::i18n::t("detailPanel.lastUpdated").to_string(),
                skill.last_updated.clone(),
            ))
        })
        .when(!git_url.is_empty(), |card| {
            card.child(link_row(
                prefix,
                crate::i18n::t("detailPanel.gitSource").to_string(),
                git_url,
            ))
        })
}

fn meta_row(prefix: &str, key: &str, label: String, value: String) -> Div {
    let tip = value.clone();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .min_w_0()
        .child(field_label(label))
        .child(
            div()
                .id(ElementId::Name(
                    format!("{prefix}-detail-meta-{key}").into(),
                ))
                .min_w_0()
                .text_xs()
                .text_color(rgb(palette().fg))
                .truncate()
                .tooltip(move |window, cx| crate::chrome::tooltip(tip.clone()).build(window, cx))
                .child(value),
        )
}

fn link_row(prefix: &str, label: String, url: String) -> Div {
    let open = url.clone();
    let shown = url.clone();
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .min_w_0()
        .child(field_label(label))
        .child(
            div()
                .id(ElementId::Name(format!("{prefix}-detail-git").into()))
                .min_w_0()
                .text_xs()
                .text_color(rgb(palette().accent))
                .truncate()
                .cursor_pointer()
                .hover(|style| style.underline())
                .tooltip(move |window, cx| crate::chrome::tooltip(shown.clone()).build(window, cx))
                .child(url)
                .on_click(move |_, _, app| {
                    app.stop_propagation();
                    app.open_url(&open);
                }),
        )
}

fn audit_section(audits: &[SecurityAudit]) -> Div {
    let mut list = div().flex().flex_col().gap_1p5();
    for audit in audits {
        let (fg, bg) = audit_tone(&audit.result);
        list = list.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(rgb(palette().fg))
                        .truncate()
                        .child(audit.name.clone()),
                )
                .child(chip(bg, fg, audit.result.clone())),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_label(
            crate::i18n::t("marketplace.detailAudits").to_string(),
        ))
        .child(
            div()
                .p_3()
                .rounded_lg()
                .bg(rgb(palette().card))
                .border_1()
                .border_color(rgb(palette().border))
                .child(list),
        )
}

/// Color supports the result text. A result that is neither pass nor fail
/// stays neutral instead of implying a verdict.
fn audit_tone(result: &str) -> (u32, u32) {
    let lower = result.to_ascii_lowercase();
    if lower.contains("fail") || lower.contains("error") || lower.contains("high") {
        (palette().danger, palette().danger_bg)
    } else if lower.contains("pass")
        || lower.contains("ok")
        || lower.contains("safe")
        || lower.contains("clean")
    {
        (palette().ok, palette().ok_bg)
    } else {
        (palette().fg, palette().well)
    }
}

fn skill_md_button(prefix: &'static str, title: String, markdown: String) -> impl IntoElement {
    div()
        .id(ElementId::Name(format!("{prefix}-view-skill-md").into()))
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .w_full()
        .py_2()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .text_color(rgb(palette().fg))
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .child(Icon::new(IconName::FileText).with_size(px(14.0)))
        .child(crate::i18n::t("detailPanel.viewSkillMd").to_string())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            crate::my_skills::skill_reader::open_skill_markdown(
                title.clone(),
                markdown.clone(),
                window,
                cx,
            );
        })
        .interaction_spring(
            format!("{prefix}-view-skill-md"),
            true,
            MotionPaint::new().bg(rgb(palette().card)),
            MotionPaint::new().bg(rgb(palette().card_hover)),
        )
}

fn detail_actions(
    prefix: &'static str,
    skill: &Skill,
    busy: bool,
    source_url: Option<String>,
    on_toggle_install: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let installed = skill.installed;
    let label = if installed {
        crate::i18n::t("common.uninstall")
    } else {
        crate::i18n::t("common.install")
    };
    let (bg, fg, hover) = if busy {
        (
            palette().card_hover,
            palette().fg_muted,
            palette().card_hover,
        )
    } else if installed {
        (
            palette().danger_bg,
            palette().danger,
            palette().danger_hover,
        )
    } else {
        (
            palette().accent,
            palette().on_accent,
            palette().accent_hover,
        )
    };
    let icon_name = if installed {
        IconName::Trash
    } else {
        IconName::Download
    };
    div()
        .flex()
        .flex_col()
        .gap_2()
        .flex_shrink_0()
        .px_4()
        .py_3()
        .border_t_1()
        .border_color(rgb(palette().border))
        .child(
            div()
                .id(ElementId::Name(format!("{prefix}-detail-install").into()))
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .w_full()
                .py_2()
                .rounded_lg()
                .bg(rgb(bg))
                .text_color(rgb(fg))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .when(!busy, |button| button.cursor_pointer())
                .child(icon_spin(
                    ElementId::Name(format!("{prefix}-detail-install-spin").into()),
                    icon_name,
                    14.0,
                    fg,
                    busy,
                ))
                .child(label.to_string())
                .on_click(move |event, window, app| {
                    app.stop_propagation();
                    if !busy {
                        on_toggle_install(event, window, app);
                    }
                })
                .interaction_spring(
                    format!("{prefix}-detail-install"),
                    !busy,
                    MotionPaint::new().bg(rgb(bg)),
                    MotionPaint::new().bg(rgb(if busy { bg } else { hover })),
                ),
        )
        .when_some(source_url, |actions, url| {
            actions.child(source_link(prefix, url))
        })
}

fn source_link(prefix: &'static str, url: String) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("{prefix}-detail-source").into()))
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .w_full()
        .py_1()
        .text_xs()
        .text_color(rgb(palette().accent))
        .cursor_pointer()
        .hover(|style| style.underline())
        .child(Icon::new(IconName::ExternalLink).with_size(px(12.0)))
        .child(crate::i18n::t("detailPanel.viewOnSkillsSh").to_string())
        .on_click(move |_, _, app| {
            app.stop_propagation();
            app.open_url(&url);
        })
}

fn phase_audits(phase: &DetailPhase) -> Vec<SecurityAudit> {
    match phase {
        DetailPhase::Ready(details) => details.security_audits.clone(),
        _ => Vec::new(),
    }
}

fn phase_text(
    phase: &DetailPhase,
    pick: impl Fn(&ss_marketplace::MarketplaceSkillDetails) -> Option<&str>,
) -> Option<String> {
    let DetailPhase::Ready(details) = phase else {
        return None;
    };
    nonempty(pick(details)).map(str::to_string)
}

fn shown_description(skill: &Skill, phase: &DetailPhase) -> Option<String> {
    if let DetailPhase::Ready(details) = phase {
        if let Some(summary) = nonempty(details.summary.as_deref()) {
            return Some(summary.to_string());
        }
    }
    skill_blurb(skill).map(str::to_string)
}

fn shown_readme(phase: &DetailPhase, description: Option<&str>) -> Option<String> {
    let DetailPhase::Ready(details) = phase else {
        return None;
    };
    let readme = nonempty(details.readme.as_deref())?;
    if description.is_some_and(|description| description.trim() == readme) {
        return None;
    }
    Some(readme.to_string())
}

fn nonempty(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|text| !text.is_empty())
}

fn text_block(text: &str, color: u32) -> Div {
    let mut block = div().flex().flex_col().gap_1();
    let mut any = false;
    for line in text.lines() {
        any = true;
        block = block.child(
            div()
                .text_xs()
                .text_color(rgb(color))
                .child(if line.is_empty() {
                    " ".to_string()
                } else {
                    line.to_string()
                }),
        );
    }
    if !any {
        block = block.child(
            div()
                .text_xs()
                .text_color(rgb(color))
                .child(text.to_string()),
        );
    }
    block
}

fn section_label(label: String) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(palette().fg_muted))
        .child(label)
}

fn field_label(label: String) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(palette().fg_muted))
        .flex_shrink_0()
        .child(label)
}

fn muted_line(label: String) -> Div {
    div()
        .text_xs()
        .text_color(rgb(palette().fg_muted))
        .child(label)
}
