//! SkillStar CLI types and dispatcher.

use clap::{Parser, Subcommand};

mod channel;
mod commands;
mod doctor;
mod install;
mod install_preview;
mod manage;
mod mcp_approve;
mod team;
mod update;

pub use channel::cmd_channel;
pub use commands::*;
pub use install::cmd_install;
pub use manage::{cmd_publish, cmd_remove};
pub use update::{UpdateOpts, cmd_update};

mod helpers;
pub use helpers::*;

/// CLI root type — owned by this crate.
#[derive(Parser)]
#[command(
    name = "skillstar",
    about = "SkillStar — Skill management for AI agents",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// All top-level CLI commands.
#[derive(Subcommand)]
pub enum Commands {
    /// List installed skills (hub + local authored)
    List {
        /// Only show local-authored skills
        #[arg(long)]
        local: bool,
        /// Only show hub (repo-backed) skills
        #[arg(long)]
        hub: bool,
    },
    /// Search skills in the marketplace
    #[command(alias = "search")]
    Find {
        /// Search query — fuzzy matches name, description, author
        query: Option<String>,
        /// Max number of results
        #[arg(long, short = 'n', default_value_t = 20)]
        limit: u32,
        /// Output as JSON (non-interactive)
        #[arg(long)]
        json: bool,
    },
    /// Install a skill from a Git URL, owner/repo, or local path
    #[command(alias = "add")]
    Install {
        /// Install to selected Agent user directories instead of a project
        #[arg(long, short = 'g')]
        global: bool,
        /// Target project path for project-level install (defaults to current dir)
        #[arg(long, conflicts_with = "global")]
        project: Option<String>,
        /// Target agent id(s), repeatable or comma-separated, e.g. --agent codex,opencode
        #[arg(long = "agent", short = 'a', value_delimiter = ',')]
        agent: Vec<String>,
        /// Explicit skill name (useful when one repo contains multiple skills)
        #[arg(long)]
        name: Option<String>,
        /// Skill name filter(s) for selective install from multi-skill repos (repeatable or comma-separated)
        #[arg(long = "skill", short = 's', value_delimiter = ',')]
        skill: Vec<String>,
        /// List skills in the source without installing
        #[arg(long, short = 'l', conflicts_with_all = ["all", "preview"])]
        list: bool,
        /// Install every discovered skill to every supported Agent without prompts
        #[arg(long, conflicts_with_all = ["skill", "name"])]
        all: bool,
        /// Skip prompts (all skills; Settings-enabled Agents; Project scope)
        #[arg(long, short = 'y')]
        yes: bool,
        /// Copy files instead of linking for project or global Agent targets
        #[arg(long)]
        copy: bool,
        /// Preview/dry-run: show what would be installed without mutating hub, lockfile, or project links
        #[arg(long)]
        preview: bool,
        /// Refresh the source cache from upstream before scanning
        #[arg(long)]
        refresh: bool,
        /// Git URL of the skill repository
        url: String,
    },
    /// Update installed skills
    ///
    /// Without a name: check upstream first and update only the Skills that
    /// changed. With a name: update that Skill from upstream now.
    Update {
        /// Name of a specific skill to update (updates all changed ones if omitted)
        name: Option<String>,
        /// Only check upstream and report; change nothing
        #[arg(long, conflicts_with = "dry_run")]
        check: bool,
        /// Show what would be updated; change nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Report skill storage health. `--fix` repairs only what SkillStar can prove it owns.
    /// `--adopt` previews taking Agent-installed skills under management; `--apply` does it.
    Doctor {
        /// Print the report, and the repair outcome when `--fix` or `--adopt` is set, as JSON
        #[arg(long)]
        json: bool,
        /// Apply the ownership repair plan. Does not adopt Agent directories.
        #[arg(long)]
        fix: bool,
        /// With `--fix`, show what would change and write nothing
        #[arg(long, requires = "fix")]
        dry_run: bool,
        /// Preview Agent-skill intake. Writes nothing unless `--apply` is also set.
        #[arg(long)]
        adopt: bool,
        /// With `--adopt`, carry out the previewed intake.
        #[arg(long, requires = "adopt")]
        apply: bool,
    },
    /// Shared channels: check, apply and roll back channel releases
    Channel {
        #[command(subcommand)]
        command: ChannelCommand,
    },
    /// Remove one or more installed skills
    #[command(alias = "rm", alias = "uninstall")]
    Remove {
        /// Skill name(s) or source(s) to remove — space- or comma-separated (e.g. `rm a b` or `rm a,b`); a name that is not an installed skill matches a lock source and removes every skill from it
        #[arg(required_unless_present = "all", value_delimiter = ',')]
        names: Vec<String>,
        /// Remove every installed skill
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Create a new skill template in the current directory
    #[command(alias = "create")]
    Init {
        /// Folder name to create (defaults to `my-new-skill`)
        name: Option<String>,
    },
    /// Publish current directory as a skill to GitHub
    Publish,
    /// Force launch GUI mode
    Gui,
    /// Terminal commands for the project-skill MCP. `serve` is handled before Clap.
    Mcp {
        #[command(subcommand)]
        command: McpCliCommand,
    },
    /// Team intelligence: recall, health, friction notes, digest
    Team {
        #[command(subcommand)]
        command: TeamCommand,
    },
}

/// Subcommands of `skillstar team`.
#[derive(Subcommand)]
pub enum TeamCommand {
    /// Search installed skills and local notes (BM25 + neighbor boost)
    Recall {
        /// Keywords describing the current task
        query: String,
        /// Max number of hits
        #[arg(long, short = 'n', default_value_t = 12)]
        limit: u32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show skill health (usage × freshness × recall)
    Health {
        #[arg(long)]
        json: bool,
    },
    /// Summarize coverage, silent skills, friction, and recent notes
    Digest {
        #[arg(long)]
        json: bool,
    },
    /// Save a friction-triggered note (local; not a tutorial)
    Share {
        #[arg(long)]
        title: String,
        #[arg(long)]
        body: String,
        /// Optional related installed skill
        #[arg(long)]
        skill: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[arg(long, default_value_t = 0.7)]
        confidence: f32,
        #[arg(long)]
        json: bool,
    },
    /// List saved team notes
    Notes {
        #[arg(long)]
        json: bool,
    },
    /// Record session friction (interrupts, rejects, retries, corrections)
    Friction {
        #[arg(long, default_value_t = 0)]
        interrupts: u32,
        #[arg(long, default_value_t = 0)]
        rejects: u32,
        #[arg(long, default_value_t = 0)]
        retries: u32,
        #[arg(long, default_value_t = 0)]
        corrections: u32,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Record that an installed skill was used
    Used {
        /// Installed skill name
        name: String,
    },
}

/// Subcommands of `skillstar channel`.
#[derive(Subcommand)]
pub enum ChannelCommand {
    /// List subscribed channels (repository id, release, Skills)
    List,
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
    },
    /// Export the channel's installed Skills as a Claude Code plugin
    /// marketplace directory (local-only: no GitHub sign-in, no network)
    ExportMarketplace {
        repository_id: u64,
        /// Directory to create; must not exist or be empty
        #[arg(long = "out")]
        out: std::path::PathBuf,
        #[arg(long)]
        json: bool,
    },
}

/// Subcommands of `skillstar mcp` that a person runs in a terminal.
/// `mcp serve` is dispatched before Clap.
#[derive(Subcommand)]
pub enum McpCliCommand {
    /// Show a plan diff and record a SkillStar approval
    Approve {
        /// Plan id from `recommend_project_skills`
        plan_id: String,
    },
}

/// Options passed to the install handler.
pub struct InstallOpts<'a> {
    pub url: &'a str,
    pub name: Option<&'a str>,
    pub skill: &'a [String],
    pub global: bool,
    pub project: Option<&'a str>,
    pub agent: &'a [String],
    pub list: bool,
    pub all: bool,
    pub yes: bool,
    pub copy: bool,
    pub preview: bool,
    pub refresh: bool,
}

/// Options passed to the remove handler.
pub struct RemoveOpts<'a> {
    pub names: &'a [String],
    pub all: bool,
    pub yes: bool,
}

/// Dispatch the CLI. `migrate_and_run` is the process seam: the product
/// binary passes marketplace snapshot init, tests can pass a no-op.
pub fn run(args: Vec<String>, migrate_and_run: fn()) {
    // Channel ownership is enforced by the skills domain without startup registration.

    // D-081: remove the pre-D-081 hub/cache model once, idempotently.
    ss_skills::legacy_cleanup::run_once();

    // Migration (only runs once at startup)
    migrate_and_run();

    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::List { local, hub } => cmd_list(cmd_list_filter(local, hub)),
        Commands::Find { query, limit, json } => cmd_find(query.as_deref(), limit, json),
        Commands::Install {
            url,
            global,
            project,
            agent,
            name,
            skill,
            list,
            all,
            yes,
            copy,
            preview,
            refresh,
        } => cmd_install(InstallOpts {
            url: &url,
            name: name.as_deref(),
            skill: &skill,
            global,
            project: project.as_deref(),
            agent: &agent,
            list,
            all,
            yes,
            copy,
            preview,
            refresh,
        }),
        Commands::Update {
            name,
            check,
            dry_run,
        } => cmd_update(UpdateOpts {
            name: name.as_deref(),
            check,
            dry_run,
        }),
        Commands::Doctor {
            json,
            fix,
            dry_run,
            adopt,
            apply,
        } => doctor::cmd_doctor(json, fix, dry_run, adopt, apply),
        Commands::Channel { command } => cmd_channel(command),
        Commands::Remove { names, all, yes } => cmd_remove(RemoveOpts {
            names: &names,
            all,
            yes,
        }),
        Commands::Init { name } => cmd_init(name.as_deref()),
        Commands::Publish => cmd_publish(),
        Commands::Gui => println!("Launching SkillStar GUI..."),
        Commands::Mcp { command } => match command {
            McpCliCommand::Approve { plan_id } => mcp_approve::cmd_approve(&plan_id),
        },
        Commands::Team { command } => match command {
            TeamCommand::Recall { query, limit, json } => team::cmd_recall(&query, limit, json),
            TeamCommand::Health { json } => team::cmd_health(json),
            TeamCommand::Digest { json } => team::cmd_digest(json),
            TeamCommand::Share {
                title,
                body,
                skill,
                tags,
                confidence,
                json,
            } => team::cmd_share(&title, &body, skill.as_deref(), &tags, confidence, json),
            TeamCommand::Notes { json } => team::cmd_notes(json),
            TeamCommand::Friction {
                interrupts,
                rejects,
                retries,
                corrections,
                task,
                json,
            } => team::cmd_friction(
                interrupts,
                rejects,
                retries,
                corrections,
                task.as_deref(),
                json,
            ),
            TeamCommand::Used { name } => team::cmd_used(&name),
        },
    }
}

/// Return true when `args[1]` is a known CLI subcommand/flag that must not
/// fall through to GUI mode. Driven from the same Clap `Commands` surface
/// (and its aliases) so new subcommands only need updating here.
pub fn is_cli_subcommand(first_arg: &str) -> bool {
    matches!(
        first_arg,
        "list"
            | "find"
            | "search"
            | "install"
            | "add"
            | "update"
            | "remove"
            | "rm"
            | "uninstall"
            | "init"
            | "create"
            | "publish"
            | "channel"
            | "mcp"
            | "team"
            | "help"
            | "-h"
            | "--help"
            | "-V"
            | "--version"
    )
}

/// `gui` is a CLI subcommand that intentionally falls through to GUI mode.
pub fn is_gui_force_arg(first_arg: &str) -> bool {
    first_arg == "gui"
}

// ── Input classification (install/add routing) ───────────────────────

/// Classify a raw `install`/`add` argument to route to the right installer.
pub enum AddKind {
    Repo,
    Bundle(std::path::PathBuf),
    LocalDir(std::path::PathBuf),
}

pub fn classify_add_input(input: &str) -> AddKind {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git@")
        || lower.starts_with("ssh://")
    {
        return AddKind::Repo;
    }
    let looks_like_path = trimmed.starts_with('.')
        || trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.starts_with('\\')
        || (trimmed.len() >= 2 && trimmed.chars().nth(1) == Some(':'));
    if looks_like_path {
        let expanded = expand_tilde(trimmed);
        let path = std::path::PathBuf::from(&expanded);
        if is_bundle_file(&path) {
            return AddKind::Bundle(path);
        }
        if path.is_dir() {
            return AddKind::LocalDir(path);
        }
    }
    AddKind::Repo
}

fn expand_tilde(input: &str) -> String {
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().to_string();
    }
    if input == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home.to_string_lossy().to_string();
    }
    input.to_string()
}

fn is_bundle_file(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ags") | Some("agd")
    )
}

#[cfg(test)]
mod mode_tests {
    use super::{is_cli_subcommand, is_gui_force_arg};

    #[test]
    fn known_cli_subcommands_are_detected() {
        for arg in [
            "list",
            "find",
            "search",
            "install",
            "add",
            "update",
            "remove",
            "rm",
            "uninstall",
            "init",
            "create",
            "publish",
            "channel",
            "mcp",
            "team",
            "help",
            "-h",
            "--help",
            "-V",
            "--version",
        ] {
            assert!(is_cli_subcommand(arg), "{arg} should be CLI");
            assert!(!is_gui_force_arg(arg));
        }
    }

    #[test]
    fn gui_force_is_not_cli_dispatch() {
        assert!(is_gui_force_arg("gui"));
        assert!(!is_cli_subcommand("gui"));
    }

    #[test]
    fn unknown_args_do_not_look_like_cli() {
        assert!(!is_cli_subcommand("totally-unknown"));
        assert!(!is_cli_subcommand("--some-os-flag"));
    }
}
