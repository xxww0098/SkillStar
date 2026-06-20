//! SkillStar CLI types and dispatcher.

use clap::{Parser, Subcommand};

mod commands;
pub use commands::*;

mod helpers;
pub use helpers::*;

/// CLI root type — owned by this crate.
#[derive(Parser)]
#[command(
    name = "skillstar",
    about = "SkillStar — Skill management for AI agents"
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
        /// Install to hub only (do not link into current project)
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
        /// Install every skill discovered in the source
        #[arg(long, conflicts_with_all = ["skill", "name"])]
        all: bool,
        /// Skip interactive prompts (auto-detect agents, take defaults)
        #[arg(long, short = 'y')]
        yes: bool,
        /// Prefer copy deployment over symlink for project links
        #[arg(long)]
        copy: bool,
        /// Preview/dry-run: show what would be installed without mutating hub, lockfile, or project links
        #[arg(long)]
        preview: bool,
        /// Git URL of the skill repository
        url: String,
    },
    /// Update installed skills
    Update {
        /// Name of a specific skill to update (updates all if omitted)
        name: Option<String>,
    },
    /// Remove one or more installed skills
    #[command(alias = "rm", alias = "uninstall")]
    Remove {
        /// Skill name(s) to remove — space- or comma-separated (e.g. `rm a b` or `rm a,b`)
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
    /// Run health checks on installed packs or all packs
    Doctor {
        /// Pack name to check (checks all packs if omitted)
        name: Option<String>,
    },
    /// Manage installed skill packs
    Pack {
        #[command(subcommand)]
        action: PackAction,
    },
    /// Force launch GUI mode
    Gui,
}

/// Pack sub-commands.
#[derive(Subcommand)]
pub enum PackAction {
    /// List installed skill packs
    List,
    /// Remove an installed skill pack by name
    Remove {
        /// Pack name to remove
        name: String,
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
}

/// Options passed to the remove handler.
pub struct RemoveOpts<'a> {
    pub names: &'a [String],
    pub all: bool,
    pub yes: bool,
}

pub struct CliHandlers {
    pub migrate_and_run: fn(),
    pub install: fn(InstallOpts<'_>),
    pub update: fn(name: Option<&str>),
    pub remove: fn(RemoveOpts<'_>),
    pub publish: fn(),
    pub doctor: fn(name: Option<&str>),
    pub pack_list: fn(),
    pub pack_remove: fn(name: &str),
    pub gui: fn(),
}

pub fn run(args: Vec<String>, handlers: CliHandlers) {
    // Migration (only runs once at startup)
    (handlers.migrate_and_run)();

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
        } => (handlers.install)(InstallOpts {
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
        }),
        Commands::Update { name } => (handlers.update)(name.as_deref()),
        Commands::Remove { names, all, yes } => (handlers.remove)(RemoveOpts {
            names: &names,
            all,
            yes,
        }),
        Commands::Init { name } => cmd_init(name.as_deref()),
        Commands::Publish => (handlers.publish)(),
        Commands::Doctor { name } => (handlers.doctor)(name.as_deref()),
        Commands::Pack { action } => match action {
            PackAction::List => (handlers.pack_list)(),
            PackAction::Remove { name } => (handlers.pack_remove)(&name),
        },
        Commands::Gui => (handlers.gui)(),
    }
}
