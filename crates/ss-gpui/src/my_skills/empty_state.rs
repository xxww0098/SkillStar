//! Empty states for the My Skills page.

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle};
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::*;

use super::MySkillsPage;
use crate::nav::{NavPage, SelectPage};
use crate::theme::palette;

/// Fills the skills scroller and centers a short status. Not a one-line stub at the top.
pub(super) fn centered_fill() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .min_h_full()
        .items_center()
        .justify_center()
        .gap_3()
        .p_5()
}

/// The circle chip every page empty state sets its glyph in. Fed to the
/// kit's unframed media slot so the glyph keeps the app palette.
fn media_chip(inner: impl IntoElement) -> EmptyMedia {
    EmptyMedia::new().child(
        div()
            .p_3()
            .rounded_full()
            .bg(rgb(palette().card))
            .border_1()
            .border_color(rgb(palette().border))
            .child(inner),
    )
}

pub fn render_empty_installed(view: WeakEntity<MySkillsPage>) -> impl IntoElement {
    Empty::new()
        .header(
            EmptyHeader::new()
                .media(EmptyMedia::new().child(div().text_3xl().child("🧩")))
                .title(
                    EmptyTitle::new()
                        .text_lg()
                        .font_semibold()
                        .child(crate::i18n::t("emptyState.mySkillsTitle")),
                )
                .description(
                    EmptyDescription::new().child(crate::i18n::t("emptyState.mySkillsDesc")),
                ),
        )
        .child(
            Button::new("my-skills-goto-marketplace")
                .primary()
                .icon(NavPage::Marketplace.icon())
                .label(crate::i18n::t("emptyState.mySkillsCta"))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |_, cx| {
                        cx.emit(SelectPage(NavPage::Marketplace));
                    });
                }),
        )
}

pub fn render_empty_search(query: &str, view: WeakEntity<MySkillsPage>) -> impl IntoElement {
    let q = query.to_string();
    Empty::new()
        .header(
            EmptyHeader::new()
                .media(media_chip(
                    Icon::new(IconName::Search)
                        .with_size(px(24.0))
                        .text_color(rgb(palette().fg_muted)),
                ))
                .title(
                    EmptyTitle::new()
                        .text_lg()
                        .font_semibold()
                        .child(crate::i18n::t("emptyState.noMatchingTitle")),
                )
                .description(EmptyDescription::new().child(if q.is_empty() {
                    crate::i18n::t("mySkills.noMatching")
                } else {
                    crate::i18n::t("skillCards.tryDifferent")
                })),
        )
        .child(
            Button::new("my-skills-clear-search")
                .label(crate::i18n::t("settings.clearAgentFilters"))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.clear_filters(cx);
                    });
                }),
        )
}

pub fn render_empty_updates(view: WeakEntity<MySkillsPage>) -> impl IntoElement {
    Empty::new()
        .header(
            EmptyHeader::new()
                .media(media_chip(
                    Icon::new(IconName::Check)
                        .with_size(px(24.0))
                        .text_color(rgb(palette().ok)),
                ))
                .title(
                    EmptyTitle::new()
                        .text_lg()
                        .font_semibold()
                        .child(crate::i18n::t("emptyState.upToDateTitle")),
                )
                .description(
                    EmptyDescription::new().child(crate::i18n::t("emptyState.upToDateDesc")),
                ),
        )
        .child(
            Button::new("my-skills-show-all")
                .label(crate::i18n::t("toolbar.showAllSkills"))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.only_updates = false;
                        this.revise(cx);
                    });
                }),
        )
}

pub fn render_remote_scope() -> impl IntoElement {
    Empty::new().header(
        EmptyHeader::new()
            .media(media_chip(
                Icon::new(IconName::Server)
                    .with_size(px(28.0))
                    .text_color(rgb(palette().accent)),
            ))
            .title(
                EmptyTitle::new()
                    .text_lg()
                    .font_semibold()
                    .child(crate::i18n::t("emptyState.remoteTitle")),
            )
            .description(EmptyDescription::new().child(crate::i18n::t("emptyState.remoteDesc"))),
    )
}

/// 频道空状态的三种门态。`Unknown` 保持纯静态提示(状态还在读);其余两态
/// 在描述下追加登录卡或已登录行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChannelsGate {
    Unknown,
    SignedOut,
    Connected(String),
}

pub(crate) fn channels_gate(
    github: Option<&ss_skills::github_auth::GitHubConnectionStatus>,
) -> ChannelsGate {
    match github {
        None => ChannelsGate::Unknown,
        Some(ss_skills::github_auth::GitHubConnectionStatus::SignedOut) => ChannelsGate::SignedOut,
        Some(ss_skills::github_auth::GitHubConnectionStatus::Expired { .. }) => {
            ChannelsGate::SignedOut
        }
        Some(ss_skills::github_auth::GitHubConnectionStatus::Connected { identity, .. }) => {
            ChannelsGate::Connected(identity.login.clone())
        }
    }
}

pub fn render_channels_scope(
    view: WeakEntity<MySkillsPage>,
    github: Option<&ss_skills::github_auth::GitHubConnectionStatus>,
) -> impl IntoElement {
    let fill = Empty::new().header(
        EmptyHeader::new()
            .media(media_chip(
                Icon::new(IconName::Layers)
                    .with_size(px(28.0))
                    .text_color(rgb(palette().violet)),
            ))
            .title(
                EmptyTitle::new()
                    .text_lg()
                    .font_semibold()
                    .child(crate::i18n::t("emptyState.channelsTitle")),
            )
            .description(EmptyDescription::new().child(crate::i18n::t("emptyState.channelsDesc"))),
    );
    match channels_gate(github) {
        ChannelsGate::Unknown => fill,
        ChannelsGate::SignedOut => fill
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .max_w(px(400.0))
                    .text_center()
                    .child(crate::i18n::t("sharedChannels.signInHint")),
            )
            .child(
                Button::new("channels-github-sign-in")
                    .primary()
                    .icon(IconName::GitBranch)
                    .label(crate::i18n::t("sharedChannels.signInButton"))
                    .on_click(move |_, window, cx| {
                        super::github_sign_in::open_sign_in(view.clone(), window, cx);
                    }),
            ),
        ChannelsGate::Connected(login) => fill.child(
            div()
                .text_sm()
                .text_color(rgb(palette().fg_muted))
                .child(crate::i18n::tf(
                    "sharedChannels.signedInAs",
                    &[("login", &login)],
                )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{ChannelsGate, channels_gate};
    use ss_skills::github_auth::{GitHubConnectionStatus, GitHubIdentity};

    fn identity(login: &str) -> GitHubIdentity {
        GitHubIdentity {
            id: 1,
            login: login.to_string(),
            avatar_url: None,
        }
    }

    #[test]
    fn channels_gate_maps_status_to_empty_state_variant() {
        assert_eq!(channels_gate(None), ChannelsGate::Unknown);
        assert_eq!(
            channels_gate(Some(&GitHubConnectionStatus::SignedOut)),
            ChannelsGate::SignedOut
        );
        assert_eq!(
            channels_gate(Some(&GitHubConnectionStatus::Expired {
                identity: Some(identity("dev"))
            })),
            ChannelsGate::SignedOut
        );
        assert_eq!(
            channels_gate(Some(&GitHubConnectionStatus::Connected {
                identity: identity("octocat"),
                access_expires_at: None,
            })),
            ChannelsGate::Connected("octocat".to_string())
        );
    }
}
