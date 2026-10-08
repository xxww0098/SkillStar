//! GitHub 设备授权登录对话框 — 共享频道页空状态的登录入口。
//!
//! 整个壳共用一份 `GitHubAuthFacade`(OnceLock):身份缓存在页面刷新间存活,
//! 对话框的 cancel 总是打中自己 poll 所用的同一运行时。轮询链由
//! `DeviceFlowPoll::Pending/SlowDown` 的 retry 驱动;关闭对话框时 `Drop`
//! 调 `cancel_device_flow`,下一次 poll 以 NoPendingAuthorization 终止链条,
//! weak handle 更新失败即丢弃,不会复活已关闭的视图。

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::WindowExt;
use gpui_kit::*;

use ss_skills::github_auth::{
    DeviceFlowPoll, FileCredentialStore, GitHubAuthFacade, ProductionGitHubGateway, SystemClock,
};

use super::MySkillsPage;
use crate::chrome::{InteractionSpring, MotionPaint, icon, icon_spin};
use crate::spawn_domain;
use crate::theme::palette;

pub(crate) type SharedAuthFacade =
    Arc<GitHubAuthFacade<ProductionGitHubGateway, FileCredentialStore, SystemClock>>;

/// 全壳共享的登录 facade,每次调用返回同一份克隆。
pub(crate) fn shared_auth_facade() -> SharedAuthFacade {
    static FACADE: OnceLock<SharedAuthFacade> = OnceLock::new();
    FACADE
        .get_or_init(|| {
            Arc::new(GitHubAuthFacade::new(
                ProductionGitHubGateway::from_environment(),
                FileCredentialStore::default(),
                SystemClock,
            ))
        })
        .clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Starting,
    Waiting {
        user_code: String,
        verification_uri: String,
        /// 授权码剩余有效秒数,展示用;过期判定以域层为准。
        remaining_secs: u64,
    },
    Failed(String),
}

pub(crate) struct GitHubSignInDialog {
    page: WeakEntity<MySkillsPage>,
    facade: SharedAuthFacade,
    phase: Phase,
    copied: bool,
    /// `Connected` 回调里拿不到 `Window`;置位后第一次 render 关闭对话框。
    close_pending: bool,
}

/// 从共享频道空状态打开登录对话框。镜像 `toolbar::open_import`:实体在
/// builder 外创建,避免每帧重建丢状态。
pub(crate) fn open_sign_in(page: WeakEntity<MySkillsPage>, window: &mut Window, cx: &mut App) {
    let entity = cx.new(|cx| GitHubSignInDialog::new(page, cx));
    let weak = entity.downgrade();
    crate::chrome::open_centered(
        window,
        cx,
        420.0,
        crate::chrome::DialogChrome::Flush,
        move |dialog, frame, _, _| {
            let surface = if crate::theme::is_light() {
                palette().card
            } else {
                palette().panel
            };
            dialog
                .w(px(420.0))
                .p_0()
                .rounded(px(12.0))
                .bg(rgb(surface))
                .border_color(rgb(palette().border))
                .child(frame.measure(entity.clone()))
                .on_close({
                    let weak = weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |this, _| this.cancel());
                    }
                })
        },
    );
}

impl GitHubSignInDialog {
    fn new(page: WeakEntity<MySkillsPage>, cx: &mut Context<Self>) -> Self {
        let this = Self {
            page,
            facade: shared_auth_facade(),
            phase: Phase::Starting,
            copied: false,
            close_pending: false,
        };
        this.start(cx);
        this
    }

    /// 取消进行中的设备授权。X / Escape / 背景点击与 `Drop` 都走这里。
    fn cancel(&self) {
        let _ = self.facade.cancel_device_flow();
    }

    fn start(&self, cx: &mut Context<Self>) {
        let facade = self.facade.clone();
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move { facade.start_device_flow().await.map_err(anyhow::Error::new) },
            |this, cx, result| match result {
                Ok(authorization) => {
                    let remaining_secs = (authorization.expires_at - chrono::Utc::now())
                        .num_seconds()
                        .clamp(0, u64::MAX as i64) as u64;
                    let interval = authorization.interval_seconds.max(1);
                    this.phase = Phase::Waiting {
                        user_code: authorization.user_code,
                        verification_uri: authorization.verification_uri,
                        remaining_secs,
                    };
                    this.copied = false;
                    this.schedule_poll(cx, interval);
                }
                Err(error) => this.phase = Phase::Failed(error.to_string()),
            },
        );
    }

    fn schedule_poll(&self, cx: &mut Context<Self>, delay_secs: u64) {
        let facade = self.facade.clone();
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::time::sleep(Duration::from_secs(delay_secs.max(1))).await;
                facade.poll_device_flow().await.map_err(anyhow::Error::new)
            },
            |this, cx, result| match result {
                Ok(DeviceFlowPoll::Pending {
                    retry_after_seconds,
                })
                | Ok(DeviceFlowPoll::SlowDown {
                    retry_after_seconds,
                }) => {
                    this.schedule_poll(cx, retry_after_seconds);
                }
                Ok(DeviceFlowPoll::Connected { connection }) => {
                    let connection = connection.clone();
                    let _ = this.page.update(cx, |page, cx| {
                        page.github = Some(connection);
                        page.revise(cx);
                    });
                    this.close_pending = true;
                }
                Ok(DeviceFlowPoll::Denied) => {
                    this.phase =
                        Phase::Failed(crate::i18n::t("settings.githubAuthDenied").to_string());
                }
                Ok(DeviceFlowPoll::Expired) => {
                    this.phase =
                        Phase::Failed(crate::i18n::t("settings.githubAuthExpired").to_string());
                }
                Err(error) => this.phase = Phase::Failed(error.to_string()),
            },
        );
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Starting;
        self.copied = false;
        self.start(cx);
    }

    fn render_header(&self) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .w_full()
            .px(px(24.0))
            .pt(px(16.0))
            .pb(px(12.0))
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(palette().border_soft))
            .child(
                div()
                    .size(px(32.0))
                    .rounded_xl()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(palette().accent).alpha(0.10))
                    .child(icon(IconName::GitBranch, 16.0, palette().accent)),
            )
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(palette().fg))
                    .child(crate::i18n::t("sharedChannels.signInTitle")),
            )
    }

    fn render_body(&mut self, cx: &mut Context<Self>) -> Div {
        match self.phase.clone() {
            Phase::Starting => self.render_starting(),
            Phase::Waiting {
                user_code,
                verification_uri,
                remaining_secs,
            } => self.render_waiting(cx, &user_code, &verification_uri, remaining_secs),
            Phase::Failed(message) => self.render_failed(cx, &message),
        }
    }

    fn body(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap_3()
            .px(px(24.0))
            .py(px(16.0))
    }

    fn render_starting(&self) -> Div {
        self.body().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(icon_spin(IconName::Loader, 14.0, palette().fg_muted, true))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(palette().fg_muted))
                        .child(crate::i18n::t("common.loading")),
                ),
        )
    }

    fn render_waiting(
        &mut self,
        cx: &mut Context<Self>,
        user_code: &str,
        verification_uri: &str,
        remaining_secs: u64,
    ) -> Div {
        let minutes = remaining_secs / 60;
        let seconds = remaining_secs % 60;
        let expiry = crate::i18n::tf(
            "settings.githubAuthExpiresIn",
            &[("time", &format!("{minutes:02}:{seconds:02}"))],
        );
        let view = cx.entity();
        let uri = verification_uri.to_string();

        self.body()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .child(div().child(crate::i18n::t("settings.githubAuthStepOpen")))
                    .child(div().child(crate::i18n::t("settings.githubAuthStepApprove"))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .py_3()
                    .px_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(palette().border))
                    .bg(rgb(palette().card))
                    .child(
                        div()
                            .text_xl()
                            .font_semibold()
                            .text_color(rgb(palette().fg))
                            .child(user_code.to_string()),
                    )
                    .child(
                        div()
                            .id("github-sign-in-copy")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette().border))
                            .bg(rgb(palette().card))
                            .text_color(rgb(palette().fg))
                            .text_sm()
                            .cursor_pointer()
                            .child(if self.copied {
                                crate::i18n::t("settings.githubAuthCopied")
                            } else {
                                crate::i18n::t("settings.githubAuthCopy")
                            })
                            .on_click({
                                let view = view.clone();
                                let code = user_code.to_string();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                                    view.update(cx, |this, cx| {
                                        this.copied = true;
                                        cx.notify();
                                    });
                                }
                            })
                            .interaction_spring(
                                "github-sign-in-copy",
                                true,
                                MotionPaint::new().bg(rgb(palette().card)),
                                MotionPaint::new().bg(rgb(palette().card_hover)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("github-sign-in-open")
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .w_full()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(palette().accent))
                    .text_color(rgb(palette().on_accent))
                    .text_sm()
                    .font_medium()
                    .cursor_pointer()
                    .child(icon(IconName::GitBranch, 14.0, palette().on_accent))
                    .child(crate::i18n::t("settings.githubAuthStart"))
                    .on_click({
                        let uri = uri.clone();
                        move |_, _, _| crate::os_open::open_external(&uri)
                    })
                    .interaction_spring(
                        "github-sign-in-open",
                        true,
                        MotionPaint::new().opacity(1.0),
                        MotionPaint::new().opacity(0.9),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .child(div().child(crate::i18n::t("settings.githubAuthWaiting")))
                    .child(div().child(expiry)),
            )
    }

    fn render_failed(&mut self, cx: &mut Context<Self>, message: &str) -> Div {
        let view = cx.entity();
        self.body()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .child(message.to_string()),
            )
            .child(
                div()
                    .id("github-sign-in-retry")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(palette().accent))
                    .text_color(rgb(palette().on_accent))
                    .text_sm()
                    .font_medium()
                    .cursor_pointer()
                    .child(crate::i18n::t("common.retry"))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.retry(cx));
                    })
                    .interaction_spring(
                        "github-sign-in-retry",
                        true,
                        MotionPaint::new().opacity(1.0),
                        MotionPaint::new().opacity(0.9),
                    ),
            )
    }
}

impl Drop for GitHubSignInDialog {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Render for GitHubSignInDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.close_pending {
            self.close_pending = false;
            window.close_dialog(cx);
            return div().into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(self.render_header())
            .child(self.render_body(cx))
            .into_any_element()
    }
}
