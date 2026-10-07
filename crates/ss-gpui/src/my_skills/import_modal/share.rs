//! `ShareCodePreviewPhase` — the `ags-`/`agd-` preview phase: deck header,
//! warning banners (embedded content, private repos, already installed),
//! the read-only skill list, and the Back / Install footer. Split out of
//! `phases.rs` purely for the per-file line budget.

use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::phases::{badge, count_pill};
use super::{ImportDialog, Phase};
use crate::chrome::{InteractionSpring, MotionPaint, ghost_button, icon};
use crate::theme::palette;

impl ImportDialog {
    /// `ShareCodePreviewPhase`: deck header + warning banners + read-only
    /// skill list + Back / Install bar. `data === null` renders the error
    /// block (React reserves it for parse/password failures; the password
    /// branch has no Rust decoder support, so every failure lands here).
    pub(super) fn render_share_preview(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.entity().downgrade();
        let Some(share) = &self.share else {
            return div()
                .flex()
                .flex_col()
                .px(px(24.0))
                .py(px(24.0))
                .gap_4()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .py_2()
                        .child(
                            div()
                                .size(px(56.0))
                                .rounded_2xl()
                                .bg(rgb(palette().warn).alpha(0.10))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icon(IconName::KeyRound, 28.0, palette().warn)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(palette().danger))
                                .text_center()
                                .child(self.error.clone().unwrap_or_default()),
                        ),
                )
                .child(div().w_full().child(ghost_button(
                    "import-share-back",
                    crate::i18n::t("common.back"),
                    view.clone(),
                    |this, _window, cx| {
                        this.phase = Phase::Input;
                        this.error = None;
                        cx.notify();
                    },
                )));
        };

        let data = &share.payload;
        let existing: HashSet<String> = self
            .share_existing
            .iter()
            .map(|n| n.trim().to_lowercase())
            .collect();
        let installable = data
            .s
            .iter()
            .filter(|s| !existing.contains(&s.n.trim().to_lowercase()))
            .count();
        let has_embedded = data.s.iter().any(|s| s.c.is_some());
        let has_private = data.s.iter().any(|s| s.p.unwrap_or(false));

        // Header: emoji well + name/desc + count
        let mut col = div().flex().flex_col().child(
            div()
                .px(px(24.0))
                .pt(px(20.0))
                .pb_3()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .size(px(40.0))
                                .rounded_xl()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_lg()
                                .bg(linear_gradient(
                                    135.0,
                                    linear_color_stop(rgb(palette().accent).alpha(0.15), 0.),
                                    linear_color_stop(rgb(palette().violet).alpha(0.15), 1.),
                                ))
                                .child(if data.i.trim().is_empty() {
                                    "⭐".to_string()
                                } else {
                                    data.i.clone()
                                }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(data.n.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(palette().fg_muted))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(data.d.clone()),
                                ),
                        )
                        .child(count_pill(format!("{} skills", data.s.len()))),
                )
                .when(has_embedded, |d| {
                    d.child(hint_banner(
                        IconName::Package,
                        crate::i18n::t("importShareCodeModal.embeddedDesc"),
                        palette().accent,
                    ))
                })
                .when(has_private, |d| {
                    d.child(hint_banner(
                        IconName::TriangleAlert,
                        crate::i18n::t("importShareCodeModal.privateRepoDesc"),
                        palette().warn,
                    ))
                })
                .when(!self.share_existing.is_empty(), |d| {
                    d.child(hint_banner(
                        IconName::Check,
                        crate::i18n::tf(
                            "shareCodeImport.alreadyDetected",
                            &[("count", &self.share_existing.len().to_string())],
                        ),
                        palette().ok,
                    ))
                }),
        );

        // Skill list
        let mut rows = div().flex().flex_col().gap(px(2.0));
        for (ix, skill) in data.s.iter().enumerate() {
            let is_existing = existing.contains(&skill.n.trim().to_lowercase());
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_xl()
                    .id(ElementId::Name(format!("share-skill-{ix}").into()))
                    .interaction_spring(
                        format!("share-skill-{ix}"),
                        true,
                        MotionPaint::new(),
                        MotionPaint::new().bg(rgb(palette().panel_hover)),
                    )
                    .child(
                        div()
                            .size(px(16.0))
                            .flex_shrink_0()
                            .rounded(px(4.0))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(is_existing, |d| {
                                d.bg(rgb(palette().ok).alpha(0.20))
                                    .border_color(rgb(palette().ok).alpha(0.40))
                            })
                            .when(!is_existing, |d| {
                                d.bg(rgb(palette().accent))
                                    .border_color(rgb(palette().accent))
                            })
                            .child(icon(IconName::Check, 10.0, palette().on_accent)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(skill.n.clone()),
                            )
                            .when(is_existing, |d| {
                                d.child(badge(
                                    crate::i18n::t("githubImportModal.installed"),
                                    palette().ok,
                                    palette().ok_bg,
                                ))
                            })
                            .when(skill.c.is_some(), |d| {
                                d.child(badge("embedded".into(), palette().info, palette().info_bg))
                            })
                            .when(skill.p.unwrap_or(false), |d| {
                                d.child(badge("private".into(), palette().warn, palette().warn_bg))
                            }),
                    )
                    .when(!skill.u.is_empty(), |d| match skill.remote() {
                        Ok(remote) => d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .max_w(px(220.0))
                                .flex_shrink_0()
                                .child(icon(IconName::GitBranch, 12.0, palette().fg_muted))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(palette().fg_muted))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(remote.label()),
                                ),
                        ),
                        Err(_) => d.child(badge(
                            crate::i18n::t("shareCodeImport.unsupportedSource"),
                            palette().danger,
                            palette().danger_bg,
                        )),
                    }),
            );
        }
        col = col.child(
            div()
                .px(px(24.0))
                .pb_2()
                .max_h(px(280.0))
                .overflow_y_scrollbar()
                .child(rows),
        );

        // Footer: Back + Install N
        let back = view.clone();
        let install = view;
        col.child(
            div()
                .px(px(24.0))
                .py(px(14.0))
                .border_t_1()
                .border_color(rgb(palette().border_soft))
                .flex()
                .items_center()
                .justify_between()
                .child(ghost_button(
                    "import-share-back2",
                    crate::i18n::t("common.back"),
                    back,
                    |this, _window, cx| {
                        this.phase = Phase::Input;
                        this.share = None;
                        this.error = None;
                        cx.notify();
                    },
                ))
                .child(
                    div()
                        .id("import-share-install")
                        .h(px(28.0))
                        .px(px(20.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .rounded_md()
                        .bg(rgb(palette().accent))
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(palette().on_accent))
                        .child(icon(IconName::Download, 14.0, palette().on_accent))
                        .child(crate::i18n::tf(
                            "shareCodeImport.installSkills",
                            &[("count", &installable.to_string())],
                        ))
                        .on_click(move |_, _window, cx| {
                            let _ = install.update(cx, |this, cx| this.install_share(cx));
                        })
                        .interaction_spring(
                            "import-share-install",
                            true,
                            MotionPaint::new().bg(rgb(palette().accent)),
                            MotionPaint::new().bg(rgb(palette().accent_hover)),
                        ),
                ),
        )
    }
}

/// Warning/info strip — `rounded-lg bg-X/5 border-X/20 px-2.5 py-2 text-xs`.
fn hint_banner(glyph: IconName, text: SharedString, ink: u32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .rounded_lg()
        .bg(rgb(ink).alpha(0.05))
        .border_1()
        .border_color(rgb(ink).alpha(0.20))
        .px(px(10.0))
        .py_2()
        .text_xs()
        .text_color(rgb(ink))
        .child(icon(glyph, 14.0, ink))
        .child(text)
}
