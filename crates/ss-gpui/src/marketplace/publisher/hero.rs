//! Hero Publisher Profile banner component for PublisherDetail page.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_marketplace::OfficialPublisher;

use super::super::{avatar_palette, format_installs, icon};
use super::PublisherDetailPage;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

impl PublisherDetailPage {
    pub(super) fn render_hero_banner(&self, pub_: &OfficialPublisher) -> impl IntoElement {
        let (bg_color, fg_color) = avatar_palette(&pub_.name);
        let initial = pub_
            .name
            .chars()
            .next()
            .unwrap_or('P')
            .to_uppercase()
            .to_string();
        let total_installs: u32 = self.repos.iter().map(|r| r.installs).sum();
        let total_skills: u32 = if !self.repos.is_empty() {
            self.repos.iter().map(|r| r.skill_count).sum()
        } else {
            pub_.skill_count
        };
        let url = pub_.url.clone();

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .p_4()
            .rounded_xl()
            .bg(rgb(palette().card))
            .border_1()
            .border_color(rgb(palette().border))
            // Left: Avatar + Details
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .flex_1()
                    .min_w_0()
                    // Large 56x56 Avatar
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(56.0))
                            .rounded_2xl()
                            .bg(rgb(bg_color))
                            .border_1()
                            .border_color(rgb(palette().border))
                            .flex_shrink_0()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(fg_color))
                                    .child(initial),
                            ),
                    )
                    // Details
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(palette().fg))
                                            .child(pub_.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(2.0))
                                            .px_2()
                                            .py(px(2.0))
                                            .rounded_full()
                                            .bg(rgb(palette().ok_bg))
                                            .border_1()
                                            .border_color(rgb(palette().ok_border))
                                            .text_color(rgb(palette().ok))
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(icon(IconName::BadgeCheck, 12.0, palette().ok))
                                            .child(crate::i18n::t("publisherDetail.official")),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_4()
                                    .text_xs()
                                    .text_color(rgb(palette().fg_muted))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(icon(IconName::Folder, 13.0, palette().fg_muted))
                                            .child(format!("{} repositories", pub_.repo_count)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(icon(
                                                IconName::Package,
                                                13.0,
                                                palette().fg_muted,
                                            ))
                                            .child(format!("{} skills total", total_skills)),
                                    )
                                    .when(total_installs > 0, |d| {
                                        d.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .child(icon(
                                                    IconName::Download,
                                                    13.0,
                                                    palette().fg_muted,
                                                ))
                                                .child(format!(
                                                    "{} installs",
                                                    format_installs(total_installs)
                                                )),
                                        )
                                    }),
                            ),
                    ),
            )
            // Right: External Link
            .child(
                div()
                    .id("pub-hero-open-web")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_3()
                    .py(px(5.0))
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(rgb(palette().card))
                    .border_1()
                    .border_color(rgb(palette().border))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().accent_fg))
                    .child(icon(IconName::ExternalLink, 12.0, palette().accent_fg))
                    .child(crate::i18n::t("publisherDetail.viewOnSkillsSh"))
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .interaction_spring(
                        "pub-hero-open-web",
                        true,
                        MotionPaint::new()
                            .bg(rgb(palette().card))
                            .fg(rgb(palette().accent_fg)),
                        MotionPaint::new()
                            .bg(rgb(palette().card_hover))
                            .fg(rgb(palette().fg)),
                    ),
            )
    }
}
