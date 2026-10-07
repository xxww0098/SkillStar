//! GitHub mirror/accelerator section. React source:
//! `src/features/settings/sections/GitHubMirrorSection.tsx`. The ordered
//! list is the fallback chain — `order[0]` is the selected source.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::config::github_mirror::{self, CUSTOM_ENTRY_ID, builtin_presets};

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::i18n::{t, tf};
use crate::spawn_domain;
use crate::theme::palette;

use super::{SettingsPage, SettingsSection, card, collapse_chevron, meta_chip, section_shell};

impl SettingsPage {
    /// Complete entry order. Missing `order` falls back to the built-in presets.
    pub(crate) fn gh_mirror_order(&self) -> Vec<String> {
        self.gh_mirror
            .order
            .clone()
            .unwrap_or_else(|| builtin_presets().iter().map(|p| p.id.clone()).collect())
    }

    /// URL shown in the header chip. Reads the in-memory order, not disk.
    fn gh_effective_url(&self) -> String {
        let Some(id) = self.gh_mirror_order().into_iter().next() else {
            return String::new();
        };
        if id == CUSTOM_ENTRY_ID {
            return self.gh_mirror.custom_url.clone().unwrap_or_default();
        }
        builtin_presets()
            .into_iter()
            .find(|preset| preset.id == id)
            .map(|preset| preset.url)
            .unwrap_or_default()
    }

    pub(crate) fn save_gh_mirror(&mut self, cx: &mut Context<Self>) {
        let order = self.gh_mirror_order();
        self.gh_mirror.preset_id = order
            .first()
            .filter(|id| id.as_str() != CUSTOM_ENTRY_ID)
            .cloned();
        self.gh_mirror.order = Some(order);
        let url = self.gh_mirror_custom_url.read(cx).value().to_string();
        self.gh_mirror.custom_url = (!url.trim().is_empty()).then(|| url.trim().to_string());
        self.gh_mirror_status = Some(match github_mirror::save_config(&self.gh_mirror) {
            Ok(()) => t("common.saved").to_string(),
            Err(err) => tf("settings.connectionFailed", &[("error", &err.to_string())]).to_string(),
        });
        cx.notify();
    }

    pub(crate) fn render_github_mirror(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let enabled = self.gh_mirror.enabled;
        let open = self.gh_expanded;
        let effective = self.gh_effective_url();
        let chip =
            (enabled && !effective.is_empty()).then(|| meta_chip(effective).into_any_element());
        let toggle = Self::toggle("gh-mirror-enabled", enabled, view.clone(), |this, cx| {
            this.gh_mirror.enabled = !this.gh_mirror.enabled;
            this.save_gh_mirror(cx);
        });

        let mut body = card().when(!enabled, |d| d.opacity(0.7));
        let v = view.clone();
        body = body.child(
            div()
                .id("gh-mirror-collapse")
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .px_4()
                .py_3()
                .cursor_pointer()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(palette().fg))
                        .child(t("settings.githubMirrorConfig")),
                )
                .child(collapse_chevron(open))
                .on_click(move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| {
                        let closing = this.gh_expanded;
                        this.gh_expanded = !this.gh_expanded;
                        if closing {
                            this.save_gh_mirror(cx);
                        } else {
                            cx.notify();
                        }
                    });
                })
                .interaction_spring(
                    "gh-mirror-collapse",
                    true,
                    MotionPaint::new(),
                    MotionPaint::new().bg(rgb(palette().card_hover)),
                ),
        );

        if open {
            body = body.child(self.gh_mirror_form(view));
        }

        section_shell(
            SettingsSection::GitHubMirror,
            chip,
            Some(toggle.into_any_element()),
            body,
        )
    }

    fn gh_mirror_form(&self, view: WeakEntity<Self>) -> Div {
        let order = self.gh_mirror_order();
        let presets = builtin_presets();
        let selected = order.first().cloned();
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .w_full()
            .px_4()
            .pt_1()
            .pb_4()
            .border_t_1()
            .border_color(rgb(palette().border))
            .child(
                div()
                    .px_1()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .whitespace_normal()
                    .child(t("settings.githubMirrorNotice")),
            );

        for (index, entry_id) in order.iter().enumerate() {
            let is_custom = entry_id == CUSTOM_ENTRY_ID;
            let preset = presets.iter().find(|preset| &preset.id == entry_id);
            if !is_custom && preset.is_none() {
                continue;
            }
            let name = if is_custom {
                t("settings.mirrorCustom").to_string()
            } else {
                preset.map(|preset| preset.name.clone()).unwrap_or_default()
            };
            let url = if is_custom {
                self.gh_mirror.custom_url.clone().unwrap_or_default()
            } else {
                preset.map(|preset| preset.url.clone()).unwrap_or_default()
            };
            let is_selected = selected.as_deref() == Some(entry_id.as_str());
            list = list.child(self.gh_entry_row(
                entry_id,
                &name,
                &url,
                index,
                order.len(),
                is_selected,
                is_custom,
                view.clone(),
            ));
        }

        if let Some(status) = &self.gh_mirror_status {
            let ok = status == &t("common.saved").to_string();
            list = list.child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(rgb(if ok { palette().ok } else { palette().danger }))
                    .when(ok, |d| {
                        d.child(
                            Icon::new(IconName::Check)
                                .size(px(12.0))
                                .text_color(rgb(palette().ok)),
                        )
                    })
                    .child(status.clone()),
            );
        }
        list
    }

    fn gh_entry_row(
        &self,
        entry_id: &str,
        name: &str,
        url: &str,
        index: usize,
        len: usize,
        selected: bool,
        custom: bool,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let entry = entry_id.to_string();
        let select_id = entry.clone();
        let v = view.clone();
        let mut row = div()
            .id(ElementId::Name(format!("gh-entry-{entry_id}").into()))
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .w_full()
            .px_3()
            .py(px(10.0))
            .rounded_lg()
            .border_1()
            .cursor_pointer()
            .when(selected, |d| {
                d.border_color(rgb(palette().accent))
                    .bg(rgb(palette().accent_soft))
            })
            .when(!selected, |d| d.border_color(rgb(palette().border)))
            .on_click(move |_, _, cx| {
                let select_id = select_id.clone();
                let _ = v.update(cx, |this, cx| {
                    let mut order = this.gh_mirror_order();
                    order.retain(|item| item != &select_id);
                    order.insert(0, select_id);
                    this.gh_mirror.order = Some(order);
                    this.save_gh_mirror(cx);
                });
            })
            .interaction_spring(
                format!("gh-entry-{entry_id}"),
                true,
                if selected {
                    MotionPaint::new().bg(rgb(palette().accent_soft))
                } else {
                    MotionPaint::new()
                },
                if selected {
                    MotionPaint::new().bg(rgb(palette().accent_soft))
                } else {
                    MotionPaint::new().bg(rgb(palette().card_hover))
                },
            )
            .child(radio_dot(selected))
            .child(self.gh_entry_label(entry_id, name, url, selected, custom));

        row = row.child(reorder_buttons(entry_id, index, len, view.clone()));
        if !custom || selected {
            row = row.child(self.gh_test_button(entry_id, url, view));
        }
        row
    }

    fn gh_entry_label(
        &self,
        entry_id: &str,
        name: &str,
        url: &str,
        selected: bool,
        custom: bool,
    ) -> Div {
        let mut label = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().fg))
                    .child(name.to_string()),
            );
        if custom && selected {
            label = label.child(
                div()
                    .id(ElementId::Name(format!("gh-url-{entry_id}").into()))
                    .mt_1()
                    .on_click(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(Input::new(&self.gh_mirror_custom_url)),
            );
        } else if !url.is_empty() {
            label = label.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .truncate()
                    .child(url.to_string()),
            );
        }
        label
    }

    fn gh_test_button(
        &self,
        entry_id: &str,
        url: &str,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let testing = self.gh_testing_id.as_deref() == Some(entry_id);
        let result = self.gh_test_results.get(entry_id);
        let (color, label) = if testing {
            (palette().fg_muted, "…".to_string())
        } else {
            match result {
                Some(Ok(ms)) => (palette().ok, format!("{ms}ms")),
                Some(Err(_)) => (palette().danger, t("settings.mirrorTestFail").to_string()),
                None => (palette().fg_muted, t("settings.mirrorTest").to_string()),
            }
        };
        let probe_url = url.to_string();
        let probe_id = entry_id.to_string();
        let can_test = !testing && !probe_url.is_empty();
        div()
            .id(ElementId::Name(format!("gh-test-{entry_id}").into()))
            .flex_shrink_0()
            .px(px(10.0))
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(match result {
                Some(Ok(_)) => palette().ok_border,
                Some(Err(_)) => palette().danger_border,
                None => palette().border,
            }))
            .flex()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(rgb(color))
            .when(can_test, |d| d.cursor_pointer())
            .when(!can_test, |d| d.opacity(0.6))
            .when(matches!(result, Some(Ok(_))), |d| {
                d.child(
                    Icon::new(IconName::Wifi)
                        .size(px(12.0))
                        .text_color(rgb(palette().ok)),
                )
            })
            .when(matches!(result, Some(Err(_))) && !testing, |d| {
                d.child(
                    Icon::new(IconName::WifiOff)
                        .size(px(12.0))
                        .text_color(rgb(palette().danger)),
                )
            })
            .when(testing, |d| {
                d.child(
                    Icon::new(IconName::Loader)
                        .size(px(12.0))
                        .text_color(rgb(palette().fg_muted)),
                )
            })
            .child(label)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                if !can_test {
                    return;
                }
                let probe_id = probe_id.clone();
                let probe_url = probe_url.clone();
                let _ = view.update(cx, |this, cx| {
                    this.gh_testing_id = Some(probe_id.clone());
                    cx.notify();
                    let entity = cx.entity();
                    spawn_domain(
                        &entity,
                        cx,
                        async move {
                            github_mirror::test_mirror(&probe_url)
                                .await
                                .map_err(|err| err.to_string())
                        },
                        move |this, cx, res| {
                            this.gh_testing_id = None;
                            this.gh_test_results.insert(probe_id, res);
                            cx.notify();
                        },
                    );
                });
            })
            .interaction_spring(
                format!("gh-test-{entry_id}"),
                can_test,
                MotionPaint::new(),
                MotionPaint::new().bg(rgb(palette().card_hover)),
            )
    }
}

fn radio_dot(selected: bool) -> Div {
    div()
        .size(px(14.0))
        .flex_shrink_0()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |d| {
            d.bg(rgb(palette().accent)).child(
                div()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(rgb(palette().on_accent)),
            )
        })
        .when(!selected, |d| {
            d.border_1().border_color(rgb(palette().fg_muted))
        })
}

fn reorder_buttons(
    entry_id: &str,
    index: usize,
    len: usize,
    view: WeakEntity<SettingsPage>,
) -> Div {
    let mut col = div().flex().flex_col().flex_shrink_0();
    for (delta, icon, suffix) in [
        (-1isize, IconName::ArrowUp, "up"),
        (1, IconName::ArrowDown, "down"),
    ] {
        let disabled = (delta < 0 && index == 0) || (delta > 0 && index + 1 == len);
        let entry = entry_id.to_string();
        let v = view.clone();
        col = col.child(
            div()
                .id(ElementId::Name(format!("gh-{suffix}-{entry_id}").into()))
                .p_1()
                .rounded_md()
                .when(!disabled, |d| d.cursor_pointer())
                .when(disabled, |d| d.opacity(0.3))
                .child(
                    Icon::new(icon)
                        .size(px(12.0))
                        .text_color(rgb(palette().fg_muted)),
                )
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if disabled {
                        return;
                    }
                    let entry = entry.clone();
                    let _ = v.update(cx, |this, cx| {
                        let mut order = this.gh_mirror_order();
                        let Some(i) = order.iter().position(|item| item == &entry) else {
                            return;
                        };
                        let j = i as isize + delta;
                        if j < 0 || j as usize >= order.len() {
                            return;
                        }
                        order.swap(i, j as usize);
                        this.gh_mirror.order = Some(order);
                        this.save_gh_mirror(cx);
                    });
                })
                .interaction_spring(
                    format!("gh-{suffix}-{entry_id}"),
                    !disabled,
                    MotionPaint::new(),
                    MotionPaint::new().bg(rgb(palette().card_hover)),
                ),
        );
    }
    col
}
