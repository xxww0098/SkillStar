//! Repo tiles for Publisher Detail. Skill tiles reuse the marketplace card.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_marketplace::PublisherRepo;

use super::super::{format_installs, icon, market_tile};
use super::PublisherDetailPage;

use crate::theme::palette;

impl PublisherDetailPage {
    pub(super) fn render_repo_card(
        &self,
        repo: &PublisherRepo,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let source = repo.source.clone();
        let owner = self
            .publisher
            .as_ref()
            .map(|publisher| publisher.name.as_str())
            .unwrap_or("");
        let title = if owner.is_empty() {
            repo.repo.clone()
        } else {
            format!("{owner}/{}", repo.repo)
        };
        let installs = if !repo.installs_label.is_empty() {
            Some(repo.installs_label.clone())
        } else if repo.installs > 0 {
            Some(format_installs(repo.installs))
        } else {
            None
        };

        market_tile(
            ElementId::Name(format!("pub-repo-{}", repo.repo).into()),
            false,
        )
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .cursor_pointer()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .font_family("monospace")
                        .text_color(rgb(palette().fg))
                        .truncate()
                        .child(title),
                )
                .child(icon(IconName::ChevronRight, 14.0, palette().fg_muted)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(icon(IconName::Package, 12.0, palette().fg_muted))
                        .child(crate::i18n::tf(
                            "marketplace.skillCount",
                            &[("count", &repo.skill_count.to_string())],
                        )),
                )
                .when_some(installs, |row, label| {
                    row.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(icon(IconName::Download, 12.0, palette().fg_muted))
                            .child(label),
                    )
                }),
        )
        .on_click(move |_, _, cx| {
            let src = source.clone();
            let _ = view.update(cx, |this, cx| {
                this.load_repo_skills(cx, &src);
            });
        })
    }

    pub(super) fn render_market_card(
        &self,
        skill: &Skill,
        view: WeakEntity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let busy = self.busy.as_deref() == Some(skill.name.as_str());
        let installed = skill.installed;
        let highlighted = self.detail.matches(skill);
        let action_name = skill.name.clone();
        let action_url = skill.git_url.clone();
        let clicked = skill.clone();
        let open_view = view.clone();
        super::super::render_market_card(skill, busy, highlighted, "pd", cx, move |_, _, cx| {
            cx.stop_propagation();
            let _ = view.update(cx, |this, cx| {
                this.set_installed(action_url.clone(), action_name.clone(), !installed, cx);
            });
        })
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let skill = clicked.clone();
            let _ = open_view.update(cx, |this, cx| this.open_market_skill(&skill, cx));
        })
    }
}
