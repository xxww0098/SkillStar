//! About section. React source: `src/features/settings/sections/AboutSection.tsx`.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::infra::release_check::{RELEASES_PAGE_URL, ReleaseCheckOutcome};
use ss_skills::git::gh_manager::GitStatus;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::i18n::{t, tf};
use crate::spawn_domain;

use super::{SettingsPage, SettingsSection, card, section_shell};
use crate::theme::palette;

impl SettingsPage {
    pub(crate) fn render_about(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let mut body = card();
        body = body.child(self.git_row(view.clone()));
        body = body.child(self.gh_row(view.clone()));
        body = body.child(self.version_row(view.clone()));
        body = body.child(self.data_path_row());
        section_shell(SettingsSection::About, None, None, body)
    }

    pub(crate) fn run_release_check(&mut self, cx: &mut Context<Self>) {
        if self.release_checking {
            return;
        }
        self.release_checking = true;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async {
                Ok::<_, anyhow::Error>(
                    ss_core::infra::release_check::run_check(crate::product_version()).await,
                )
            },
            |this, _cx, result| {
                this.release_checking = false;
                if let Ok(outcome) = result {
                    this.release_check = Some(ss_core::infra::release_check::ReleaseCheckRecord {
                        last_checked_unix: ss_core::infra::github_api_cooldown::now_unix(),
                        current_version: crate::product_version().to_string(),
                        outcome,
                    });
                }
            },
        );
        cx.notify();
    }

    fn git_row(&self, view: WeakEntity<Self>) -> Div {
        let mut block = div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(rgb(palette().border));
        match &self.git_status {
            Some(GitStatus::Installed { .. }) => {
                block = block
                    .child(tool_name(IconName::CircleCheck, palette().ok, "Git"))
                    .child(badge(t("settings.gitInstalled"), palette().ok));
            }
            Some(GitStatus::NotInstalled { download_url, .. }) => {
                let url = download_url.clone();
                block = block
                    .child(tool_name(IconName::CircleX, palette().danger, "Git"))
                    .child(
                        div()
                            .id("git-install")
                            .h(px(28.0))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette().border))
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(palette().fg))
                            .child(
                                Icon::new(IconName::ExternalLink)
                                    .size(px(12.0))
                                    .text_color(rgb(palette().fg_muted)),
                            )
                            .child(t("settings.gitInstall"))
                            .on_click(move |_, _, _| crate::os_open::open_external(&url))
                            .interaction_spring(
                                "git-install",
                                true,
                                MotionPaint::new(),
                                MotionPaint::new().bg(rgb(palette().card_hover)),
                            ),
                    );
            }
            None => {
                block = block.child(tool_name(IconName::CircleCheck, palette().fg_muted, "Git"));
            }
        }
        let mut wrap = div().flex().flex_col().w_full().child(block);
        if let Some(GitStatus::NotInstalled {
            os,
            install_instructions,
            ..
        }) = &self.git_status
            && !install_instructions.is_empty()
        {
            let mut commands = div()
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .gap_2()
                .bg(rgb(palette().input))
                .border_b_1()
                .border_color(rgb(palette().border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(
                            Icon::new(IconName::Terminal)
                                .size(px(14.0))
                                .text_color(rgb(palette().fg_muted)),
                        )
                        .child(tf("settings.gitInstallCommandLabel", &[("os", os)])),
                );
            for inst in install_instructions {
                commands = commands.child(command_row(
                    &format!("git-{}", inst.label),
                    &inst.label,
                    &inst.command,
                    self.copied.as_deref(),
                    view.clone(),
                ));
            }
            wrap = wrap.child(commands);
        }
        wrap
    }

    fn gh_row(&self, view: WeakEntity<Self>) -> Div {
        let mut block = div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(rgb(palette().border));
        if self.gh_installed {
            block = block
                .child(tool_name(IconName::CircleCheck, palette().ok, "GitHub CLI"))
                .child(badge(t("settings.ghInstalled"), palette().ok));
        } else {
            block = block
                .child(tool_name(IconName::CircleX, palette().danger, "GitHub CLI"))
                .child(
                    div()
                        .id("gh-install")
                        .h(px(28.0))
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_1()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette().border))
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(palette().fg))
                        .child(
                            Icon::new(IconName::ExternalLink)
                                .size(px(12.0))
                                .text_color(rgb(palette().fg_muted)),
                        )
                        .child(t("settings.ghInstall"))
                        .on_click(|_, _, _| {
                            crate::os_open::open_external("https://cli.github.com/")
                        })
                        .interaction_spring(
                            "gh-install",
                            true,
                            MotionPaint::new(),
                            MotionPaint::new().bg(rgb(palette().card_hover)),
                        ),
                );
        }
        let mut wrap = div().flex().flex_col().w_full().child(block);
        if !self.gh_installed {
            let platform = if cfg!(target_os = "macos") {
                "macos"
            } else if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "linux") {
                "linux"
            } else {
                "unknown"
            };
            let command = gh_install_command();
            let mut panel = div()
                .px_4()
                .py_3()
                .bg(rgb(palette().input))
                .border_b_1()
                .border_color(rgb(palette().border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .mb_2()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(
                            Icon::new(IconName::Terminal)
                                .size(px(14.0))
                                .text_color(rgb(palette().fg_muted)),
                        )
                        .child(tf(
                            "settings.ghInstallCommandLabel",
                            &[(
                                "platform",
                                t(&format!("settings.ghInstallPlatform_{platform}")).as_ref(),
                            )],
                        )),
                );
            if let Some(command) = command {
                panel = panel.child(command_row("gh", "", command, self.copied.as_deref(), view));
            } else {
                panel = panel.child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(t("settings.ghInstallCommandUnavailable")),
                );
            }
            wrap = wrap.child(panel);
        }
        wrap
    }

    fn version_row(&self, view: WeakEntity<Self>) -> Div {
        let checking = self.release_checking;
        let check_view = view.clone();
        div()
            .px_4()
            .py_4()
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(rgb(palette().border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(t("settings.version")),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(palette().violet))
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Icon::new(IconName::Sparkles)
                                    .size(px(14.0))
                                    .text_color(rgb(palette().violet_fg)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette().violet_fg))
                                    .child(format!("v{}", crate::product_version())),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(release_check_chip(self), |d, chip| d.child(chip))
                    .child(
                        div()
                            .id("about-releases")
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .cursor_pointer()
                            .child(
                                Icon::new(IconName::ExternalLink)
                                    .size(px(14.0))
                                    .text_color(rgb(palette().fg_muted)),
                            )
                            .on_click(move |_, _, _| {
                                crate::os_open::open_external(RELEASES_PAGE_URL)
                            })
                            .interaction_spring(
                                "about-releases",
                                true,
                                MotionPaint::new(),
                                MotionPaint::new().bg(rgb(palette().card_hover)),
                            ),
                    )
                    .child(
                        div()
                            .id("about-check-update")
                            .h(px(28.0))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette().border))
                            .cursor_pointer()
                            .text_xs()
                            .text_color(if checking {
                                rgb(palette().fg_muted)
                            } else {
                                rgb(palette().fg)
                            })
                            .child(
                                Icon::new(IconName::RefreshCw)
                                    .size(px(12.0))
                                    .text_color(rgb(palette().fg_muted)),
                            )
                            .child(if checking {
                                t("settings.checkingUpdate")
                            } else {
                                t("settings.checkUpdate")
                            })
                            .on_click(move |_, _, cx| {
                                let _ = check_view.update(cx, |this, cx| {
                                    this.run_release_check(cx);
                                });
                            })
                            .interaction_spring(
                                "about-check-update",
                                true,
                                MotionPaint::new(),
                                MotionPaint::new().bg(rgb(palette().card_hover)),
                            ),
                    ),
            )
    }

    fn data_path_row(&self) -> Div {
        let path = self.data_root.clone();
        let open = path.clone();
        let platform = if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "windows") {
            "Windows"
        } else {
            "Linux"
        };
        div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .min_w(px(56.0))
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(platform),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(rgb(palette().fg))
                    .child(display_home(&path)),
            )
            .child(
                div()
                    .id("about-open-data")
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
                    .on_click(move |_, _, _| crate::os_open::open_external(&open))
                    .interaction_spring(
                        "about-open-data",
                        true,
                        MotionPaint::new(),
                        MotionPaint::new().bg(rgb(palette().card_hover)),
                    ),
            )
    }
}

fn gh_install_command() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        Some("brew install gh")
    }
    #[cfg(target_os = "windows")]
    {
        Some("winget install --id GitHub.cli")
    }
    #[cfg(target_os = "linux")]
    {
        Some("sudo apt install gh")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// The outcome chip next to the version row. `None` before the first check.
fn release_check_chip(page: &SettingsPage) -> Option<AnyElement> {
    if page.release_checking {
        return Some(
            plain_chip(t("settings.checkingUpdate"), palette().fg_muted).into_any_element(),
        );
    }
    match page.release_check.as_ref().map(|record| &record.outcome) {
        None => None,
        Some(ReleaseCheckOutcome::UpToDate) => {
            Some(plain_chip(t("settings.upToDate"), palette().ok).into_any_element())
        }
        Some(ReleaseCheckOutcome::NoPublishedRelease) => Some(
            plain_chip(t("settings.noPublishedRelease"), palette().fg_muted).into_any_element(),
        ),
        Some(ReleaseCheckOutcome::Failed { .. }) => {
            Some(plain_chip(t("settings.updateCheckFailed"), palette().fg_muted).into_any_element())
        }
        Some(ReleaseCheckOutcome::Available { tag, url }) => {
            let label = tf("settings.updateFoundDesc", &[("version", tag)]);
            let open = url.clone();
            Some(
                div()
                    .id("about-update-available")
                    .px_2()
                    .py(px(2.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette().violet))
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(palette().violet_fg))
                    .child(
                        Icon::new(IconName::Download)
                            .size(px(12.0))
                            .text_color(rgb(palette().violet_fg)),
                    )
                    .child(label)
                    .on_click(move |_, _, _| crate::os_open::open_external(&open))
                    .interaction_spring(
                        "about-update-available",
                        true,
                        MotionPaint::new(),
                        MotionPaint::new().bg(rgb(palette().card_hover)),
                    )
                    .into_any_element(),
            )
        }
    }
}

fn plain_chip(label: SharedString, color: u32) -> Div {
    div()
        .px_2()
        .py(px(2.0))
        .rounded_md()
        .border_1()
        .border_color(rgb(palette().border))
        .text_xs()
        .text_color(rgb(color))
        .child(label)
}

fn display_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

fn tool_name(icon: IconName, color: u32, name: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .child(Icon::new(icon).size(px(16.0)).text_color(rgb(color)))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(palette().fg))
                .when(name == "Git", |d| {
                    d.child(
                        Icon::new(IconName::GitBranch)
                            .size(px(14.0))
                            .text_color(rgb(palette().fg_muted)),
                    )
                })
                .child(name),
        )
}

fn badge(label: SharedString, color: u32) -> Div {
    div()
        .px_2()
        .py(px(2.0))
        .rounded_md()
        .bg(rgb(palette().ok_bg))
        .text_xs()
        .text_color(rgb(color))
        .child(label)
}

fn command_row(
    id: &str,
    label: &str,
    command: &str,
    copied: Option<&str>,
    view: WeakEntity<SettingsPage>,
) -> Div {
    let copy_id = id.to_string();
    let text = command.to_string();
    let is_copied = copied == Some(id);
    div()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .px(px(10.0))
        .py_2()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .when(!label.is_empty(), |d| d.child(label.to_string())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette().fg))
                        .child(command.to_string()),
                ),
        )
        .child(
            div()
                .id(ElementId::Name(format!("copy-{id}").into()))
                .p(px(6.0))
                .rounded_md()
                .cursor_pointer()
                .child(
                    Icon::new(if is_copied {
                        IconName::Check
                    } else {
                        IconName::Copy
                    })
                    .size(px(14.0))
                    .text_color(rgb(if is_copied {
                        palette().ok
                    } else {
                        palette().fg_muted
                    })),
                )
                .on_click(move |_, _, cx| {
                    let copy_id = copy_id.clone();
                    let text = text.clone();
                    let _ = view.update(cx, |this, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        this.copied = Some(copy_id);
                        cx.notify();
                    });
                })
                .interaction_spring(
                    format!("copy-{id}"),
                    true,
                    MotionPaint::new(),
                    MotionPaint::new().bg(rgb(palette().card_hover)),
                ),
        )
}
