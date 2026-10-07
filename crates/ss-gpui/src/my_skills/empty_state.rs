//! Empty states for the My Skills page.

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::*;

use super::MySkillsPage;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::nav::{NavPage, SelectPage};
use crate::theme::palette;

/// Fills the skills scroller and centers a short status. Not a one-line stub at the top.
pub(super) fn centered_fill() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .min_h_full()
        .items_center()
        .justify_center()
        .gap_3()
        .p_5()
}

pub fn render_empty_installed(view: WeakEntity<MySkillsPage>) -> impl IntoElement {
    centered_fill()
        .child(div().text_3xl().child("🧩"))
        .child(
            div()
                .text_lg()
                .font_semibold()
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("emptyState.mySkillsTitle")),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(palette().fg_muted))
                .child(crate::i18n::t("emptyState.mySkillsDesc")),
        )
        .child(
            div()
                .id("my-skills-goto-marketplace")
                .flex()
                .items_center()
                .gap_2()
                .px_4()
                .py_2()
                .rounded_lg()
                .bg(rgb(palette().accent))
                .text_color(rgb(palette().on_accent))
                .text_sm()
                .font_medium()
                .cursor_pointer()
                .child(Icon::new(NavPage::Marketplace.icon()).with_size(px(16.0)))
                .child(crate::i18n::t("emptyState.mySkillsCta"))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |_, cx| {
                        cx.emit(SelectPage(NavPage::Marketplace));
                    });
                })
                .interaction_spring(
                    "my-skills-goto-marketplace",
                    true,
                    MotionPaint::new().opacity(1.0),
                    MotionPaint::new().opacity(0.9),
                ),
        )
}

pub fn render_empty_search(query: &str, view: WeakEntity<MySkillsPage>) -> impl IntoElement {
    let q = query.to_string();
    centered_fill()
        .child(
            div()
                .p_3()
                .rounded_full()
                .bg(rgb(palette().card))
                .border_1()
                .border_color(rgb(palette().border))
                .child(
                    Icon::new(IconName::Search)
                        .with_size(px(24.0))
                        .text_color(rgb(palette().fg_muted)),
                ),
        )
        .child(
            div()
                .text_lg()
                .font_semibold()
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("emptyState.noMatchingTitle")),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(palette().fg_muted))
                .child(if q.is_empty() {
                    crate::i18n::t("mySkills.noMatching")
                } else {
                    crate::i18n::t("skillCards.tryDifferent")
                }),
        )
        .child(
            div()
                .id("my-skills-clear-search")
                .px_3()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette().border))
                .bg(rgb(palette().card))
                .text_color(rgb(palette().fg))
                .text_sm()
                .cursor_pointer()
                .child(crate::i18n::t("settings.clearAgentFilters"))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.clear_filters(cx);
                    });
                })
                .interaction_spring(
                    "my-skills-clear-search",
                    true,
                    MotionPaint::new().bg(rgb(palette().card)),
                    MotionPaint::new().bg(rgb(palette().card_hover)),
                ),
        )
}

pub fn render_empty_updates(view: WeakEntity<MySkillsPage>) -> impl IntoElement {
    centered_fill()
        .child(
            div()
                .p_3()
                .rounded_full()
                .bg(rgb(palette().card))
                .border_1()
                .border_color(rgb(palette().border))
                .child(
                    Icon::new(IconName::Check)
                        .with_size(px(24.0))
                        .text_color(rgb(palette().ok)),
                ),
        )
        .child(
            div()
                .text_lg()
                .font_semibold()
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("emptyState.upToDateTitle")),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(palette().fg_muted))
                .child(crate::i18n::t("emptyState.upToDateDesc")),
        )
        .child(
            div()
                .id("my-skills-show-all")
                .px_3()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette().border))
                .bg(rgb(palette().card))
                .text_color(rgb(palette().fg))
                .text_sm()
                .cursor_pointer()
                .child(crate::i18n::t("toolbar.showAllSkills"))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.only_updates = false;
                        this.revise(cx);
                    });
                })
                .interaction_spring(
                    "my-skills-show-all",
                    true,
                    MotionPaint::new().bg(rgb(palette().card)),
                    MotionPaint::new().bg(rgb(palette().card_hover)),
                ),
        )
}

pub fn render_remote_scope() -> impl IntoElement {
    centered_fill()
        .child(
            div()
                .p_3()
                .rounded_full()
                .bg(rgb(palette().card))
                .border_1()
                .border_color(rgb(palette().border))
                .child(
                    Icon::new(IconName::Server)
                        .with_size(px(28.0))
                        .text_color(rgb(palette().accent)),
                ),
        )
        .child(
            div()
                .text_lg()
                .font_semibold()
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("emptyState.remoteTitle")),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(palette().fg_muted))
                .max_w(px(400.0))
                .text_center()
                .child(crate::i18n::t("emptyState.remoteDesc")),
        )
}

pub fn render_channels_scope() -> impl IntoElement {
    centered_fill()
        .child(
            div()
                .p_3()
                .rounded_full()
                .bg(rgb(palette().card))
                .border_1()
                .border_color(rgb(palette().border))
                .child(
                    Icon::new(IconName::Layers)
                        .with_size(px(28.0))
                        .text_color(rgb(palette().violet)),
                ),
        )
        .child(
            div()
                .text_lg()
                .font_semibold()
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("emptyState.channelsTitle")),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(palette().fg_muted))
                .max_w(px(400.0))
                .text_center()
                .child(crate::i18n::t("emptyState.channelsDesc")),
        )
}
