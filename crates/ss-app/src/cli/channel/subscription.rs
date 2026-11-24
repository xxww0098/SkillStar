//! Subscriber CLI handlers for shared channels: review, subscribe,
//! check, apply, rollback, resume-follow, and removed/revoked skill cleanup.

use super::{ChannelCommand, fail, print_json};
use crate::channel_ops::{
    ApplyChannelUpdateRequest, ChannelSkillUpdateResolution, ChannelSubscriptionRemoteStatus,
    ChannelSubscriptionReview, ChannelUpdateItemState, ChannelUpdateSnapshot,
    ConvertRemovedChannelSkillRequest, LocalDivergenceResolution, RollbackChannelSkillRequest,
    SharedChannelError, SubscribeChannelRequest, list_local_subscriptions, subscription_facade,
    suggested_local_name,
};

pub(super) async fn run(command: ChannelCommand) -> Result<(), SharedChannelError> {
    let facade = subscription_facade()?;
    match command {
        ChannelCommand::Review {
            repository_id,
            json,
        } => {
            let review = facade.review(repository_id).await?;
            if json {
                print_json(&review);
            } else {
                print_review(&review);
            }
        }
        ChannelCommand::Subscribe {
            repository_id,
            skills,
            empty,
            json,
        } => {
            let review = facade.review(repository_id).await?;
            let selected_skill_ids = resolve_subscribe_selection(&review, &skills, empty)?;
            let subscription = facade
                .subscribe(SubscribeChannelRequest {
                    repository_id,
                    target: review.target,
                    selected_skill_ids,
                })
                .await?;
            if json {
                print_json(&subscription);
            } else {
                println!(
                    "✓ Subscribed to channel {} at {} ({} skill(s))",
                    subscription.repository_id,
                    subscription.target.tag_name,
                    subscription.skills.len()
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
            json,
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
            if json {
                print_json(&result);
            } else {
                print_snapshot(repository_id, &result.snapshot, false);
                if result.applied_skill_ids.is_empty() {
                    println!("Nothing was applied.");
                } else {
                    println!("✓ Applied: {}", result.applied_skill_ids.join(", "));
                }
            }
        }
        ChannelCommand::Rollback {
            repository_id,
            skill,
            revision,
            keep_local,
            discard_local,
            json,
        } => {
            let targets = facade
                .list_skill_rollback_targets(repository_id, &skill)
                .await?;
            let Some(revision) = revision else {
                if json {
                    print_json(&targets);
                } else {
                    for target in &targets {
                        println!(
                            "{}  {}  {}  {}",
                            target.target.revision,
                            target.target.tag_name,
                            target.published_at,
                            target.title
                        );
                    }
                    println!(
                        "Pass --revision <N> to roll '{skill}' back to one of these releases."
                    );
                }
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
                    local_name: suggested_local_name(&skill),
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
            if json {
                print_json(&result);
            } else {
                println!(
                    "✓ '{skill}' is pinned to {}; `skillstar channel apply {repository_id}` does not move it until you resume following.",
                    result.pin.target.tag_name
                );
            }
        }
        ChannelCommand::ResumeFollow {
            repository_id,
            skill,
            json,
        } => {
            let snapshot = facade.resume_following_skill(repository_id, &skill).await?;
            if json {
                print_json(&snapshot);
            } else {
                println!("✓ Resumed following channel updates for '{skill}'.");
                print_snapshot(repository_id, &snapshot, false);
            }
        }
        ChannelCommand::RemovedUninstall {
            repository_id,
            skill,
            revoked,
            json,
        } => {
            if is_revoked_channel(repository_id, revoked)? {
                let res = facade.uninstall_revoked_skill(repository_id, &skill).await?;
                if json {
                    print_json(&res);
                } else {
                    println!("✓ Uninstalled revoked channel skill '{}'.", res.skill_id);
                }
            } else {
                let res = facade.uninstall_removed_skill(repository_id, &skill).await?;
                if json {
                    print_json(&res);
                } else {
                    println!("✓ Uninstalled removed channel skill '{}'.", res.skill_id);
                }
            }
        }
        ChannelCommand::RemovedToLocal {
            repository_id,
            skill,
            local_name,
            revoked,
            json,
        } => {
            let target_name = local_name.unwrap_or_else(|| suggested_local_name(&skill));
            let request = ConvertRemovedChannelSkillRequest {
                repository_id,
                skill_id: skill,
                local_name: target_name,
            };
            if is_revoked_channel(repository_id, revoked)? {
                let res = facade.convert_revoked_skill_to_local(request).await?;
                if json {
                    print_json(&res);
                } else {
                    println!(
                        "✓ Converted '{}' to local skill '{}'.",
                        res.skill_id,
                        res.local_name.as_deref().unwrap_or("-")
                    );
                }
            } else {
                let res = facade.convert_removed_skill_to_local(request).await?;
                if json {
                    print_json(&res);
                } else {
                    println!(
                        "✓ Converted '{}' to local skill '{}'.",
                        res.skill_id,
                        res.local_name.as_deref().unwrap_or("-")
                    );
                }
            }
        }
        _ => unreachable!("non-subscription channel command routed to subscription"),
    }
    Ok(())
}

fn is_revoked_channel(repository_id: u64, force_revoked: bool) -> Result<bool, SharedChannelError> {
    if force_revoked {
        return Ok(true);
    }
    Ok(list_local_subscriptions()?
        .into_iter()
        .find(|sub| sub.repository_id == repository_id)
        .is_some_and(|sub| sub.remote_state.status == ChannelSubscriptionRemoteStatus::Revoked))
}

fn resolve_subscribe_selection(
    review: &ChannelSubscriptionReview,
    requested: &[String],
    empty: bool,
) -> Result<Vec<String>, SharedChannelError> {
    if empty {
        return Ok(Vec::new());
    }
    if requested.is_empty() {
        return Ok(review
            .skills
            .iter()
            .map(|skill| skill.id.clone())
            .collect());
    }
    let mut selected = Vec::with_capacity(requested.len());
    for name in requested {
        let matched = review
            .skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                SharedChannelError::new(
                    crate::channel_ops::SharedChannelErrorCode::SubscriptionSelectionInvalid,
                    format!("Skill '{name}' is not present in release {}", review.target.tag_name),
                )
            })?;
        if !selected.iter().any(|id: &String| id.eq_ignore_ascii_case(&matched.id)) {
            selected.push(matched.id.clone());
        }
    }
    Ok(selected)
}

fn print_review(review: &ChannelSubscriptionReview) {
    println!(
        "Channel {}/{} ({}) — {} ({})",
        review.channel.owner,
        review.channel.name,
        review.channel.repository_id,
        review.target.tag_name,
        review.title
    );
    println!("  Publisher: @{} at {}", review.publisher.login, review.published_at);
    if !review.notes.trim().is_empty() {
        println!("  Notes: {}", review.notes.trim());
    }
    println!(
        "  Exposure: private={} full_contents={} full_history={}",
        review.exposure.private_repository,
        review.exposure.full_repository_contents_readable,
        review.exposure.full_history_readable
    );
    println!("  Skills ({}):", review.skills.len());
    for skill in &review.skills {
        let root = if skill.content_root.is_empty() {
            "."
        } else {
            &skill.content_root
        };
        println!("    - {} ({})", skill.id, root);
    }
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
        print_json(snapshot);
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
    use crate::channel_ops::{
        ChannelPublisherIdentity, ChannelReleaseTarget, ChannelRepositoryExposure,
        ChannelSubscriptionReviewSkill, ChannelUpdateBlockReason, ChannelUpdateChange,
        ChannelUpdateItem, ChannelUpdateStatus, SharedChannelAuthorization,
        SharedChannelDescriptor, SharedChannelRole, SharedChannelStatus,
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

    #[test]
    fn resolve_subscribe_selection_supports_all_subset_empty_and_rejects_unknown() {
        let review = ChannelSubscriptionReview {
            channel: SharedChannelDescriptor {
                descriptor_version: 1,
                repository_id: 42,
                organization_id: 7,
                owner: "acme".into(),
                name: "skills".into(),
                html_url: "https://github.com/acme/skills".into(),
                clone_url: "https://github.com/acme/skills.git".into(),
                role: SharedChannelRole::Subscriber,
                status: SharedChannelStatus::Active,
                authorization: SharedChannelAuthorization::default(),
                created_at: String::new(),
                updated_at: String::new(),
            },
            target: ChannelReleaseTarget {
                revision: 1,
                tag_name: "channel-v000001".into(),
                commit_sha: "a".repeat(40),
            },
            title: "Initial".into(),
            notes: String::new(),
            publisher: ChannelPublisherIdentity {
                id: 1,
                login: "alice".into(),
            },
            published_at: String::new(),
            exposure: ChannelRepositoryExposure {
                private_repository: true,
                full_repository_contents_readable: true,
                full_history_readable: true,
            },
            skills: vec![
                ChannelSubscriptionReviewSkill {
                    id: "Alpha".into(),
                    content_root: "skills/alpha".into(),
                    content_hash: "sha256:00".into(),
                    content_hash_version: 2,
                    selected: true,
                },
                ChannelSubscriptionReviewSkill {
                    id: "Beta".into(),
                    content_root: "skills/beta".into(),
                    content_hash: "sha256:11".into(),
                    content_hash_version: 2,
                    selected: true,
                },
            ],
            read_only: false,
        };

        assert_eq!(
            resolve_subscribe_selection(&review, &[], false).unwrap(),
            vec!["Alpha".to_string(), "Beta".to_string()]
        );
        assert!(
            resolve_subscribe_selection(&review, &[], true)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            resolve_subscribe_selection(&review, &["beta".into()], false).unwrap(),
            vec!["Beta".to_string()]
        );
        assert!(resolve_subscribe_selection(&review, &["missing".into()], false).is_err());
    }
}
