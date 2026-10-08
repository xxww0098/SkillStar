//! `DeckShareSheet` — the share button on a deck card opens this dialog.
//!
//! Two ways out for one deck: a 7-day `agd-` share code built from the
//! deck's remote sources (local-only skills cannot ride a code), or a
//! `.agd` bundle with the full content of every installed member. The
//! paste-side twins (`apply_share_code` / `apply_bundle`) live in
//! `deck_import.rs`.

use std::path::PathBuf;

use crate::notify::Notice;
use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::share_install::{
    ShareCodeKind, ShareCodePayload, ShareCodeSkill, encode_share_code,
};
use ss_skills::skill_group::SkillGroup;

use crate::chrome::{ghost_button, icon, primary_button};
use crate::spawn_domain;
use crate::theme::palette;

pub(crate) struct DeckShareSheet {
    group: SkillGroup,
    /// The `agd-` code, once the hub lookup lands.
    code: Option<String>,
    /// Members with a remote source — what the code carries.
    code_remote: usize,
    /// Installed-only members a code cannot carry; the bundle can.
    code_local: usize,
    exporting: bool,
    export_path: Option<PathBuf>,
}

impl DeckShareSheet {
    fn new(group: SkillGroup, cx: &mut Context<Self>) -> Self {
        // A code carries repo URLs: deck `skill_sources` first, the hub's
        // git remotes as fallback. Both reads stay off the UI thread.
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async {
                ss_skills::installed_skill::list_installed_skills()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|skill| (skill.name, skill.git_url))
                    .collect::<Vec<(String, String)>>()
            },
            |this, _cx, hub| {
                let mut entries = Vec::new();
                let mut local = 0usize;
                for name in this.group.skills.clone() {
                    let source = this
                        .group
                        .skill_sources
                        .get(&name)
                        .filter(|url| !url.is_empty())
                        .cloned()
                        .or_else(|| {
                            hub.iter()
                                .find(|(hub_name, url)| hub_name == &name && !url.is_empty())
                                .map(|(_, url)| url.clone())
                        });
                    match source {
                        Some(url) => entries.push(ShareCodeSkill {
                            n: name,
                            u: url,
                            c: None,
                            p: None,
                        }),
                        None => local += 1,
                    }
                }
                this.code_remote = entries.len();
                this.code_local = local;
                this.code = Some(encode_share_code(
                    ShareCodeKind::Deck,
                    &ShareCodePayload {
                        n: this.group.name.clone(),
                        d: this.group.description.clone(),
                        i: this.group.icon.clone(),
                        s: entries,
                    },
                ));
            },
        );
        Self {
            group,
            code: None,
            code_remote: 0,
            code_local: 0,
            exporting: false,
            export_path: None,
        }
    }

    fn copy_code(&self, cx: &mut App) {
        let Some(code) = &self.code else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
        crate::notify::toast(
            Notice::success(crate::i18n::t("shareResultCard.copied")),
            cx,
        );
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        self.exporting = true;
        self.export_path = None;
        let names = self.group.skills.clone();
        let deck = self.group.name.clone();
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    ss_skills::skill_bundle::export_deck_bundle(&names, &deck)
                })
                .await
                .map_err(anyhow::Error::new)
                .and_then(|result| result)
            },
            |this, cx, result: anyhow::Result<PathBuf>| {
                this.exporting = false;
                match result {
                    Ok(path) => this.export_path = Some(path),
                    Err(err) => crate::notify::toast(Notice::error(format!("{err:#}")), cx),
                }
            },
        );
    }
}

impl Render for DeckShareSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();

        // ── Share code ────────────────────────────────────────────
        let mut code_block = div()
            .w_full()
            .max_h(px(96.0))
            .overflow_y_scrollbar()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().well))
            .text_size(px(11.0))
            .text_color(rgb(palette().fg_muted));
        code_block = match &self.code {
            Some(code) => code_block.child(code.clone()),
            None => code_block.child(crate::i18n::t("skillCards.shareCodeGenerating")),
        };

        let copy = view.clone();
        let code_section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title(
                IconName::QrCode,
                crate::i18n::t("skillCards.shareCodeSection"),
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(format!(
                        "{} · {}",
                        crate::i18n::t("shareResultCard.pasteToImportHint"),
                        crate::i18n::tf(
                            "skillCards.shareCodeCount",
                            &[("count", &self.code_remote.to_string())],
                        ),
                    )),
            )
            .child(code_block)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(primary_button(
                        "share-copy",
                        crate::i18n::t("common.copy"),
                        copy,
                        |this, _window, cx| this.copy_code(cx),
                    ))
                    .when(self.code_local > 0, |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .text_xs()
                                .text_color(rgb(palette().warn))
                                .child(
                                    crate::i18n::tf(
                                        "skillCards.shareCodeExcluded",
                                        &[("count", &self.code_local.to_string())],
                                    )
                                    .to_string(),
                                ),
                        )
                    }),
            );

        // ── Bundle ────────────────────────────────────────────────
        let export = view.clone();
        let mut bundle_section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title(
                IconName::PackageOpen,
                crate::i18n::t("skillCards.shareBundleSection"),
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t("skillCards.shareBundleHint")),
            )
            .child(primary_button(
                "share-export",
                if self.exporting {
                    crate::i18n::t("skillCards.shareBundleExporting")
                } else {
                    crate::i18n::t("skillCards.shareBundleExport")
                },
                export,
                |this, _window, cx| this.export(cx),
            ))
            .when(self.exporting, |d| d.opacity(0.6));
        if let Some(path) = &self.export_path {
            let folder = path.parent().map(std::path::Path::to_path_buf);
            bundle_section = bundle_section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .truncate()
                            .child(path.to_string_lossy().to_string()),
                    )
                    .when_some(folder, |d, folder| {
                        d.child(ghost_button(
                            "share-open-folder",
                            crate::i18n::t("skillCards.shareBundleOpenFolder"),
                            view.clone(),
                            move |_, _, _| crate::os_open::open_folder(&folder),
                        ))
                    }),
            );
        }

        let body = div()
            .flex()
            .flex_col()
            .gap_4()
            .px(px(24.0))
            .py(px(16.0))
            .child(code_section)
            .child(div().border_t_1().border_color(rgb(palette().border_soft)))
            .child(bundle_section);

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header(&self.group.name))
            .child(body)
    }
}

fn section_title(name: IconName, label: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(palette().fg))
        .child(icon(name, 16.0, palette().accent))
        .child(label.into())
}

/// Same header chrome as `create_group`: `px-6 pt-4 pb-3` + border-b.
fn header(deck_name: &str) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .w_full()
        .px(px(24.0))
        .pt(px(16.0))
        .pb(px(12.0))
        .flex_shrink_0()
        .border_b_1()
        .border_color(rgb(palette().border_soft))
        .child(icon(IconName::Share2, 16.0, palette().accent))
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("skillCards.shareDeck")),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.0))
                .text_color(rgb(palette().fg_muted))
                .truncate()
                .child(deck_name.to_string()),
        )
}

/// Open the deck share sheet as a centered dialog.
pub(crate) fn open_share_sheet(group: &SkillGroup, window: &mut Window, cx: &mut App) {
    let entity = cx.new(|cx| DeckShareSheet::new(group.clone(), cx));
    crate::chrome::open_centered(
        window,
        cx,
        440.0,
        crate::chrome::DialogChrome::Flush,
        move |dialog, frame, _, _| {
            let surface = if crate::theme::is_light() {
                palette().card
            } else {
                palette().panel
            };
            let column = div().flex().flex_col().w_full().child(entity.clone());
            dialog
                .w(px(460.0))
                .p_0()
                .rounded(px(12.0))
                .bg(rgb(surface))
                .border_color(rgb(palette().border))
                .child(frame.measure(column))
        },
    )
}
