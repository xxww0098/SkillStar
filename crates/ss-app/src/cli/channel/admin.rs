//! Administrative CLI handlers for shared channels: orgs, channel creation,
//! existing-repo registration, immutable publication and membership/inbox.

use super::{ChannelCommand, confirm_or_yes, fail, print_json};
use crate::channel_ops::{
    ChannelInviteRole, CreateChannelInvitationRequest, CreateSharedChannelRequest,
    ExistingChannelScanRequest, SharedChannelError, membership_facade, publication_facade,
    registration_facade, shared_channel_facade,
};

pub(super) async fn run(command: ChannelCommand) -> Result<(), SharedChannelError> {
    match command {
        ChannelCommand::Orgs { json } => {
            let orgs = shared_channel_facade()?.list_organizations().await?;
            if json {
                print_json(&orgs);
            } else if orgs.is_empty() {
                println!("No GitHub organizations available for the signed-in user.");
            } else {
                for org in orgs {
                    println!(
                        "{}  {}  [admin={}]",
                        org.id, org.login, org.viewer_is_admin
                    );
                }
            }
        }
        ChannelCommand::Create {
            organization,
            name,
            description,
            json,
        } => {
            let descriptor = shared_channel_facade()?
                .create_channel(CreateSharedChannelRequest {
                    organization,
                    repository_name: name,
                    description,
                })
                .await?;
            if json {
                print_json(&descriptor);
            } else {
                println!(
                    "✓ Created channel {} ({}/{}) [{:?}]",
                    descriptor.repository_id, descriptor.owner, descriptor.name, descriptor.status
                );
            }
        }
        ChannelCommand::ResumeCreate {
            repository_id,
            json,
        } => {
            let descriptor = shared_channel_facade()?
                .resume_channel(repository_id)
                .await?;
            if json {
                print_json(&descriptor);
            } else {
                println!(
                    "✓ Channel {} ({}/{}) status: {:?}",
                    descriptor.repository_id, descriptor.owner, descriptor.name, descriptor.status
                );
            }
        }
        ChannelCommand::RegisterScan {
            organization_id,
            repository_id,
            json,
        } => {
            let facade = registration_facade()?;
            if let Some(repository_id) = repository_id {
                let session_id = uuid::Uuid::new_v4().to_string();
                let preview = facade
                    .scan(
                        ExistingChannelScanRequest {
                            organization_id,
                            repository_id,
                        },
                        session_id,
                    )
                    .await?;
                if json {
                    print_json(&preview);
                } else {
                    print_register_preview(&preview);
                }
            } else {
                let candidates = facade.list_candidates(organization_id).await?;
                if json {
                    print_json(&candidates);
                } else if candidates.is_empty() {
                    println!("No eligible organization-private repositories found.");
                } else {
                    for repo in candidates {
                        println!(
                            "{}  {}/{}  [{:?} | registered={}]",
                            repo.repository_id,
                            repo.owner,
                            repo.name,
                            repo.role,
                            repo.already_registered
                        );
                    }
                }
            }
        }
        ChannelCommand::RegisterConfirm {
            organization_id,
            repository_id,
            yes,
            json,
        } => {
            let facade = registration_facade()?;
            let session_id = uuid::Uuid::new_v4().to_string();
            let preview = facade
                .scan(
                    ExistingChannelScanRequest {
                        organization_id,
                        repository_id,
                    },
                    session_id,
                )
                .await?;
            if !json {
                print_register_preview(&preview);
            }
            confirm_or_yes(
                "Register this repository as a shared channel (members can read the full repository and Git history)?",
                yes,
            );
            let descriptor = facade.confirm(&preview.session_id).await?;
            if json {
                print_json(&descriptor);
            } else {
                println!(
                    "✓ Registered existing channel {} ({}/{})",
                    descriptor.repository_id, descriptor.owner, descriptor.name
                );
            }
        }
        ChannelCommand::PublishPreview {
            repository_id,
            json,
        } => {
            let session_id = uuid::Uuid::new_v4().to_string();
            let preview = publication_facade()?
                .preview(repository_id, session_id)
                .await?;
            if json {
                print_json(&preview);
            } else {
                print_publish_preview(&preview);
            }
        }
        ChannelCommand::Publish {
            repository_id,
            title,
            notes,
            commit,
            yes,
            json,
        } => {
            let facade = publication_facade()?;
            let session_id = uuid::Uuid::new_v4().to_string();
            let preview = facade.preview(repository_id, session_id).await?;
            if let Some(expected) = commit.as_deref()
                && !preview.commit_sha.eq_ignore_ascii_case(expected.trim())
            {
                let _ = facade.cancel(&preview.session_id);
                fail(&format!(
                    "Default branch moved from {} to {}; run `skillstar channel publish-preview {}` again.",
                    expected.trim(),
                    preview.commit_sha,
                    repository_id
                ));
            }
            if !json {
                print_publish_preview(&preview);
            }
            confirm_or_yes(
                &format!(
                    "Publish immutable release {} at commit {}?",
                    preview.tag_name, preview.commit_sha
                ),
                yes,
            );
            let published = facade.publish(&preview.session_id, title, notes).await?;
            if json {
                print_json(&published);
            } else {
                println!(
                    "✓ Published {} ({})",
                    published.manifest.tag_name, published.release.html_url
                );
            }
        }
        ChannelCommand::Members {
            repository_id,
            json,
        } => {
            let snap = membership_facade()?.list_membership(repository_id).await?;
            if json {
                print_json(&snap);
            } else {
                println!("Members ({}):", snap.members.len());
                for m in snap.members {
                    println!(
                        "  {}  [{:?} / {}]",
                        m.user.login, m.role, m.github_role_name
                    );
                }
                println!("Pending invitations ({}):", snap.invitations.len());
                for inv in snap.invitations {
                    let invitee = inv
                        .invitee
                        .as_ref()
                        .map(|u| u.login.as_str())
                        .unwrap_or("-");
                    println!(
                        "  {}  @{}  [{:?} / {:?}]",
                        inv.id, invitee, inv.role, inv.status
                    );
                }
            }
        }
        ChannelCommand::Invite {
            repository_id,
            username,
            role,
            json,
        } => {
            let parsed_role = match role.as_str() {
                "publisher" => ChannelInviteRole::Publisher,
                _ => ChannelInviteRole::Subscriber,
            };
            let action = membership_facade()?
                .invite(CreateChannelInvitationRequest {
                    repository_id,
                    username,
                    role: parsed_role,
                })
                .await?;
            if json {
                print_json(&action);
            } else {
                println!(
                    "Invitation for @{} ({:?}): {:?}",
                    action.username, action.role, action.status
                );
            }
        }
        ChannelCommand::CancelInvite {
            repository_id,
            invitation_id,
            json,
        } => {
            let action = membership_facade()?
                .cancel(repository_id, invitation_id)
                .await?;
            if json {
                print_json(&action);
            } else {
                println!(
                    "✓ Cancelled invitation {} for @{}",
                    invitation_id, action.username
                );
            }
        }
        ChannelCommand::Reinvite {
            repository_id,
            invitation_id,
            json,
        } => {
            let action = membership_facade()?
                .resend(repository_id, invitation_id)
                .await?;
            if json {
                print_json(&action);
            } else {
                println!(
                    "✓ Re-sent invitation for @{} ({:?}): {:?}",
                    action.username, action.role, action.status
                );
            }
        }
        ChannelCommand::Revoke {
            repository_id,
            username,
            json,
        } => {
            let result = membership_facade()?
                .revoke_member(repository_id, &username)
                .await?;
            if json {
                print_json(&result);
            } else {
                println!(
                    "Revoke @{} on channel {}: {:?}",
                    result.username, result.repository_id, result.status
                );
                if let Some(role) = result.effective_role {
                    println!(
                        "  Note: inherited GitHub access remains ({:?}, source: {:?}); manage team/org access in GitHub.",
                        role, result.access_source
                    );
                }
            }
        }
        ChannelCommand::Inbox { json } => {
            let invitations = membership_facade()?.list_inbox().await?;
            if json {
                print_json(&invitations);
            } else if invitations.is_empty() {
                println!("No pending channel invitations in inbox.");
            } else {
                for inv in invitations {
                    let inviter = inv
                        .inviter
                        .as_ref()
                        .map(|u| u.login.as_str())
                        .unwrap_or("unknown");
                    println!(
                        "{}  {}/{} (repo {})  from @{}  [{:?}]",
                        inv.id,
                        inv.owner,
                        inv.repository_name,
                        inv.repository_id,
                        inviter,
                        inv.effective_role
                    );
                }
            }
        }
        ChannelCommand::AcceptInvite {
            invitation_id,
            resume_repo,
            json,
        } => {
            let facade = membership_facade()?;
            let descriptor = if let Some(repo_id) = resume_repo {
                facade.resume_accepted_channel(repo_id).await?
            } else if let Some(id) = invitation_id {
                facade.accept(id).await?
            } else {
                fail("Provide <invitation_id> or --resume-repo <repository_id>");
            };
            if json {
                print_json(&descriptor);
            } else {
                println!(
                    "✓ Accepted channel {} ({}/{}) [{:?}]",
                    descriptor.repository_id, descriptor.owner, descriptor.name, descriptor.status
                );
            }
        }
        ChannelCommand::DeclineInvite {
            invitation_id,
            json,
        } => {
            let action = membership_facade()?.decline(invitation_id).await?;
            if json {
                print_json(&action);
            } else {
                println!("✓ Declined invitation {invitation_id}");
            }
        }
        _ => unreachable!("non-admin channel command routed to admin"),
    }
    Ok(())
}

fn print_register_preview(preview: &crate::channel_ops::ExistingChannelScanPreview) {
    println!(
        "Scan preview for {}/{} (repo {}): {} total files",
        preview.repository.owner,
        preview.repository.name,
        preview.repository.repository_id,
        preview.total_files
    );
    println!("  Skills ({}):", preview.skills.len());
    for skill in &preview.skills {
        println!(
            "    - {} ({})",
            skill.id,
            if skill.folder_path.is_empty() {
                "."
            } else {
                &skill.folder_path
            }
        );
    }
    if !preview.non_skill_files.is_empty() {
        println!("  Non-skill tracked files ({}):", preview.non_skill_files.len());
        for path in &preview.non_skill_files {
            println!("    - {path}");
        }
    }
    if preview.exposure.full_repository_contents_readable || preview.exposure.full_history_readable
    {
        println!(
            "  Warning: channel members can read the full repository contents and complete Git history."
        );
    }
}

fn print_publish_preview(preview: &crate::channel_ops::ChannelPublishPreview) {
    println!(
        "Publish preview for channel {}: {} at commit {}",
        preview.repository_id, preview.tag_name, preview.commit_sha
    );
    for item in &preview.changes {
        println!(
            "  {:?}  {}  ({})",
            item.status,
            item.id,
            if item.content_root.is_empty() {
                "."
            } else {
                &item.content_root
            }
        );
    }
}
