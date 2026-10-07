//! Per-repo actions on the origin menu: reinstall every skill the repo
//! currently ships, or uninstall every skill installed from that source.

use anyhow::anyhow;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use ss_core::types::skill::SkillType;

use super::MySkillsPage;
use crate::spawn_domain;

impl MySkillsPage {
    /// Full-depth rescan of one hub repository, then overwrite-install
    /// everything that scan found. The menu row spins its refresh glyph until this
    /// returns; a second click while one repo is running is ignored.
    pub fn reinstall_repo_source(&mut self, source: &str, cx: &mut Context<Self>) {
        if self.reinstalling_repo.is_some() {
            return;
        }
        let url = self
            .skills
            .iter()
            .find(|skill| {
                skill.source.as_deref() == Some(source)
                    && skill.skill_type == SkillType::Hub
                    && !skill.git_url.is_empty()
            })
            .map(|skill| skill.git_url.clone());
        let Some(url) = url else {
            crate::notify::toast(
                Notification::error(crate::i18n::tf(
                    "mySkills.reinstallRepoSourceMissing",
                    &[("source", source)],
                )),
                cx,
            );
            return;
        };

        self.reinstalling_repo = Some(source.to_string());
        self.revise(cx);
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || reinstall_repo_blocking(url))
                    .await
                    .unwrap_or_else(|err| Err(anyhow!("{err}")))
            },
            |this, cx, result: anyhow::Result<usize>| {
                let source = this.reinstalling_repo.take().unwrap_or_default();
                match result {
                    Ok(count) => {
                        crate::notify::toast(
                            Notification::success(crate::i18n::tf(
                                "mySkills.reinstallRepoSuccess",
                                &[("count", &count.to_string())],
                            )),
                            cx,
                        );
                    }
                    Err(err) => {
                        let headline =
                            crate::i18n::tf("mySkills.reinstallRepoFailed", &[("source", &source)]);
                        crate::notify::toast(
                            Notification::error(format!("{headline}\n{err:#}")),
                            cx,
                        );
                    }
                }
                this.refresh(cx);
            },
        );
    }

    /// Uninstall every installed skill whose `source` is this repo.
    /// Clears the repo filter when it was pointing at the same source.
    pub fn uninstall_repo_source(&mut self, source: &str, cx: &mut Context<Self>) {
        let names: Vec<String> = self
            .skills
            .iter()
            .filter(|skill| skill.source.as_deref() == Some(source))
            .map(|skill| skill.name.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        if self.repo_filter.as_deref() == Some(source) {
            self.repo_filter = None;
        }
        for name in &names {
            self.selected_batch.remove(name);
            if self.selected_skill.as_deref() == Some(name.as_str()) {
                self.select_detail(None);
            }
        }
        self.busy = Some(format!("repo-remove:{source}"));
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    let mut failed = Vec::new();
                    for name in names {
                        if let Err(err) = ss_skills::skill_install::uninstall_skill(&name) {
                            failed.push(format!("{name}: {err}"));
                        }
                    }
                    if failed.is_empty() {
                        Ok(())
                    } else {
                        Err(anyhow!(failed.join("\n")))
                    }
                })
                .await
                .unwrap_or_else(|err| Err(anyhow!("{err}")))
            },
            |this, cx, result: anyhow::Result<()>| {
                this.busy = None;
                if let Err(err) = result {
                    crate::notify::toast(Notification::error(format!("{err:#}")), cx);
                }
                this.refresh(cx);
            },
        );
    }
}

fn reinstall_repo_blocking(url: String) -> anyhow::Result<usize> {
    let facade = ss_skills::git_skill::GitSkillFacade::from_file_store();
    let scan = facade
        .scan_repo(&url, true)
        .map_err(|err| anyhow!("{err:#}"))?;
    if scan.skills.is_empty() {
        anyhow::bail!("{}", crate::i18n::t("mySkills.reinstallRepoNoSkills"));
    }
    let targets: Vec<ss_skills::repo_scanner::SkillInstallTarget> = scan
        .skills
        .iter()
        .map(|skill| ss_skills::repo_scanner::SkillInstallTarget {
            id: skill.id.clone(),
            folder_path: skill.folder_path.clone(),
            pinned: false,
        })
        .collect();
    let installed = facade
        .install_from_scan(&scan, &targets)
        .map_err(|err| anyhow!("{err:#}"))?;
    Ok(installed.len())
}
