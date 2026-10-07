//! Live labels for the import dialog's loading phases.
//!
//! Tauri paints these from the skillstar://git-progress event. This shell has
//! no event bus, so the dialog owns a progress sink and applies the same
//! phase/stage mapping on the GPUI thread.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;

use gpui_kit::*;
use ss_skills::git::transport::{
    GitOperationPhase, GitOperationProgress, GitProgressSink, InstallStage,
};
use ss_skills::git_skill::GitSkillFacade;

use super::ImportDialog;
use super::Phase;

/// Which catalog key a git progress event should show. Mirrors the React
/// listener's preparing/running/cancelled split, and uses stage once the
/// pipeline moves past the download.
pub(super) fn progress_key(phase: GitOperationPhase, stage: Option<InstallStage>) -> &'static str {
    match (phase, stage) {
        (GitOperationPhase::Preparing, _) => "githubImportModal.gitPreparing",
        (GitOperationPhase::Cancelled, _) => "githubImportModal.gitCancelled",
        (GitOperationPhase::Running, Some(InstallStage::Discovering)) => {
            "githubImportModal.discovering"
        }
        (
            GitOperationPhase::Running,
            Some(InstallStage::Materializing | InstallStage::Deploying),
        ) => "githubImportModal.materializing",
        _ => "githubImportModal.gitRunning",
    }
}

struct DialogProgressSink {
    tx: Sender<GitOperationProgress>,
}

impl GitProgressSink for DialogProgressSink {
    fn emit(&self, progress: GitOperationProgress) {
        let _ = self.tx.send(progress);
    }
}

/// Facade whose progress events land on the dialog while a scan or install
/// is on screen. The watcher stops when the channel closes or the dialog
/// leaves the loading phases.
pub(super) fn tracked_facade(cx: &mut Context<ImportDialog>) -> GitSkillFacade {
    let (tx, rx) = std::sync::mpsc::channel();
    watch(rx, cx);
    GitSkillFacade::from_file_store_with_progress(Arc::new(DialogProgressSink { tx }))
}

fn watch(rx: Receiver<GitOperationProgress>, cx: &mut Context<ImportDialog>) {
    cx.spawn(async move |dialog, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
            let mut latest = None;
            let mut disconnected = false;
            loop {
                match rx.try_recv() {
                    Ok(progress) => latest = Some(progress),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            let Some(progress) = latest else {
                if disconnected {
                    break;
                }
                continue;
            };
            let still = dialog.update(cx, |dialog, cx| {
                if !matches!(dialog.phase, Phase::Scanning | Phase::Installing) {
                    return false;
                }
                dialog.progress =
                    Some(crate::i18n::t(progress_key(progress.phase, progress.stage)));
                cx.notify();
                true
            });
            if !matches!(still, Ok(true)) || disconnected {
                break;
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::progress_key;
    use ss_skills::git::transport::{GitOperationPhase, InstallStage};

    #[test]
    fn phases_map_to_the_same_labels_as_the_tauri_listener() {
        assert_eq!(
            progress_key(GitOperationPhase::Preparing, None),
            "githubImportModal.gitPreparing"
        );
        assert_eq!(
            progress_key(GitOperationPhase::Running, Some(InstallStage::Fetching)),
            "githubImportModal.gitRunning"
        );
        assert_eq!(
            progress_key(GitOperationPhase::Running, Some(InstallStage::Discovering)),
            "githubImportModal.discovering"
        );
        assert_eq!(
            progress_key(
                GitOperationPhase::Running,
                Some(InstallStage::Materializing)
            ),
            "githubImportModal.materializing"
        );
        assert_eq!(
            progress_key(GitOperationPhase::Cancelled, None),
            "githubImportModal.gitCancelled"
        );
    }
}
