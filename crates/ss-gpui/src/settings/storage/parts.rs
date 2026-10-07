//! Standalone pieces of the storage section: the maintenance buttons, the
//! folder reveal, and the two path cards.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_app::storage_maintenance::StorageOverview;

use super::SettingsPage;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::i18n::{t, tf};
use crate::settings::DeleteTarget;
use crate::theme::palette;

pub(super) fn clean_button(
    cleaning: bool,
    loading: bool,
    view: WeakEntity<SettingsPage>,
) -> impl IntoElement {
    div()
        .id("storage-clean")
        .h(px(28.0))
        .px_3()
        .flex()
        .items_center()
        .gap_1()
        .flex_shrink_0()
        .rounded_md()
        .border_1()
        .border_color(rgb(if cleaning || loading {
            palette().border
        } else {
            palette().danger_border
        }))
        .text_xs()
        .text_color(rgb(palette().danger))
        .when(!cleaning && !loading, |d| d.cursor_pointer())
        .when(cleaning || loading, |d| d.opacity(0.6))
        .when(cleaning, |d| {
            d.child(
                Icon::new(IconName::Loader)
                    .size(px(14.0))
                    .text_color(rgb(palette().danger)),
            )
        })
        .when(!cleaning, |d| {
            d.child(
                Icon::new(IconName::Trash)
                    .size(px(14.0))
                    .text_color(rgb(palette().danger)),
            )
        })
        .child(if cleaning {
            t("settings.cleaning")
        } else {
            t("settings.cleanAllCaches")
        })
        .on_click(move |_, _, cx| {
            if cleaning || loading {
                return;
            }
            let _ = view.update(cx, |this, cx| this.clean_caches(cx));
        })
        .interaction_spring(
            "storage-clean",
            !cleaning && !loading,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().danger_bg)),
        )
}

pub(super) fn delete_button(
    id: &'static str,
    target: DeleteTarget,
    page: &SettingsPage,
    view: WeakEntity<SettingsPage>,
) -> impl IntoElement {
    let busy_key = match target {
        DeleteTarget::Hub => "hub",
        DeleteTarget::Cache => "cache",
    };
    let busy = page.storage_busy == Some(busy_key);
    let armed = page.storage_confirm == Some(target);
    let mut button = div()
        .id(id)
        .h(px(28.0))
        .min_w(px(28.0))
        .px_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer();
    if busy {
        button = button.child(
            Icon::new(IconName::Loader)
                .size(px(16.0))
                .text_color(rgb(palette().danger)),
        );
    } else if armed {
        button = button.child(
            div()
                .px_1()
                .text_xs()
                .text_color(rgb(palette().danger))
                .child(t("common.delete")),
        );
    } else {
        button = button.child(
            Icon::new(IconName::Trash)
                .size(px(16.0))
                .text_color(rgb(palette().fg_muted)),
        );
    }
    button
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| this.arm_delete(target, cx));
        })
        .interaction_spring(
            id,
            !busy,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().danger_bg)),
        )
}

pub(super) fn folder_button(
    id: &str,
    path: &str,
    _view: WeakEntity<SettingsPage>,
) -> impl IntoElement {
    let path = path.to_string();
    div()
        .id(ElementId::Name(format!("storage-open-{id}").into()))
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .child(
            Icon::new(IconName::FolderOpen)
                .size(px(16.0))
                .text_color(rgb(palette().fg_muted)),
        )
        .on_click(move |_, _, _| crate::os_open::open_folder(&path))
        .interaction_spring(
            format!("storage-open-{id}"),
            true,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().card_hover)),
        )
}

pub(super) fn path_structure(overview: &StorageOverview) -> Div {
    let relative = hub_relative(&overview.data_root_path, &overview.hub_root_path);
    div()
        .mb_3()
        .rounded_xl()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(root_card(
                    t("settings.storageRootData"),
                    t("settings.storageSourceData"),
                    &overview.data_root_path,
                    &[t("settings.storageConfig"), t("settings.storageHistory")],
                ))
                .child(root_card(
                    t("settings.storageRootHub"),
                    t("settings.storageSourceHub"),
                    &overview.hub_root_path,
                    &[
                        t("settings.storageHub"),
                        t("settings.storageLocal"),
                        t("settings.repoCache"),
                    ],
                )),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(if overview.is_hub_under_data {
                    tf(
                        "settings.storagePathRelationNested",
                        &[("relative", relative.as_str())],
                    )
                } else {
                    t("settings.storagePathRelationIndependent")
                }),
        )
}

fn root_card(title: SharedString, tag: SharedString, path: &str, includes: &[SharedString]) -> Div {
    let includes = includes
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .join(" · ");
    div()
        .flex_1()
        .min_w_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().well))
        .p(px(10.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(palette().fg))
                .child(title)
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(palette().border))
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(tag),
                ),
        )
        .child(
            div()
                .mt_1()
                .text_xs()
                .text_color(rgb(palette().fg))
                .whitespace_normal()
                .child(path.to_string()),
        )
        .child(
            div()
                .mt_1()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(format!("{}: {includes}", t("settings.storagePathContains"))),
        )
}

fn hub_relative(parent: &str, child: &str) -> String {
    let parent = parent.replace('\\', "/");
    let child = child.replace('\\', "/");
    let parent = parent.trim_end_matches('/');
    let child = child.trim_end_matches('/');
    let prefix = format!("{parent}/");
    if child.len() > prefix.len() && child[..prefix.len()].eq_ignore_ascii_case(&prefix) {
        child[prefix.len()..].to_string()
    } else {
        ".agents".into()
    }
}
