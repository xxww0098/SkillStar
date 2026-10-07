//! Phase renderers for the URL input, loading, completed and failed
//! screens, plus the `ModalHeader` row and the small chips/banners/buttons
//! they share (`share.rs` and `select.rs` reuse them). React sources:
//! `import-modal/*.tsx`.

use gpui_kit::assets::IconName;
use gpui_kit::component::Disableable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::{ImportDialog, Phase, looks_like_share_code};
use crate::chrome::{InteractionSpring, MotionPaint, icon, icon_spin};
use crate::theme::palette;

/// Rows that fit before the recent-repo list has to scroll.
const RECENT_VISIBLE: usize = 4;
/// Viewport height. A `max_h` on the scrollable does not create wheel overflow.
const RECENT_VIEW_H: f32 = 144.0;

impl ImportDialog {
    /// `ModalHeader` — `px-6 pt-4 pb-3` + border-b, 32px accent well with the
    /// Download glyph, `text-heading-sm` title. The Pack phase is React's
    /// `CreateGroupModal`, whose header carries no icon.
    pub(super) fn render_header(&self) -> Div {
        let pack = self.phase == Phase::Pack;
        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px(px(24.0))
            .pt(px(16.0))
            .pb(px(12.0))
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(palette().border_soft))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .when(!pack, |d| {
                        d.child(
                            div()
                                .size(px(32.0))
                                .rounded_xl()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(rgb(palette().accent).alpha(0.10))
                                .child(icon(IconName::Download, 16.0, palette().accent)),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(palette().fg))
                            .child(if pack {
                                crate::i18n::t("createGroupModal.newGroup")
                            } else {
                                crate::i18n::t("common.import")
                            }),
                    ),
            )
    }

    /// Source step: address field with scan and full-depth scan, then the
    /// local file and folder actions on one row, then recent repositories.
    pub(super) fn render_input(&self, cx: &mut Context<Self>) -> Div {
        let url_value = self.url.read(cx).value().to_string();
        let can_scan = !url_value.trim().is_empty();
        let view = cx.entity().downgrade();

        let mut field = div().flex().flex_col().gap(px(8.0)).child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(rgb(palette().fg_muted))
                .child(crate::i18n::t("shareCodeImport.smartInputHint")),
        );

        // Share-code detected banner (clipboard-prefilled flow).
        if looks_like_share_code(&url_value) {
            field = field.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette().accent).alpha(0.20))
                    .bg(rgb(palette().accent).alpha(0.05))
                    .text_xs()
                    .text_color(rgb(palette().accent))
                    .child(icon(IconName::Share2, 14.0, palette().accent))
                    .child(crate::i18n::t("shareCodeImport.detected")),
            );
        }

        // Default medium field is text-sm. `.large()` was text-base in a 44px
        // bar, so the placeholder read heavier than the rest of the dialog.
        let scan_view = view.clone();
        let deep_view = view.clone();
        let deep_hint = crate::i18n::t("githubImportModal.deepScanHint");
        field = field.child(
            Input::new(&self.url)
                .w_full()
                .font_weight(FontWeight::NORMAL)
                .prefix(div().flex().items_center().child(icon(
                    IconName::Search,
                    14.0,
                    palette().fg_muted,
                ))),
        );
        field = field.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(scan_button(
                    "import-scan",
                    crate::i18n::t("githubImportModal.scan"),
                    true,
                    can_scan,
                    scan_view,
                    false,
                ))
                .child(
                    div()
                        .id("import-deep-scan-tip")
                        .flex_shrink_0()
                        .tooltip(move |window, cx| {
                            crate::chrome::tooltip(deep_hint.clone()).build(window, cx)
                        })
                        .child(scan_button(
                            "import-deep-scan-start",
                            crate::i18n::t("githubImportModal.fullDepthLabel"),
                            false,
                            can_scan,
                            deep_view,
                            true,
                        )),
                ),
        );

        let mut col = div()
            .flex()
            .flex_col()
            .px(px(24.0))
            .pt(px(16.0))
            .pb(px(20.0))
            .gap(px(16.0))
            .child(field);

        // — or — local file / folder sources, one row.
        col = col.child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(div().flex_1().h(px(1.0)).bg(rgb(palette().border_soft)))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(palette().fg_muted))
                                .child(crate::i18n::t("common.or")),
                        )
                        .child(div().flex_1().h(px(1.0)).bg(rgb(palette().border_soft))),
                )
                .child(
                    div()
                        .flex()
                        .items_stretch()
                        .gap_2()
                        .child(source_button(
                            "import-pick-file",
                            IconName::Package,
                            crate::i18n::t("importBundleModal.pickFile"),
                            view.clone(),
                            |this, cx| this.pick_bundle(cx),
                        ))
                        .child(source_button(
                            "import-pick-folder",
                            // FolderTree's 16px GPUI raster reads as "t匚", not a folder.
                            IconName::Folder,
                            crate::i18n::t("importModal.adoptFolder"),
                            view.clone(),
                            |this, cx| this.pick_folder(cx),
                        )),
                ),
        );

        // Recent repo history. More than four rows scroll inside a fixed
        // viewport. `max_h` on the scrollable stays on the content after
        // `overflow_y_scrollbar` rewrites it, so the wheel has no overflow.
        if !self.history.is_empty() {
            let scroll = self.history.len() > RECENT_VISIBLE;
            let mut list = div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .w_full()
                .min_w_0()
                .rounded_lg();
            if scroll {
                list = list.h(px(RECENT_VIEW_H)).flex_shrink_0();
            }
            for (ix, entry) in self.history.iter().enumerate() {
                let source = entry.source.clone();
                let pick = view.clone();
                list = list.child(
                    div()
                        .id(ElementId::Integer(ix as u64))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .cursor_pointer()
                        .child(icon(IconName::Clock, 14.0, palette().fg_muted))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(palette().fg))
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(source.clone()),
                        )
                        .on_click(move |_, window, cx| {
                            let source = source.clone();
                            let _ = pick.update(cx, |this, cx| {
                                let _ = this.url.update(cx, |state, cx| {
                                    state.set_value(source.clone(), window, cx)
                                });
                                this.scan(source, this.full_depth, cx);
                            });
                        })
                        .interaction_spring(
                            format!("import-history-{ix}"),
                            true,
                            MotionPaint::new(),
                            MotionPaint::new().bg(rgb(palette().panel_hover)),
                        ),
                );
            }
            col = col.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("githubImportModal.recentRepos")),
                    )
                    .child(if scroll {
                        div()
                            .h(px(RECENT_VIEW_H))
                            .w_full()
                            .min_w_0()
                            .flex_shrink_0()
                            .child(list.overflow_y_scrollbar().id("import-recent-repos"))
                            .into_any_element()
                    } else {
                        list.into_any_element()
                    }),
            );
        }
        col
    }

    /// `LoadingPhase`: spinner well + message + optional Cancel.
    pub(super) fn render_loading(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.entity().downgrade();
        let cancellable = matches!(self.phase, Phase::Scanning | Phase::Installing);
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(64.0))
            .gap_4()
            .child(
                div()
                    .size(px(48.0))
                    .rounded_2xl()
                    .bg(rgb(palette().accent).alpha(0.10))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon_spin(
                        "import-loading-spin",
                        IconName::Loader,
                        24.0,
                        palette().accent,
                        true,
                    )),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .child(self.progress.clone().unwrap_or_default()),
            )
            .when(cancellable, |d| {
                d.child(ghost_button(
                    "import-cancel",
                    crate::i18n::t("common.cancel"),
                    view.clone(),
                    |this, _window, _cx| this.cancel_active(),
                ))
            })
    }

    /// `CompletedPhase`: success well + headline + optional share summary.
    pub(super) fn render_completed(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.entity().downgrade();
        let mut col = div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(40.0))
            .px(px(24.0))
            .gap_4()
            .child(
                div()
                    .size(px(56.0))
                    .rounded_2xl()
                    .bg(rgb(palette().ok).alpha(0.10))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(IconName::CircleCheckBig, 28.0, palette().ok)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .w_full()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::i18n::t("githubImportModal.titleComplete")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(palette().fg_muted))
                            .text_center()
                            .child(match &self.summary {
                                Some(summary) => crate::i18n::tf(
                                    "shareCodeImport.resultSummary",
                                    &[
                                        ("total", &summary.requested_count.to_string()),
                                        ("existing", &summary.existing_names.len().to_string()),
                                        (
                                            "installed",
                                            &(summary.installed_names.len()
                                                + summary.embedded_names.len())
                                            .to_string(),
                                        ),
                                        ("skipped", &summary.skipped.len().to_string()),
                                    ],
                                ),
                                None => crate::i18n::tf(
                                    "githubImportModal.descComplete",
                                    &[("count", &self.installed.to_string())],
                                ),
                            }),
                    ),
            );

        // Share-code summary chips: already had / installed now / skipped.
        if let Some(summary) = &self.summary {
            let mut block = div()
                .w_full()
                .max_h(px(220.0))
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap_3();
            if !summary.existing_names.is_empty() {
                block = block.child(chip_group(
                    crate::i18n::tf(
                        "shareCodeImport.alreadyHadTitle",
                        &[("count", &summary.existing_names.len().to_string())],
                    ),
                    &summary.existing_names,
                    palette().fg,
                    palette().well,
                ));
            }
            let installed_names: Vec<String> = summary
                .installed_names
                .iter()
                .chain(summary.embedded_names.iter())
                .cloned()
                .collect();
            if !installed_names.is_empty() {
                block = block.child(chip_group(
                    crate::i18n::tf(
                        "shareCodeImport.installedNowTitle",
                        &[("count", &installed_names.len().to_string())],
                    ),
                    &installed_names,
                    palette().ok,
                    palette().ok_bg,
                ));
            }
            if !summary.skipped.is_empty() {
                let mut skipped = div().flex().flex_col().gap(px(4.0)).child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(palette().fg_muted))
                        .child(crate::i18n::tf(
                            "shareCodeImport.skippedTitle",
                            &[("count", &summary.skipped.len().to_string())],
                        )),
                );
                for entry in &summary.skipped {
                    // `embedded_failed` rolls into `install_failed`, matching
                    // React's knownReasons fallback.
                    let reason = match entry.reason.as_str() {
                        "repo_missing" | "no_source" | "install_failed" | "unsupported_source"
                        | "embedded_failed" | "cancelled" => entry.reason.as_str(),
                        _ => "install_failed",
                    };
                    skipped = skipped.child(
                        div()
                            .text_xs()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette().warn).alpha(0.20))
                            .bg(rgb(palette().warn).alpha(0.05))
                            .px_2()
                            .py(px(6.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .flex_shrink_0()
                                    .text_color(rgb(palette().warn))
                                    .child(crate::i18n::t(&format!(
                                        "shareCodeImport.skipReason.{reason}"
                                    ))),
                            ),
                    );
                }
                block = block.child(skipped);
            }
            col = col.child(block);
        }

        // Installed fine, but linking into enabled Agents fell short.
        if let Some(notice) = &self.error {
            col = col.child(
                div()
                    .w_full()
                    .text_xs()
                    .text_color(rgb(palette().warn))
                    .child(notice.clone()),
            );
        }

        col.child(div().flex().gap_2().mt_2().child(primary_button(
            "import-done",
            crate::i18n::t("githubImportModal.done"),
            view.clone(),
            |this, window, cx| this.close(window, cx),
        )))
    }

    /// `ErrorPhase`: amber well + headline + message + Try again (→ input).
    pub(super) fn render_failed(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.entity().downgrade();
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(56.0))
            .px(px(24.0))
            .gap_4()
            .child(
                div()
                    .size(px(56.0))
                    .rounded_2xl()
                    .bg(rgb(palette().warn).alpha(0.10))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(IconName::TriangleAlert, 28.0, palette().warn)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::i18n::t("githubImportModal.somethingWrong")),
                    )
                    .child(
                        div()
                            .max_w(px(320.0))
                            .text_sm()
                            .text_color(rgb(palette().fg_muted))
                            .text_center()
                            .child(self.error.clone().unwrap_or_default()),
                    ),
            )
            .child(
                div()
                    .id("import-retry")
                    .mt_1()
                    .h(px(28.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(palette().fg))
                    .interaction_spring(
                        "import-retry",
                        true,
                        MotionPaint::new(),
                        MotionPaint::new().bg(rgb(palette().panel_hover)),
                    )
                    .child(icon(IconName::RotateCcw, 14.0, palette().fg_muted))
                    .child(crate::i18n::t("githubImportModal.tryAgain"))
                    .on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| this.reset(window, cx));
                    }),
            )
    }
}

// ── Shared chips / banners / buttons ───────────────────────────────

/// Muted count pill — `text-micro bg-muted px-1.5 py-0.5 rounded-md`.
pub(super) fn count_pill(label: String) -> Div {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded_md()
        .bg(rgb(palette().well))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().fg_muted))
        .flex_shrink_0()
        .child(label)
}

/// Small inline status chip — `text-micro px-1.5 py-0.5 rounded-full`.
pub(super) fn badge(label: SharedString, fg: u32, bg: u32) -> Div {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded_full()
        .bg(rgb(bg))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .flex_shrink_0()
        .text_color(rgb(fg))
        .child(label)
}

/// `Button variant="ghost" size="sm"` — label-only row action.
pub(super) fn ghost_button(
    id: &'static str,
    label: SharedString,
    view: WeakEntity<ImportDialog>,
    action: impl Fn(&mut ImportDialog, &mut Window, &mut Context<ImportDialog>) + 'static,
) -> crate::chrome::MotionDiv {
    div()
        .id(id)
        .h(px(28.0))
        .px_3()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().fg))
        .interaction_spring(
            id,
            true,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().panel_hover)),
        )
        .child(label)
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| action(this, window, cx));
        })
}

/// `Button size="sm"` primary fill — footer installs and Done.
pub(super) fn primary_button(
    id: &'static str,
    label: SharedString,
    view: WeakEntity<ImportDialog>,
    action: impl Fn(&mut ImportDialog, &mut Window, &mut Context<ImportDialog>) + 'static,
) -> crate::chrome::MotionDiv {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded_md()
        .bg(rgb(palette().accent))
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().on_accent))
        .child(label)
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| action(this, window, cx));
        })
        .interaction_spring(
            id,
            true,
            MotionPaint::new().bg(rgb(palette().accent)),
            MotionPaint::new().bg(rgb(palette().accent_hover)),
        )
}

/// Scan or full-depth scan. Primary is the Enter action; outline is the
/// slower nested search.
fn scan_button(
    id: &'static str,
    label: SharedString,
    primary: bool,
    enabled: bool,
    view: WeakEntity<ImportDialog>,
    full_depth: bool,
) -> Button {
    let mut button = Button::new(id)
        .flex_shrink_0()
        .label(label)
        .disabled(!enabled)
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| {
                let text = this.url.read(cx).value().trim().to_string();
                if text.is_empty() {
                    return;
                }
                this.scan(text, full_depth, cx);
            });
        });
    if primary {
        button = button.primary();
    } else {
        button = button.outline();
    }
    button
}

/// Local file or folder import. The two actions share one row.
fn source_button(
    id: &'static str,
    glyph: IconName,
    label: SharedString,
    view: WeakEntity<ImportDialog>,
    action: impl Fn(&mut ImportDialog, &mut Context<ImportDialog>) + 'static,
) -> Div {
    div().flex_1().min_w_0().child(
        Button::new(id)
            .w_full()
            .outline()
            .icon(glyph)
            .label(label)
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| action(this, cx));
            }),
    )
}

/// Chips under the Completed share summary — muted label + wrapped pills.
fn chip_group(title: SharedString, names: &[String], fg: u32, bg: u32) -> Div {
    let mut wrap = div().flex().flex_wrap().gap(px(6.0));
    for name in names {
        wrap = wrap.child(
            div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded_md()
                .bg(rgb(bg))
                .text_size(px(11.0))
                .text_color(rgb(fg))
                .child(name.clone()),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(palette().fg_muted))
                .child(title),
        )
        .child(wrap)
}
