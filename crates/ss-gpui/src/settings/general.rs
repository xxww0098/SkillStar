//! Language, appearance, background run, and proxy.
//! React sources: the matching files under `src/features/settings/sections/`.

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::i18n::{t, tf};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::config::proxy::{ProxyType, save_config};

use super::{
    SettingsPage, SettingsSection, card, choice_pills, collapse_chevron, field_label, meta_chip,
    section_shell,
};
use crate::theme::palette;

impl SettingsPage {
    pub(crate) fn render_language(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let langs = [
            ("zh-CN", SharedString::from("简体中文")),
            ("en", SharedString::from("English")),
        ];
        section_shell(
            SettingsSection::Language,
            None,
            None,
            card().px_4().py_4().child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(t("settings.language")),
                    )
                    .child(choice_pills(
                        "lang",
                        &langs,
                        &self.prefs.language,
                        view,
                        |this, lang, cx| {
                            this.prefs.language = lang.to_string();
                            this.save_prefs(cx);
                            crate::i18n::set_language(cx, lang);
                        },
                    )),
            ),
        )
    }

    pub(crate) fn render_appearance(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let styles = [
            ("paper", t("settings.backgroundPaper")),
            ("current", t("settings.backgroundCurrent")),
        ];
        section_shell(
            SettingsSection::Appearance,
            None,
            None,
            card().px_4().py_4().child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(t("settings.backgroundStyle")),
                    )
                    .child(choice_pills(
                        "appearance",
                        &styles,
                        &self.prefs.background_style,
                        view,
                        |this, style, cx| {
                            this.prefs.background_style = style.to_string();
                            this.save_prefs(cx);
                            crate::theme::set_mode(style == "paper", cx);
                        },
                    )),
            ),
        )
    }

    pub(crate) fn render_background_run(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let enabled = self.prefs.background_run;
        section_shell(
            SettingsSection::BackgroundRun,
            None,
            None,
            card().px_4().py_4().child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .max_w(px(520.0))
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .child(t("settings.backgroundRunHint")),
                    )
                    .child(Self::toggle("bg-run-toggle", enabled, view, |this, cx| {
                        this.prefs.background_run = !this.prefs.background_run;
                        this.save_prefs(cx);
                    })),
            ),
        )
    }

    pub(crate) fn render_proxy(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let enabled = self.proxy.enabled;
        let open = self.proxy_expanded;
        let chip = (enabled && !self.proxy.host.is_empty()).then(|| {
            meta_chip(format!(
                "{}://{}:{}",
                self.proxy.proxy_type.as_scheme().to_ascii_uppercase(),
                self.proxy.host,
                self.proxy.port
            ))
            .into_any_element()
        });
        let toggle = Self::toggle("proxy-enabled", enabled, view.clone(), |this, cx| {
            this.proxy.enabled = !this.proxy.enabled;
            this.save_proxy(cx);
        });

        let mut body = card().when(!enabled, |d| d.opacity(0.7));
        let v = view.clone();
        body = body.child(
            div()
                .id("proxy-collapse")
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
                        .child(t("settings.proxyConfigTitle")),
                )
                .child(collapse_chevron(open))
                .on_click(move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| {
                        this.proxy_expanded = !this.proxy_expanded;
                        cx.notify();
                    });
                })
                .interaction_spring(
                    "proxy-collapse",
                    true,
                    MotionPaint::new(),
                    MotionPaint::new().bg(rgb(palette().card_hover)),
                ),
        );

        if open {
            let mut form = div()
                .flex()
                .flex_col()
                .gap_3()
                .w_full()
                .px_4()
                .pt_1()
                .pb_4()
                .border_t_1()
                .border_color(rgb(palette().border))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_3()
                        .w_full()
                        .items_end()
                        .child(
                            field_label(
                                t("settings.proxyType"),
                                self.proxy_type_menu(view.clone()),
                            )
                            .w(px(120.0))
                            .flex_none(),
                        )
                        .child(
                            field_label(
                                t("settings.proxyHost"),
                                div().w_full().child(Input::new(&self.proxy_host)),
                            )
                            .flex_1(),
                        )
                        .child(
                            field_label(
                                t("settings.proxyPort"),
                                div().w_full().child(Input::new(&self.proxy_port)),
                            )
                            .w(px(80.0))
                            .flex_none(),
                        ),
                );
            if matches!(
                self.proxy.proxy_type,
                ProxyType::Socks5 | ProxyType::Socks5h
            ) {
                form = form.child(
                    div()
                        .px_1()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(t("settings.proxySocks5hHint")),
                );
            }
            form = form.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .w_full()
                    .items_end()
                    .child(
                        field_label(
                            t("settings.proxyUsername"),
                            div().w_full().child(Input::new(&self.proxy_user)),
                        )
                        .flex_1(),
                    )
                    .child(
                        field_label(
                            t("settings.proxyPassword"),
                            div().w_full().child(Input::new(&self.proxy_pass)),
                        )
                        .flex_1(),
                    )
                    .child(
                        field_label(
                            t("settings.proxyBypass"),
                            div().w_full().child(Input::new(&self.proxy_bypass)),
                        )
                        .flex_1(),
                    ),
            );
            if let Some(status) = &self.proxy_status {
                let saved = status == &t("common.saved").to_string();
                form = form.child(
                    div()
                        .flex()
                        .justify_end()
                        .min_h(px(20.0))
                        .text_xs()
                        .text_color(rgb(if saved {
                            palette().ok
                        } else {
                            palette().danger
                        }))
                        .child(status.clone()),
                );
            }
            body = body.child(form);
        }

        section_shell(
            SettingsSection::Proxy,
            chip,
            Some(toggle.into_any_element()),
            body,
        )
    }

    fn proxy_type_menu(&self, view: WeakEntity<Self>) -> Div {
        let open = self.proxy_type_open;
        let current = self.proxy.proxy_type.as_scheme().to_ascii_uppercase();
        let v = view.clone();
        let mut menu = div().w_full().child(
            div()
                .id("proxy-type-button")
                .h(px(36.0))
                .px_3()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .rounded_xl()
                .border_1()
                .border_color(rgb(palette().border))
                .bg(rgb(palette().bg))
                .cursor_pointer()
                .child(div().text_sm().text_color(rgb(palette().fg)).child(current))
                .child(collapse_chevron(open))
                .on_click(move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| {
                        this.proxy_type_open = !this.proxy_type_open;
                        cx.notify();
                    });
                }),
        );
        if open {
            let mut list = div()
                .mt_1()
                .w_full()
                .rounded_xl()
                .border_1()
                .border_color(rgb(palette().border))
                .bg(rgb(palette().card))
                .overflow_hidden();
            for (id, label) in [
                ("http", "HTTP"),
                ("https", "HTTPS"),
                ("socks5", "SOCKS5"),
                ("socks5h", "SOCKS5H"),
            ] {
                let v = view.clone();
                list = list.child(
                    div()
                        .id(ElementId::Name(format!("proxy-type-{id}").into()))
                        .px_3()
                        .py_2()
                        .w_full()
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(palette().fg))
                        .child(label)
                        .on_click(move |_, _, cx| {
                            let _ = v.update(cx, |this, cx| {
                                this.proxy.proxy_type = match id {
                                    "https" => ProxyType::Https,
                                    "socks5" => ProxyType::Socks5,
                                    "socks5h" => ProxyType::Socks5h,
                                    _ => ProxyType::Http,
                                };
                                this.proxy_type_open = false;
                                this.save_proxy(cx);
                            });
                        })
                        .interaction_spring(
                            format!("proxy-type-{id}"),
                            true,
                            MotionPaint::new(),
                            MotionPaint::new().bg(rgb(palette().card_hover)),
                        ),
                );
            }
            menu = menu.child(list);
        }
        menu
    }

    pub(super) fn save_proxy(&mut self, cx: &mut Context<Self>) {
        let read = |e: &Entity<InputState>| e.read(cx).value().to_string();
        self.proxy.host = read(&self.proxy_host);
        self.proxy.port = read(&self.proxy_port).parse().unwrap_or(7897);
        let user = read(&self.proxy_user);
        self.proxy.username = (!user.is_empty()).then_some(user);
        let pass = read(&self.proxy_pass);
        self.proxy.password = (!pass.is_empty()).then_some(pass);
        let bypass = read(&self.proxy_bypass);
        self.proxy.bypass = (!bypass.is_empty()).then_some(bypass);

        self.proxy_status = Some(match save_config(&self.proxy) {
            Ok(()) => t("common.saved").to_string(),
            Err(err) => tf("settings.connectionFailed", &[("error", &err.to_string())]).to_string(),
        });
        cx.notify();
    }
}
