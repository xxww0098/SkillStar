//! Dialog chrome: veil, card, action rows.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_usage::catalog::AuthMode;

use super::AccountsPage;
use super::rail::icon_id;
use crate::accounts::theme::palette;
use crate::chrome::icon;
use crate::chrome::{InteractionSpring, MotionPaint};

pub(crate) fn overlay(view: WeakEntity<AccountsPage>, card: Div) -> impl IntoElement {
    let dismiss = view;
    // The veil is this layer, not an absolute sibling. An absolute mask
    // sits above the card and swallows 登录 / 保存.
    // The card is a child, so it still receives the pointer. The page
    // behind this veil does not: a normal hitbox would let those cards hover.
    div()
        .id("accounts-overlay")
        .occlude()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .bg(crate::theme::prompt_veil())
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = dismiss.update(cx, |this, cx| {
                this.confirm_delete_id = None;
                this.confirm_reset_id = None;
                this.reset_acked = false;
                this.close_add(cx);
            });
        })
        .child(card.id("accounts-dialog-card").on_click(|_, _, cx| {
            cx.stop_propagation();
        }))
}

pub(crate) fn dialog_card(width: f32) -> Div {
    div()
        .relative()
        .w(px(width))
        .max_w_full()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .pt(px(22.0))
        .pb(px(24.0))
        .rounded(px(24.0))
        .border_1()
        .border_color(rgb(palette().os_hair))
        .bg(rgb(palette().panel))
        .shadow_xl()
}

pub(crate) fn dialog_head(
    catalog_id: &str,
    title: &str,
    subtitle: &str,
    view: WeakEntity<AccountsPage>,
) -> Div {
    let close = view;
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .pl(px(24.0))
        .pr(px(14.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .min_w_0()
                .when(!catalog_id.is_empty(), |row| {
                    row.child(
                        div()
                            .size(px(36.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(rgb(palette().os_hair))
                            .bg(rgb(palette().os_fill))
                            .flex()
                            .items_center()
                            .justify_center()
                            .overflow_hidden()
                            .child(
                                img(crate::agent_icons::agent_icon_path(icon_id(catalog_id)))
                                    .size(px(20.0)),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(1.0))
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(16.0))
                                .line_height(px(24.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(palette().fg))
                                .child(title.to_string()),
                        )
                        .when(!subtitle.is_empty(), |col| {
                            col.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(palette().os_muted))
                                    .child(subtitle.to_string()),
                            )
                        }),
                ),
        )
        .child(
            div()
                .id("accounts-dialog-close")
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0))
                .cursor_pointer()
                .text_color(rgb(palette().os_muted))
                .interaction_spring(
                    "accounts-dialog-close",
                    true,
                    MotionPaint::new(),
                    MotionPaint::new()
                        .bg(rgb(palette().os_fill))
                        .fg(rgb(palette().fg)),
                )
                .child(icon(
                    gpui_kit::assets::IconName::X,
                    14.0,
                    palette().os_muted,
                ))
                .on_click(move |_, _, cx| {
                    let _ = close.update(cx, |this, cx| {
                        this.confirm_delete_id = None;
                        this.confirm_reset_id = None;
                        this.reset_acked = false;
                        this.close_add(cx);
                    });
                }),
        )
}

pub(crate) fn paste_hint<'a>(mode: AuthMode, warning: Option<&'a str>) -> Option<&'a str> {
    if let Some(warning) = warning.filter(|text| !text.is_empty()) {
        return Some(warning);
    }
    match mode {
        AuthMode::TokenImport | AuthMode::OAuth => None,
        _ => {
            let hint = field_copy(mode).1;
            (!hint.is_empty()).then_some(hint)
        }
    }
}

pub(crate) fn or_divider() -> Div {
    let line = || div().flex_1().h(px(1.0)).bg(rgb(palette().os_hair));
    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .w_full()
        .child(line())
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(palette().os_faint))
                .child("或粘贴"),
        )
        .child(line())
}

pub(crate) fn region_name(id: &str) -> &str {
    match id {
        "zai" => "Z.ai",
        "bigmodel" => "智谱",
        other => other,
    }
}

pub(crate) fn field_copy(mode: AuthMode) -> (&'static str, &'static str) {
    match mode {
        AuthMode::ApiKey => ("API Key", "粘贴服务商控制台里的密钥"),
        AuthMode::Cookie => ("Cookie", "粘贴浏览器里复制的 Cookie"),
        AuthMode::TokenImport => ("凭证", "卡密、会话 JSON 或 refresh token"),
        AuthMode::Manual => ("显示名称", "只在列表里显示，不会登录"),
        AuthMode::OAuth => ("", ""),
    }
}

pub(crate) fn action_row(
    element_id: &str,
    label: &str,
    hint: &str,
    busy_label: &str,
    primary: bool,
    spinning: bool,
    disabled: bool,
    view: WeakEntity<AccountsPage>,
    run: impl Fn(&mut AccountsPage, &mut Context<AccountsPage>) + 'static,
) -> impl IntoElement {
    let colors = palette();
    let title = if spinning { busy_label } else { label };
    let bg = if primary { colors.accent } else { colors.panel };
    let fg = if primary { colors.on_accent } else { colors.fg };
    let border = if primary {
        colors.accent
    } else {
        colors.os_edge
    };
    let hover_bg = if primary {
        colors.accent_hover
    } else {
        colors.os_fill
    };
    let trailing = if spinning {
        crate::chrome::icon_spin(
            ElementId::Name(format!("{element_id}-spin").into()),
            gpui_kit::assets::IconName::Loader,
            14.0,
            if primary {
                colors.on_accent
            } else {
                colors.accent
            },
            true,
        )
        .into_any_element()
    } else {
        div().into_any_element()
    };
    div()
        .id(ElementId::Name(element_id.to_string().into()))
        .w_full()
        .min_h(px(52.0))
        .px(px(14.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgb(border))
        .bg(rgb(bg))
        .when(disabled && !spinning, |row| row.opacity(0.45))
        .when(!disabled, |row| row.cursor_pointer())
        .interaction_spring(
            element_id.to_string(),
            !disabled,
            MotionPaint::new().bg(rgb(bg)).border(rgb(border)),
            MotionPaint::new().bg(rgb(hover_bg)).border(rgb(if primary {
                colors.accent_hover
            } else {
                colors.os_active_edge
            })),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(fg))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(if primary {
                            colors.on_accent
                        } else {
                            colors.os_muted
                        }))
                        .when(primary, |line| line.opacity(0.82))
                        .child(hint.to_string()),
                ),
        )
        .child(trailing)
        .on_click(move |_, _, cx| {
            if disabled {
                return;
            }
            let _ = view.update(cx, |this, cx| run(this, cx));
        })
}

pub(crate) fn commit_button(
    element_id: &str,
    label: &str,
    busy_label: &str,
    primary: bool,
    spinning: bool,
    disabled: bool,
    view: WeakEntity<AccountsPage>,
    run: impl Fn(&mut AccountsPage, &mut Context<AccountsPage>) + 'static,
) -> impl IntoElement {
    let bg = if primary {
        palette().accent
    } else {
        palette().panel
    };
    let fg = if primary {
        palette().on_accent
    } else {
        palette().fg
    };
    let border = if primary {
        palette().accent
    } else {
        palette().os_edge
    };
    let hover = if primary {
        palette().accent_hover
    } else {
        palette().os_fill
    };
    let hover_border = if primary {
        palette().accent_hover
    } else {
        palette().os_active_edge
    };
    div()
        .id(ElementId::Name(element_id.to_string().into()))
        .w_full()
        .h(px(40.0))
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgb(border))
        .bg(rgb(bg))
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(fg))
        .when(disabled && !spinning, |row| row.opacity(0.45))
        .when(!disabled, |row| row.cursor_pointer())
        .interaction_spring(
            element_id.to_string(),
            !disabled,
            MotionPaint::new().bg(rgb(bg)).border(rgb(border)),
            MotionPaint::new().bg(rgb(hover)).border(rgb(hover_border)),
        )
        .child(if spinning {
            busy_label.to_string()
        } else {
            label.to_string()
        })
        .when(spinning, |row| {
            row.child(crate::chrome::icon_spin(
                ElementId::Name(format!("{element_id}-spin").into()),
                gpui_kit::assets::IconName::Loader,
                14.0,
                if primary {
                    palette().on_accent
                } else {
                    palette().accent
                },
                true,
            ))
        })
        .on_click(move |_, _, cx| {
            if disabled {
                return;
            }
            let _ = view.update(cx, |this, cx| run(this, cx));
        })
}

pub(crate) fn ghost_button(
    element_id: &str,
    label: &str,
    view: WeakEntity<AccountsPage>,
    run: impl Fn(&mut AccountsPage, &mut Context<AccountsPage>) + 'static,
) -> impl IntoElement {
    div()
        .id(ElementId::Name(element_id.to_string().into()))
        .h(px(36.0))
        .px_4()
        .flex()
        .items_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(palette().os_edge))
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .interaction_spring(
            element_id.to_string(),
            true,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().os_fill)),
        )
        .child(label.to_string())
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| run(this, cx));
        })
}

pub(crate) fn primary_button(
    element_id: &str,
    label: &str,
    danger: bool,
    view: WeakEntity<AccountsPage>,
    run: impl Fn(&mut AccountsPage, &mut Context<AccountsPage>) + 'static,
) -> impl IntoElement {
    div()
        .id(ElementId::Name(element_id.to_string().into()))
        .h(px(36.0))
        .px_4()
        .flex()
        .items_center()
        .rounded(px(8.0))
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(if danger {
            palette().os_bad
        } else {
            palette().fg
        }))
        .bg(rgb(if danger {
            palette().os_fill
        } else {
            palette().os_fill_2
        }))
        .border_1()
        .border_color(rgb(if danger {
            palette().os_bad
        } else {
            palette().os_edge
        }))
        .cursor_pointer()
        .child(label.to_string())
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| run(this, cx));
        })
        .interaction_spring(
            element_id.to_string(),
            true,
            MotionPaint::new().bg(rgb(if danger {
                palette().os_fill
            } else {
                palette().os_fill_2
            })),
            MotionPaint::new().bg(rgb(palette().os_fill_2)),
        )
}

#[cfg(test)]
mod tests {
    use super::{field_copy, paste_hint, region_name};
    use ss_usage::catalog::AuthMode;

    #[test]
    fn secret_fields_have_a_visible_label() {
        assert_eq!(field_copy(AuthMode::ApiKey).0, "API Key");
        assert_eq!(field_copy(AuthMode::TokenImport).0, "凭证");
        assert!(!field_copy(AuthMode::Cookie).1.is_empty());
        assert!(field_copy(AuthMode::OAuth).0.is_empty());
        assert!(paste_hint(AuthMode::TokenImport, None).is_none());
        assert_eq!(
            paste_hint(AuthMode::ApiKey, None),
            Some("粘贴服务商控制台里的密钥")
        );
        assert_eq!(
            paste_hint(AuthMode::TokenImport, Some("注意")),
            Some("注意")
        );
        assert_eq!(region_name("bigmodel"), "智谱");
    }
}
