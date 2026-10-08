//! `skillstar channel`: check, apply and roll back shared-channel releases.
//!
//! Thin terminal surface over the shared-channel facade; review, divergence
//! rules and rollback stay in ss-skills. `export-marketplace` is local-only
//! and runs without the facade or a GitHub sign-in.

use std::path::Path;

use ss_skills::channels::shared_channels::{
    ApplyChannelUpdateRequest, ChannelSkillUpdateResolution, ChannelUpdateItemState,
    ChannelUpdateSnapshot, RollbackChannelSkillRequest, SharedChannelError,
    export_channel_marketplace,
};
use ss_skills::skill_update::LocalDivergenceResolution;

use super::ChannelCommand;
use crate::channel_facade::{ProductionChannelFacade, production_facade};

pub fn cmd_channel(command: ChannelCommand) {
    if let ChannelCommand::ExportMarketplace {
        repository_id,
        out,
        json,
    } = command
    {
        export_marketplace(repository_id, &out, json);
        return;
    }
    let facade = match production_facade() {
        Ok(facade) => facade,
        Err(error) => fail(&format!(
            "Shared channels need a GitHub sign-in (open SkillStar → Settings): {error}"
        )),
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => fail(&format!("Failed to start async runtime: {error}")),
    };
    let outcome = runtime.block_on(run(&facade, command));
    if let Err(error) = outcome {
        fail(&error.message);
    }
}

fn fail(message: &str) -> ! {
    eprintln!("✗ {message}");
    std::process::exit(1);
}

#[derive(serde::Serialize)]
struct ExportMarketplaceOutput<'a> {
    repository_id: u64,
    revision: u64,
    path: &'a str,
    plugins: &'a [String],
    files: u64,
}

fn export_marketplace(repository_id: u64, out: &Path, json: bool) {
    match export_channel_marketplace(repository_id, out) {
        Ok(exported) => {
            if json {
                let output = ExportMarketplaceOutput {
                    repository_id: exported.repository_id,
                    revision: exported.revision,
                    path: &exported.root.display().to_string(),
                    plugins: &exported.plugin_names,
                    files: exported.files,
                };
                match serde_json::to_string_pretty(&output) {
                    Ok(text) => println!("{text}"),
                    Err(error) => fail(&format!("Failed to serialize the export: {error}")),
                }
                return;
            }
            println!(
                "✓ Exported {} plugin(s) ({}) at revision {} to {} — {} files",
                exported.plugin_names.len(),
                exported.plugin_names.join(", "),
                exported.revision,
                exported.root.display(),
                exported.files
            );
            println!(
                "Push the directory to a git repository, then install it with: claude plugin marketplace add <owner>/<repo>"
            );
        }
        Err(error) => fail(&format!(
            "Export failed ({:?}): {}",
            error.code, error.message
        )),
    }
}

async fn run(
    facade: &ProductionChannelFacade,
    command: ChannelCommand,
) -> Result<(), SharedChannelError> {
    match command {
        ChannelCommand::List => {
            let subscriptions = facade.list_subscriptions()?;
            if subscriptions.is_empty() {
                println!("No shared channel subscriptions.");
            }
            for subscription in subscriptions {
                let revision = subscription
                    .target
                    .as_ref()
                    .map(|target| target.tag_name.clone())
                    .unwrap_or_else(|| "-".into());
                println!(
                    "{}  {}  {}",
                    subscription.repository_id,
                    revision,
                    subscription.selected_skill_ids.join(", ")
                );
            }
        }
        ChannelCommand::Check {
            repository_id,
            json,
        } => {
            let ids = match repository_id {
                Some(id) => vec![id],
                None => facade
                    .list_subscriptions()?
                    .into_iter()
                    .map(|subscription| subscription.repository_id)
                    .collect(),
            };
            for id in ids {
                let snapshot = facade.check_update(id).await?;
                print_snapshot(id, &snapshot, json);
            }
        }
        ChannelCommand::Apply {
            repository_id,
            keep_local,
            discard_local,
        } => {
            let checked = facade.check_update(repository_id).await?;
            let resolutions = resolutions(&checked, &keep_local, &discard_local);
            let result = facade
                .apply_update(ApplyChannelUpdateRequest {
                    repository_id,
                    target: checked.target.clone(),
                    resolutions,
                })
                .await?;
            print_snapshot(repository_id, &result.snapshot, false);
            if result.applied_skill_ids.is_empty() {
                println!("Nothing was applied.");
            } else {
                println!("✓ Applied: {}", result.applied_skill_ids.join(", "));
            }
        }
        ChannelCommand::Rollback {
            repository_id,
            skill,
            revision,
            keep_local,
            discard_local,
        } => {
            let targets = facade
                .list_skill_rollback_targets(repository_id, &skill)
                .await?;
            let Some(revision) = revision else {
                for target in &targets {
                    println!(
                        "{}  {}  {}  {}",
                        target.target.revision,
                        target.target.tag_name,
                        target.published_at,
                        target.title
                    );
                }
                println!("Pass --revision <N> to roll '{skill}' back to one of these releases.");
                return Ok(());
            };
            let Some(target) = targets
                .into_iter()
                .find(|target| target.target.revision == revision)
            else {
                fail(&format!(
                    "Revision {revision} is not an earlier release of '{skill}' in channel {repository_id}"
                ));
            };
            let resolution = if keep_local {
                Some(LocalDivergenceResolution::Preserve {
                    local_name: ss_skills::skill_update::divergence::suggested_local_name(&skill),
                })
            } else if discard_local {
                Some(LocalDivergenceResolution::Discard)
            } else {
                None
            };
            let result = facade
                .rollback_skill(RollbackChannelSkillRequest {
                    repository_id,
                    skill_id: skill.clone(),
                    target: target.target,
                    resolution,
                })
                .await?;
            println!(
                "✓ '{skill}' is pinned to {}; `skillstar channel apply {repository_id}` does not move it until you resume following.",
                result.pin.target.tag_name
            );
        }
        ChannelCommand::ExportMarketplace { .. } => {
            // Handled in `cmd_channel` before the facade is built: the export
            // path is local-only and must not require a GitHub sign-in.
            unreachable!("export-marketplace is dispatched before the facade is constructed")
        }
    }
    Ok(())
}

/// Explicit choices for Skills the check blocked on local edits.
fn resolutions(
    snapshot: &ChannelUpdateSnapshot,
    keep_local: &[String],
    discard_local: &[String],
) -> Vec<ChannelSkillUpdateResolution> {
    let named = |list: &[String], id: &str| list.iter().any(|name| name.eq_ignore_ascii_case(id));
    snapshot
        .items
        .iter()
        .filter(|item| item.state == ChannelUpdateItemState::Blocked)
        .filter_map(|item| {
            let resolution = if named(keep_local, &item.id) {
                LocalDivergenceResolution::Preserve {
                    local_name: item
                        .suggested_local_name
                        .clone()
                        .unwrap_or_else(|| format!("{}.local", item.id)),
                }
            } else if named(discard_local, &item.id) {
                LocalDivergenceResolution::Discard
            } else {
                return None;
            };
            Some(ChannelSkillUpdateResolution {
                skill_id: item.id.clone(),
                resolution,
            })
        })
        .collect()
}

fn print_snapshot(repository_id: u64, snapshot: &ChannelUpdateSnapshot, json: bool) {
    if json {
        match serde_json::to_string_pretty(snapshot) {
            Ok(text) => println!("{text}"),
            Err(error) => eprintln!("✗ Failed to serialize channel {repository_id}: {error}"),
        }
        return;
    }
    println!(
        "Channel {repository_id}: {:?} ({} — {})",
        snapshot.status, snapshot.target.tag_name, snapshot.title
    );
    for item in &snapshot.items {
        let mut line = format!("  {:?} {} [{:?}]", item.change, item.id, item.state);
        if let Some(reason) = item.block_reason {
            line.push_str(&format!(" blocked: {reason:?}"));
        }
        if let Some(error) = &item.error {
            line.push_str(&format!(" error: {error}"));
        }
        println!("{line}");
    }
    if snapshot
        .items
        .iter()
        .any(|item| item.state == ChannelUpdateItemState::Blocked)
    {
        println!(
            "  Blocked Skills have local edits: pass --keep-local <id> (keep a .local copy) or --discard-local <id> to apply."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss_skills::channels::shared_channels::{
        ChannelPublisherIdentity, ChannelReleaseTarget, ChannelUpdateBlockReason,
        ChannelUpdateChange, ChannelUpdateItem, ChannelUpdateStatus,
    };

    fn item(id: &str, state: ChannelUpdateItemState) -> ChannelUpdateItem {
        ChannelUpdateItem {
            id: id.into(),
            change: ChannelUpdateChange::Updated,
            state,
            selected: true,
            from_content_hash: None,
            to_content_hash: None,
            block_reason: (state == ChannelUpdateItemState::Blocked)
                .then_some(ChannelUpdateBlockReason::LocalContentChanged),
            suggested_local_name: Some(format!("{id}.local")),
            error: None,
            pinned_target: None,
            error_code: None,
        }
    }

    #[test]
    fn only_named_blocked_skills_get_a_resolution() {
        let snapshot = ChannelUpdateSnapshot {
            target: ChannelReleaseTarget {
                revision: 2,
                tag_name: "channel-v000002".into(),
                commit_sha: "a".repeat(40),
            },
            title: "t".into(),
            notes: String::new(),
            publisher: ChannelPublisherIdentity {
                id: 1,
                login: "p".into(),
            },
            published_at: String::new(),
            checked_at: String::new(),
            status: ChannelUpdateStatus::Blocked,
            acknowledgement_required: false,
            items: vec![
                item("kept", ChannelUpdateItemState::Blocked),
                item("dropped", ChannelUpdateItemState::Blocked),
                item("untouched", ChannelUpdateItemState::Blocked),
                item("clean", ChannelUpdateItemState::Available),
            ],
            check_error: None,
            check_error_code: None,
        };

        let resolved = resolutions(
            &snapshot,
            &["kept".into(), "clean".into()],
            &["DROPPED".into()],
        );

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].skill_id, "kept");
        assert_eq!(
            resolved[0].resolution,
            LocalDivergenceResolution::Preserve {
                local_name: "kept.local".into()
            }
        );
        assert_eq!(resolved[1].skill_id, "dropped");
        assert_eq!(resolved[1].resolution, LocalDivergenceResolution::Discard);
    }
}
