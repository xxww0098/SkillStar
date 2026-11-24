//! `skillstar channel`: local status, GitHub organization channel admin,
//! publication, membership, subscription lifecycle and marketplace export.

mod admin;
mod subscription;

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde::Serialize;

use crate::channel_ops::{
    SharedChannelError, SharedChannelErrorCode, channel_auto_update_state,
    export_channel_marketplace, list_local_channels, list_local_subscriptions,
    set_channel_auto_update_enabled,
};

/// Subcommands of `skillstar channel`.
#[derive(Subcommand)]
pub enum ChannelCommand {
    /// List local channels and subscriptions (works offline / without GitHub sign-in)
    List {
        #[arg(long)]
        json: bool,
    },
    /// List GitHub organizations for the signed-in identity
    Orgs {
        #[arg(long)]
        json: bool,
    },
    /// Create a new organization-private shared channel repository
    Create {
        organization: String,
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        json: bool,
    },
    /// Resume activating a pending created shared channel after App installation
    ResumeCreate {
        repository_id: u64,
        #[arg(long)]
        json: bool,
    },
    /// List candidate org repositories or scan an existing repository before registration
    RegisterScan {
        organization_id: u64,
        repository_id: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Confirm registering an existing organization-private repository as a shared channel
    RegisterConfirm {
        organization_id: u64,
        repository_id: u64,
        #[arg(long, short = 'y')]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Preview changes on the default branch for the next immutable channel release
    PublishPreview {
        repository_id: u64,
        #[arg(long)]
        json: bool,
    },
    /// Publish an immutable release from the default branch
    Publish {
        repository_id: u64,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        notes: String,
        /// Optional expected HEAD commit SHA from `publish-preview` to guard against concurrent pushes
        #[arg(long)]
        commit: Option<String>,
        #[arg(long, short = 'y')]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// List effective members and pending repository invitations
    Members {
        repository_id: u64,
        #[arg(long)]
        json: bool,
    },
    /// Invite a GitHub user to a shared channel as subscriber or publisher
    Invite {
        repository_id: u64,
        username: String,
        #[arg(long, default_value = "subscriber", value_parser = ["subscriber", "publisher"])]
        role: String,
        #[arg(long)]
        json: bool,
    },
    /// Cancel a pending channel repository invitation
    CancelInvite {
        repository_id: u64,
        invitation_id: u64,
        #[arg(long)]
        json: bool,
    },
    /// Cancel and recreate a pending non-admin channel invitation
    Reinvite {
        repository_id: u64,
        invitation_id: u64,
        #[arg(long)]
        json: bool,
    },
    /// Remove a user's direct collaborator grant and verify remaining GitHub access
    Revoke {
        repository_id: u64,
        username: String,
        #[arg(long)]
        json: bool,
    },
    /// List pending GitHub repository invitations in your inbox
    Inbox {
        #[arg(long)]
        json: bool,
    },
    /// Accept a repository invitation (or resume an accepted import by --resume-repo)
    AcceptInvite {
        #[arg(required_unless_present = "resume_repo")]
        invitation_id: Option<u64>,
        #[arg(long = "resume-repo", conflicts_with = "invitation_id")]
        resume_repo: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Decline a pending repository invitation
    DeclineInvite {
        invitation_id: u64,
        #[arg(long)]
        json: bool,
    },
    /// Review the latest verified release of an active channel before subscribing
    Review {
        repository_id: u64,
        #[arg(long)]
        json: bool,
    },
    /// Subscribe to an active channel's latest verified release
    Subscribe {
        repository_id: u64,
        /// Specific skill id(s) to subscribe (defaults to all non-removed skills unless --empty)
        #[arg(long = "skill", short = 's', value_delimiter = ',', conflicts_with = "empty")]
        skills: Vec<String>,
        /// Subscribe to 0 skills initially while tracking future releases
        #[arg(long)]
        empty: bool,
        #[arg(long)]
        json: bool,
    },
    /// Inspect or toggle per-channel protected automatic updates (local-only)
    AutoUpdate {
        repository_id: u64,
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        #[arg(long)]
        disable: bool,
        #[arg(long)]
        json: bool,
    },
    /// Check subscribed channels for a newer release (all when omitted)
    Check {
        repository_id: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Check a channel and apply its latest release
    Apply {
        repository_id: u64,
        /// Blocked Skill(s) whose local edits are kept as a `.local` copy before upgrading
        #[arg(long = "keep-local", value_delimiter = ',')]
        keep_local: Vec<String>,
        /// Blocked Skill(s) whose local edits are discarded by the upgrade
        #[arg(long = "discard-local", value_delimiter = ',')]
        discard_local: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Roll one Skill back to an earlier release and pin it there
    /// (lists the releases when --revision is omitted)
    Rollback {
        repository_id: u64,
        skill: String,
        #[arg(long)]
        revision: Option<u64>,
        /// Keep local edits as a `.local` copy before rolling back
        #[arg(long, conflicts_with = "discard_local")]
        keep_local: bool,
        /// Discard local edits
        #[arg(long)]
        discard_local: bool,
        #[arg(long)]
        json: bool,
    },
    /// Unpin a rolled-back Skill so channel updates follow the latest release again
    ResumeFollow {
        repository_id: u64,
        skill: String,
        #[arg(long)]
        json: bool,
    },
    /// Uninstall a Skill marked removed_from_channel (or from a revoked channel)
    RemovedUninstall {
        repository_id: u64,
        skill: String,
        /// Force revoked-channel handling (auto-detected from local subscription when omitted)
        #[arg(long)]
        revoked: bool,
        #[arg(long)]
        json: bool,
    },
    /// Convert a removed/revoked channel Skill into an independent local copy
    RemovedToLocal {
        repository_id: u64,
        skill: String,
        /// Destination local skill name (defaults to `<skill>.local`)
        #[arg(long = "local-name")]
        local_name: Option<String>,
        /// Force revoked-channel handling (auto-detected from local subscription when omitted)
        #[arg(long)]
        revoked: bool,
        #[arg(long)]
        json: bool,
    },
    /// Export the channel's installed Skills as a Claude Code plugin
    /// marketplace directory (local-only: no GitHub sign-in, no network)
    ExportMarketplace {
        repository_id: u64,
        /// Directory to create; must not exist or be empty
        #[arg(long = "out")]
        out: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

pub fn cmd_channel(command: ChannelCommand) {
    match command {
        ChannelCommand::List { json } => {
            if let Err(error) = run_local_list(json) {
                fail_channel(&error);
            }
        }
        ChannelCommand::AutoUpdate {
            repository_id,
            enable,
            disable,
            json,
        } => {
            if let Err(error) = run_local_auto_update(repository_id, enable, disable, json) {
                fail_channel(&error);
            }
        }
        ChannelCommand::ExportMarketplace {
            repository_id,
            out,
            json,
        } => export_marketplace(repository_id, &out, json),
        remote => {
            let runtime = build_runtime();
            let outcome = runtime.block_on(dispatch_remote(remote));
            if let Err(error) = outcome {
                fail_channel(&error);
            }
        }
    }
}

async fn dispatch_remote(command: ChannelCommand) -> Result<(), SharedChannelError> {
    match command {
        ChannelCommand::Orgs { .. }
        | ChannelCommand::Create { .. }
        | ChannelCommand::ResumeCreate { .. }
        | ChannelCommand::RegisterScan { .. }
        | ChannelCommand::RegisterConfirm { .. }
        | ChannelCommand::PublishPreview { .. }
        | ChannelCommand::Publish { .. }
        | ChannelCommand::Members { .. }
        | ChannelCommand::Invite { .. }
        | ChannelCommand::CancelInvite { .. }
        | ChannelCommand::Reinvite { .. }
        | ChannelCommand::Revoke { .. }
        | ChannelCommand::Inbox { .. }
        | ChannelCommand::AcceptInvite { .. }
        | ChannelCommand::DeclineInvite { .. } => admin::run(command).await,
        ChannelCommand::Review { .. }
        | ChannelCommand::Subscribe { .. }
        | ChannelCommand::Check { .. }
        | ChannelCommand::Apply { .. }
        | ChannelCommand::Rollback { .. }
        | ChannelCommand::ResumeFollow { .. }
        | ChannelCommand::RemovedUninstall { .. }
        | ChannelCommand::RemovedToLocal { .. } => subscription::run(command).await,
        ChannelCommand::List { .. }
        | ChannelCommand::AutoUpdate { .. }
        | ChannelCommand::ExportMarketplace { .. } => {
            unreachable!("local-only channel commands are dispatched before runtime")
        }
    }
}

fn build_runtime() -> tokio::runtime::Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => fail(&format!("Failed to start async runtime: {error}")),
    }
}

#[derive(Serialize)]
struct LocalListOutput {
    channels: Vec<crate::channel_ops::SharedChannelDescriptor>,
    subscriptions: Vec<crate::channel_ops::ChannelSubscriptionView>,
}

fn run_local_list(json: bool) -> Result<(), SharedChannelError> {
    let channels = list_local_channels()?;
    let subscriptions = list_local_subscriptions()?;
    if json {
        print_json(&LocalListOutput {
            channels,
            subscriptions,
        });
        return Ok(());
    }
    if subscriptions.is_empty() && channels.is_empty() {
        println!("No shared channels or subscriptions.");
        return Ok(());
    }
    if !subscriptions.is_empty() {
        println!("Subscriptions:");
        for sub in &subscriptions {
            let revision = sub
                .target
                .as_ref()
                .map(|target| target.tag_name.as_str())
                .unwrap_or("-");
            let skills = if sub.selected_skill_ids.is_empty() {
                "-".to_string()
            } else {
                sub.selected_skill_ids.join(", ")
            };
            println!(
                "  {}  {}  [{:?} | auto-update={}]  {}",
                sub.repository_id,
                revision,
                sub.remote_state.status,
                if sub.auto_update.enabled { "on" } else { "off" },
                skills
            );
        }
    }
    if !channels.is_empty() {
        println!("Registered channels:");
        for ch in &channels {
            println!(
                "  {}  {}/{}  [{:?} | {:?}]",
                ch.repository_id, ch.owner, ch.name, ch.role, ch.status
            );
        }
    }
    Ok(())
}

fn run_local_auto_update(
    repository_id: u64,
    enable: bool,
    disable: bool,
    json: bool,
) -> Result<(), SharedChannelError> {
    let state = if enable || disable {
        build_runtime().block_on(set_channel_auto_update_enabled(repository_id, enable))?
    } else {
        channel_auto_update_state(repository_id)?
    };
    if json {
        print_json(&state);
    } else {
        println!(
            "Channel {repository_id} auto-update: {}",
            if state.enabled { "enabled" } else { "disabled" }
        );
        if let Some(next) = &state.next_check_at {
            println!("  Next check: {next}");
        }
        if let Some(last) = &state.last_run {
            println!("  Last run: {:?} ({})", last.status, last.started_at);
        }
    }
    Ok(())
}

#[derive(Serialize)]
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
                print_json(&output);
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

pub(super) fn confirm_or_yes(prompt: &str, yes: bool) {
    if yes {
        return;
    }
    if !io::stdin().is_terminal() {
        fail("Refusing to proceed without --yes in non-interactive mode.");
    }
    print!("{prompt} [y/N]: ");
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        fail("Failed to read confirmation.");
    }
    if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        fail("Cancelled.");
    }
}

pub(super) fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(error) => fail(&format!("Failed to serialize JSON output: {error}")),
    }
}

pub(super) fn fail_channel(error: &SharedChannelError) -> ! {
    if error.code == SharedChannelErrorCode::NotAuthenticated {
        fail(&format!(
            "Shared channels require a GitHub sign-in (open SkillStar desktop app → My Skills → Channels, or sign in from the sidebar): {}",
            error.message
        ));
    }
    fail(&error.message)
}

pub(super) fn fail(message: &str) -> ! {
    eprintln!("✗ {message}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::ChannelCommand;
    use crate::cli::{Cli, Commands};

    #[test]
    fn parses_all_channel_subcommands() {
        let cases: &[&[&str]] = &[
            &["skillstar", "channel", "list", "--json"],
            &["skillstar", "channel", "orgs", "--json"],
            &["skillstar", "channel", "create", "acme", "team-skills", "--description", "desc", "--json"],
            &["skillstar", "channel", "resume-create", "42", "--json"],
            &["skillstar", "channel", "register-scan", "10", "42", "--json"],
            &["skillstar", "channel", "register-confirm", "10", "42", "--yes", "--json"],
            &["skillstar", "channel", "publish-preview", "42", "--json"],
            &["skillstar", "channel", "publish", "42", "--title", "v1", "--notes", "n", "--yes", "--json"],
            &["skillstar", "channel", "members", "42", "--json"],
            &["skillstar", "channel", "invite", "42", "octocat", "--role", "publisher", "--json"],
            &["skillstar", "channel", "cancel-invite", "42", "101", "--json"],
            &["skillstar", "channel", "reinvite", "42", "101", "--json"],
            &["skillstar", "channel", "revoke", "42", "octocat", "--json"],
            &["skillstar", "channel", "inbox", "--json"],
            &["skillstar", "channel", "accept-invite", "101", "--json"],
            &["skillstar", "channel", "accept-invite", "--resume-repo", "42", "--json"],
            &["skillstar", "channel", "decline-invite", "101", "--json"],
            &["skillstar", "channel", "review", "42", "--json"],
            &["skillstar", "channel", "subscribe", "42", "--skill", "s1", "--json"],
            &["skillstar", "channel", "subscribe", "42", "--empty", "--json"],
            &["skillstar", "channel", "auto-update", "42", "--disable", "--json"],
            &["skillstar", "channel", "check", "42", "--json"],
            &["skillstar", "channel", "apply", "42", "--keep-local", "s1", "--json"],
            &["skillstar", "channel", "rollback", "42", "s1", "--revision", "3", "--discard-local", "--json"],
            &["skillstar", "channel", "resume-follow", "42", "s1", "--json"],
            &["skillstar", "channel", "removed-uninstall", "42", "s1", "--revoked", "--json"],
            &["skillstar", "channel", "removed-to-local", "42", "s1", "--local-name", "s1-copy", "--json"],
            &["skillstar", "channel", "export-marketplace", "42", "--out", "/tmp/mkt", "--json"],
        ];

        assert_eq!(cases.len(), 28);
        for argv in cases {
            let cli = Cli::try_parse_from(*argv)
                .unwrap_or_else(|err| panic!("failed to parse {:?}: {err}", argv));
            assert!(matches!(cli.command, Commands::Channel { .. }));
        }

        let cli = Cli::try_parse_from([
            "skillstar",
            "channel",
            "auto-update",
            "42",
            "--disable",
        ])
        .unwrap();
        match cli.command {
            Commands::Channel {
                command:
                    ChannelCommand::AutoUpdate {
                        repository_id,
                        enable,
                        disable,
                        json,
                    },
            } => {
                assert_eq!(repository_id, 42);
                assert!(!enable);
                assert!(disable);
                assert!(!json);
            }
            _ => panic!("expected channel auto-update"),
        }
    }
}
