//! Network doctor section. React source:
//! `src/features/settings/sections/NetworkDoctorSection.tsx`.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::config::network_doctor::NetworkHostCheck;

use crate::chrome::{InteractionSpring, MotionPaint, icon_spin};
use crate::i18n::t;
use crate::spawn_domain;
use crate::theme::palette;

use super::{SettingsPage, SettingsSection, card, section_shell};

impl SettingsPage {
    pub(crate) fn run_diagnosis(&mut self, cx: &mut Context<Self>) {
        if self.diagnosing {
            return;
        }
        self.diagnosing = true;
        self.diagnosis_error = None;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            ss_core::config::network_doctor::diagnose_network(),
            |this, cx, res| {
                this.diagnosing = false;
                match res {
                    Ok(diagnosis) => {
                        this.diagnosis = Some(diagnosis);
                        this.diagnosis_error = None;
                    }
                    Err(err) => this.diagnosis_error = Some(err.to_string()),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(crate) fn render_network_doctor(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let diagnosing = self.diagnosing;
        let v = view.clone();
        // The section's single action: accent ghost so it reads as the
        // primary button without introducing a filled style nothing else
        // on the page uses.
        let mut action = div()
            .id("doctor-run")
            .h(px(28.0))
            .px_3()
            .flex()
            .items_center()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette().accent_soft_edge))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(palette().accent))
            .when(diagnosing, |d| {
                d.opacity(0.7)
                    .child(icon_spin(IconName::Loader, 14.0, palette().accent, true))
            })
            .when(!diagnosing, |d| d.cursor_pointer())
            .child(t("settings.networkDoctorRun"))
            .interaction_spring(
                "doctor-run",
                !diagnosing,
                MotionPaint::new(),
                MotionPaint::new().bg(rgb(palette().accent_soft)),
            );
        if !diagnosing {
            action = action.on_click(move |_, _, cx| {
                let _ = v.update(cx, |this, cx| this.run_diagnosis(cx));
            });
        }

        let mut body = card().px_4().py_3().flex().flex_col().gap_3().child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(t("settings.networkDoctorHint")),
        );
        if let Some(err) = &self.diagnosis_error {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().danger))
                    .child(err.clone()),
            );
        }
        if let Some(diagnosis) = &self.diagnosis {
            let mut checks = div().flex().flex_col().gap(px(6.0));
            for check in &diagnosis.checks {
                checks = checks.child(check_row(check));
            }
            body = body.child(checks);
            if !diagnosis.recommendations.is_empty() {
                let mut notes = div()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(palette().warn_border))
                    .bg(rgb(palette().warn_bg))
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().warn))
                            .child(t("settings.networkDoctorRecommendations")),
                    );
                for key in &diagnosis.recommendations {
                    notes = notes.child(
                        div()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .whitespace_normal()
                            .child(format!("• {}", recommendation(key))),
                    );
                }
                body = body.child(notes);
            }
        }

        section_shell(
            SettingsSection::NetworkDoctor,
            None,
            Some(action.into_any_element()),
            body,
        )
    }
}

fn check_row(check: &NetworkHostCheck) -> Div {
    let (bg, fg, label) = match check.status.as_str() {
        "ok" => (
            palette().ok_bg,
            palette().ok,
            check
                .latency_ms
                .map(|ms| format!("{ms}ms"))
                .unwrap_or_else(|| "ok".into()),
        ),
        "fail" => (palette().danger_bg, palette().danger, "fail".into()),
        _ => (palette().well, palette().fg_muted, check.status.clone()),
    };
    let mut text = div().min_w_0().flex_1().child(
        div()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(palette().fg))
            .truncate()
            .child(check.label.clone()),
    );
    if !check.url.is_empty() {
        text = text.child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .truncate()
                .child(check.url.clone()),
        );
    }
    if let Some(detail) = &check.detail {
        if !detail.is_empty() {
            text = text.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .truncate()
                    .child(detail.clone()),
            );
        }
    }
    div()
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .gap_3()
        .child(text)
        .child(
            div()
                .flex_shrink_0()
                .px(px(6.0))
                .py(px(2.0))
                .rounded_md()
                .bg(rgb(bg))
                .text_xs()
                .text_color(rgb(fg))
                .child(label),
        )
}

fn recommendation(key: &str) -> SharedString {
    t(match key {
        "enable_github_mirrors" => "settings.networkDoctorRecEnableMirrors",
        "enable_proxy" => "settings.networkDoctorRecEnableProxy",
        "use_socks5h" => "settings.networkDoctorRecUseSocks5h",
        "check_proxy_reachability" => "settings.networkDoctorRecCheckProxy",
        "use_marketplace_wrap" => "settings.networkDoctorRecMarketplaceWrap",
        "all_github_paths_blocked" => "settings.networkDoctorRecAllBlocked",
        other => other,
    })
}
